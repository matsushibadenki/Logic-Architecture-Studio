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
pub fn run(
    root: &Path,
    revision: u64,
    doc: &Document,
    vectors: &[TestVector],
) -> Result<Value, String> {
    run_source(root, revision, doc, vectors, &doc.rtl()?)
}
fn run_source(
    root: &Path,
    revision: u64,
    doc: &Document,
    vectors: &[TestVector],
    source: &str,
) -> Result<Value, String> {
    let contract = design_core::verify(doc, revision, vectors)?;
    let (bench, reference) = testbench(doc, vectors)?;
    let compiler = super::executable("iverilog").ok_or("Icarus Verilog is not installed")?;
    let runtime = super::executable("vvp").ok_or("Icarus vvp runtime is not installed")?;
    let run = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let dir = root.join(format!(".logic/runs/rtl-{revision}-{run}"));
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
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
    let mut result = json!({"revision":revision,"engine":"Icarus Verilog / Rust · sampled vectors (not formal proof)","success":false,"stage":"compile","total":vectors.len(),"rtl_passed":0,"contract_passed":contract.passed,"failures":contract.failures,"artifact_directory":dir});
    let outcome = (|| -> Result<(), String> {
        if !execute(
            &compiler,
            &[
                "-g2012",
                "-s",
                "__las_verification_tb",
                "-o",
                "simulation.vvp",
                "design.v",
                "testbench.v",
            ],
            &dir,
            "compile.log",
            deadline,
        )? {
            return Err("RTL compilation failed".into());
        }
        result["stage"] = json!("simulation");
        if !execute(
            &runtime,
            &["-n", "simulation.vvp"],
            &dir,
            "simulation.log",
            deadline,
        )? {
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
