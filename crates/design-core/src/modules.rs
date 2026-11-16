use crate::*;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
impl ModuleDefinition {
    pub fn ports(&self) -> (Vec<ModulePort>, Vec<ModulePort>) {
        let ports = |kind| {
            self.nodes
                .iter()
                .filter(|n| n.kind == kind)
                .map(|n| ModulePort {
                    name: n.name.clone(),
                    width: n.width,
                    signed: n.signed,
                })
                .collect()
        };
        (ports(Kind::Input), ports(Kind::Output))
    }
}
fn unique(prefix: &str, used: &mut BTreeSet<String>) -> String {
    for i in 0.. {
        let s = format!("{prefix}{i}");
        if used.insert(s.clone()) {
            return s;
        }
    }
    unreachable!()
}
fn scoped(path: &str, id: &str) -> String {
    let digest = Sha256::digest(format!("{path}/{id}").as_bytes());
    format!("m_{:x}", digest)[..42].to_string()
}
impl Document {
    pub fn definition_graph(&self, name: &str) -> Result<Self, String> {
        let definition = self.modules.get(name).ok_or("Unknown module definition")?;
        Ok(Self {
            name: name.into(),
            nodes: definition.nodes.clone(),
            edges: definition.edges.clone(),
            modules: self.modules.clone(),
        })
    }

    pub fn has_modules(&self) -> bool {
        self.nodes.iter().any(|n| n.kind == Kind::Module)
    }
    /// Elaborate references without changing the saved hierarchy. Boundary buffers
    /// preserve signed widths and let the existing sequential engine see registers
    /// inside arbitrary nested instances, including feedback across boundaries.
    pub fn flatten(&self) -> Result<Document, String> {
        struct Builder<'a> {
            library: &'a BTreeMap<String, Arc<ModuleDefinition>>,
            nodes: Vec<Node>,
            edges: Vec<Edge>,
            stack: Vec<String>,
        }
        type Ports = BTreeMap<(String, String), String>;
        impl Builder<'_> {
            fn expand(
                &mut self,
                nodes: &entities::Entities<Node>,
                edges: &entities::Entities<Edge>,
                path: &str,
            ) -> Result<(Ports, Ports), String> {
                if self.stack.len() > 16 {
                    return Err("Module nesting exceeds 16 levels".into());
                }
                let mut seen_ids = BTreeSet::new();
                let mut seen_names = BTreeSet::new();
                for n in nodes {
                    if !valid_identifier(&n.id)
                        || !valid_identifier(&n.name)
                        || !seen_ids.insert(&n.id)
                        || !seen_names.insert(&n.name)
                    {
                        return Err("Invalid or duplicate module node identifier".into());
                    }
                }
                let mut ins = Ports::new();
                let mut outs = Ports::new();
                for n in nodes {
                    if let NodeConfig::Module {
                        definition,
                        inputs,
                        outputs,
                    } = &n.config
                    {
                        if n.kind != Kind::Module {
                            return Err("Module configuration on primitive node".into());
                        }
                        let def = self
                            .library
                            .get(definition)
                            .ok_or("Unknown module definition")?;
                        if self.stack.contains(definition) {
                            return Err("Recursive module reference".into());
                        }
                        if (inputs.clone(), outputs.clone()) != def.ports() {
                            return Err("Module interface mismatch".into());
                        }
                        self.stack.push(definition.clone());
                        let subpath = format!("{path}/{}", n.id);
                        let (subin, subout) = self.expand(&def.nodes, &def.edges, &subpath)?;
                        self.stack.pop();
                        for p in def.nodes.iter().filter(|p| p.kind == Kind::Input) {
                            ins.insert(
                                (n.id.clone(), p.name.clone()),
                                subin[&(p.id.clone(), "a".into())].clone(),
                            );
                        }
                        for p in def.nodes.iter().filter(|p| p.kind == Kind::Output) {
                            outs.insert(
                                (n.id.clone(), p.name.clone()),
                                subout[&(p.id.clone(), "y".into())].clone(),
                            );
                        }
                    } else {
                        if n.kind == Kind::Module {
                            return Err("Missing module configuration".into());
                        }
                        let mut flat = n.clone();
                        if !path.is_empty() {
                            flat.id = scoped(path, &n.id);
                            let readable = format!(
                                "{}_{}",
                                path.trim_start_matches('/').replace('/', "_"),
                                n.name
                            );
                            flat.name = format!(
                                "{}__{}",
                                &readable[..readable.len().min(20)],
                                &flat.id[2..]
                            );
                            // Definition IO is an internal wire; only root IO remains external.
                            if matches!(flat.kind, Kind::Input | Kind::Output) {
                                flat.kind = Kind::ZeroExtend;
                                flat.config = NodeConfig::ZeroExtend {
                                    input_width: flat.width,
                                };
                            }
                        }
                        for (port, _) in flat.inputs() {
                            ins.insert((n.id.clone(), port.into()), flat.id.clone());
                        }
                        // Boundary outputs become drivers inside an instance.
                        for (port, _, _) in flat.outputs() {
                            outs.insert((n.id.clone(), port.into()), flat.id.clone());
                        }
                        self.nodes.push(flat);
                        if self.nodes.len() > 2000 {
                            return Err("Expanded design exceeds 2000 nodes".into());
                        }
                    }
                }
                for e in edges {
                    let source = nodes.by_id(&e.from).ok_or("Unknown module source")?;
                    let target = nodes.by_id(&e.to).ok_or("Unknown module target")?;
                    let (width, signed) = source.output(&e.source_port)?;
                    if !target
                        .inputs()
                        .iter()
                        .any(|p| p.0 == e.port && p.1 == width)
                        || target.input_signed(&e.port).is_some_and(|s| s != signed)
                    {
                        return Err("Module connection width/signedness mismatch".into());
                    }
                    let from = outs
                        .get(&(e.from.clone(), e.source_port.clone()))
                        .ok_or("Unknown module source port")?;
                    let to = ins
                        .get(&(e.to.clone(), e.port.clone()))
                        .ok_or("Unknown module destination port")?;
                    let target = nodes.by_id(&e.to).ok_or("Unknown destination")?;
                    self.edges.push(Edge {
                        id: if path.is_empty() {
                            e.id.clone()
                        } else {
                            scoped(path, &e.id)
                        },
                        from: from.clone(),
                        source_port: "y".into(),
                        to: to.clone(),
                        port: if target.kind == Kind::Module {
                            "a".into()
                        } else {
                            e.port.clone()
                        },
                    });
                    if self.edges.len() > 8000 {
                        return Err("Expanded design exceeds 8000 edges".into());
                    }
                }
                Ok((ins, outs))
            }
        }
        let mut b = Builder {
            library: &self.modules,
            nodes: vec![],
            edges: vec![],
            stack: vec![],
        };
        b.expand(&self.nodes, &self.edges, "")?;
        Ok(Document {
            name: self.name.clone(),
            nodes: b.nodes.into(),
            edges: b.edges.into(),
            modules: BTreeMap::new(),
        })
    }
    // Called only after full graph validation, which rejects recursive references.
    pub(crate) fn emit_rtl_definitions(
        &self,
        nodes: &entities::Entities<Node>,
        clocked: &mut BTreeMap<String, bool>,
        rtl: &mut String,
    ) -> Result<bool, String> {
        let mut sequential = false;
        for node in nodes {
            if let NodeConfig::Module { definition, .. } = &node.config {
                if !clocked.contains_key(definition) {
                    let def = &self.modules[definition];
                    let child_clocked = self.emit_rtl_definitions(&def.nodes, clocked, rtl)?;
                    let graph = Document {
                        name: definition.clone(),
                        nodes: def.nodes.clone(),
                        edges: def.edges.clone(),
                        modules: BTreeMap::new(),
                    };
                    *rtl += &graph.rtl_module(&format!("las_def_{definition}"), clocked)?;
                    rtl.push('\n');
                    clocked.insert(definition.clone(), child_clocked);
                }
                sequential |= clocked[definition];
            } else {
                sequential |= node.kind == Kind::Register;
            }
        }
        Ok(sequential)
    }
    pub fn module_node(
        &self,
        definition: &str,
        id: String,
        x: f32,
        y: f32,
    ) -> Result<Node, String> {
        let def = self
            .modules
            .get(definition)
            .ok_or("Unknown module definition")?;
        let (inputs, outputs) = def.ports();
        if inputs.len() > 16 || outputs.is_empty() || outputs.len() > 16 {
            return Err("Modules need 1–16 outputs and at most 16 inputs".into());
        }
        Ok(Node {
            name: id.clone(),
            id,
            kind: Kind::Module,
            width: outputs.iter().map(|p| p.width).max().unwrap(),
            signed: false,
            config: NodeConfig::Module {
                definition: definition.into(),
                inputs,
                outputs,
            },
            x,
            y,
        })
    }
    pub fn group_module(&self, ids: &[String], name: &str) -> Result<(Self, String), String> {
        if !valid_identifier(name) || self.modules.contains_key(name) || self.modules.len() >= 64 {
            return Err("Choose a unique HDL module name (up to 64 definitions)".into());
        }
        let mut selected = BTreeSet::new();
        for id in ids {
            let n = self.node(id)?;
            // Preserve external signal names/test vectors when selecting the entire diagram.
            if !matches!(n.kind, Kind::Input | Kind::Output) {
                selected.insert(id.clone());
            }
        }
        if selected.is_empty() || selected.len() > 512 {
            return Err("Select 1–512 processing nodes; top-level IO stays outside".into());
        }
        let mut members: Vec<_> = self
            .nodes
            .iter()
            .filter(|n| selected.contains(&n.id))
            .cloned()
            .collect();
        let x = members.iter().map(|n| n.x).fold(f32::INFINITY, f32::min);
        let y = members.iter().map(|n| n.y).fold(f32::INFINITY, f32::min);
        for n in &mut members {
            n.x -= x - 360.;
            n.y -= y;
        }
        let mut internal: Vec<_> = self
            .edges
            .iter()
            .filter(|e| selected.contains(&e.from) && selected.contains(&e.to))
            .cloned()
            .collect();
        let mut used: BTreeSet<_> = members
            .iter()
            .flat_map(|n| [n.id.clone(), n.name.clone()])
            .collect();
        let mut edge_ids: BTreeSet<_> = internal.iter().map(|e| e.id.clone()).collect();
        type Endpoint = (String, String);
        type InputBoundary = (String, Option<Endpoint>);
        let mut inputs: BTreeMap<Endpoint, InputBoundary> = BTreeMap::new();
        for n in self.nodes.iter().filter(|n| selected.contains(&n.id)) {
            for (port, width) in n.inputs() {
                let driver = self.edges.iter().find(|e| e.to == n.id && e.port == port);
                if driver.is_some_and(|e| selected.contains(&e.from)) {
                    continue;
                }
                let key = driver
                    .map(|e| (e.from.clone(), e.source_port.clone()))
                    .unwrap_or((n.id.clone(), port.into()));
                let boundary = if let Some((id, _)) = inputs.get(&key) {
                    id.clone()
                } else {
                    let id = unique("in", &mut used);
                    let signed = driver
                        .map(|e| {
                            self.node(&e.from)
                                .unwrap()
                                .output(&e.source_port)
                                .unwrap()
                                .1
                        })
                        .unwrap_or(n.input_signed(port).unwrap_or(false));
                    members.push(Node {
                        id: id.clone(),
                        name: id.clone(),
                        kind: Kind::Input,
                        width,
                        signed,
                        config: NodeConfig::None,
                        x: 0.,
                        y: inputs.len() as f32 * 320.,
                    });
                    inputs.insert(
                        key,
                        (
                            id.clone(),
                            driver.map(|e| (e.from.clone(), e.source_port.clone())),
                        ),
                    );
                    id
                };
                internal.push(Edge {
                    id: unique("edge", &mut edge_ids),
                    from: boundary,
                    source_port: "y".into(),
                    to: n.id.clone(),
                    port: port.into(),
                });
            }
        }
        let mut outputs = BTreeMap::new();
        for n in self.nodes.iter().filter(|n| selected.contains(&n.id)) {
            for (port, width, signed) in n.outputs() {
                let outgoing: Vec<_> = self
                    .edges
                    .iter()
                    .filter(|e| e.from == n.id && e.source_port == port)
                    .collect();
                if !outgoing.is_empty() && outgoing.iter().all(|e| selected.contains(&e.to)) {
                    continue;
                }
                let id = unique("out", &mut used);
                members.push(Node {
                    id: id.clone(),
                    name: id.clone(),
                    kind: Kind::Output,
                    width,
                    signed,
                    config: NodeConfig::None,
                    x: 1080.,
                    y: outputs.len() as f32 * 320.,
                });
                internal.push(Edge {
                    id: unique("edge", &mut edge_ids),
                    from: n.id.clone(),
                    source_port: port.into(),
                    to: id.clone(),
                    port: "a".into(),
                });
                outputs.insert((n.id.clone(), port.to_string()), id);
            }
        }
        if outputs.is_empty() {
            return Err("Selected module has no external output".into());
        }
        let mut next = self.clone();
        next.modules.insert(
            name.into(),
            Arc::new(ModuleDefinition {
                nodes: members.into(),
                edges: internal.into(),
            }),
        );
        let mut ids: BTreeSet<_> = self
            .nodes
            .iter()
            .flat_map(|n| [n.id.clone(), n.name.clone()])
            .collect();
        let instance = unique("mod", &mut ids);
        let module = next.module_node(name, instance.clone(), x, y)?;
        next.nodes.retain(|n| !selected.contains(&n.id));
        next.nodes.push(module);
        let mut edges: Vec<_> = self
            .edges
            .iter()
            .filter(|e| !selected.contains(&e.to))
            .cloned()
            .collect();
        for e in &mut edges {
            if selected.contains(&e.from) {
                e.source_port = outputs[&(e.from.clone(), e.source_port.clone())].clone();
                e.from = instance.clone();
            }
        }
        let mut ids: BTreeSet<_> = edges.iter().map(|e| e.id.clone()).collect();
        for (_, (port, driver)) in inputs {
            if let Some((from, source_port)) = driver {
                edges.push(Edge {
                    id: unique("edge", &mut ids),
                    from,
                    source_port,
                    to: instance.clone(),
                    port,
                });
            }
        }
        next.edges = edges.into();
        next.validate(false)?;
        Ok((next, instance))
    }
    pub fn instantiate_module(&self, name: &str, x: f32, y: f32) -> Result<(Self, String), String> {
        let mut used = self
            .nodes
            .iter()
            .flat_map(|n| [n.id.clone(), n.name.clone()])
            .collect();
        let id = unique("mod", &mut used);
        let mut next = self.clone();
        next.nodes.push(self.module_node(name, id.clone(), x, y)?);
        next.validate(false)?;
        Ok((next, id))
    }
}
impl Document {
    pub fn explode_module(&self, id: &str) -> Result<(Self, Vec<String>), String> {
        let instance = self.node(id)?;
        let NodeConfig::Module { definition, .. } = &instance.config else {
            return Err("Select a module instance".into());
        };
        let def = self
            .modules
            .get(definition)
            .ok_or("Unknown module definition")?;
        let mut next = self.clone();
        let mut used: BTreeSet<_> = self
            .nodes
            .iter()
            .flat_map(|n| [n.id.clone(), n.name.clone()])
            .collect();
        let mut mapping = BTreeMap::new();
        let mut added = Vec::new();
        let min_x = def
            .nodes
            .iter()
            .filter(|n| !matches!(n.kind, Kind::Input | Kind::Output))
            .map(|n| n.x)
            .fold(f32::INFINITY, f32::min);
        let min_y = def
            .nodes
            .iter()
            .filter(|n| !matches!(n.kind, Kind::Input | Kind::Output))
            .map(|n| n.y)
            .fold(f32::INFINITY, f32::min);
        for n in def
            .nodes
            .iter()
            .filter(|n| !matches!(n.kind, Kind::Input | Kind::Output))
        {
            let mut copy = n.clone();
            let key = unique(&format!("{}_", n.kind.key()), &mut used);
            mapping.insert(n.id.clone(), key.clone());
            copy.id = key.clone();
            copy.name = key.clone();
            copy.x = instance.x + n.x - min_x;
            copy.y = instance.y + n.y - min_y;
            next.nodes.push(copy);
            added.push(key);
        }
        let source = |from: &str, port: &str| -> Option<(String, String)> {
            let mut from = from.to_string();
            let mut port = port.to_string();
            for _ in 0..=def.nodes.len() {
                let n = def.nodes.by_id(&from)?;
                if n.kind != Kind::Input {
                    return mapping.get(&from).map(|key| (key.clone(), port));
                }
                let driver = self.edges.iter().find(|e| e.to == id && e.port == n.name)?;
                if driver.from != id {
                    return Some((driver.from.clone(), driver.source_port.clone()));
                }
                // A clocked module may feed its own output back into an input.
                // Resolve that boundary to the new internal node before removing it.
                let output = def
                    .nodes
                    .iter()
                    .find(|n| n.kind == Kind::Output && n.name == driver.source_port)?;
                let internal = def
                    .edges
                    .iter()
                    .find(|e| e.to == output.id && e.port == "a")?;
                from = internal.from.clone();
                port = internal.source_port.clone();
            }
            None
        };
        let mut edges = Vec::new();
        for e in &self.edges {
            if e.to == id {
                continue;
            }
            let mut copy = e.clone();
            if e.from == id {
                let out = def
                    .nodes
                    .iter()
                    .find(|n| n.kind == Kind::Output && n.name == e.source_port)
                    .ok_or("Missing module output")?;
                let driver = def.edges.iter().find(|e| e.to == out.id && e.port == "a");
                let Some((from, port)) = driver.and_then(|e| source(&e.from, &e.source_port))
                else {
                    continue;
                };
                copy.from = from;
                copy.source_port = port;
            }
            edges.push(copy);
        }
        let mut edge_ids: BTreeSet<_> = edges.iter().map(|e| e.id.clone()).collect();
        for e in &def.edges {
            let Some(to) = mapping.get(&e.to) else {
                continue;
            };
            if let Some((from, source_port)) = source(&e.from, &e.source_port) {
                edges.push(Edge {
                    id: unique("edge", &mut edge_ids),
                    from,
                    source_port,
                    to: to.clone(),
                    port: e.port.clone(),
                });
            }
        }
        next.nodes.retain(|n| n.id != id);
        next.edges = edges.into();
        next.validate(false)?;
        Ok((next, added))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn group_nest_reuse_expand_preserves_every_output() {
        let original = Document::demo();
        let (first, id) = original
            .group_module(&["add".into(), "bit_and".into()], "Arithmetic")
            .unwrap();
        assert_eq!(first.node(&id).unwrap().outputs().len(), 2);
        let (nested, outer) = first
            .group_module(&[id.clone(), "bit_xor".into(), "select".into()], "ALU")
            .unwrap();
        assert_eq!(nested.modules.len(), 2);
        for a in [0, 1, 13, u32::MAX as u64] {
            for b in [0, 2, 15, u32::MAX as u64] {
                for op in 0..3 {
                    let inputs =
                        BTreeMap::from([("a".into(), a), ("b".into(), b), ("op".into(), op)]);
                    assert_eq!(
                        original.evaluate(&inputs).unwrap(),
                        nested.evaluate(&inputs).unwrap()
                    );
                    let (expanded, _) = nested.explode_module(&outer).unwrap();
                    assert_eq!(
                        original.evaluate(&inputs).unwrap(),
                        expanded.evaluate(&inputs).unwrap()
                    );
                }
            }
        }
        let (reused, second) = nested.instantiate_module("ALU", 1800., 0.).unwrap();
        assert_ne!(second, outer);
        assert_eq!(
            reused
                .flatten()
                .unwrap()
                .nodes
                .iter()
                .filter(|n| n.kind == Kind::Add)
                .count(),
            2
        );
        let text = serde_json::to_string(&nested).unwrap();
        let restored: Document = serde_json::from_str(&text).unwrap();
        assert_eq!(restored, nested);
        assert_eq!(restored.rtl().unwrap(), nested.rtl().unwrap());
    }
    #[test]
    fn shared_definition_edit_rejects_interface_changes_and_supports_undo() {
        let mut workspace = Workspace::default();
        workspace
            .dispatch(
                "module.group",
                serde_json::json!({"expected_revision":0,"name":"Adder","ids":["add"]}),
                "test",
            )
            .unwrap();
        let before = workspace.document.clone();
        let edit = |id: &str, changes: serde_json::Value| {
            let mut op = serde_json::json!({"op":"update_node","id":id});
            op.as_object_mut()
                .unwrap()
                .extend(changes.as_object().unwrap().clone());
            serde_json::json!({"expected_revision":1,"definition":"Adder","operations":[op]})
        };
        assert!(workspace
            .dispatch(
                "module.edit",
                edit("in0", serde_json::json!({"name":"changed"})),
                "test",
            )
            .is_err());
        assert_eq!(workspace.document, before);
        let patch = edit("add", serde_json::json!({"kind":"sub"}));
        workspace
            .dispatch("module.edit", patch.clone(), "test")
            .unwrap();
        let inputs = BTreeMap::from([("a".into(), 9), ("b".into(), 3), ("op".into(), 0)]);
        assert_eq!(workspace.document.evaluate(&inputs).unwrap()["result"], 6);
        assert!(workspace.dispatch("module.edit", patch, "test").is_err());
        workspace
            .dispatch(
                "design.undo",
                serde_json::json!({"expected_revision":2}),
                "test",
            )
            .unwrap();
        assert_eq!(workspace.document, before);
    }

    #[test]
    fn recursion_bad_interfaces_and_stale_edits_are_rejected() {
        let (mut doc, id) = Document::demo()
            .group_module(&["add".into()], "Adder")
            .unwrap();
        doc.nodes
            .update(&id, |n| {
                if let NodeConfig::Module { inputs, .. } = &mut n.config {
                    inputs[0].width = 3;
                }
            })
            .unwrap();
        assert!(doc.validate(false).is_err());
        let (mut doc, id) = Document::demo()
            .group_module(&["add".into()], "Adder")
            .unwrap();
        let instance = doc.node(&id).unwrap().clone();
        Arc::make_mut(doc.modules.get_mut("Adder").unwrap())
            .nodes
            .push(instance);
        assert!(doc.validate(false).unwrap_err().contains("Recursive"));
        let mut workspace = Workspace::default();
        assert!(workspace
            .dispatch(
                "module.group",
                serde_json::json!({"expected_revision":1,"name":"Adder","ids":["add"]}),
                "test"
            )
            .is_err());
        workspace
            .dispatch(
                "module.group",
                serde_json::json!({"expected_revision":0,"name":"Adder","ids":["add"]}),
                "test",
            )
            .unwrap();
        workspace
            .dispatch(
                "design.undo",
                serde_json::json!({"expected_revision":1}),
                "test",
            )
            .unwrap();
        assert_eq!(*workspace.document, Document::demo());
    }
}
#[cfg(test)]
mod sequential_tests {
    use super::*;
    #[test]
    fn independent_register_instances_and_feedback_expansion() {
        let doc: Document = serde_json::from_value(serde_json::json!({
            "name":"Registers", "nodes":[
                {"id":"a","name":"a","kind":"input","width":8,"x":0,"y":0},
                {"id":"r","name":"r","kind":"register","width":8,"x":360,"y":0},
                {"id":"q","name":"q","kind":"output","width":8,"x":720,"y":0}
            ], "edges":[
                {"id":"ar","from":"a","to":"r","port":"d"},
                {"id":"rq","from":"r","to":"q","port":"a"}
            ]
        }))
        .unwrap();
        let (grouped, first) = doc.group_module(&["r".into()], "RegisterCell").unwrap();
        let (mut reused, second) = grouped
            .instantiate_module("RegisterCell", 360., 360.)
            .unwrap();
        let mut input = doc.node("a").unwrap().clone();
        input.id = "b".into();
        input.name = "b".into();
        reused.nodes.push(input);
        let mut output = doc.node("q").unwrap().clone();
        output.id = "s".into();
        output.name = "s".into();
        reused.nodes.push(output);
        reused.edges.push(Edge {
            id: "br".into(),
            from: "b".into(),
            source_port: "y".into(),
            to: second.clone(),
            port: "in0".into(),
        });
        reused.edges.push(Edge {
            id: "rs".into(),
            from: second,
            source_port: "out0".into(),
            to: "s".into(),
            port: "a".into(),
        });
        let rows = testbench::Testbench::import_csv(
            &reused,
            "cycle,reset,in.a,in.b,expect.q,expect.s\n1,1,3,9,0,0\n2,0,3,9,3,9\n3,0,7,21,7,21\n",
        )
        .unwrap();
        let tb = testbench::Testbench { version: 0, rows };
        let checks = tb.run(&reused).unwrap().2;
        assert_eq!(checks.passed, checks.total);
        let mut feedback = grouped;
        let incoming = feedback.edges.iter_mut().find(|e| e.to == first).unwrap();
        incoming.from = first.clone();
        incoming.source_port = "out0".into();
        feedback.validate(true).unwrap();
        let (expanded, _) = feedback.explode_module(&first).unwrap();
        expanded.validate(true).unwrap();
        let register = expanded
            .nodes
            .iter()
            .find(|n| n.kind == Kind::Register)
            .unwrap();
        assert!(expanded
            .edges
            .iter()
            .any(|e| e.from == register.id && e.to == register.id));
    }
    #[test]
    fn nested_signed_registers_reset_and_expectations_match_original() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../../examples/signed-pipeline.json")).unwrap();
        let doc: Document = serde_json::from_value(fixture["document"].clone()).unwrap();
        let rows = testbench::Testbench::import_csv(
            &doc,
            include_str!("../../../examples/signed-pipeline-stimuli.csv"),
        )
        .unwrap();
        let tb = testbench::Testbench { version: 0, rows };
        let (_, expected, checks) = tb.run(&doc).unwrap();
        assert_eq!(checks.passed, checks.total);
        let (first, inner) = doc
            .group_module(&["sum".into(), "acc".into()], "Accumulator")
            .unwrap();
        let (nested, outer) = first
            .group_module(&[inner, "delay".into()], "Pipeline")
            .unwrap();
        let (_, actual, checks) = tb.run(&nested).unwrap();
        assert_eq!(checks.passed, checks.total);
        assert_eq!(
            expected.iter().map(|s| &s.outputs).collect::<Vec<_>>(),
            actual.iter().map(|s| &s.outputs).collect::<Vec<_>>()
        );
        assert!(nested.rtl().unwrap().contains("posedge las_clk"));
        let (two, copy) = nested.instantiate_module("Pipeline", 2000., 0.).unwrap();
        let flat = two.flatten().unwrap();
        assert_eq!(
            flat.nodes
                .iter()
                .filter(|n| n.kind == Kind::Register)
                .count(),
            4
        );
        assert_ne!(outer, copy);
        let (expanded, _) = nested.explode_module(&outer).unwrap();
        assert_eq!(tb.run(&expanded).unwrap().2.passed, checks.total);
    }
}
