use crate::entities::{Entities, Entity};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Input,
    Output,
    Add,
    Sub,
    And,
    Or,
    Xor,
    Not,
    Mux,
    Register,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Node {
    pub id: String,
    pub name: String,
    pub kind: Kind,
    pub width: u8,
    pub x: f32,
    pub y: f32,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Edge {
    pub id: String,
    pub from: String,
    pub to: String,
    pub port: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub name: String,
    pub nodes: Entities<Node>,
    pub edges: Entities<Edge>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: String,
    pub node: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CycleInput {
    pub reset: bool,
    pub inputs: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CycleSample {
    /// Post-edge values keyed by stable node ID, retained in Rust for annotation.
    #[serde(skip_serializing)]
    pub values: BTreeMap<String, u64>,
    pub reset: bool,
    pub outputs: BTreeMap<String, String>,
    pub registers: BTreeMap<String, String>,
}

pub fn valid_identifier(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.as_bytes()[0].is_ascii_alphabetic()
        && s.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
        && ![
            "module",
            "endmodule",
            "input",
            "output",
            "wire",
            "reg",
            "assign",
            "always",
            "begin",
            "end",
            "logic",
            "if",
            "else",
            "case",
            "endcase",
            "generate",
            "endgenerate",
            "integer",
            "parameter",
            "localparam",
            "signed",
            "unsigned",
            "initial",
            "for",
            "while",
            "function",
            "endfunction",
            "task",
            "endtask",
            "inout",
            "tri",
            "supply0",
            "supply1",
        ]
        .contains(&s)
}
impl Kind {
    pub fn key(&self) -> &'static str {
        match self {
            Self::Input => "input",
            Self::Output => "output",
            Self::Add => "add",
            Self::Sub => "sub",
            Self::And => "and",
            Self::Or => "or",
            Self::Xor => "xor",
            Self::Not => "not",
            Self::Mux => "mux",
            Self::Register => "register",
        }
    }

    pub fn input_names(&self) -> &'static [&'static str] {
        match self {
            Self::Input => &[],
            Self::Register => &["d"],
            Self::Output | Self::Not => &["a"],
            Self::Mux => &["d0", "d1", "d2", "sel"],
            _ => &["a", "b"],
        }
    }
}
pub const NODE_WIDTH: f32 = 288.;
/// Square by default. Leave room for the header, footer and each port row.
pub fn node_height(kind: &Kind) -> f32 {
    NODE_WIDTH.max((kind.input_names().len().max(1) + 1) as f32 * 64.)
}
impl Node {
    pub fn inputs(&self) -> Vec<(&'static str, u8)> {
        self.kind
            .input_names()
            .iter()
            .map(|&name| (name, if name == "sel" { 2 } else { self.width }))
            .collect()
    }
}

impl Document {
    pub fn encoded_len(&self) -> usize {
        b"{\"name\":,\"nodes\":,\"edges\":}".len()
            + crate::encoded_size(&self.name)
            + self.nodes.encoded_len()
            + self.edges.encoded_len()
    }

    pub fn demo() -> Self {
        let specs = [
            ("a", Kind::Input, 32, 40., 80.),
            ("b", Kind::Input, 32, 40., 400.),
            ("op", Kind::Input, 2, 40., 720.),
            ("add", Kind::Add, 32, 400., 70.),
            ("bit_and", Kind::And, 32, 400., 390.),
            ("bit_xor", Kind::Xor, 32, 400., 710.),
            ("select", Kind::Mux, 32, 760., 380.),
            ("result", Kind::Output, 32, 1120., 400.),
        ];
        let nodes = specs
            .into_iter()
            .map(|(id, kind, width, x, y)| Node {
                id: id.into(),
                name: id.into(),
                kind,
                width,
                x,
                y,
            })
            .collect();
        let mut edges = vec![];
        for to in ["add", "bit_and", "bit_xor"] {
            for port in ["a", "b"] {
                edges.push(Edge {
                    id: format!("{port}_{to}"),
                    from: port.into(),
                    to: to.into(),
                    port: port.into(),
                });
            }
        }
        for (from, port) in [
            ("add", "d0"),
            ("bit_and", "d1"),
            ("bit_xor", "d2"),
            ("op", "sel"),
        ] {
            edges.push(Edge {
                id: format!("{from}_select"),
                from: from.into(),
                to: "select".into(),
                port: port.into(),
            });
        }
        edges.push(Edge {
            id: "select_result".into(),
            from: "select".into(),
            to: "result".into(),
            port: "a".into(),
        });
        Self {
            name: "ALU32".into(),
            nodes,
            edges: edges.into(),
        }
    }
    pub fn node(&self, id: &str) -> Result<&Node, String> {
        self.nodes
            .by_id(id)
            .ok_or_else(|| format!("Unknown node: {id}"))
    }
    pub fn validate(&self, complete: bool) -> Result<(), String> {
        if !valid_identifier(&self.name) {
            return Err("Invalid module identifier".into());
        }
        if self.nodes.len() > 2000 || self.edges.len() > 8000 {
            return Err("Design size limit exceeded".into());
        }
        let mut ids = BTreeSet::new();
        let mut names = BTreeSet::new();
        for n in &self.nodes {
            if !valid_identifier(&n.id)
                || !valid_identifier(&n.name)
                || !ids.insert(&n.id)
                || !names.insert(&n.name)
            {
                return Err("Node IDs and names must be unique HDL identifiers".into());
            }
            if n.width == 0 || n.width > 64 {
                return Err("Width must be 1–64 bits".into());
            }
            if !n.x.is_finite() || !n.y.is_finite() || n.x.abs() > 100000. || n.y.abs() > 100000. {
                return Err("Invalid coordinates".into());
            }
        }
        let mut edge_ids = BTreeSet::new();
        let mut destinations = BTreeSet::new();
        for e in &self.edges {
            if !valid_identifier(&e.id) || !edge_ids.insert(&e.id) {
                return Err("Invalid or duplicate connection ID".into());
            }
            let source = self.node(&e.from)?;
            let target = self.node(&e.to)?;
            if source.kind == Kind::Output {
                return Err("Output nodes cannot drive connections".into());
            }
            let width = target
                .inputs()
                .into_iter()
                .find(|(p, _)| *p == e.port)
                .map(|(_, w)| w)
                .ok_or("Unknown destination port")?;
            if source.width != width {
                return Err(format!(
                    "Width mismatch: {} ({} bits) → {}.{} ({} bits)",
                    source.name, source.width, target.name, e.port, width
                ));
            }
            if !destinations.insert((&e.to, &e.port)) {
                return Err("An input can have only one driver".into());
            }
        }
        self.order()?;
        if complete {
            for n in &self.nodes {
                for (port, _) in n.inputs() {
                    if !self.edges.iter().any(|e| e.to == n.id && e.port == port) {
                        return Err(format!("Unconnected input: {}.{port}", n.name));
                    }
                }
            }
            if !self.nodes.iter().any(|n| n.kind == Kind::Output) {
                return Err("At least one output is required".into());
            }
        }
        Ok(())
    }
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        let mut result = vec![];
        for n in &self.nodes {
            for (port, _) in n.inputs() {
                if !self.edges.iter().any(|e| e.to == n.id && e.port == port) {
                    result.push(Diagnostic {
                        code: "unconnected_input".into(),
                        node: Some(n.id.clone()),
                        message: format!("{}.{port}", n.name),
                    });
                }
            }
        }
        result
    }
    pub fn order(&self) -> Result<Vec<String>, String> {
        let mut degree: BTreeMap<String, usize> =
            self.nodes.iter().map(|n| (n.id.clone(), 0)).collect();
        for e in &self.edges {
            if self.node(&e.to)?.kind == Kind::Register {
                continue;
            }
            *degree.get_mut(&e.to).ok_or("Unknown target")? += 1;
        }
        let mut ready: Vec<String> = degree
            .iter()
            .filter(|(_, d)| **d == 0)
            .map(|(id, _)| id.clone())
            .collect();
        let mut order = vec![];
        while let Some(id) = ready.pop() {
            for e in self.edges.iter().filter(|e| e.from == id) {
                if self.node(&e.to)?.kind == Kind::Register {
                    continue;
                }
                let d = degree.get_mut(&e.to).unwrap();
                *d -= 1;
                if *d == 0 {
                    ready.push(e.to.clone());
                }
            }
            order.push(id);
        }
        if order.len() != self.nodes.len() {
            Err("Combinational cycle detected".into())
        } else {
            Ok(order)
        }
    }
    pub fn evaluate(
        &self,
        inputs: &BTreeMap<String, u64>,
    ) -> Result<BTreeMap<String, u64>, String> {
        self.require_combinational()?;
        self.validate(true)?;
        Ok(self.outputs(&self.evaluate_values(inputs, &BTreeMap::new(), &self.order()?)?))
    }
    pub fn require_combinational(&self) -> Result<(), String> {
        if self.nodes.iter().any(|node| node.kind == Kind::Register) {
            return Err("Sequential circuit requires cycle simulation".into());
        }
        Ok(())
    }
    pub fn evaluate_signals(
        &self,
        inputs: &BTreeMap<String, u64>,
    ) -> Result<BTreeMap<String, u64>, String> {
        self.require_combinational()?;
        self.validate(true)?;
        self.evaluate_values(inputs, &BTreeMap::new(), &self.order()?)
    }
    fn evaluate_values(
        &self,
        inputs: &BTreeMap<String, u64>,
        registers: &BTreeMap<String, u64>,
        order: &[String],
    ) -> Result<BTreeMap<String, u64>, String> {
        let drivers: BTreeMap<_, _> = self
            .edges
            .iter()
            .map(|edge| ((edge.to.as_str(), edge.port.as_str()), &edge.from))
            .collect();
        let mut values: BTreeMap<String, u64> = BTreeMap::new();
        for id in order {
            let n = self.node(id)?;
            let get = |port: &str| values[drivers[&(id.as_str(), port)]];
            let value = match n.kind {
                Kind::Input => {
                    let value = *inputs
                        .get(&n.name)
                        .ok_or_else(|| format!("Missing input: {}", n.name))?;
                    if value > mask(n.width) {
                        return Err(format!("Input {} exceeds unsigned {}-bit range; use an explicit width conversion", n.name, n.width));
                    }
                    value
                }
                Kind::Register => registers[id],
                Kind::Output => get("a"),
                Kind::Add => get("a").wrapping_add(get("b")),
                Kind::Sub => get("a").wrapping_sub(get("b")),
                Kind::And => get("a") & get("b"),
                Kind::Or => get("a") | get("b"),
                Kind::Xor => get("a") ^ get("b"),
                Kind::Not => !get("a"),
                Kind::Mux => match get("sel") {
                    0 => get("d0"),
                    1 => get("d1"),
                    _ => get("d2"),
                },
            };
            values.insert(id.clone(), value & mask(n.width));
        }
        Ok(values)
    }
    fn outputs(&self, values: &BTreeMap<String, u64>) -> BTreeMap<String, u64> {
        self.nodes
            .iter()
            .filter(|n| n.kind == Kind::Output)
            .map(|n| (n.name.clone(), values[&n.id]))
            .collect()
    }
    pub fn simulate_cycles(&self, cycles: &[CycleInput]) -> Result<Vec<CycleSample>, String> {
        self.validate(true)?;
        if cycles.len().saturating_mul(self.nodes.len()) > 524_288 {
            return Err("Trace exceeds 524288 node samples; use fewer cycles".into());
        }
        if cycles.is_empty() || cycles.len() > 256 || !cycles[0].reset {
            return Err("Supply 1–256 cycles, starting with reset".into());
        }
        let order = self.order()?;
        let register_inputs = self
            .edges
            .iter()
            .filter(|edge| {
                self.node(&edge.to)
                    .is_ok_and(|node| node.kind == Kind::Register)
            })
            .collect::<Vec<_>>();
        if register_inputs.is_empty() {
            return Err("Cycle simulation requires a register".into());
        }
        let mut registers = register_inputs
            .iter()
            .map(|edge| (edge.to.clone(), 0))
            .collect();
        let mut samples = Vec::with_capacity(cycles.len());
        for cycle in cycles {
            let inputs = cycle
                .inputs
                .iter()
                .map(|(name, value)| Ok((name.clone(), crate::number(value)?)))
                .collect::<Result<BTreeMap<_, _>, String>>()?;
            if inputs.len()
                != self
                    .nodes
                    .iter()
                    .filter(|node| node.kind == Kind::Input)
                    .count()
            {
                return Err("Supply exactly the design inputs each cycle".into());
            }
            let before = self.evaluate_values(&inputs, &registers, &order)?;
            registers = register_inputs
                .iter()
                .map(|edge| {
                    (
                        edge.to.clone(),
                        if cycle.reset { 0 } else { before[&edge.from] },
                    )
                })
                .collect();
            let after = self.evaluate_values(&inputs, &registers, &order)?;
            samples.push(CycleSample {
                values: after.clone(),
                reset: cycle.reset,
                outputs: self
                    .outputs(&after)
                    .into_iter()
                    .map(|(name, value)| (name, value.to_string()))
                    .collect(),
                registers: self
                    .nodes
                    .iter()
                    .filter(|node| node.kind == Kind::Register)
                    .map(|node| (node.name.clone(), registers[&node.id].to_string()))
                    .collect(),
            });
        }
        Ok(samples)
    }
    pub fn rtl(&self) -> Result<String, String> {
        self.validate(true)?;
        // Prefix every user identifier; this also prevents collisions with HDL keywords.
        let signal = |id: &str| format!("n_{}", self.node(id).unwrap().name);
        let mut ports: Vec<_> = self
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, Kind::Input | Kind::Output))
            .map(|n| {
                format!(
                    "  {} wire [{}:0] n_{}",
                    if n.kind == Kind::Input {
                        "input"
                    } else {
                        "output"
                    },
                    n.width - 1,
                    n.name
                )
            })
            .collect();
        if self.nodes.iter().any(|node| node.kind == Kind::Register) {
            ports.extend([
                "  input wire las_clk".into(),
                "  input wire las_reset".into(),
            ]);
        }
        let mut rtl = format!(
            "// Generated by Logic Architecture Studio\nmodule las_{} (\n{}\n);\n",
            self.name,
            ports.join(",\n")
        );
        for n in &self.nodes {
            if !matches!(n.kind, Kind::Input | Kind::Output) {
                let storage = if n.kind == Kind::Register {
                    "reg"
                } else {
                    "wire"
                };
                rtl += &format!("  {storage} [{}:0] n_{};\n", n.width - 1, n.name);
            }
        }
        for id in self.order()? {
            let n = self.node(&id)?;
            if n.kind == Kind::Input {
                continue;
            }
            let get = |port: &str| {
                signal(
                    &self
                        .edges
                        .iter()
                        .find(|e| e.to == id && e.port == port)
                        .unwrap()
                        .from,
                )
            };
            if n.kind == Kind::Register {
                rtl += &format!(
                    "  always @(posedge las_clk) {} <= las_reset ? {}'d0 : {};\n",
                    signal(&id),
                    n.width,
                    get("d")
                );
                continue;
            }
            let expr = match n.kind {
                Kind::Output => get("a"),
                Kind::Not => format!("~{}", get("a")),
                Kind::Mux => format!(
                    "{} == 2'd0 ? {} : ({} == 2'd1 ? {} : {})",
                    get("sel"),
                    get("d0"),
                    get("sel"),
                    get("d1"),
                    get("d2")
                ),
                _ => format!(
                    "{} {} {}",
                    get("a"),
                    match n.kind {
                        Kind::Add => "+",
                        Kind::Sub => "-",
                        Kind::And => "&",
                        Kind::Or => "|",
                        _ => "^",
                    },
                    get("b")
                ),
            };
            rtl += &format!("  assign {} = {};\n", signal(&id), expr);
        }
        rtl += "endmodule\n";
        Ok(rtl)
    }
}
pub fn mask(width: u8) -> u64 {
    if width == 64 {
        u64::MAX
    } else {
        (1u64 << width) - 1
    }
}

impl Entity for Node {
    fn id(&self) -> &str {
        &self.id
    }
}
impl Entity for Edge {
    fn id(&self) -> &str {
        &self.id
    }
}
