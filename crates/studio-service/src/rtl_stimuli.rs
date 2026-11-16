//! Run saved, validated stimulus rows against generated RTL only.
use crate::rtl_verification::{execute, log};
use design_core::{testbench::CheckReport, CycleInput, CycleSample, Document, Kind, Workspace};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub(crate) const VCD_LIMIT: u64 = 12 * 1024 * 1024;
pub(crate) struct Run {
    pub cycles: Vec<CycleInput>,
    pub samples: Vec<CycleSample>,
    pub checks: CheckReport,
    pub comparison: Value,
    pub vcd: String,
}
fn bench(doc: &Document, cycles: &[CycleInput]) -> Result<String, String> {
    if cycles.len().saturating_mul(doc.nodes.len()) > 131_072 {
        return Err("External trace exceeds 131072 node samples; use fewer cycles".into());
    }
    let sequential = doc.nodes.iter().any(|n| n.kind == Kind::Register);
    let ports: Vec<_> = doc
        .nodes
        .iter()
        .filter(|n| matches!(n.kind, Kind::Input | Kind::Output))
        .collect();
    let mut text = String::from(
        "`timescale 1ns/1ps\nmodule testbench_las_stimuli;\nreg tb_clk=0; reg tb_reset=0;\n",
    );
    for (i, n) in ports.iter().enumerate() {
        text += &format!(
            "{}{} [{}:0] p{i};\n",
            if n.kind == Kind::Input { "reg" } else { "wire" },
            if n.signed { " signed" } else { "" },
            n.width - 1
        );
    }
    let mut connections: Vec<_> = ports
        .iter()
        .enumerate()
        .map(|(i, n)| format!(".n_{}(p{i})", n.name))
        .collect();
    if sequential {
        connections.extend([".las_clk(tb_clk)".into(), ".las_reset(tb_reset)".into()]);
    }
    text += &format!(
        "las_{} dut ({});\ninitial begin\n$dumpfile(\"simulation.vcd\"); $dumpvars(0,dut);\n",
        doc.name,
        connections.join(",")
    );
    let signals = doc
        .nodes
        .iter()
        .map(|n| format!("dut.n_{}", n.name))
        .collect::<Vec<_>>()
        .join(",");
    for (index, cycle) in cycles.iter().enumerate() {
        let inputs = doc.parse_signals(&cycle.inputs, Kind::Input)?;
        text += &format!("tb_reset={};\n", u8::from(cycle.reset));
        for (i, n) in ports
            .iter()
            .enumerate()
            .filter(|(_, n)| n.kind == Kind::Input)
        {
            text += &format!("p{i}={}'h{:X};\n", n.width, inputs[&n.name]);
        }
        text += if sequential {
            "#5; tb_clk=1; #1;\n"
        } else {
            "#6;\n"
        };
        text += &format!(
            "$display(\"LAS_SAMPLE {index} {}\",{});\n#4; tb_clk=0;\n",
            vec!["%h"; doc.nodes.len()].join(" "),
            signals
        );
    }
    text += "$display(\"LAS_DONE\"); $finish;\nend\nendmodule\n";
    if text.len() > 16 * 1024 * 1024 {
        return Err("RTL testbench exceeds 16 MiB".into());
    }
    Ok(text)
}
fn samples(doc: &Document, cycles: &[CycleInput], text: &str) -> Result<Vec<CycleSample>, String> {
    let mut samples = vec![];
    let mut done = false;
    for line in text.lines() {
        if line == "LAS_DONE" {
            if done || samples.len() != cycles.len() {
                return Err("Incomplete or duplicate RTL completion".into());
            }
            done = true;
        } else if line.starts_with("LAS_SAMPLE ") {
            let fields: Vec<_> = line.split_whitespace().collect();
            let i = samples.len();
            if done
                || i >= cycles.len()
                || fields.len() != doc.nodes.len() + 2
                || fields[1].parse::<usize>().ok() != Some(i)
            {
                return Err("Malformed RTL sample stream".into());
            }
            let mut values = BTreeMap::new();
            let mut outputs = BTreeMap::new();
            let mut registers = BTreeMap::new();
            for (node, field) in doc.nodes.iter().zip(&fields[2..]) {
                let bits = u64::from_str_radix(field, 16).map_err(|_| {
                    format!(
                        "Cycle {}: {} contains X/Z or an invalid RTL value",
                        i + 1,
                        node.name
                    )
                })?;
                if bits > design_core::mask(node.width) {
                    return Err("RTL value exceeds declared width".into());
                }
                values.insert(node.id.clone(), bits);
                if node.kind == Kind::Output {
                    outputs.insert(node.name.clone(), node.decimal(bits));
                }
                if node.kind == Kind::Register {
                    registers.insert(node.name.clone(), node.decimal(bits));
                }
            }
            samples.push(CycleSample {
                reset: cycles[i].reset,
                values,
                outputs,
                registers,
            });
        }
    }
    if !done {
        return Err("Missing RTL completion marker".into());
    }
    Ok(samples)
}
fn compare(
    workspace: &Workspace,
    actual: &[CycleSample],
    reference: &[CycleSample],
) -> Result<(CheckReport, Value), String> {
    let doc = &workspace.document;
    let checks = workspace.testbench.check_samples(doc, actual)?;
    let mut passed = 0;
    let mut failures = vec![];
    for (i, (sample, rust)) in actual.iter().zip(reference).enumerate() {
        for node in &doc.nodes {
            if sample.values[&node.id] == rust.values[&node.id] {
                passed += 1;
            } else if failures.len() < 100 {
                failures.push(json!({"cycle":i,"signal":node.name,"expected":node.decimal(rust.values[&node.id]),"actual":node.decimal(sample.values[&node.id])}));
            }
        }
    }
    Ok((
        checks,
        json!({"total":actual.len()*doc.nodes.len(),"passed":passed,"failures":failures}),
    ))
}
pub(crate) fn run(root: &Path, workspace: &Workspace, engine: &str) -> Result<Run, String> {
    if workspace.document.has_modules() {
        let mut flat = workspace.clone();
        flat.document = std::sync::Arc::new(workspace.document.flatten()?);
        return run(root, &flat, engine);
    }
    let source = workspace.document.rtl()?;
    run_source(root, workspace, engine, &source)
}
fn run_source(
    root: &Path,
    workspace: &Workspace,
    engine: &str,
    source: &str,
) -> Result<Run, String> {
    let compiler = crate::executable(match engine {
        "icarus" => "iverilog",
        "verilator" => "verilator",
        _ => return Err("Unsupported RTL engine".into()),
    })
    .ok_or_else(|| format!("{engine} is not installed"))?;
    let runtime = if engine == "icarus" {
        Some(crate::executable("vvp").ok_or("Icarus vvp is not installed")?)
    } else {
        None
    };
    let (cycles, reference, _) = workspace.testbench.run(&workspace.document)?;
    let bench = bench(&workspace.document, &cycles)?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let dir = root.join(format!(
        ".logic/runs/stimuli-{engine}-{}-{}-{stamp}",
        workspace.revision, workspace.testbench.version
    ));
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let dir = dir.canonicalize().map_err(|e| e.to_string())?;
    let result = (|| -> Result<Run, String> {
        fs::write(dir.join("design.v"), source).map_err(|e| e.to_string())?;
        fs::write(dir.join("testbench.v"), bench).map_err(|e| e.to_string())?;
        fs::write(
            dir.join("stimuli.json"),
            serde_json::to_vec_pretty(&workspace.testbench).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        fs::write(
            dir.join("reference.json"),
            serde_json::to_vec(&reference).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        let deadline = Instant::now() + Duration::from_secs(30);
        let args: &[&str] = if engine == "icarus" {
            &[
                "-g2012",
                "-s",
                "testbench_las_stimuli",
                "-o",
                "simulation.vvp",
                "design.v",
                "testbench.v",
            ]
        } else {
            &[
                "--binary",
                "--timing",
                "--trace",
                "--top-module",
                "testbench_las_stimuli",
                "--Mdir",
                "obj_dir",
                "-o",
                "las_sim",
                "--build-jobs",
                "2",
                "-Wno-fatal",
                "design.v",
                "testbench.v",
            ]
        };
        if !execute(&compiler, args, &dir, "compile.log", deadline)? {
            return Err("RTL compilation failed".into());
        }
        let binary = runtime.unwrap_or_else(|| dir.join("obj_dir/las_sim"));
        let args: &[&str] = if engine == "icarus" {
            &["-n", "simulation.vvp"]
        } else {
            &[]
        };
        if !execute(&binary, args, &dir, "simulation.log", deadline)? {
            return Err("RTL simulation failed".into());
        }
        let actual = samples(&workspace.document, &cycles, &log(&dir, "simulation.log"))?;
        if serde_json::to_vec(&(&cycles, &actual))
            .map_err(|e| e.to_string())?
            .len()
            > 4 * 1024 * 1024
        {
            return Err("Trace exceeds 4 MiB".into());
        }
        let path = dir.join("simulation.vcd");
        if fs::metadata(&path).map_err(|e| e.to_string())?.len() > VCD_LIMIT {
            return Err("VCD exceeds 12 MiB".into());
        }
        let vcd = fs::read_to_string(path).map_err(|e| e.to_string())?;
        if !vcd.contains("$enddefinitions") || !vcd.contains("$var") || !vcd.contains("#") {
            return Err("RTL did not produce a valid VCD".into());
        }
        let (checks, mut comparison) = compare(workspace, &actual, &reference)?;
        comparison["engine"] = json!(engine);
        comparison["artifact_directory"] = json!(dir);
        comparison["success"] =
            json!(comparison["passed"] == comparison["total"] && checks.passed == checks.total);
        fs::write(dir.join("result.json"),serde_json::to_vec_pretty(&json!({"revision":workspace.revision,"testbench_version":workspace.testbench.version,"comparison":comparison,"checks":checks,"samples":actual})).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
        Ok(Run {
            cycles,
            samples: actual,
            checks,
            comparison,
            vcd,
        })
    })();
    result.map_err(|e| {
        let _ = fs::write(dir.join("error.txt"), &e);
        format!("{e} — {}", dir.display())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{path::PathBuf, sync::Arc};
    struct Root(PathBuf);
    impl Root {
        fn new(name: &str) -> Self {
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let p = std::env::temp_dir()
                .join(format!("las-stimuli-{name}-{}-{stamp}", std::process::id()));
            fs::create_dir_all(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for Root {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    #[ignore = "Requires installed Icarus, Verilator and C++ compiler"]
    fn nested_modules_preserve_signed_pipeline_in_both_simulators() {
        let mut w = workspace(true);
        let (first, inner) = w
            .document
            .group_module(&["sum".into(), "acc".into()], "Accumulator")
            .unwrap();
        let (nested, _) = first
            .group_module(&[inner, "delay".into()], "Pipeline")
            .unwrap();
        w.document = Arc::new(nested);
        let rows = &mut std::sync::Arc::make_mut(&mut w.testbench).rows;
        rows[1].expected.insert("current".into(), "lt 0".into());
        rows[2]
            .expected
            .insert("result".into(), "eq prev.out.current".into());
        rows[5]
            .expected
            .insert("current".into(), "eq in.inc".into());
        for engine in ["icarus", "verilator"] {
            let root = Root::new("nested-modules");
            let result = run(&root.0, &w, engine).unwrap();
            assert_eq!(result.checks.passed, result.checks.total);
            assert_eq!(result.comparison["passed"], result.comparison["total"]);
            assert!(result.vcd.contains("$enddefinitions"));
            let mut session = crate::simulation::Session::default();
            session.install_external(&w, result).unwrap();
            let state = session.execute(&w, "simulation.state", &json!({})).unwrap();
            assert!(state["session"]["annotation"]["values"]
                .as_object()
                .unwrap()
                .contains_key("current"));
        }
    }

    #[test]
    #[ignore = "Requires installed Icarus and Yosys"]
    fn exported_hierarchy_matches_signed_pipeline_and_synthesizes() {
        let w = workspace(true);
        let (first, inner) = w
            .document
            .group_module(&["sum".into(), "acc".into()], "Accumulator")
            .unwrap();
        let (nested, _) = first
            .group_module(&[inner, "delay".into()], "Pipeline")
            .unwrap();
        let source = nested.rtl().unwrap();
        assert_eq!(source.matches("module las_def_Accumulator (").count(), 1);
        assert_eq!(source.matches("module las_def_Pipeline (").count(), 1);
        assert!(source.contains("las_def_Accumulator inst_"));
        assert!(source.contains("las_def_Pipeline inst_"));
        let (cycles, samples, _) = w.testbench.run(&nested).unwrap();
        let mut bench = String::from(
            "module test_export;
             reg clk = 0, reset = 0;
             reg signed [7:0] inc;
             wire signed [7:0] current, result;
             las_SignedPipeline dut(.n_inc(inc), .n_current(current), .n_result(result),
                                    .las_clk(clk), .las_reset(reset));
             initial begin
",
        );
        for (index, (cycle, sample)) in cycles.iter().zip(&samples).enumerate() {
            bench += &format!(
                "inc = {}; reset = {}; #5; clk = 1; #1;
                 if (current !== {} || result !== {}) $fatal(1, \"cycle {}\");
                 #4; clk = 0;\n",
                cycle.inputs["inc"],
                u8::from(cycle.reset),
                sample.outputs["current"],
                sample.outputs["result"],
                index + 1
            );
        }
        bench += "$finish; end endmodule\n";
        let root = Root::new("hierarchical-export");
        fs::write(root.0.join("design.v"), &source).unwrap();
        fs::write(root.0.join("bench.v"), bench).unwrap();
        let deadline = Instant::now() + Duration::from_secs(30);
        for (tool, args) in [
            (
                "iverilog",
                &[
                    "-g2012",
                    "-s",
                    "test_export",
                    "-o",
                    "sim.vvp",
                    "design.v",
                    "bench.v",
                ][..],
            ),
            ("vvp", &["sim.vvp"][..]),
        ] {
            let binary = crate::executable(tool).unwrap();
            assert!(
                execute(&binary, args, &root.0, "export.log", deadline).unwrap(),
                "{tool}: {}",
                log(&root.0, "export.log"),
            );
        }
        let synthesis = crate::synthesis(&root.0, 0, &source, &nested.name).unwrap();
        assert_eq!(synthesis["success"], true, "{}", synthesis["log"]);
        let artifact = Path::new(synthesis["artifact_directory"].as_str().unwrap());
        let netlist: Value =
            serde_json::from_slice(&fs::read(artifact.join("netlist.json")).unwrap()).unwrap();
        assert_eq!(
            netlist["modules"]["las_SignedPipeline"]["attributes"]["top"]
                .as_str()
                .unwrap()
                .trim_start_matches('0'),
            "1"
        );
        assert!(netlist["modules"]["las_def_Accumulator"].is_object());
    }

    fn workspace(sequential: bool) -> Workspace {
        let project: Value = serde_json::from_str(if sequential {
            include_str!("../../../examples/signed-pipeline.json")
        } else {
            include_str!("../../../examples/signed-add.json")
        })
        .unwrap();
        let mut w = Workspace::default();
        w.document = Arc::new(serde_json::from_value(project["document"].clone()).unwrap());
        Arc::make_mut(&mut w.testbench).rows = design_core::testbench::Testbench::import_csv(
            &w.document,
            if sequential {
                include_str!("../../../examples/signed-pipeline-stimuli.csv")
            } else {
                include_str!("../../../examples/signed-stimuli.csv")
            },
        )
        .unwrap();
        w
    }
    #[test]
    #[ignore = "Requires installed Icarus, Verilator and C++ compiler"]
    fn gate_symbols_rtl_matches_independent_truth_table() {
        let root = Root::new("gates");
        let fixture: Value =
            serde_json::from_str(include_str!("../../../examples/logic-gates.json")).unwrap();
        let mut w = Workspace::default();
        w.document = Arc::new(serde_json::from_value(fixture["document"].clone()).unwrap());
        Arc::make_mut(&mut w.testbench).rows = [0u64, 1, 85, 255]
            .into_iter()
            .flat_map(|a| {
                [0u64, 1, 170, 255]
                    .into_iter()
                    .map(move |b| design_core::testbench::Stimulus {
                        reset: false,
                        inputs: BTreeMap::from([
                            ("a".into(), a.to_string()),
                            ("b".into(), b.to_string()),
                        ]),
                        expected: [
                            ("and", a & b),
                            ("or", a | b),
                            ("xor", a ^ b),
                            ("not", !a),
                            ("nand", !(a & b)),
                            ("nor", !(a | b)),
                            ("xnor", !(a ^ b)),
                        ]
                        .into_iter()
                        .map(|(k, v)| (format!("out_{k}"), (v & 255).to_string()))
                        .collect(),
                    })
            })
            .collect();
        for engine in ["icarus", "verilator"] {
            let result = run(&root.0, &w, engine).unwrap();
            assert_eq!(result.comparison["success"], true);
            assert_eq!(result.checks.passed, 112);
            assert_eq!(result.checks.total, 112);
        }
    }
    #[test]
    fn bench_and_parser_enforce_edges_limits_and_complete_known_values() {
        let w = workspace(true);
        let (cycles, reference, _) = w.testbench.run(&w.document).unwrap();
        let text = bench(&w.document, &cycles).unwrap();
        assert!(text.contains("#5; tb_clk=1; #1;"));
        assert!(text.contains("p0=8'h80;"));
        assert!(text.contains("$dumpvars(0,dut)"));
        let mut text = String::new();
        for (i, s) in reference.iter().enumerate() {
            text += &format!(
                "LAS_SAMPLE {i} {}\n",
                w.document
                    .nodes
                    .iter()
                    .map(|n| format!("{:x}", s.values[&n.id]))
                    .collect::<Vec<_>>()
                    .join(" ")
            );
        }
        text += "LAS_DONE\n";
        assert_eq!(
            samples(&w.document, &cycles, &text).unwrap()[1].outputs["current"],
            "-128"
        );
        for bad in [
            text.replace("LAS_DONE", ""),
            text.clone() + "LAS_DONE\n",
            text.replace("LAS_SAMPLE 1", "LAS_SAMPLE 2"),
            text.replacen("80", "xx", 1),
            text.replacen("80", "100", 1),
        ] {
            assert!(samples(&w.document, &cycles, &bad).is_err());
        }
        let mut actual = samples(&w.document, &cycles, &text).unwrap();
        actual[1].values.insert("sum".into(), 1);
        let (_, report) = compare(&w, &actual, &reference).unwrap();
        assert_eq!(report["passed"], report["total"].as_u64().unwrap() - 1);
        assert_eq!(report["failures"][0]["signal"], "sum");
    }
    #[test]
    #[ignore = "Requires installed Icarus, Verilator and C++ compiler"]
    fn saved_stimuli_match_both_engines_with_signed_pipeline_reset_and_vcd() {
        let root = Root::new("both");
        for sequential in [false, true] {
            let w = workspace(sequential);
            for engine in ["icarus", "verilator"] {
                let run = run(&root.0, &w, engine).unwrap();
                assert_eq!(run.comparison["success"], true, "{}", run.comparison);
                assert_eq!(run.checks.total, if sequential { 14 } else { 4 });
                assert_eq!(run.checks.passed, run.checks.total);
                assert!(run.vcd.contains("$enddefinitions"));
                assert!(
                    run.vcd.contains("#10000"),
                    "{engine}/{sequential}: {}",
                    run.vcd
                );
                if sequential {
                    assert_eq!(run.samples[1].outputs["current"], "-128");
                    assert_eq!(run.samples[2].outputs["result"], "-128");
                    assert_eq!(run.samples[4].registers["acc"], "0");
                    assert!(run.vcd.contains("las_clk"));
                    assert!(run.vcd.contains("las_reset"));
                    assert!(run.vcd.contains("#5000"));
                    assert!(run.vcd.contains("#15000"));
                }
                // The shared trace and export contain RTL observations, never a replacement Rust trace.
                let vcd = run.vcd.clone();
                let mut session = crate::simulation::Session::default();
                session.install_external(&w, run).unwrap();
                let state = session.execute(&w, "simulation.state", &json!({})).unwrap();
                assert_eq!(state["session"]["external"]["engine"], engine);
                let export = session
                    .execute(
                        &w,
                        "simulation.vcd",
                        &json!({"expected_revision":0,"trace_id":state["session"]["id"]}),
                    )
                    .unwrap();
                assert_eq!(export["source"], vcd);
            }
        }
        for width in [1u8, 64] {
            let mut w = workspace(true);
            for n in Arc::make_mut(&mut w.document).nodes.iter_mut() {
                n.width = width;
            }
            let min = -(1i128 << (width - 1));
            let modulus = 1i128 << width;
            let wrap = |v: i128| {
                let bits = v.rem_euclid(modulus);
                if bits >= -min {
                    bits - modulus
                } else {
                    bits
                }
            };
            let mut q = 0;
            let mut rows = vec![];
            for (reset, input) in [
                (true, -1),
                (false, -1),
                (false, -1),
                (true, 0),
                (false, min),
                (false, -1),
            ] {
                let delay = if reset { 0 } else { q };
                q = if reset { 0 } else { wrap(q + input) };
                rows.push(design_core::testbench::Stimulus {
                    reset,
                    inputs: BTreeMap::from([("inc".into(), input.to_string())]),
                    expected: BTreeMap::from([
                        ("current".into(), q.to_string()),
                        ("result".into(), delay.to_string()),
                    ]),
                });
            }
            Arc::make_mut(&mut w.testbench).rows = rows;
            for engine in ["icarus", "verilator"] {
                let result = run(&root.0, &w, engine).unwrap();
                assert_eq!(result.comparison["success"], true);
                assert_eq!(result.checks.passed, 12);
            }
        }
    }
    #[test]
    #[ignore = "Requires installed Icarus"]
    fn wrong_expectations_and_mutated_rtl_fail_independently() {
        let root = Root::new("failure");
        let mut w = workspace(false);
        Arc::make_mut(&mut w.testbench).rows[0]
            .expected
            .insert("result".into(), "0".into());
        let r = run(&root.0, &w, "icarus").unwrap();
        assert_eq!(r.comparison["passed"], r.comparison["total"]);
        assert_eq!(r.comparison["success"], false);
        assert_eq!(r.checks.passed, 3);
        let w = workspace(false);
        let rtl = w.document.rtl().unwrap().replace("n_a + n_b", "n_a - n_b");
        let r = run_source(&root.0, &w, "icarus", &rtl).unwrap();
        assert_eq!(r.samples[2].outputs["result"], "-9");
        assert_eq!(r.comparison["success"], false);
        assert!(!r.comparison["failures"].as_array().unwrap().is_empty());
        assert!(run_source(&root.0, &w, "icarus", "invalid verilog").is_err());
        let unknown = w
            .document
            .rtl()
            .unwrap()
            .replace("n_a + n_b", "8'bxxxxxxxx");
        assert!(run_source(&root.0, &w, "icarus", &unknown)
            .err()
            .unwrap()
            .contains("X/Z"));
    }
    #[test]
    #[ignore = "Requires installed Verilator and C++ compiler"]
    fn service_rejects_stale_run_without_overwriting_shared_trace() {
        let root = Root::new("stale");
        let w = workspace(true);
        fs::create_dir_all(root.0.join(".logic")).unwrap();
        fs::write(
            root.0.join(".logic/workspace.json"),
            serde_json::to_vec(&w).unwrap(),
        )
        .unwrap();
        let service = crate::Service::open(&root.0).unwrap();
        let req = |method: &str, args: Value| crate::Request {
            method: method.into(),
            args,
            actor: "test".into(),
        };
        let initial = service
            .execute(req(
                "simulation.testbench",
                json!({"expected_revision":0,"expected_version":0}),
            ))
            .unwrap();
        assert!(service
            .execute(req(
                "simulation.rtl_testbench",
                json!({"expected_revision":0,"expected_version":1,"engine":"verilator"})
            ))
            .is_err());
        let worker = service.clone();
        let task = std::thread::spawn(move || {
            worker.execute(crate::Request {
                method: "simulation.rtl_testbench".into(),
                args: json!({"expected_revision":0,"expected_version":0,"engine":"verilator"}),
                actor: "test".into(),
            })
        });
        let deadline = Instant::now() + Duration::from_secs(10);
        while !root.0.join(".logic/runs").exists() {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(10));
        }
        service
            .execute(req(
                "testbench.set",
                json!({"expected_revision":0,"expected_version":0,"rows":w.testbench.rows}),
            ))
            .unwrap();
        assert!(task
            .join()
            .unwrap()
            .unwrap_err()
            .contains("changed during RTL"));
        let state = service.execute(req("simulation.state", json!({}))).unwrap();
        assert_eq!(initial["session"]["id"], state["session"]["id"]);
        let result = service
            .execute(req(
                "simulation.rtl_testbench",
                json!({"expected_revision":0,"expected_version":1,"engine":"verilator"}),
            ))
            .unwrap();
        assert_eq!(result["session"]["testbench_version"], 1);
        assert_eq!(result["session"]["external"]["success"], true);
        let export = service
            .execute(req(
                "simulation.vcd",
                json!({"expected_revision":0,"trace_id":result["session"]["id"]}),
            ))
            .unwrap();
        assert!(export["source"].as_str().unwrap().contains("las_clk"));
    }
}
