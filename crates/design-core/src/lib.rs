mod preferences;
pub use preferences::{default_node_colors, parse_node_color};
pub mod entities;
pub mod model;
pub mod protocol;
pub use model::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    AddNode {
        node: Node,
    },
    RemoveNode {
        id: String,
    },
    UpdateNode {
        id: String,
        name: Option<String>,
        kind: Option<Kind>,
        width: Option<u8>,
        x: Option<f32>,
        y: Option<f32>,
    },
    Connect {
        edge: Edge,
    },
    Disconnect {
        id: String,
    },
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Patch {
    pub expected_revision: u64,
    pub label: String,
    pub operations: Vec<Operation>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Revision {
    pub revision: u64,
    pub label: String,
    pub actor: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TestVector {
    pub inputs: BTreeMap<String, String>,
    pub expected: BTreeMap<String, String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TestReport {
    pub revision: u64,
    pub passed: usize,
    pub total: usize,
    pub failures: Vec<String>,
    pub engine: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Candidate {
    pub id: String,
    pub name: String,
    pub document: Arc<Document>,
    pub report: Option<TestReport>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Experiment {
    pub id: String,
    pub name: String,
    pub base_revision: u64,
    pub candidates: Vec<Candidate>,
}
#[derive(Debug, Clone, Default)]
struct HistoryCache(Option<(Vec<usize>, Vec<usize>)>);
// A recomputable byte-count cache is not part of workspace semantics.
impl PartialEq for HistoryCache {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}
fn encoded_size(value: &impl Serialize) -> usize {
    struct Counter(usize);
    impl std::io::Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 += bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut count = Counter(0);
    if serde_json::to_writer(&mut count, value).is_err() {
        usize::MAX / 1024
    } else {
        count.0
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Workspace {
    pub schema: u32,
    pub revision: u64,
    pub document: Arc<Document>,
    pub history: Vec<Revision>,
    pub undo: Vec<Arc<Document>>,
    pub redo: Vec<Arc<Document>>,
    pub report: Option<TestReport>,
    pub experiments: Vec<Experiment>,
    pub selected: Option<String>,
    #[serde(default)]
    pub selection: Vec<String>,
    pub locale: String,
    #[serde(default = "default_node_colors")]
    pub node_colors: BTreeMap<String, String>,
    pub panels: BTreeMap<String, String>,
    #[serde(skip)]
    history_cache: HistoryCache,
}
impl Default for Workspace {
    fn default() -> Self {
        Self {
            schema: 1,
            revision: 0,
            document: Arc::new(Document::demo()),
            history: vec![],
            undo: vec![],
            redo: vec![],
            report: None,
            experiments: vec![],
            selected: Some("add".into()),
            selection: vec!["add".into()],
            locale: "ja".into(),
            node_colors: default_node_colors(),
            history_cache: HistoryCache::default(),
            panels: BTreeMap::new(),
        }
    }
}
fn parse<T: serde::de::DeserializeOwned>(v: Value) -> Result<T, String> {
    serde_json::from_value(v).map_err(|e| e.to_string())
}
fn field<'a>(v: &'a Value, k: &str) -> Result<&'a str, String> {
    v[k].as_str().ok_or_else(|| format!("Missing string: {k}"))
}
fn revision(v: &Value) -> Result<u64, String> {
    v["expected_revision"]
        .as_u64()
        .ok_or("expected_revision is required".into())
}
pub fn apply(document: &Document, operations: &[Operation]) -> Result<Document, String> {
    if operations.is_empty() || operations.len() > 512 {
        return Err("Patch needs 1–512 operations".into());
    }
    let mut next = document.clone();
    for operation in operations {
        match operation {
            Operation::AddNode { node } => next.nodes.push(node.clone()),
            Operation::RemoveNode { id } => {
                next.node(id)?;
                next.nodes.retain(|n| n.id != *id);
                next.edges.retain(|e| e.from != *id && e.to != *id);
            }
            Operation::UpdateNode {
                id,
                name,
                kind,
                width,
                x,
                y,
            } => {
                let n = next
                    .nodes
                    .iter_mut()
                    .find(|n| n.id == *id)
                    .ok_or("Unknown node")?;
                if let Some(v) = name {
                    n.name = v.clone();
                }
                if let Some(v) = kind {
                    n.kind = v.clone();
                }
                if let Some(v) = width {
                    n.width = *v;
                }
                if let Some(v) = x {
                    n.x = *v;
                }
                if let Some(v) = y {
                    n.y = *v;
                }
            }
            Operation::Connect { edge } => next.edges.push(edge.clone()),
            Operation::Disconnect { id } => {
                if !next.edges.iter().any(|e| e.id == *id) {
                    return Err("Unknown connection".into());
                }
                next.edges.retain(|e| e.id != *id);
            }
        }
    }
    next.validate(false)?;
    Ok(next)
}
pub fn is_position_patch(args: &Value) -> bool {
    serde_json::from_value::<Patch>(args.clone()).is_ok_and(|p| !p.operations.is_empty() && p.operations.iter().all(|op|matches!(op,Operation::UpdateNode {name:None,kind:None,width:None,x,y,..} if x.is_some() || y.is_some())))
}
fn apply_positions(document: &Document, operations: &[Operation]) -> Result<Document, String> {
    if operations.is_empty() || operations.len() > 512 {
        return Err("Patch needs 1–512 operations".into());
    }
    let mut next = document.clone();
    for op in operations {
        let Operation::UpdateNode {
            id,
            name: None,
            kind: None,
            width: None,
            x,
            y,
        } = op
        else {
            return Err("Position-only patch required".into());
        };
        if x.iter()
            .chain(y)
            .any(|v| !v.is_finite() || v.abs() > 100000.)
        {
            return Err("Invalid coordinates".into());
        }
        next.nodes.update(id, |node| {
            if let Some(x) = x {
                node.x = *x;
            }
            if let Some(y) = y {
                node.y = *y;
            }
        })?;
    }
    Ok(next)
}
impl Workspace {
    pub fn selected_nodes(&self) -> Vec<String> {
        if self.selection.is_empty() {
            self.selected.iter().cloned().collect()
        } else {
            self.selection.clone()
        }
    }
    pub fn snapshot(&self) -> Value {
        json!({"document":self.document,"revision":self.revision,"history":self.history,"can_undo":!self.undo.is_empty(),"can_redo":!self.redo.is_empty(),"report":self.report,"experiments":self.experiments,"selected":self.selected,"selection":self.selected_nodes(),"locale":self.locale,"node_colors":self.node_colors,"panels":self.panels,"diagnostics":self.document.diagnostics()})
    }
    fn check_revision(&self, r: u64) -> Result<(), String> {
        if r != self.revision {
            Err(format!(
                "Revision conflict: expected {r}, current {}. Refresh before retrying.",
                self.revision
            ))
        } else {
            Ok(())
        }
    }
    fn record(&mut self, label: String, actor: &str) {
        self.revision += 1;
        self.history.push(Revision {
            revision: self.revision,
            label,
            actor: actor.into(),
        });
        if self.history.len() > 100 {
            self.history.remove(0);
        }
        if self
            .selected
            .as_ref()
            .is_some_and(|id| self.document.node(id).is_err())
        {
            self.selected = None;
        }
        self.selection.retain(|id| self.document.node(id).is_ok());
        if self.selected.is_none() {
            self.selected = self.selection.first().cloned();
        }
    }
    pub fn init_history_budget(&mut self) {
        self.history_cache.0 = Some((
            self.undo.iter().map(|doc| doc.encoded_len()).collect(),
            self.redo.iter().map(|doc| doc.encoded_len()).collect(),
        ));
    }
    fn take_history_budget(&mut self) -> (Vec<usize>, Vec<usize>) {
        if self
            .history_cache
            .0
            .as_ref()
            .is_none_or(|(u, r)| u.len() != self.undo.len() || r.len() != self.redo.len())
        {
            self.init_history_budget();
        }
        self.history_cache.0.take().unwrap()
    }
    fn trim_history(&mut self, mut sizes: (Vec<usize>, Vec<usize>)) {
        while self.undo.len() + self.redo.len() > 100
            || 7 + sizes.0.iter().chain(&sizes.1).sum::<usize>()
                + sizes.0.len().saturating_sub(1)
                + sizes.1.len().saturating_sub(1)
                > 16 * 1024 * 1024
        {
            if !self.undo.is_empty() {
                self.undo.remove(0);
                sizes.0.remove(0);
            } else if !self.redo.is_empty() {
                self.redo.remove(0);
                sizes.1.remove(0);
            } else {
                break;
            }
        }
        self.history_cache.0 = Some(sizes);
    }
    pub fn limit_history(&mut self) {
        // Public callers may have edited the public vectors directly.
        self.init_history_budget();
        let sizes = self.take_history_budget();
        self.trim_history(sizes);
    }
    fn commit(&mut self, doc: impl Into<Arc<Document>>, label: String, actor: &str) {
        let mut sizes = self.take_history_budget();
        sizes.0.push(self.document.encoded_len());
        sizes.1.clear();
        self.undo.push(self.document.clone());
        self.redo.clear();
        self.trim_history(sizes);
        self.document = doc.into();
        self.record(label, actor);
    }
    fn reply(&self, compact: bool) -> Value {
        if compact {
            json!({"revision":self.revision,"committed":true})
        } else {
            self.snapshot()
        }
    }
    pub fn dispatch_view(
        &mut self,
        method: &str,
        args: Value,
        actor: &str,
    ) -> Result<Value, String> {
        self.dispatch_inner(method, args, actor, true)
    }
    pub fn dispatch(&mut self, method: &str, args: Value, actor: &str) -> Result<Value, String> {
        self.dispatch_inner(method, args, actor, false)
    }
    fn dispatch_inner(
        &mut self,
        method: &str,
        args: Value,
        actor: &str,
        compact: bool,
    ) -> Result<Value, String> {
        match method {
            "project.inspect" | "design.get_ir" => Ok(self.snapshot()),
            "project.export" => Ok(json!({"schema":1,"document":self.document})),
            "project.import" => {
                self.check_revision(revision(&args)?)?;
                let project = &args["project"];
                if project["schema"].as_u64() != Some(1) {
                    return Err("Unsupported project schema".into());
                }
                let document: Document = parse(project["document"].clone())?;
                document.validate(false)?;
                self.commit(document, "Import project".into(), actor);
                Ok(self.reply(compact))
            }
            "design.propose_patch" | "design.apply_patch" => {
                let patch: Patch = parse(args)?;
                self.check_revision(patch.expected_revision)?;
                if patch.label.len() > 160 {
                    return Err("Patch label too long".into());
                }
                let positions = patch.operations.iter().all(|op|matches!(op,Operation::UpdateNode {name:None,kind:None,width:None,x,y,..} if x.is_some() || y.is_some()));
                let next = if positions {
                    apply_positions(&self.document, &patch.operations)?
                } else {
                    apply(&self.document, &patch.operations)?
                };
                if method == "design.propose_patch" {
                    return Ok(
                        json!({"valid":true,"patch":patch,"before":self.document,"after":next,"diagnostics":next.diagnostics()}),
                    );
                }
                self.commit(next, patch.label, actor);
                Ok(self.reply(compact))
            }
            "design.undo" | "design.redo" => {
                self.check_revision(revision(&args)?)?;
                let mut sizes = self.take_history_budget();
                if method == "design.undo" {
                    let previous = self.undo.pop().ok_or("Nothing to undo")?;
                    sizes.0.pop();
                    sizes.1.push(self.document.encoded_len());
                    self.redo.push(self.document.clone());
                    self.document = previous;
                } else {
                    let next = self.redo.pop().ok_or("Nothing to redo")?;
                    sizes.1.pop();
                    sizes.0.push(self.document.encoded_len());
                    self.undo.push(self.document.clone());
                    self.document = next;
                }
                self.trim_history(sizes);
                self.record(method.into(), actor);
                Ok(self.reply(compact))
            }
            "layout.auto_arrange" => {
                self.check_revision(revision(&args)?)?;
                let mut doc = (*self.document).clone();
                let mut levels: BTreeMap<String, usize> = BTreeMap::new();
                let mut next_y: BTreeMap<usize, f32> = BTreeMap::new();
                for id in doc.order()? {
                    let level = doc
                        .edges
                        .iter()
                        .filter(|e| {
                            e.to == id
                                && doc.node(&id).is_ok_and(|node| node.kind != Kind::Register)
                        })
                        .map(|e| levels[&e.from] + 1)
                        .max()
                        .unwrap_or(0);
                    levels.insert(id.clone(), level);
                    let y = next_y.entry(level).or_insert(70.);
                    let n = doc.nodes.iter_mut().find(|n| n.id == id).unwrap();
                    n.x = 40. + level as f32 * (NODE_WIDTH + 72.);
                    n.y = *y;
                    *y += node_height(&n.kind) + 32.;
                }
                self.commit(doc, "layout.auto_arrange".into(), actor);
                Ok(self.reply(compact))
            }
            "view.focus_module" => {
                let id = field(&args, "id")?;
                self.document.node(id)?;
                self.selected = Some(id.into());
                self.selection = vec![id.into()];
                Ok(self.reply(compact))
            }
            "view.select_nodes" => {
                self.check_revision(revision(&args)?)?;
                let mut ids: Vec<String> = parse(args["ids"].clone())?;
                if ids.len() > 512 {
                    return Err("Select at most 512 nodes".into());
                }
                ids.sort();
                if ids.windows(2).any(|pair| pair[0] == pair[1]) {
                    return Err("Duplicate selection ID".into());
                }
                for id in &ids {
                    self.document.node(id)?;
                }
                self.selected = ids.first().cloned();
                self.selection = ids;
                Ok(self.reply(compact))
            }
            "view.preferences" => {
                // Validate every field before publishing any preference changes.
                let locale = args
                    .get("locale")
                    .map(|v| v.as_str().ok_or("Invalid locale"))
                    .transpose()?;
                if locale.is_some_and(|v| !["en", "ja", "zh"].contains(&v)) {
                    return Err("Unknown locale".into());
                }
                let panel = args
                    .get("panel")
                    .map(|v| v.as_str().ok_or("Invalid panel"))
                    .transpose()?;
                let position = if let Some(panel) = panel {
                    if !["inspector", "results", "library"].contains(&panel) {
                        return Err("Unknown panel".into());
                    }
                    let position = field(&args, "position")?;
                    if !["left", "right", "bottom", "floating", "hidden"].contains(&position) {
                        return Err("Unknown panel position".into());
                    }
                    Some(position)
                } else {
                    None
                };
                let reset = args
                    .get("reset_colors")
                    .map(|v| v.as_bool().ok_or("Invalid reset_colors"))
                    .transpose()?
                    .unwrap_or(false);
                let defaults = default_node_colors();
                let mut colors = if reset {
                    defaults.clone()
                } else {
                    self.node_colors.clone()
                };
                if let Some(changes) = args.get("node_colors") {
                    let changes: BTreeMap<String, Option<String>> = parse(changes.clone())?;
                    for (kind, color) in changes {
                        let default = defaults.get(&kind).ok_or("Unknown node kind")?;
                        if let Some(color) = color {
                            if parse_node_color(&color).is_none() {
                                return Err("Color must be #RRGGBB".into());
                            }
                            colors.insert(kind, color.to_ascii_lowercase());
                        } else {
                            colors.insert(kind, default.clone());
                        }
                    }
                }
                if let Some(locale) = locale {
                    self.locale = locale.into();
                }
                if let (Some(panel), Some(position)) = (panel, position) {
                    self.panels.insert(panel.into(), position.into());
                }
                self.node_colors = colors;
                Ok(self.reply(compact))
            }
            "rtl.generate" => Ok(
                json!({"revision":self.revision,"language":"Verilog-2005","source":self.document.rtl()?}),
            ),
            "simulation.cycles" => {
                if args["expected_revision"].as_u64() != Some(self.revision) {
                    return Err("Revision conflict".into());
                }
                let cycles: Vec<CycleInput> = parse(args["cycles"].clone())?;
                Ok(
                    json!({"revision": self.revision, "samples": self.document.simulate_cycles(&cycles)?}),
                )
            }
            "simulation.evaluate" => {
                let strings: BTreeMap<String, String> = parse(args["inputs"].clone())?;
                let inputs = strings
                    .into_iter()
                    .map(|(k, v)| Ok((k, number(&v)?)))
                    .collect::<Result<_, String>>()?;
                let output = self.document.evaluate(&inputs)?;
                Ok(
                    json!({"revision":self.revision,"outputs":output.into_iter().map(|(k,v)|(k,v.to_string())).collect::<BTreeMap<_,_>>()}),
                )
            }
            "verify.run_tests" => {
                let vectors = if args["vectors"].is_null() {
                    alu_vectors(&self.document)?
                } else {
                    parse(args["vectors"].clone())?
                };
                let report = verify(&self.document, self.revision, &vectors)?;
                self.report = Some(report.clone());
                Ok(json!(report))
            }
            "revision.diff" => {
                let prev = self.undo.last().ok_or("No previous revision")?;
                Ok(json!({"before":prev,"after":self.document,"revision":self.revision}))
            }
            "experiment.create" => {
                self.check_revision(revision(&args)?)?;
                if self.experiments.len() >= 20 {
                    return Err("Experiment limit (20) reached".into());
                }
                let name = field(&args, "name")?.to_string();
                if name.len() > 120 {
                    return Err("Name too long".into());
                }
                let variants = args["variants"].as_array().ok_or("variants is required")?;
                if variants.is_empty() || variants.len() > 8 {
                    return Err("Use 1–8 candidates".into());
                }
                let id = format!("exp_{}", self.experiments.len() + 1);
                let mut candidates = vec![];
                for (i, v) in variants.iter().enumerate() {
                    let ops: Vec<Operation> = parse(v["operations"].clone())?;
                    let document = if ops.is_empty() {
                        self.document.clone()
                    } else {
                        Arc::new(apply(&self.document, &ops)?)
                    };
                    candidates.push(Candidate {
                        id: format!("candidate_{i}"),
                        name: field(v, "name")?.into(),
                        document,
                        report: None,
                    });
                }
                self.experiments.push(Experiment {
                    id: id.clone(),
                    name,
                    base_revision: self.revision,
                    candidates,
                });
                Ok(json!({"id":id}))
            }
            "experiment.run" => {
                let id = field(&args, "id")?;
                let exp = self
                    .experiments
                    .iter_mut()
                    .find(|e| e.id == id)
                    .ok_or("Unknown experiment")?;
                let vectors = if args["vectors"].is_null() {
                    alu_vectors(&exp.candidates[0].document)?
                } else {
                    parse(args["vectors"].clone())?
                };
                for c in &mut exp.candidates {
                    c.report = Some(verify(&c.document, exp.base_revision, &vectors)?);
                }
                Ok(json!(exp))
            }
            "experiment.compare" | "experiment.get_status" => {
                let id = field(&args, "id")?;
                let exp = self
                    .experiments
                    .iter()
                    .find(|e| e.id == id)
                    .ok_or("Unknown experiment")?;
                Ok(json!(exp))
            }
            "experiment.adopt" => {
                self.check_revision(revision(&args)?)?;
                let exp = self
                    .experiments
                    .iter()
                    .find(|e| e.id == args["id"])
                    .ok_or("Unknown experiment")?;
                if exp.base_revision != self.revision {
                    return Err("Experiment is stale; rebase before adoption".into());
                }
                let c = exp
                    .candidates
                    .iter()
                    .find(|c| c.id == args["candidate_id"])
                    .ok_or("Unknown candidate")?;
                if !c
                    .report
                    .as_ref()
                    .is_some_and(|r| r.passed == r.total && r.total > 0)
                {
                    return Err("Only passing candidates can be adopted".into());
                }
                self.commit(c.document.clone(), format!("Adopt {}", c.name), actor);
                Ok(self.reply(compact))
            }
            _ => Err(format!("Unsupported command: {method}")),
        }
    }
}
/// Parse an unsigned signal value without losing 64-bit integer precision.
pub fn number(s: &str) -> Result<u64, String> {
    if let Some(s) = s.strip_prefix("0x") {
        u64::from_str_radix(s, 16)
    } else {
        s.parse()
    }
    .map_err(|_| "Expected unsigned 64-bit decimal or hexadecimal string".into())
}
pub fn alu_width(doc: &Document) -> Result<u8, String> {
    doc.require_combinational()?;
    let a = doc
        .nodes
        .iter()
        .find(|n| n.name == "a" && n.kind == Kind::Input)
        .ok_or("Provide explicit test vectors for this design")?;
    let b = doc
        .nodes
        .iter()
        .find(|n| n.name == "b" && n.kind == Kind::Input)
        .ok_or("Provide explicit test vectors for this design")?;
    if a.width != b.width
        || !doc
            .nodes
            .iter()
            .any(|n| n.name == "op" && n.kind == Kind::Input && n.width == 2)
        || !doc
            .nodes
            .iter()
            .any(|n| n.name == "result" && n.kind == Kind::Output && n.width == a.width)
        || doc.nodes.iter().filter(|n| n.kind == Kind::Input).count() != 3
    {
        return Err(
            "Built-in ALU contract requires a, b, op → result; supply vectors for other circuits"
                .into(),
        );
    }
    Ok(a.width)
}

pub fn alu_vectors(doc: &Document) -> Result<Vec<TestVector>, String> {
    let width = alu_width(doc)?;
    let max = mask(width);
    let values = [0, 1, max, max >> 1];
    let mut pairs = vec![];
    for x in values {
        for y in values {
            pairs.push((x, y));
        }
    }
    let mut seed = 0x4c41535f524e475fu64;
    for _ in 0..64 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let x = seed & max;
        seed = seed.rotate_left(19) ^ 0x9e3779b97f4a7c15;
        pairs.push((x, seed & max));
    }
    let mut vectors = vec![];
    for (x, y) in pairs {
        for op in 0..4 {
            let result = match op {
                0 => x.wrapping_add(y) & max,
                1 => x & y,
                _ => x ^ y,
            };
            vectors.push(TestVector {
                inputs: BTreeMap::from([
                    ("a".into(), x.to_string()),
                    ("b".into(), y.to_string()),
                    ("op".into(), op.to_string()),
                ]),
                expected: BTreeMap::from([("result".into(), result.to_string())]),
            });
        }
    }
    Ok(vectors)
}
pub fn verify(doc: &Document, rev: u64, vectors: &[TestVector]) -> Result<TestReport, String> {
    doc.validate(true)?;
    if vectors.is_empty() || vectors.len() > 4096 {
        return Err("Use 1–4096 test vectors".into());
    }
    let outputs: Vec<_> = doc
        .nodes
        .iter()
        .filter(|n| n.kind == Kind::Output)
        .map(|n| n.name.clone())
        .collect();
    let mut failures = vec![];
    let mut passed = 0;
    for (i, v) in vectors.iter().enumerate() {
        if v.expected.len() != outputs.len() || !outputs.iter().all(|k| v.expected.contains_key(k))
        {
            return Err("Every output needs an expected value".into());
        }
        let inputs = v
            .inputs
            .iter()
            .map(|(k, v)| Ok((k.clone(), number(v)?)))
            .collect::<Result<_, String>>()?;
        let expected = v
            .expected
            .iter()
            .map(|(k, v)| Ok((k.clone(), number(v)?)))
            .collect::<Result<BTreeMap<_, _>, String>>()?;
        let actual = doc.evaluate(&inputs)?;
        if actual == expected {
            passed += 1;
        } else if failures.len() < 20 {
            failures.push(format!("Vector {i}: expected {expected:?}, got {actual:?}"));
        }
    }
    Ok(TestReport {
        revision: rev,
        passed,
        total: vectors.len(),
        failures,
        engine: "Rust behavioral evaluator • ALU contract / supplied vectors (not formal proof)"
            .into(),
    })
}
#[cfg(test)]
mod tests;
