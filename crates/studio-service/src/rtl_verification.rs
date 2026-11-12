//! Sampled generated-RTL verification. Only validated, internally generated code
//! is executed, with fixed arguments and bounded wall time/output.
use design_core::{Document, Kind, TestVector};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::Read,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
const LOG_LIMIT: u64 = 4 * 1024 * 1024;

fn testbench(
    doc: &Document,
    vectors: &[TestVector],
) -> Result<(String, Vec<BTreeMap<String, u64>>), String> {
    let ports: Vec<_> = doc
        .nodes
        .iter()
        .filter(|n| matches!(n.kind, Kind::Input | Kind::Output))
        .collect();
    if vectors
        .len()
        .saturating_mul(ports.iter().filter(|n| n.kind == Kind::Output).count())
        > 131_072
    {
        return Err("RTL samples exceed 131072 output values".into());
    }
    let mut text = "`timescale 1ns/1ps\nmodule __las_verification_tb;\n".to_string();
    for (i, n) in ports.iter().enumerate() {
        text += &format!(
            "{} [{}:0] p{i};\n",
            if n.kind == Kind::Input { "reg" } else { "wire" },
            n.width - 1
        );
    }
    text += &format!(
        "las_{} dut ({});\ninitial begin\n",
        doc.name,
        ports
            .iter()
            .enumerate()
            .map(|(i, n)| format!(".n_{}(p{i})", n.name))
            .collect::<Vec<_>>()
            .join(",")
    );
    let mut reference = vec![];
    for (index, vector) in vectors.iter().enumerate() {
        if text.len() > 16 * 1024 * 1024 {
            return Err("RTL testbench exceeds 16 MiB".into());
        }
        let inputs = vector
            .inputs
            .iter()
            .map(|(k, v)| Ok((k.clone(), design_core::number(v)?)))
            .collect::<Result<BTreeMap<_, _>, String>>()?;
        if inputs.len() != ports.iter().filter(|n| n.kind == Kind::Input).count() {
            return Err("Supply exactly the design inputs".into());
        }
        reference.push(doc.evaluate(&inputs)?);
        for (i, n) in ports
            .iter()
            .enumerate()
            .filter(|(_, n)| n.kind == Kind::Input)
        {
            text += &format!(
                "p{i} = {}'h{:x};\n",
                n.width,
                inputs.get(&n.name).ok_or("Missing input")? & design_core::mask(n.width)
            );
        }
        let outputs: Vec<_> = ports
            .iter()
            .enumerate()
            .filter(|(_, n)| n.kind == Kind::Output)
            .map(|(i, _)| format!("p{i}"))
            .collect();
        text += &format!(
            "#1; $display(\"LAS_RESULT {index} {}\", {});\n",
            vec!["%h"; outputs.len()].join(" "),
            outputs.join(",")
        );
    }
    text += "$display(\"LAS_DONE\"); $finish;\nend\nendmodule\n";
    if text.len() > 16 * 1024 * 1024 {
        return Err("RTL testbench exceeds 16 MiB".into());
    }
    Ok((text, reference))
}
fn execute(
    binary: &Path,
    args: &[&str],
    dir: &Path,
    log_name: &str,
    deadline: Instant,
) -> Result<bool, String> {
    let log = File::create(dir.join(log_name)).map_err(|e| e.to_string())?;
    let mut command = Command::new(binary);
    command
        .current_dir(dir)
        .args(args)
        .stdin(Stdio::null())
        .stdout(log.try_clone().map_err(|e| e.to_string())?)
        .stderr(log);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command.spawn().map_err(|e| e.to_string())?;
    loop {
        let status = child.try_wait();
        let excessive = fs::metadata(dir.join(log_name))
            .map(|m| m.len() > LOG_LIMIT)
            .unwrap_or(false);
        if Instant::now() >= deadline || excessive || status.is_err() {
            #[cfg(unix)]
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGKILL);
            }
            let _ = child.kill();
            let _ = child.wait();
            return Err(if excessive {
                "Tool output exceeded 4 MiB"
            } else {
                "Tool timed out or could not be monitored"
            }
            .into());
        }
        if let Some(status) = status.map_err(|e| e.to_string())? {
            return Ok(status.success());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}
fn log(dir: &Path, name: &str) -> String {
    let mut bytes = vec![];
    if let Ok(file) = File::open(dir.join(name)) {
        let _ = file.take(LOG_LIMIT).read_to_end(&mut bytes);
    }
    String::from_utf8_lossy(&bytes).into_owned()
}
fn compare(
    text: &str,
    outputs: &[String],
    reference: &[BTreeMap<String, u64>],
) -> Result<(usize, Vec<String>), String> {
    let mut seen = 0;
    let mut passed = 0;
    let mut failures = vec![];
    let mut done = false;
    for line in text.lines() {
        if line == "LAS_DONE" {
            if done || seen != reference.len() {
                return Err("Incomplete or duplicated RTL result stream".into());
            }
            done = true;
        }
        if !line.starts_with("LAS_RESULT ") {
            continue;
        }
        let words: Vec<_> = line.split_whitespace().collect();
        if done
            || words.len() != outputs.len() + 2
            || words[1].parse::<usize>().ok() != Some(seen)
            || seen >= reference.len()
        {
            return Err("Malformed RTL result stream".into());
        }
        let matches = outputs.iter().enumerate().all(|(i, name)| {
            u64::from_str_radix(words[i + 2], 16).ok() == reference[seen].get(name).copied()
        });
        if matches {
            passed += 1;
        } else if failures.len() < 20 {
            failures.push(format!(
                "Vector {seen}: RTL {} ; Rust {:?}",
                words[2..].join(" "),
                reference[seen]
            ));
        }
        seen += 1;
    }
    if !done || seen != reference.len() {
        return Err("RTL simulation did not produce every vector and completion marker".into());
    }
    Ok((passed, failures))
}
#[derive(Clone, Copy)]
enum Engine {
    Icarus,
    Verilator,
}
impl Engine {
    fn id(self) -> &'static str {
        match self {
            Self::Icarus => "icarus",
            Self::Verilator => "verilator",
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::Icarus => "Icarus Verilog",
            Self::Verilator => "Verilator",
        }
    }
}
pub fn run_verilator(
    root: &Path,
    revision: u64,
    doc: &Document,
    vectors: &[TestVector],
) -> Result<Value, String> {
    run_engine_source(root, revision, doc, vectors, &doc.rtl()?, Engine::Verilator)
}
pub fn run(
    root: &Path,
    revision: u64,
    doc: &Document,
    vectors: &[TestVector],
) -> Result<Value, String> {
    run_engine_source(root, revision, doc, vectors, &doc.rtl()?, Engine::Icarus)
}
#[cfg(test)]
fn run_source(
    root: &Path,
    revision: u64,
    doc: &Document,
    vectors: &[TestVector],
    source: &str,
) -> Result<Value, String> {
    run_engine_source(root, revision, doc, vectors, source, Engine::Icarus)
}
fn run_engine_source(
    root: &Path,
    revision: u64,
    doc: &Document,
    vectors: &[TestVector],
    source: &str,
    engine: Engine,
) -> Result<Value, String> {
    let contract = design_core::verify(doc, revision, vectors)?;
    let (bench, reference) = testbench(doc, vectors)?;
    let compiler = super::executable(match engine {
        Engine::Icarus => "iverilog",
        Engine::Verilator => "verilator",
    })
    .ok_or_else(|| format!("{} is not installed", engine.label()))?;
    let runtime = match engine {
        Engine::Icarus => {
            Some(super::executable("vvp").ok_or("Icarus vvp runtime is not installed")?)
        }
        Engine::Verilator => None,
    };
    let run = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let dir = root.join(format!(".logic/runs/rtl-{}-{revision}-{run}", engine.id()));
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let dir = dir.canonicalize().map_err(|e| e.to_string())?;
    fs::write(dir.join("design.v"), source).map_err(|e| e.to_string())?;
    fs::write(dir.join("testbench.v"), bench).map_err(|e| e.to_string())?;
    fs::write(
        dir.join("vectors.json"),
        serde_json::to_vec(vectors).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    fs::write(
        dir.join("reference.json"),
        serde_json::to_vec(&reference).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut result = json!({"revision":revision,"backend":engine.id(),"engine":format!("{} / Rust · sampled vectors (not formal proof)",engine.label()),"success":false,"stage":"compile","total":vectors.len(),"rtl_passed":0,"contract_passed":contract.passed,"failures":contract.failures,"artifact_directory":dir});
    let outcome = (|| -> Result<(), String> {
        let arguments: &[&str] = match engine {
            Engine::Icarus => &[
                "-g2012",
                "-s",
                "__las_verification_tb",
                "-o",
                "simulation.vvp",
                "design.v",
                "testbench.v",
            ],
            Engine::Verilator => &[
                "--binary",
                "--timing",
                "--top-module",
                "__las_verification_tb",
                "--Mdir",
                "obj_dir",
                "-o",
                "las_sim",
                "--build-jobs",
                "2",
                "-Wno-fatal",
                "design.v",
                "testbench.v",
            ],
        };
        if !execute(&compiler, arguments, &dir, "compile.log", deadline)? {
            return Err("RTL compilation failed".into());
        }
        result["stage"] = json!("simulation");
        let runtime = runtime.unwrap_or_else(|| dir.join("obj_dir/las_sim"));
        let runtime = runtime.canonicalize().map_err(|e| e.to_string())?;
        let args: &[&str] = match engine {
            Engine::Icarus => &["-n", "simulation.vvp"],
            Engine::Verilator => &[],
        };
        if !execute(&runtime, args, &dir, "simulation.log", deadline)? {
            return Err("RTL simulation failed".into());
        }
        let outputs: Vec<_> = doc
            .nodes
            .iter()
            .filter(|n| n.kind == Kind::Output)
            .map(|n| n.name.clone())
            .collect();
        let (passed, failures) = compare(&log(&dir, "simulation.log"), &outputs, &reference)?;
        result["rtl_passed"] = json!(passed);
        result["success"] = json!(passed == vectors.len() && contract.passed == vectors.len());
        result["rtl_failures"] = json!(failures);
        result["stage"] = json!("complete");
        Ok(())
    })();
    if let Err(error) = outcome {
        result["error"] = json!(error);
    }
    result["log"] = json!(format!(
        "{}\n{}",
        log(&dir, "compile.log"),
        log(&dir, "simulation.log")
    )
    .chars()
    .take(256 * 1024)
    .collect::<String>());
    fs::write(
        dir.join("result.json"),
        serde_json::to_vec_pretty(&result).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(result)
}

pub fn prove_alu(root: &Path, revision: u64, doc: &Document) -> Result<Value, String> {
    let width = design_core::alu_width(doc)?;
    let source = doc.rtl()?;
    let yosys = super::executable("yosys").ok_or("Yosys is not installed")?;
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let dir = root.join(format!(".logic/runs/formal-{revision}-{timestamp}"));
    fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let dir = dir.canonicalize().map_err(|error| error.to_string())?;
    let high_bit = width - 1;
    let module = &doc.name;
    let miter = format!(
        "module __las_alu_miter(input [{high_bit}:0] a, b, input [1:0] op, output mismatch);
         wire [{high_bit}:0] actual;
         reg [{high_bit}:0] expected;
         las_{module} dut(.n_a(a), .n_b(b), .n_op(op), .n_result(actual));
         always @* begin
             case (op)
                 2'd0: expected = a + b;
                 2'd1: expected = a & b;
                 default: expected = a ^ b;
             endcase
         end
         assign mismatch = actual != expected;
         endmodule"
    );
    let script = "read_verilog design.v miter.v
prep -top __las_alu_miter -flatten
sat -timeout 25 -set-def-inputs -prove mismatch 0 -verify -show a -show b -show op -show actual -show expected -dump_json counterexample.json";
    for (name, content) in [
        ("design.v", source.as_str()),
        ("miter.v", &miter),
        ("prove.ys", script),
    ] {
        fs::write(dir.join(name), content).map_err(|error| error.to_string())?;
    }
    let outcome = execute(
        &yosys,
        &["-Q", "-s", "prove.ys"],
        &dir,
        "yosys.log",
        Instant::now() + Duration::from_secs(30),
    );
    let output = log(&dir, "yosys.log");
    let witness = File::open(dir.join("counterexample.json"))
        .ok()
        .and_then(|file| serde_json::from_reader::<_, Value>(file.take(64 * 1024)).ok())
        .and_then(|wave| {
            let signals = wave["signal"].as_array()?;
            let mut values = BTreeMap::new();
            for (name, bits) in [
                ("a", width),
                ("b", width),
                ("op", 2),
                ("actual", width),
                ("expected", width),
            ] {
                let signal = signals.iter().find(|signal| signal["name"] == name)?;
                let binary = signal["data"][0].as_str()?;
                if binary.len() != usize::from(bits) {
                    return None;
                }
                let value = u64::from_str_radix(binary, 2).ok()?;
                values.insert(name, format!("0x{value:x}"));
            }
            (values["actual"] != values["expected"]).then_some(values)
        });
    let status = match &outcome {
        Ok(true) if output.contains("SAT proof finished - no model found: SUCCESS!") => "proved",
        Ok(false) if witness.is_some() && output.contains("proof did fail") => "counterexample",
        _ => "inconclusive",
    };
    let result = json!({
        "revision": revision, "engine": "Yosys SAT", "status": status,
        "contract": "alu", "width": width,
        "counterexample": witness, "artifact_directory": dir,
        "error": outcome.err(), "log": output.chars().take(256 * 1024).collect::<String>(),
    });
    fs::write(
        dir.join("result.json"),
        serde_json::to_vec_pretty(&result).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Root(std::path::PathBuf);
    impl Root {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("las-rtl-{name}-{}", std::process::id()));
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }
    impl Drop for Root {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn rejects_missing_duplicate_out_of_order_and_unknown_result_values() {
        let reference = vec![BTreeMap::from([("out".into(), u64::MAX)])];
        let outputs = vec!["out".into()];
        for text in [
            "",
            "LAS_RESULT 0 ffffffffffffffff",
            "LAS_DONE",
            "LAS_RESULT 1 0\nLAS_DONE",
            "LAS_RESULT 0 0\nLAS_RESULT 0 0\nLAS_DONE",
        ] {
            assert!(compare(text, &outputs, &reference).is_err());
        }
        let (passed, failures) = compare(
            "LAS_RESULT 0 xxxxxxxxxxxxxxxx\nLAS_DONE",
            &outputs,
            &reference,
        )
        .unwrap();
        assert_eq!(passed, 0);
        assert_eq!(failures.len(), 1);
        assert_eq!(
            compare(
                "LAS_RESULT 0 ffffffffffffffff\nLAS_DONE",
                &outputs,
                &reference
            )
            .unwrap()
            .0,
            1
        );
    }
    #[test]
    #[ignore = "Requires installed Icarus Verilog and child processes"]
    fn real_icarus_covers_widths_operators_contracts_and_bad_generated_rtl() {
        let root = Root::new("real");
        for width in [1, 8, 32, 64] {
            let mut doc = Document::demo();
            for node in doc.nodes.iter_mut() {
                if node.id != "op" {
                    node.width = width;
                }
            }
            let vectors = design_core::alu_vectors(&doc).unwrap();
            let result = run(&root.0, 17, &doc, &vectors).unwrap();
            assert_eq!(result["success"], true, "{result}");
            assert_eq!(result["rtl_passed"], 320);
            for (id, kind) in [
                ("add", Kind::Sub),
                ("bit_and", Kind::Or),
                ("bit_xor", Kind::Not),
            ] {
                doc.nodes.iter_mut().find(|n| n.id == id).unwrap().kind = kind;
            }
            doc.edges.retain(|e| e.id != "b_bit_xor");
            let custom: Vec<_> = vectors
                .iter()
                .map(|v| {
                    let inputs = v
                        .inputs
                        .iter()
                        .map(|(k, v)| (k.clone(), design_core::number(v).unwrap()))
                        .collect();
                    TestVector {
                        inputs: v.inputs.clone(),
                        expected: doc
                            .evaluate(&inputs)
                            .unwrap()
                            .into_iter()
                            .map(|(k, v)| (k, v.to_string()))
                            .collect(),
                    }
                })
                .collect();
            assert_eq!(run(&root.0, 18, &doc, &custom).unwrap()["success"], true);
        }
        let mut doc = Document::demo();
        doc.name = "tb".into(); // Must not collide with the generated testbench.
        let vectors = design_core::alu_vectors(&doc).unwrap();
        let source = doc.rtl().unwrap().replace("n_a + n_b", "n_a - n_b");
        let result = run_source(&root.0, 20, &doc, &vectors, &source).unwrap();
        assert_eq!(result["contract_passed"], 320);
        assert_eq!(result["success"], false);
        assert!(result["rtl_passed"].as_u64().unwrap() < 320);
        let result = run_source(&root.0, 21, &doc, &vectors, "invalid Verilog").unwrap();
        assert_eq!(result["success"], false);
        assert_eq!(result["stage"], "compile");
        assert!(result["error"].is_string());
        let mut bad_contract = vectors.clone();
        bad_contract[0].expected.insert("result".into(), "1".into());
        let result = run(&root.0, 22, &doc, &bad_contract).unwrap();
        assert_eq!(result["rtl_passed"], 320);
        assert_eq!(result["contract_passed"], 319);
        assert_eq!(result["success"], false);
        let service = crate::Service::open(&root.0).unwrap();
        let before = service.state.lock().unwrap().clone();
        let request = |revision| crate::Request {
            method: "verify.rtl".into(),
            args: json!({"expected_revision":revision}),
            actor: "test".into(),
        };
        assert!(service.execute(request(999)).is_err());
        assert_eq!(service.execute(request(0)).unwrap()["success"], true);
        assert_eq!(*service.state.lock().unwrap(), before);
    }
    #[test]
    #[ignore = "Requires Verilator, Icarus, C++ compiler and child processes"]
    fn real_verilator_differential_regression_and_failures() {
        let root = Root::new("verilator");
        for width in [1, 8, 32, 64] {
            let mut doc = Document::demo();
            for node in doc.nodes.iter_mut() {
                if node.id != "op" {
                    node.width = width;
                }
            }
            for alternate in [false, true] {
                if alternate {
                    for (id, kind) in [
                        ("add", Kind::Sub),
                        ("bit_and", Kind::Or),
                        ("bit_xor", Kind::Not),
                    ] {
                        doc.nodes.iter_mut().find(|n| n.id == id).unwrap().kind = kind;
                    }
                    doc.edges.retain(|e| e.id != "b_bit_xor");
                }
                let mut vectors = design_core::alu_vectors(&doc).unwrap();
                if alternate {
                    for vector in &mut vectors {
                        let inputs = vector
                            .inputs
                            .iter()
                            .map(|(k, v)| (k.clone(), design_core::number(v).unwrap()))
                            .collect();
                        vector.expected = doc
                            .evaluate(&inputs)
                            .unwrap()
                            .into_iter()
                            .map(|(k, v)| (k, v.to_string()))
                            .collect();
                    }
                }
                let icarus = run(&root.0, 0, &doc, &vectors).unwrap();
                let verilator = run_verilator(&root.0, 0, &doc, &vectors).unwrap();
                assert_eq!(
                    verilator["success"], true,
                    "width {width}, alternate {alternate}: {verilator}"
                );
                assert_eq!(verilator["rtl_passed"], icarus["rtl_passed"]);
                assert_eq!(verilator["contract_passed"], icarus["contract_passed"]);
                assert_eq!(verilator["backend"], "verilator");
            }
        }
        let mut doc = Document::demo();
        doc.name = "tb".into();
        let vectors = design_core::alu_vectors(&doc).unwrap();
        let bad = doc.rtl().unwrap().replace("n_a + n_b", "n_a - n_b");
        let result =
            run_engine_source(&root.0, 0, &doc, &vectors, &bad, Engine::Verilator).unwrap();
        assert_eq!(result["success"], false);
        assert_eq!(result["stage"], "complete");
        assert!(result["rtl_passed"].as_u64().unwrap() < 320);
        assert_eq!(result["contract_passed"], 320);
        let result = run_engine_source(
            &root.0,
            0,
            &doc,
            &vectors,
            "invalid Verilog",
            Engine::Verilator,
        )
        .unwrap();
        assert_eq!(result["success"], false);
        assert_eq!(result["stage"], "compile");
        let service = crate::Service::open(&root.0).unwrap();
        let result = service
            .execute(crate::Request {
                method: "verify.rtl".into(),
                args: json!({"expected_revision":0,"engine":"verilator"}),
                actor: "test".into(),
            })
            .unwrap();
        assert_eq!(result["success"], true, "{result}");
        assert_eq!(service.state.lock().unwrap().revision, 0);
    }
    #[test]
    #[ignore = "Requires installed Yosys"]
    fn formal_alu_proves_and_returns_counterexample() {
        let root = Root::new("formal");
        let service = crate::Service::open(&root.0).unwrap();
        let request = |revision| crate::Request {
            method: "verify.formal".into(),
            args: json!({"expected_revision": revision}),
            actor: "test".into(),
        };
        assert!(service.execute(request(1)).is_err());
        let result = service.execute(request(0)).unwrap();
        assert_eq!(result["status"], "proved", "{result}");
        assert_eq!(service.state.lock().unwrap().revision, 0);

        let mut doc = Document::demo();
        for node in doc.nodes.iter_mut() {
            if node.id != "op" {
                node.width = 64;
            }
        }
        let result = prove_alu(&root.0, 0, &doc).unwrap();
        assert_eq!(result["status"], "proved", "{result}");
        doc.nodes
            .iter_mut()
            .find(|node| node.id == "add")
            .unwrap()
            .kind = Kind::Sub;
        let result = prove_alu(&root.0, 1, &doc).unwrap();
        assert_eq!(result["status"], "counterexample", "{result}");
        let witness = &result["counterexample"];
        let value = |name: &str| design_core::number(witness[name].as_str().unwrap()).unwrap();
        assert_eq!(value("op"), 0);
        assert_eq!(value("actual"), value("a").wrapping_sub(value("b")));
        assert_eq!(value("expected"), value("a").wrapping_add(value("b")));
    }

    fn next_sample(state: &mut u64) -> u64 {
        *state ^= *state << 13;
        *state ^= *state >> 7;
        *state ^= *state << 17;
        *state
    }

    fn generated_circuit(seed: u64, widths: [u8; 2]) -> (Document, Vec<TestVector>) {
        let mut state = seed;
        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        for (lane, width) in widths.into_iter().enumerate() {
            let mut available = Vec::new();
            let selector = format!("sel_{lane}");
            for index in 0..3 {
                let name = format!("input_{lane}_{index}");
                available.push(name.clone());
                nodes.push(design_core::Node {
                    id: name.clone(),
                    name,
                    kind: Kind::Input,
                    width,
                    x: 0.,
                    y: 0.,
                });
            }
            nodes.push(design_core::Node {
                id: selector.clone(),
                name: selector.clone(),
                kind: Kind::Input,
                width: 2,
                x: 0.,
                y: 0.,
            });
            for (index, kind) in [
                Kind::Add,
                Kind::Sub,
                Kind::And,
                Kind::Or,
                Kind::Xor,
                Kind::Not,
                Kind::Mux,
                Kind::Output,
                Kind::Output,
            ]
            .into_iter()
            .enumerate()
            {
                let name = format!("gate_{lane}_{index}");
                let node = design_core::Node {
                    id: name.clone(),
                    name,
                    kind,
                    width,
                    x: 0.,
                    y: 0.,
                };
                for (port_index, (port, _)) in node.inputs().into_iter().enumerate() {
                    let source = if port == "sel" {
                        &selector
                    } else if port_index == 0 && index != 8 {
                        available.last().unwrap()
                    } else {
                        &available[(next_sample(&mut state) as usize) % available.len()]
                    };
                    edges.push(design_core::Edge {
                        id: format!("edge_{}", edges.len()),
                        from: source.clone(),
                        to: node.id.clone(),
                        port: port.into(),
                    });
                }
                if node.kind != Kind::Output {
                    available.push(node.id.clone());
                }
                nodes.push(node);
            }
        }
        // Reverse document order to exercise forward references and output ordering.
        nodes.reverse();
        let doc = Document {
            name: format!("seed_{seed:x}"),
            nodes: nodes.into(),
            edges: edges.into(),
        };
        let vectors = (0..64)
            .map(|index| {
                let inputs: BTreeMap<_, _> = doc
                    .nodes
                    .iter()
                    .filter(|node| node.kind == Kind::Input)
                    .map(|node| {
                        let value = if node.name.starts_with("sel_") {
                            index % 4
                        } else {
                            match index {
                                0..=3 => 0,
                                4..=7 => u64::MAX,
                                _ => next_sample(&mut state),
                            }
                        };
                        (node.name.clone(), value)
                    })
                    .collect();
                let expected = doc.evaluate(&inputs).unwrap();
                TestVector {
                    inputs: inputs
                        .into_iter()
                        .map(|(name, value)| (name, format!("0x{value:x}")))
                        .collect(),
                    expected: expected
                        .into_iter()
                        .map(|(name, value)| (name, value.to_string()))
                        .collect(),
                }
            })
            .collect();
        (doc, vectors)
    }

    #[test]
    #[ignore = "Requires installed Icarus, Verilator and C++ compiler"]
    fn seeded_multi_output_circuits_match_both_simulators() {
        let root = Root::new("seeded");
        for (seed, widths) in [(0xa103, [3, 17]), (0xb729, [7, 33]), (0xc451, [31, 63])] {
            let (doc, vectors) = generated_circuit(seed, widths);
            for result in [
                run(&root.0, 0, &doc, &vectors),
                run_verilator(&root.0, 0, &doc, &vectors),
            ] {
                let result = result.unwrap();
                assert_eq!(
                    result["success"], true,
                    "seed {seed:x}, widths {widths:?}: {result}"
                );
                assert_eq!(result["rtl_passed"], 64);
            }
        }
    }
    #[test]
    #[ignore = "Requires installed Icarus"]
    fn sequential_feedback_reset_and_pipeline_match_generated_rtl() {
        let root = Root::new("sequential");
        let nodes = [
            ("inc", Kind::Input),
            ("sum", Kind::Add),
            ("acc", Kind::Register),
            ("delay", Kind::Register),
            ("result", Kind::Output),
        ]
        .into_iter()
        .map(|(name, kind)| design_core::Node {
            id: name.into(),
            name: name.into(),
            kind,
            width: 3,
            x: 0.,
            y: 0.,
        })
        .collect::<Vec<_>>();
        let edges = [
            ("inc", "sum", "a"),
            ("acc", "sum", "b"),
            ("sum", "acc", "d"),
            ("acc", "delay", "d"),
            ("delay", "result", "a"),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (from, to, port))| design_core::Edge {
            id: format!("edge_{index}"),
            from: from.into(),
            to: to.into(),
            port: port.into(),
        })
        .collect::<Vec<_>>();
        let doc = Document {
            name: "accumulator".into(),
            nodes: nodes.into(),
            edges: edges.into(),
        };
        let mut workspace = design_core::Workspace::default();
        workspace.document = std::sync::Arc::new(doc.clone());
        workspace
            .dispatch(
                "layout.auto_arrange",
                json!({"expected_revision":0}),
                "test",
            )
            .unwrap();
        assert!(doc.evaluate(&BTreeMap::new()).is_err());
        assert!(doc.simulate_cycles(&[]).is_err());
        let cycles = [
            (true, 3),
            (false, 3),
            (false, 3),
            (false, 3),
            (true, 7),
            (false, 7),
        ];
        let payload =
            cycles.map(|(reset, inc)| json!({"reset":reset,"inputs":{"inc":inc.to_string()}}));
        let response = workspace
            .dispatch(
                "simulation.cycles",
                json!({"expected_revision":1,"cycles":payload}),
                "test",
            )
            .unwrap();
        let samples = response["samples"].as_array().unwrap();
        let expected = ["0 0", "3 0", "6 3", "1 6", "0 0", "7 0"];
        for (sample, expected) in samples.iter().zip(expected) {
            assert_eq!(
                format!(
                    "{} {}",
                    sample["registers"]["acc"].as_str().unwrap(),
                    sample["outputs"]["result"].as_str().unwrap()
                ),
                expected
            );
        }
        fs::write(root.0.join("design.v"), doc.rtl().unwrap()).unwrap();
        let mut bench = String::from("module tb; reg clk=0, rst; reg [2:0] inc; wire [2:0] result; las_accumulator dut(.las_clk(clk), .las_reset(rst), .n_inc(inc), .n_result(result)); initial begin\n");
        for (reset, inc) in cycles {
            bench += &format!("rst={}; inc={inc}; #1; clk=1; #1; $display(\"%0d %0d\", dut.n_acc, result); clk=0; #1;\n", u8::from(reset));
        }
        bench += "end endmodule";
        fs::write(root.0.join("tb.v"), bench).unwrap();
        let deadline = Instant::now() + Duration::from_secs(30);
        assert!(
            execute(
                &super::super::executable("iverilog").unwrap(),
                &["-s", "tb", "-o", "sim.vvp", "design.v", "tb.v"],
                &root.0,
                "compile.log",
                deadline
            )
            .unwrap(),
            "{}",
            log(&root.0, "compile.log")
        );
        assert!(execute(
            &super::super::executable("vvp").unwrap(),
            &["sim.vvp"],
            &root.0,
            "sim.log",
            deadline
        )
        .unwrap());
        assert_eq!(
            log(&root.0, "sim.log").lines().collect::<Vec<_>>(),
            expected
        );
    }

    #[cfg(unix)]
    #[test]
    #[ignore = "Requires child processes"]
    fn tool_deadline_and_output_limit_fail_closed() {
        let root = Root::new("limits");
        assert!(execute(
            Path::new("/bin/sh"),
            &["-c", "sleep 60"],
            &root.0,
            "timeout.log",
            Instant::now() + Duration::from_millis(50)
        )
        .unwrap_err()
        .contains("timed out"));
        assert!(execute(
            Path::new("/bin/sh"),
            &["-c", "yes output"],
            &root.0,
            "output.log",
            Instant::now() + Duration::from_secs(5)
        )
        .unwrap_err()
        .contains("output exceeded"));
    }
}
