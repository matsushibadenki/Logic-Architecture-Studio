//! A validated, borrowed circuit plan. All samples of one run use the same topology.
use crate::{mask, CycleInput, CycleSample, Document, Edge, Kind, Node, NodeConfig};
use std::collections::BTreeMap;

pub(crate) struct EvaluationPlan<'a> {
    document: &'a Document,
    order: Vec<&'a Node>,
    drivers: BTreeMap<(&'a str, &'a str), &'a Edge>,
}

impl<'a> EvaluationPlan<'a> {
    fn new(document: &'a Document) -> Result<Self, String> {
        document.validate(true)?;
        let order = document
            .order()?
            .iter()
            .map(|id| document.node(id))
            .collect::<Result<_, _>>()?;
        Ok(Self {
            document,
            order,
            drivers: document.drivers(),
        })
    }

    pub(crate) fn combinational(document: &'a Document) -> Result<Self, String> {
        document.require_combinational()?;
        Self::new(document)
    }

    pub(crate) fn outputs(
        &self,
        inputs: &BTreeMap<String, u64>,
    ) -> Result<BTreeMap<String, u64>, String> {
        let values = self.values(inputs, &BTreeMap::new())?;
        Ok(self
            .document
            .nodes
            .iter()
            .filter(|n| n.kind == Kind::Output)
            .map(|n| (n.name.clone(), values[&n.id]))
            .collect())
    }

    pub(crate) fn sample(&self, cycle: &CycleInput) -> Result<CycleSample, String> {
        let inputs = self.document.parse_signals(&cycle.inputs, Kind::Input)?;
        let values = self.values(&inputs, &BTreeMap::new())?;
        Ok(CycleSample {
            outputs: self.document.sample_outputs(&values),
            values,
            reset: cycle.reset,
            registers: BTreeMap::new(),
        })
    }

    fn values(
        &self,
        inputs: &BTreeMap<String, u64>,
        registers: &BTreeMap<String, u64>,
    ) -> Result<BTreeMap<String, u64>, String> {
        let mut values: BTreeMap<String, u64> = BTreeMap::new();
        for n in &self.order {
            let id = &n.id;
            let get = |port: &str| values[&self.drivers[&(id.as_str(), port)].from];
            let value = match n.kind {
                Kind::Module => return Err("Unelaborated module".into()),
                Kind::Input => {
                    let value = *inputs
                        .get(&n.name)
                        .ok_or_else(|| format!("Missing input: {}", n.name))?;
                    if value > mask(n.width) {
                        return Err(format!("Input {} exceeds unsigned {}-bit range; use an explicit width conversion", n.name, n.width));
                    }
                    value
                }
                Kind::Constant => match &n.config {
                    NodeConfig::Constant { value } => n.parse_value(value)?,
                    _ => unreachable!(),
                },
                Kind::Slice => match n.config {
                    NodeConfig::Slice { lsb, .. } => get("a") >> lsb,
                    _ => unreachable!(),
                },
                Kind::Concat => match n.config {
                    NodeConfig::Concat { low_width, .. } => (get("hi") << low_width) | get("lo"),
                    _ => unreachable!(),
                },
                Kind::ZeroExtend => get("a"),
                Kind::SignExtend => match n.config {
                    NodeConfig::SignExtend { input_width } => {
                        let value = get("a");
                        if value & (1u64 << (input_width - 1)) != 0 {
                            value | (mask(n.width) ^ mask(input_width))
                        } else {
                            value
                        }
                    }
                    _ => unreachable!(),
                },
                Kind::Register => registers[id],
                Kind::Output => get("a"),
                Kind::Add => get("a").wrapping_add(get("b")),
                Kind::Sub => get("a").wrapping_sub(get("b")),
                Kind::And => get("a") & get("b"),
                Kind::Nand => !(get("a") & get("b")),
                Kind::Nor => !(get("a") | get("b")),
                Kind::Xnor => !(get("a") ^ get("b")),
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
}

impl Document {
    pub fn evaluate(
        &self,
        inputs: &BTreeMap<String, u64>,
    ) -> Result<BTreeMap<String, u64>, String> {
        if self.has_modules() {
            return self.flatten()?.evaluate(inputs);
        }
        EvaluationPlan::combinational(self)?.outputs(inputs)
    }
    pub fn require_combinational(&self) -> Result<(), String> {
        if self.has_modules() {
            return self.flatten()?.require_combinational();
        }
        if self.nodes.iter().any(|node| node.kind == Kind::Register) {
            return Err("Sequential circuit requires cycle simulation".into());
        }
        Ok(())
    }
    pub fn combinational_sample(&self, cycle: &CycleInput) -> Result<CycleSample, String> {
        if self.has_modules() {
            return self.flatten()?.combinational_sample(cycle);
        }
        EvaluationPlan::combinational(self)?.sample(cycle)
    }

    pub fn evaluate_signals(
        &self,
        inputs: &BTreeMap<String, u64>,
    ) -> Result<BTreeMap<String, u64>, String> {
        if self.has_modules() {
            return self.flatten()?.evaluate_signals(inputs);
        }
        EvaluationPlan::combinational(self)?.values(inputs, &BTreeMap::new())
    }
    fn sample_outputs(&self, values: &BTreeMap<String, u64>) -> BTreeMap<String, String> {
        self.nodes
            .iter()
            .filter(|n| n.kind == Kind::Output)
            .map(|n| (n.name.clone(), n.decimal(values[&n.id])))
            .collect()
    }
    pub fn simulate_cycles(&self, cycles: &[CycleInput]) -> Result<Vec<CycleSample>, String> {
        if self.has_modules() {
            return self.flatten()?.simulate_cycles(cycles);
        }
        let plan = EvaluationPlan::new(self)?;
        if cycles.len().saturating_mul(self.nodes.len()) > 524_288 {
            return Err("Trace exceeds 524288 node samples; use fewer cycles".into());
        }
        if cycles.is_empty() || cycles.len() > 256 || !cycles[0].reset {
            return Err("Supply 1–256 cycles, starting with reset".into());
        }
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
            let inputs = self.parse_signals(&cycle.inputs, Kind::Input)?;
            let before = plan.values(&inputs, &registers)?;
            registers = register_inputs
                .iter()
                .map(|edge| {
                    (
                        edge.to.clone(),
                        if cycle.reset { 0 } else { before[&edge.from] },
                    )
                })
                .collect();
            let after = plan.values(&inputs, &registers)?;
            samples.push(CycleSample {
                reset: cycle.reset,
                outputs: self.sample_outputs(&after),
                values: after,
                registers: self
                    .nodes
                    .iter()
                    .filter(|node| node.kind == Kind::Register)
                    .map(|node| (node.name.clone(), node.decimal(registers[&node.id])))
                    .collect(),
            });
        }
        Ok(samples)
    }
}
