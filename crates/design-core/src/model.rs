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
impl Node {
    pub fn inputs(&self) -> Vec<(&'static str, u8)> {
        match self.kind {
            Kind::Input => vec![],
            Kind::Output | Kind::Not => vec![("a", self.width)],
            Kind::Mux => vec![
                ("d0", self.width),
                ("d1", self.width),
                ("d2", self.width),
                ("sel", 2),
            ],
            _ => vec![("a", self.width), ("b", self.width)],
        }
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
            ("a", Kind::Input, 32, 40., 100.),
            ("b", Kind::Input, 32, 40., 290.),
            ("op", Kind::Input, 2, 40., 490.),
            ("add", Kind::Add, 32, 310., 70.),
            ("bit_and", Kind::And, 32, 310., 250.),
            ("bit_xor", Kind::Xor, 32, 310., 430.),
            ("select", Kind::Mux, 32, 610., 240.),
            ("result", Kind::Output, 32, 890., 270.),
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
        self.validate(true)?;
        let mut values: BTreeMap<String, u64> = BTreeMap::new();
        for id in self.order()? {
            let n = self.node(&id)?;
            let get = |port: &str| -> u64 {
                let e = self
                    .edges
                    .iter()
                    .find(|e| e.to == id && e.port == port)
                    .unwrap();
                values[&e.from]
            };
            let value = match n.kind {
                Kind::Input => *inputs
                    .get(&n.name)
                    .ok_or_else(|| format!("Missing input: {}", n.name))?,
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
            values.insert(id, value & mask(n.width));
        }
        Ok(self
            .nodes
            .iter()
            .filter(|n| n.kind == Kind::Output)
            .map(|n| (n.name.clone(), values[&n.id]))
            .collect())
    }
    pub fn rtl(&self) -> Result<String, String> {
        self.validate(true)?;
        // Prefix every user identifier; this also prevents collisions with HDL keywords.
        let signal = |id: &str| format!("n_{}", self.node(id).unwrap().name);
        let ports: Vec<_> = self
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
        let mut rtl = format!(
            "// Generated by Logic Architecture Studio\nmodule las_{} (\n{}\n);\n",
            self.name,
            ports.join(",\n")
        );
        for n in &self.nodes {
            if !matches!(n.kind, Kind::Input | Kind::Output) {
                rtl += &format!("  wire [{}:0] n_{};\n", n.width - 1, n.name);
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
