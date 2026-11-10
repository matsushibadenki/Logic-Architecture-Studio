use design_core::{Document, Edge, Kind, Node};
pub fn circuit(count: usize) -> Document {
    let nodes = (0..count)
        .map(|i| Node {
            id: format!("n{i}"),
            name: format!("n{i}"),
            kind: if i < 2 { Kind::Input } else { Kind::Add },
            width: 32,
            x: (i % 40) as f32 * 260.,
            y: (i / 40) as f32 * 180.,
        })
        .collect();
    let edges = (2..count)
        .flat_map(|i| {
            [("a", i - 1), ("b", i - 2)].map(move |(p, from)| Edge {
                id: format!("e{i}_{p}"),
                from: format!("n{from}"),
                to: format!("n{i}"),
                port: p.into(),
            })
        })
        .collect();
    Document {
        name: format!("Large_{count}"),
        nodes,
        edges,
    }
}
