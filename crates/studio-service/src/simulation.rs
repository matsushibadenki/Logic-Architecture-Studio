//! Ephemeral, shared simulation state. Windows are replicas, never trace owners.
use design_core::{CycleInput, CycleSample, Document, Kind, Workspace};
use serde_json::{json, Value};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Default)]
pub(crate) struct Session {
    generation: u64,
    version: u64,
    trace: Option<Trace>,
}
struct Trace {
    id: String,
    revision: u64,
    document: Arc<Document>,
    cycles: Vec<CycleInput>,
    samples: Vec<CycleSample>,
    selected: usize,
    radix: u32,
    testbench_version: Option<u64>,
    checks: Option<design_core::testbench::CheckReport>,
    external: Option<Value>,
    external_vcd: Option<String>,
}
fn simulation_document(workspace: &Workspace) -> Result<Arc<Document>, String> {
    if workspace.document.has_modules() {
        Ok(Arc::new(workspace.document.flatten()?))
    } else {
        Ok(workspace.document.clone())
    }
}

impl Session {
    fn next_trace_id(&mut self) -> String {
        self.generation += 1;
        format!(
            "{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            self.generation
        )
    }

    pub(crate) fn version(&self) -> u64 {
        self.version
    }
    pub(crate) fn install_external(
        &mut self,
        workspace: &Workspace,
        run: crate::rtl_stimuli::Run,
    ) -> Result<(), String> {
        let document = simulation_document(workspace)?;
        let id = self.next_trace_id();
        self.version += 1;
        let radix = self.trace.as_ref().map_or(10, |t| t.radix);
        self.trace = Some(Trace {
            id,
            revision: workspace.revision,
            document,
            selected: run.samples.len() - 1,
            samples: run.samples,
            cycles: run.cycles,
            radix,
            testbench_version: Some(workspace.testbench.version),
            checks: Some(run.checks),
            external: Some(run.comparison),
            external_vcd: Some(run.vcd),
        });
        Ok(())
    }

    pub fn execute(
        &mut self,
        workspace: &Workspace,
        method: &str,
        args: &Value,
    ) -> Result<Value, String> {
        match method {
            "simulation.cycles" | "simulation.evaluate" | "simulation.testbench" => {
                if method != "simulation.evaluate"
                    && args["expected_revision"].as_u64() != Some(workspace.revision)
                {
                    return Err("Revision conflict".into());
                }
                let document = simulation_document(workspace)?;
                let mut checks = None;
                let mut testbench_version = None;
                let (cycles, samples) = if method == "simulation.testbench" {
                    if args["expected_version"].as_u64() != Some(workspace.testbench.version) {
                        return Err("Stimulus table changed; reload before running".into());
                    }
                    let (cycles, samples, report) = workspace.testbench.run(&document)?;
                    checks = Some(report);
                    testbench_version = Some(workspace.testbench.version);
                    (cycles, samples)
                } else if method == "simulation.cycles" {
                    let cycles: Vec<CycleInput> = serde_json::from_value(args["cycles"].clone())
                        .map_err(|e| e.to_string())?;
                    let samples = document.simulate_cycles(&cycles)?;
                    (cycles, samples)
                } else {
                    let cycle = CycleInput {
                        reset: false,
                        inputs: serde_json::from_value(args["inputs"].clone())
                            .map_err(|e| e.to_string())?,
                    };
                    let sample = document.combinational_sample(&cycle)?;
                    (vec![cycle], vec![sample])
                };
                let trace_bytes = serde_json::to_vec(&(&cycles, &samples))
                    .map_err(|e| e.to_string())?
                    .len();
                if trace_bytes > 4 * 1024 * 1024 {
                    return Err("Trace exceeds 4 MiB; use fewer cycles".into());
                }
                let id = self.next_trace_id();
                let radix = self.trace.as_ref().map_or(10, |t| t.radix);
                self.trace = Some(Trace {
                    id,
                    revision: workspace.revision,
                    document,
                    selected: samples.len() - 1,
                    samples,
                    cycles,
                    radix,
                    checks,
                    testbench_version,
                    external: None,
                    external_vcd: None,
                });
                self.version += 1;
            }
            "simulation.select" => {
                let trace = self.trace.as_mut().ok_or("No simulation trace")?;
                trace.check_request(workspace.revision, args)?;
                let index = args["index"].as_u64().ok_or("Missing sample index")? as usize;
                let radix = args["radix"].as_u64().ok_or("Missing radix")? as u32;
                if index >= trace.samples.len() || ![10, 16].contains(&radix) {
                    return Err("Invalid sample or radix".into());
                }
                trace.selected = index;
                trace.radix = radix;
                self.version += 1;
            }
            "simulation.vcd" => {
                let trace = self.trace.as_ref().ok_or("No simulation trace")?;
                trace.check_request(workspace.revision, args)?;
                let source = trace.external_vcd.clone().unwrap_or_else(|| trace.vcd());
                if source.len() > 12 * 1024 * 1024 {
                    return Err("VCD exceeds 12 MiB; use fewer cycles".into());
                }
                return Ok(
                    json!({"revision":trace.revision,"source":source,"filename":"simulation.vcd"}),
                );
            }
            "simulation.state" => {}
            _ => return Err("Unknown simulation method".into()),
        }
        let cursor = format!(
            "{}:{}:{}",
            self.trace.as_ref().map_or("none", |t| t.id.as_str()),
            self.version,
            workspace.revision
        );
        if method == "simulation.state" && args["cursor"].as_str() == Some(&cursor) {
            return Ok(json!({"changed":false,"cursor":cursor}));
        }
        let session = self.trace.as_ref().map(|trace| {
            json!({"id":trace.id,"revision":trace.revision,"cycles":trace.cycles,"samples":trace.samples,"selected":trace.selected,"radix":trace.radix,"checks":trace.checks,"external":trace.external,"testbench_version":trace.testbench_version,
                "signals": trace.document.nodes.iter().filter(|n| matches!(n.kind, Kind::Input | Kind::Output | Kind::Register)).map(|n| json!({"id":n.id,"name":n.name,"kind":n.kind,"width":n.width,"signed":n.signed})).collect::<Vec<_>>(),
                "annotation": if trace.revision == workspace.revision { trace.annotation(self.version) } else { Value::Null }})
        });
        // Keep the existing GUI/MCP evaluation response fields compatible.
        let trace = self.trace.as_ref();
        Ok(
            json!({"changed":true,"cursor":cursor,"session":session,"revision":trace.map(|t| t.revision),
            "samples": if method == "simulation.cycles" { trace.map(|t| &t.samples) } else { None },
            "outputs": if method == "simulation.evaluate" { trace.map(|t| &t.samples[0].outputs) } else { None }}),
        )
    }
}

impl Trace {
    fn annotation(&self, version: u64) -> Value {
        let sample = &self.samples[self.selected];
        let format = |node: &design_core::Node, bits| {
            if self.radix == 16 {
                format!(
                    "0x{bits:0width$X}",
                    width = usize::from(node.width).div_ceil(4)
                )
            } else {
                node.decimal(bits)
            }
        };
        let values: BTreeMap<_, _> = self
            .document
            .nodes
            .iter()
            .map(|node| (&node.id, format(node, sample.values[&node.id])))
            .collect();
        let mut register_d = BTreeMap::new();
        for edge in &self.document.edges {
            let target = self
                .document
                .nodes
                .by_id(&edge.to)
                .expect("validated trace target");
            if target.kind == Kind::Register {
                register_d.insert(&edge.to, format(target, sample.values[&edge.from]));
            }
        }
        json!({"version":version,"revision":self.revision,"values":values,"register_d":register_d})
    }

    fn check_request(&self, revision: u64, args: &Value) -> Result<(), String> {
        if args["expected_revision"].as_u64() != Some(revision) || self.revision != revision {
            return Err("Revision conflict; rerun simulation".into());
        }
        if args["trace_id"].as_str() != Some(&self.id) {
            return Err("Simulation trace replaced".into());
        }
        Ok(())
    }

    fn vcd(&self) -> String {
        use std::fmt::Write;
        let mut out = String::from("$version Logic Architecture Studio $end\n$comment Rust two-state bit-vector samples after each rising edge. 1 ns per sample is synthetic; no propagation or pre-reset state is modeled. $end\n$timescale 1 ns $end\n$scope module logic $end\n$var wire 1 rst reset $end\n");
        for (index, node) in self.document.nodes.iter().enumerate() {
            writeln!(
                out,
                "$var wire {} s{} n_{} [{}:0] $end",
                node.width,
                index,
                node.name,
                node.width - 1
            )
            .unwrap();
        }
        out.push_str("$upscope $end\n$enddefinitions $end\n");
        for (cycle, sample) in self.samples.iter().enumerate() {
            writeln!(out, "#{cycle}").unwrap();
            if cycle == 0 {
                out.push_str("$dumpvars\n");
            }
            writeln!(out, "{}rst", u8::from(sample.reset)).unwrap();
            for (index, node) in self.document.nodes.iter().enumerate() {
                let value = sample.values[&node.id];
                if cycle == 0 || self.samples[cycle - 1].values[&node.id] != value {
                    writeln!(
                        out,
                        "b{:0width$b} s{}",
                        value,
                        index,
                        width = usize::from(node.width)
                    )
                    .unwrap();
                }
            }
            if cycle == 0 {
                out.push_str("$end\n");
            }
        }
        writeln!(out, "#{}", self.samples.len()).unwrap();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn accumulator() -> Workspace {
        let nodes = [
            ("inc", Kind::Input),
            ("sum", Kind::Add),
            ("acc", Kind::Register),
            ("result", Kind::Output),
        ]
        .into_iter()
        .map(|(id, kind)| design_core::Node {
            signed: false,
            config: Default::default(),
            id: id.into(),
            name: id.into(),
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
            ("acc", "result", "a"),
        ]
        .into_iter()
        .enumerate()
        .map(|(i, (from, to, port))| design_core::Edge {
            source_port: "y".into(),
            id: format!("e{i}"),
            from: from.into(),
            to: to.into(),
            port: port.into(),
        })
        .collect::<Vec<_>>();
        let mut workspace = Workspace::default();
        workspace.document = Arc::new(Document {
            modules: Default::default(),
            name: "accumulator".into(),
            nodes: nodes.into(),
            edges: edges.into(),
        });
        workspace
    }
    fn run(session: &mut Session, workspace: &Workspace) -> Value {
        session.execute(workspace, "simulation.cycles", &json!({"expected_revision":0,"cycles":[{"reset":true,"inputs":{"inc":"3"}},{"reset":false,"inputs":{"inc":"3"}},{"reset":false,"inputs":{"inc":"3"}},{"reset":false,"inputs":{"inc":"3"}},{"reset":true,"inputs":{"inc":"7"}}]})).unwrap()
    }
    #[test]
    fn signed_feedback_trace_formats_d_q_results_and_negative_expectations() {
        let mut w = accumulator();
        w.document = Arc::new(w.document.set_signed("acc", true).unwrap());
        assert!(w.document.nodes.iter().all(|n| n.signed));
        assert!(w.document.rtl().unwrap().contains("reg signed [2:0] n_acc"));
        Arc::make_mut(&mut w.testbench).rows = design_core::testbench::Testbench::import_csv(
            &w.document,
            "cycle,reset,in.inc,expect.result\n1,1,-3,0\n2,0,-3,-3\n3,0,-3,2\n4,1,-3,0\n",
        )
        .unwrap();
        let mut session = Session::default();
        let result = session
            .execute(
                &w,
                "simulation.testbench",
                &json!({"expected_revision":0,"expected_version":0}),
            )
            .unwrap();
        assert_eq!(result["session"]["checks"]["passed"], 4);
        let selected=session.execute(&w,"simulation.select",&json!({"expected_revision":0,"trace_id":result["session"]["id"],"index":1,"radix":10})).unwrap();
        assert_eq!(selected["session"]["annotation"]["values"]["acc"], "-3");
        assert_eq!(selected["session"]["annotation"]["register_d"]["acc"], "2");
        assert_eq!(selected["session"]["samples"][1]["registers"]["acc"], "-3");
    }

    #[test]
    fn typed_annotations_and_results_are_signed_while_vcd_retains_bits() {
        let project: Value =
            serde_json::from_str(include_str!("../../../examples/extension-nodes.json")).unwrap();
        let mut workspace = Workspace::default();
        workspace.document = Arc::new(serde_json::from_value(project["document"].clone()).unwrap());
        workspace.document = Arc::new(workspace.document.set_signed("signed_bits", true).unwrap());
        let mut session = Session::default();
        let result = session
            .execute(
                &workspace,
                "simulation.evaluate",
                &json!({"inputs":{"data":"8"}}),
            )
            .unwrap();
        assert_eq!(result["outputs"]["sign_out"], "-8");
        assert_eq!(
            result["session"]["annotation"]["values"]["signed_bits"],
            "-8"
        );
        assert_eq!(result["session"]["annotation"]["values"]["zero"], "8");
        assert_eq!(result["session"]["annotation"]["values"]["sign_out"], "-8");
        let id = result["session"]["id"].clone();
        let hex = session
            .execute(
                &workspace,
                "simulation.select",
                &json!({"expected_revision":0,"trace_id":id,"index":0,"radix":16}),
            )
            .unwrap();
        assert_eq!(
            hex["session"]["annotation"]["values"]["signed_bits"],
            "0xF8"
        );
        let vcd = session
            .execute(
                &workspace,
                "simulation.vcd",
                &json!({"expected_revision":0,"trace_id":id}),
            )
            .unwrap();
        assert!(vcd["source"].as_str().unwrap().contains("b11111000"));
        assert!(!vcd["source"].as_str().unwrap().contains("b-"));
        for n in Arc::make_mut(&mut workspace.document).nodes.iter_mut() {
            n.width = 64;
            match &mut n.config {
                design_core::NodeConfig::ZeroExtend { input_width }
                | design_core::NodeConfig::SignExtend { input_width } => *input_width = 64,
                _ => {}
            }
        }
        let mut session = Session::default();
        let result = session
            .execute(
                &workspace,
                "simulation.evaluate",
                &json!({"inputs":{"data":"0x8000000000000000"}}),
            )
            .unwrap();
        assert_eq!(
            result["session"]["annotation"]["values"]["signed_bits"],
            "-9223372036854775808"
        );
    }

    #[test]
    fn shared_selection_uses_post_edge_values_and_rejects_stale_results() {
        let mut workspace = accumulator();
        let mut session = Session::default();
        let first = run(&mut session, &workspace);
        let selection =
            json!({"expected_revision":0,"trace_id":first["session"]["id"],"index":2,"radix":16});
        let result = session
            .execute(&workspace, "simulation.select", &selection)
            .unwrap();
        assert_eq!(result["session"]["annotation"]["values"]["acc"], "0x6");
        assert_eq!(result["session"]["annotation"]["values"]["sum"], "0x1");
        assert_eq!(result["session"]["annotation"]["register_d"]["acc"], "0x1");
        assert!(result["session"]["samples"][0].get("values").is_none());
        assert_eq!(
            session
                .execute(
                    &workspace,
                    "simulation.state",
                    &json!({"cursor":result["cursor"]})
                )
                .unwrap()["changed"],
            false
        );
        // A second window reads the same selection without owning or replaying cycles.
        assert_eq!(
            session
                .execute(&workspace, "simulation.state", &json!({}))
                .unwrap()["session"]["selected"],
            2
        );
        workspace.revision += 1;
        let stale = session
            .execute(
                &workspace,
                "simulation.state",
                &json!({"cursor":result["cursor"]}),
            )
            .unwrap();
        assert_eq!(stale["changed"], true);
        assert!(stale["session"]["annotation"].is_null());
        assert!(session
            .execute(&workspace, "simulation.select", &selection)
            .is_err());
        assert!(session
            .execute(
                &workspace,
                "simulation.vcd",
                &json!({"expected_revision":1,"trace_id":first["session"]["id"]})
            )
            .is_err());
    }
    #[test]
    fn invalid_inputs_keep_previous_trace_and_replacement_guards_selection() {
        let workspace = accumulator();
        let mut session = Session::default();
        let first = run(&mut session, &workspace);
        assert!(session
            .execute(
                &workspace,
                "simulation.cycles",
                &json!({"expected_revision":0,"cycles":[{"reset":true,"inputs":{"inc":"8"}}]})
            )
            .unwrap_err()
            .contains("unsigned 3-bit"));
        assert_eq!(
            session
                .execute(
                    &workspace,
                    "simulation.state",
                    &json!({"cursor":first["cursor"]})
                )
                .unwrap()["changed"],
            false
        );
        run(&mut session, &workspace);
        assert!(session.execute(&workspace, "simulation.select", &json!({"expected_revision":0,"trace_id":first["session"]["id"],"index":0,"radix":10})).is_err());
    }
    #[test]
    fn vcd_contains_all_nodes_wrap_reset_and_exact_64_bit_values() {
        let workspace = accumulator();
        let mut session = Session::default();
        let first = run(&mut session, &workspace);
        let exported = session
            .execute(
                &workspace,
                "simulation.vcd",
                &json!({"expected_revision":0,"trace_id":first["session"]["id"]}),
            )
            .unwrap();
        let source = exported["source"].as_str().unwrap();
        assert!(source.contains("$var wire 3 s2 n_acc [2:0] $end"));
        assert!(source.contains("#3\n0rst\nb100 s1\nb001 s2\nb001 s3"));
        assert!(source.contains("#4\n1rst\nb111 s0\nb111 s1\nb000 s2\nb000 s3"));
        assert!(source.ends_with("#5\n"));
        let mut workspace = Workspace::default();
        for n in &mut Arc::make_mut(&mut workspace.document).nodes {
            if n.width == 32 {
                n.width = 64;
            }
        }
        let result = session
            .execute(
                &workspace,
                "simulation.evaluate",
                &json!({"inputs":{"a":"18446744073709551615","b":"0","op":"0"}}),
            )
            .unwrap();
        assert_eq!(
            result["session"]["annotation"]["values"]["result"],
            "18446744073709551615"
        );
        assert!(session
            .trace
            .as_ref()
            .unwrap()
            .vcd()
            .contains(&format!("b{} ", "1".repeat(64))));
    }
}
