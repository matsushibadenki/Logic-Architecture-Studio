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
}
impl Session {
    pub fn execute(
        &mut self,
        workspace: &Workspace,
        method: &str,
        args: &Value,
    ) -> Result<Value, String> {
        match method {
            "simulation.cycles" | "simulation.evaluate" => {
                if method == "simulation.cycles"
                    && args["expected_revision"].as_u64() != Some(workspace.revision)
                {
                    return Err("Revision conflict".into());
                }
                let (cycles, samples) = if method == "simulation.cycles" {
                    let cycles: Vec<CycleInput> = serde_json::from_value(args["cycles"].clone())
                        .map_err(|e| e.to_string())?;
                    let samples = workspace.document.simulate_cycles(&cycles)?;
                    (cycles, samples)
                } else {
                    let strings: BTreeMap<String, String> =
                        serde_json::from_value(args["inputs"].clone())
                            .map_err(|e| e.to_string())?;
                    let inputs = strings
                        .iter()
                        .map(|(k, v)| Ok((k.clone(), design_core::number(v)?)))
                        .collect::<Result<_, String>>()?;
                    let values = workspace.document.evaluate_signals(&inputs)?;
                    let outputs = workspace
                        .document
                        .nodes
                        .iter()
                        .filter(|n| n.kind == Kind::Output)
                        .map(|n| (n.name.clone(), values[&n.id].to_string()))
                        .collect();
                    (
                        vec![CycleInput {
                            reset: false,
                            inputs: strings,
                        }],
                        vec![CycleSample {
                            reset: false,
                            values,
                            outputs,
                            registers: BTreeMap::new(),
                        }],
                    )
                };
                let trace_bytes = serde_json::to_vec(&(&cycles, &samples))
                    .map_err(|e| e.to_string())?
                    .len();
                if trace_bytes > 4 * 1024 * 1024 {
                    return Err("Trace exceeds 4 MiB; use fewer cycles".into());
                }
                self.generation += 1;
                // Include the service instance so an obsolete client cannot select a replacement trace.
                let id = format!(
                    "{}-{}-{}",
                    std::process::id(),
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_nanos(),
                    self.generation
                );
                let radix = self.trace.as_ref().map_or(10, |t| t.radix);
                self.trace = Some(Trace {
                    id,
                    revision: workspace.revision,
                    document: workspace.document.clone(),
                    selected: samples.len() - 1,
                    samples,
                    cycles,
                    radix,
                });
                self.version += 1;
            }
            "simulation.select" => {
                let trace = self.trace.as_mut().ok_or("No simulation trace")?;
                if args["expected_revision"].as_u64() != Some(workspace.revision)
                    || trace.revision != workspace.revision
                {
                    return Err("Revision conflict; rerun simulation".into());
                }
                if args["trace_id"].as_str() != Some(&trace.id) {
                    return Err("Simulation trace replaced".into());
                }
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
                if args["expected_revision"].as_u64() != Some(workspace.revision)
                    || trace.revision != workspace.revision
                {
                    return Err("Revision conflict; rerun simulation".into());
                }
                if args["trace_id"].as_str() != Some(&trace.id) {
                    return Err("Simulation trace replaced".into());
                }
                let source = trace.vcd();
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
            let sample = &trace.samples[trace.selected];
            let format = |v: u64, width: u8| if trace.radix == 16 { format!("0x{:0width$X}", v, width = usize::from(width).div_ceil(4)) } else { v.to_string() };
            let values = trace.document.nodes.iter().map(|n| (n.id.clone(), format(sample.values[&n.id], n.width))).collect::<BTreeMap<_, _>>();
            let register_d = trace.document.edges.iter().filter(|e| trace.document.node(&e.to).is_ok_and(|n| n.kind == Kind::Register)).map(|e| (e.to.clone(), format(sample.values[&e.from], trace.document.node(&e.to).unwrap().width))).collect::<BTreeMap<_, _>>();
            json!({"id":trace.id,"revision":trace.revision,"cycles":trace.cycles,"samples":trace.samples,"selected":trace.selected,"radix":trace.radix,
                "signals": trace.document.nodes.iter().filter(|n| matches!(n.kind, Kind::Input | Kind::Output | Kind::Register)).map(|n| json!({"id":n.id,"name":n.name,"kind":n.kind,"width":n.width})).collect::<Vec<_>>(),
                "annotation": if trace.revision == workspace.revision { json!({"version":self.version,"revision":trace.revision,"values":values,"register_d":register_d}) } else { Value::Null }})
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
    fn vcd(&self) -> String {
        use std::fmt::Write;
        let mut out = String::from("$version Logic Architecture Studio $end\n$comment Rust unsigned two-state samples after each rising edge. 1 ns per sample is synthetic; no propagation or pre-reset state is modeled. $end\n$timescale 1 ns $end\n$scope module logic $end\n$var wire 1 rst reset $end\n");
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
            id: format!("e{i}"),
            from: from.into(),
            to: to.into(),
            port: port.into(),
        })
        .collect::<Vec<_>>();
        let mut workspace = Workspace::default();
        workspace.document = Arc::new(Document {
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
