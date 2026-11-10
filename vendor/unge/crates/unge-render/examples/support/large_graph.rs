use unge_core::*;
pub fn document() -> Document {
    let ids: Vec<_> = (1..=10_000).map(Id::from_u128).collect();
    let port = Port {
        name: "value".into(),
        data_type: DataType::Float,
        cardinality: Cardinality::Multiple,
        required: false,
    };
    let mut commands: Vec<_> = ids
        .iter()
        .enumerate()
        .map(|(i, id)| Command::AddNode {
            node: Node {
                id: *id,
                type_id: "benchmark".into(),
                inputs: vec![port.clone()],
                outputs: vec![port.clone()],
                properties: Properties::new(),
            },
            rect: Rect {
                x: (i % 100) as f32 * 240.0,
                y: (i / 100) as f32 * 140.0,
                width: 180.0,
                height: 90.0,
            },
        })
        .collect();
    let mut edges = 0;
    'offsets: for offset in 1..ids.len() {
        for i in 0..ids.len() - offset {
            commands.push(Command::Connect {
                edge: Edge {
                    id: Id::from_u128(100_000 + edges),
                    from: Endpoint {
                        node: ids[i],
                        port: "value".into(),
                    },
                    to: Endpoint {
                        node: ids[i + offset],
                        port: "value".into(),
                    },
                },
            });
            edges += 1;
            if edges == 30_000 {
                break 'offsets;
            }
        }
    }
    let mut editor = Editor::new(Document::default(), 0).unwrap();
    editor.execute(Command::Batch { commands }).unwrap();
    editor.document().clone()
}
