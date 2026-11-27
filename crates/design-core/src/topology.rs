//! Deterministic combinational traversal shared by layout, simulation and RTL.
use crate::{Document, Edge, Kind, NODE_WIDTH};
use std::collections::BTreeMap;

impl Document {
    pub(crate) fn drivers(&self) -> BTreeMap<(&str, &str), &Edge> {
        self.edges
            .iter()
            .map(|e| ((e.to.as_str(), e.port.as_str()), e))
            .collect()
    }

    fn topological_levels(&self) -> Result<Vec<(&str, usize)>, String> {
        // Registers and unexpanded instances start columns, including in feedback loops.
        let mut pending: BTreeMap<_, _> = self
            .nodes
            .iter()
            .map(|n| (n.id.as_str(), (0usize, 0usize)))
            .collect();
        let mut outgoing: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for edge in &self.edges {
            let target = self.node(&edge.to)?;
            if !matches!(target.kind, Kind::Register | Kind::Module) {
                pending.get_mut(target.id.as_str()).unwrap().0 += 1;
                outgoing.entry(&edge.from).or_default().push(&target.id);
            }
        }
        // Preserve the historical stack ordering for ties and the stored order of fan-out edges.
        let mut ready: Vec<_> = pending
            .iter()
            .filter(|(_, (count, _))| *count == 0)
            .map(|(&id, _)| (id, 0))
            .collect();
        let mut order = Vec::with_capacity(self.nodes.len());
        while let Some((id, level)) = ready.pop() {
            if let Some(targets) = outgoing.get(id) {
                for &target in targets {
                    let (count, depth) = pending.get_mut(target).unwrap();
                    *depth = (*depth).max(level + 1);
                    *count -= 1;
                    if *count == 0 {
                        ready.push((target, *depth));
                    }
                }
            }
            order.push((id, level));
        }
        if order.len() != self.nodes.len() {
            return Err("Combinational cycle detected".into());
        }
        Ok(order)
    }

    pub fn order(&self) -> Result<Vec<String>, String> {
        Ok(self
            .topological_levels()?
            .into_iter()
            .map(|(id, _)| id.to_owned())
            .collect())
    }

    pub(crate) fn auto_arranged(&self) -> Result<Self, String> {
        let mut doc = self.clone();
        let mut next_y = BTreeMap::new();
        for (id, level) in self.topological_levels()? {
            let y = next_y.entry(level).or_insert(70.);
            let x = 40. + level as f32 * (NODE_WIDTH + 72.);
            if !crate::model::valid_coordinate(x) || !crate::model::valid_coordinate(*y) {
                return Err("Auto layout exceeds the canvas coordinate range".into());
            }
            doc.nodes.update(id, |node| {
                node.x = x;
                node.y = *y;
                *y += node.height() + 32.;
            })?;
        }
        Ok(doc)
    }
}
