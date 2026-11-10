use std::collections::BTreeSet;
use unge_core::*;
fn node() -> Node {
    let port = Port {
        name: "value".into(),
        data_type: DataType::Float,
        cardinality: Cardinality::Single,
        required: true,
    };
    Node {
        id: Id::new_v4(),
        type_id: "test".into(),
        inputs: vec![port.clone()],
        outputs: vec![port],
        properties: Properties::new(),
    }
}
fn add(node: Node) -> Command {
    Command::AddNode {
        node,
        rect: Rect::default(),
    }
}
fn connect(a: Id, b: Id) -> Command {
    Command::Connect {
        edge: Edge {
            id: Id::new_v4(),
            from: Endpoint {
                node: a,
                port: "value".into(),
            },
            to: Endpoint {
                node: b,
                port: "value".into(),
            },
        },
    }
}
fn editor() -> Editor {
    Editor::new(Document::default(), 10).unwrap()
}
#[test]
fn transaction_rollback_and_history_are_atomic() {
    let mut editor = editor();
    let a = node();
    let id = a.id;
    let original = editor.document().to_json().unwrap();
    assert!(
        editor
            .execute(Command::Batch {
                commands: vec![add(a.clone()), add(a.clone())]
            })
            .is_err()
    );
    assert_eq!(editor.document().to_json().unwrap(), original);
    assert_eq!(editor.revision(), 0);
    editor
        .execute(Command::Batch {
            commands: vec![
                add(a),
                Command::SetProperty {
                    id,
                    key: "x".into(),
                    value: Some(7.into()),
                },
            ],
        })
        .unwrap();
    let saved = editor.document().to_json().unwrap();
    assert!(editor.undo().unwrap());
    assert!(editor.document().graph().nodes().is_empty());
    assert!(editor.redo().unwrap());
    assert_eq!(editor.document().to_json().unwrap(), saved);
}
#[test]
fn invalid_type_cycle_and_cardinality_restore_document() {
    let mut editor = editor();
    let a = node();
    let b = node();
    let c = node();
    let (aid, bid, cid) = (a.id, b.id, c.id);
    editor
        .execute(Command::Batch {
            commands: vec![add(a), add(b), add(c), connect(aid, bid)],
        })
        .unwrap();
    let before = editor.document().to_json().unwrap();
    assert!(editor.execute(connect(bid, aid)).is_err());
    assert!(editor.execute(connect(cid, bid)).is_err());
    let mut wrong = node();
    wrong.inputs[0].data_type = DataType::Image;
    let wid = wrong.id;
    assert!(
        editor
            .execute(Command::Batch {
                commands: vec![add(wrong), connect(aid, wid)]
            })
            .is_err()
    );
    assert_eq!(before, editor.document().to_json().unwrap());
}
#[test]
fn deleting_node_restores_edges_groups_and_position() {
    let mut editor = editor();
    let a = node();
    let b = node();
    let (aid, bid) = (a.id, b.id);
    let gid = Id::new_v4();
    editor
        .execute(Command::Batch {
            commands: vec![
                add(a),
                add(b),
                connect(aid, bid),
                Command::SetGroup {
                    id: gid,
                    group: Some(Group {
                        id: gid,
                        label: "group".into(),
                        nodes: BTreeSet::from([aid, bid]),
                    }),
                },
            ],
        })
        .unwrap();
    let saved = editor.document().to_json().unwrap();
    editor.execute(Command::RemoveNode { id: aid }).unwrap();
    assert!(editor.document().graph().edges().is_empty());
    assert_eq!(
        editor.document().graph().groups()[&gid].nodes,
        BTreeSet::from([bid])
    );
    editor.undo().unwrap();
    assert_eq!(saved, editor.document().to_json().unwrap());
}
#[test]
fn paste_remaps_internal_connections() {
    let mut editor = editor();
    let a = node();
    let b = node();
    let (aid, bid) = (a.id, b.id);
    editor
        .execute(Command::Batch {
            commands: vec![add(a), add(b), connect(aid, bid)],
        })
        .unwrap();
    let fragment = Fragment::copy(editor.document(), &BTreeSet::from([aid, bid]));
    editor.execute(fragment.paste([200., 100.])).unwrap();
    assert_eq!(editor.document().graph().nodes().len(), 4);
    assert_eq!(editor.document().graph().edges().len(), 2);
    let added = editor
        .document()
        .graph()
        .edges()
        .values()
        .find(|e| e.from.node != aid)
        .unwrap();
    assert_ne!(added.to.node, bid);
    editor.undo().unwrap();
    assert_eq!(editor.document().graph().nodes().len(), 2);
}
#[test]
fn schema_and_broken_imports_are_rejected() {
    let mut value = serde_json::to_value(Document::default()).unwrap();
    value["schema_version"] = 99.into();
    assert!(matches!(
        Document::from_json(&serde_json::to_vec(&value).unwrap()),
        Err(Error::Version(99))
    ));
    let doc = Document::default();
    assert_eq!(
        doc,
        Document::from_json(doc.to_json().unwrap().as_bytes()).unwrap()
    );
}
#[test]
fn spatial_queries_layout_and_zoom_anchor() {
    let mut editor = editor();
    let a = node();
    let b = node();
    let (aid, bid) = (a.id, b.id);
    editor
        .execute(Command::Batch {
            commands: vec![add(a), add(b), connect(aid, bid)],
        })
        .unwrap();
    editor
        .execute(auto_layout(editor.document(), [100., 40.]).unwrap())
        .unwrap();
    assert_eq!(editor.document().placement()[&bid].x, 280.);
    let index = SpatialIndex::new(editor.document());
    assert_eq!(index.hit_test([10., 10.]), Some(aid));
    assert!(
        index
            .query(Rect {
                x: 1000.,
                y: 1000.,
                width: 100.,
                height: 100.
            })
            .is_empty()
    );
    let mut viewport = Viewport {
        origin: [20., 30.],
        zoom: 1.,
        size: [800., 600.],
    };
    let anchor = viewport.to_world([350., 240.]);
    viewport.zoom_at([350., 240.], 2.);
    assert_eq!(viewport.to_world([350., 240.]), anchor);
}
#[test]
fn new_edit_discards_redo_and_history_is_bounded() {
    let mut editor = Editor::new(Document::default(), 1).unwrap();
    editor.execute(add(node())).unwrap();
    editor.execute(add(node())).unwrap();
    assert!(editor.undo().unwrap());
    assert!(!editor.undo().unwrap());
    editor.execute(add(node())).unwrap();
    assert!(!editor.redo().unwrap());
}
#[test]
fn downstream_and_layers_are_consistent() {
    let mut editor = editor();
    let a = node();
    let b = node();
    let c = node();
    let (aid, bid, cid) = (a.id, b.id, c.id);
    editor
        .execute(Command::Batch {
            commands: vec![add(a), add(b), add(c), connect(aid, bid), connect(bid, cid)],
        })
        .unwrap();
    let graph = editor.document().graph();
    assert_eq!(
        graph.layers().unwrap(),
        vec![vec![aid], vec![bid], vec![cid]]
    );
    assert_eq!(
        GraphIndex::new(graph).downstream(graph, bid),
        BTreeSet::from([bid, cid])
    );
}

#[test]
fn workspace_roundtrip_preserves_independent_documents() {
    let mut workspace = Workspace::default();
    let a = workspace.insert(Document::default(), 10).unwrap();
    let b = workspace.insert(Document::default(), 10).unwrap();
    workspace
        .editor_mut(a)
        .unwrap()
        .execute(add(node()))
        .unwrap();
    assert_eq!(
        workspace
            .editor(b)
            .unwrap()
            .document()
            .graph()
            .nodes()
            .len(),
        0
    );
    let json = serde_json::to_vec(&workspace.to_file()).unwrap();
    let loaded = Workspace::from_file(serde_json::from_slice(&json).unwrap(), 10).unwrap();
    assert_eq!(
        loaded.editor(a).unwrap().document().graph().nodes().len(),
        1
    );
    assert_eq!(
        loaded.editor(b).unwrap().document().graph().nodes().len(),
        0
    );
}

#[test]
fn gpu_coordinate_overflow_is_rejected_without_mutation() {
    let mut editor = editor();
    let before = editor.document().to_json().unwrap();
    assert!(
        editor
            .execute(Command::AddNode {
                node: node(),
                rect: Rect {
                    x: f32::MAX,
                    y: 0.,
                    width: 100.,
                    height: 100.
                }
            })
            .is_err()
    );
    assert_eq!(before, editor.document().to_json().unwrap());
    assert!(
        Viewport {
            origin: [0., 0.],
            zoom: 0.02,
            size: [f32::MAX, 100.]
        }
        .validate()
        .is_err()
    );
}
