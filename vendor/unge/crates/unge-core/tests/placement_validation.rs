use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use unge_core::*;

fn fixture() -> Document {
    let mut editor = Editor::new(Document::default(), 0).unwrap();
    for n in 1..=3 {
        let port = Port {
            name: "value".into(),
            data_type: DataType::Float,
            cardinality: Cardinality::Multiple,
            required: false,
        };
        editor
            .execute(Command::AddNode {
                node: Node {
                    id: Id::from_u128(n),
                    type_id: "test".into(),
                    inputs: vec![port.clone()],
                    outputs: vec![port],
                    properties: Properties::new(),
                },
                rect: Rect::default(),
            })
            .unwrap();
    }
    editor.execute(connect(10, 1, 2)).unwrap();
    editor.document().clone()
}
fn connect(id: u128, from: u128, to: u128) -> Command {
    Command::Connect {
        edge: Edge {
            id: Id::from_u128(id),
            from: Endpoint {
                node: Id::from_u128(from),
                port: "value".into(),
            },
            to: Endpoint {
                node: Id::from_u128(to),
                port: "value".into(),
            },
        },
    }
}
fn moved(id: u128, x: f32) -> Command {
    Command::MoveNode {
        id: Id::from_u128(id),
        rect: Rect {
            x,
            ..Rect::default()
        },
    }
}
struct FullValidation;
impl DocumentValidator for FullValidation {
    fn validate(&self, document: &Document) -> Result<()> {
        document.validate()
    }
}
fn assert_same(actual: &Editor, reference: &Editor) {
    assert_eq!(actual.document(), reference.document());
    assert_eq!(actual.revision(), reference.revision());
    assert_eq!(actual.history_stats(), reference.history_stats());
    actual.document().validate().unwrap();
}
#[test]
fn placement_edits_and_history_match_full_validation_through_repeated_nested_batches() {
    let doc = fixture();
    let mut actual = Editor::new(doc.clone(), 8).unwrap();
    let mut reference = Editor::new(doc, 8)
        .unwrap()
        .with_validator(Arc::new(FullValidation))
        .unwrap();
    for i in 0..20 {
        let command = Command::Batch {
            commands: vec![
                moved(1, i as f32),
                Command::Batch {
                    commands: vec![
                        moved(2, -(i as f32)),
                        moved(1, i as f32 + 1.),
                        Command::Batch { commands: vec![] },
                    ],
                },
            ],
        };
        let (_, times) = actual.execute_timed(command.clone()).unwrap();
        reference.execute(command).unwrap();
        assert!(!times.full_document_validation);
        let phases = [
            times.apply_ms,
            times.core_validation_ms,
            times.host_validation_ms,
            times.history_ms,
        ];
        assert!(phases.iter().all(|v| v.is_finite() && *v >= 0.));
        assert!(times.total_ms >= phases.iter().sum::<f64>() - 1e-9);
        assert_same(&actual, &reference);
        assert_eq!(actual.undo().unwrap(), reference.undo().unwrap());
        assert_same(&actual, &reference);
        assert_eq!(actual.redo().unwrap(), reference.redo().unwrap());
        assert_same(&actual, &reference);
    }
    while actual.undo().unwrap() {
        assert!(reference.undo().unwrap());
        assert_same(&actual, &reference);
    }
    assert!(!reference.undo().unwrap());
    while actual.redo().unwrap() {
        assert!(reference.redo().unwrap());
        assert_same(&actual, &reference);
    }
    assert!(!reference.redo().unwrap());
}
#[test]
fn invalid_moves_and_mixed_graph_edits_roll_back_without_losing_redo() {
    let mut actual = Editor::new(fixture(), 16).unwrap();
    actual.execute(moved(1, 10.)).unwrap();
    actual.undo().unwrap();
    let before = actual.document().clone();
    let revision = actual.revision();
    let history = actual.history_stats();
    let bad_group = Command::SetGroup {
        id: Id::from_u128(20),
        group: Some(Group {
            id: Id::from_u128(20),
            label: "bad".into(),
            nodes: [Id::from_u128(999)].into(),
        }),
    };
    let invalid = [
        moved(1, f32::NAN),
        moved(1, 1.0e10),
        moved(999, 3.),
        connect(11, 2, 1),
        bad_group,
    ];
    for command in invalid {
        assert!(
            actual
                .execute_timed(Command::Batch {
                    commands: vec![
                        moved(1, 50.),
                        Command::Batch {
                            commands: vec![moved(2, 20.), command]
                        },
                    ]
                })
                .is_err()
        );
        assert_eq!(actual.document(), &before);
        assert_eq!(actual.revision(), revision);
        assert_eq!(actual.history_stats(), history);
        actual.document().validate().unwrap();
    }
    assert!(actual.redo().unwrap());
    assert_eq!(actual.document().placement()[&Id::from_u128(1)].x, 10.);
    let (_, times) = actual
        .execute_timed(Command::Batch {
            commands: vec![
                moved(1, 20.),
                Command::SetProperty {
                    id: Id::from_u128(1),
                    key: "value".into(),
                    value: Some(42.into()),
                },
            ],
        })
        .unwrap();
    assert!(times.full_document_validation);
}
#[test]
fn host_validator_still_checks_placement_execute_undo_and_redo() {
    struct Gate {
        blocked: Arc<AtomicBool>,
        calls: Arc<AtomicUsize>,
    }
    impl DocumentValidator for Gate {
        fn validate(&self, document: &Document) -> Result<()> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.blocked.load(Ordering::SeqCst)
                || document.placement().values().any(|r| r.x > 100.)
            {
                Err(Error::Invalid("host placement rule".into()))
            } else {
                Ok(())
            }
        }
    }
    let blocked = Arc::new(AtomicBool::new(false));
    let calls = Arc::new(AtomicUsize::new(0));
    let mut e = Editor::new(fixture(), 16)
        .unwrap()
        .with_validator(Arc::new(Gate {
            blocked: blocked.clone(),
            calls: calls.clone(),
        }))
        .unwrap();
    assert!(e.execute_timed(moved(1, 101.)).is_err());
    assert_eq!(e.document().placement()[&Id::from_u128(1)].x, 0.);
    e.execute(moved(1, 20.)).unwrap();
    e.execute(moved(1, 30.)).unwrap();
    e.undo().unwrap();
    let before = e.document().clone();
    let revision = e.revision();
    let history = e.history_stats();
    let count = calls.load(Ordering::SeqCst);
    blocked.store(true, Ordering::SeqCst);
    assert!(e.execute(moved(1, 40.)).is_err());
    assert!(
        e.execute_timed(Command::Batch {
            commands: vec![moved(1, 50.), moved(2, 10.)]
        })
        .is_err()
    );
    assert!(e.undo().is_err());
    assert!(e.redo().is_err());
    assert_eq!(calls.load(Ordering::SeqCst), count + 4);
    assert_eq!(e.document(), &before);
    assert_eq!(e.revision(), revision);
    assert_eq!(e.history_stats(), history);
    blocked.store(false, Ordering::SeqCst);
    assert!(e.redo().unwrap());
}

#[test]
fn transition_context_is_delivered_only_after_core_checks_and_rolls_back_rejections() {
    use std::sync::Mutex;
    struct Audit {
        scopes: Arc<Mutex<Vec<bool>>>,
        blocked: Arc<AtomicBool>,
    }
    impl DocumentValidator for Audit {
        fn validate(&self, document: &Document) -> Result<()> {
            document.validate()
        }
        fn validate_transition(&self, edit: ValidatedEdit<'_>) -> Result<()> {
            edit.document().validate().unwrap();
            self.scopes.lock().unwrap().push(edit.placement_only());
            if self.blocked.load(Ordering::SeqCst) {
                Err(Error::Invalid("rejected transition".into()))
            } else {
                Ok(())
            }
        }
    }
    let scopes = Arc::new(Mutex::new(Vec::new()));
    let blocked = Arc::new(AtomicBool::new(false));
    let mut editor = Editor::new(fixture(), 16)
        .unwrap()
        .with_validator(Arc::new(Audit {
            scopes: scopes.clone(),
            blocked: blocked.clone(),
        }))
        .unwrap();
    editor
        .execute(Command::Batch {
            commands: vec![moved(1, 20.), Command::Batch { commands: vec![] }],
        })
        .unwrap();
    editor
        .execute(Command::Batch {
            commands: vec![
                moved(2, 30.),
                Command::SetProperty {
                    id: Id::from_u128(1),
                    key: "value".into(),
                    value: Some(4.into()),
                },
            ],
        })
        .unwrap();
    editor.undo().unwrap();
    assert_eq!(*scopes.lock().unwrap(), vec![true, false, false]);
    assert!(editor.execute(connect(11, 2, 1)).is_err());
    assert!(editor.execute(moved(1, f32::NAN)).is_err());
    assert_eq!(scopes.lock().unwrap().len(), 3);
    let doc = editor.document().clone();
    let rev = editor.revision();
    let history = editor.history_stats();
    blocked.store(true, Ordering::SeqCst);
    assert!(editor.execute(moved(1, 40.)).is_err());
    assert!(editor.undo().is_err());
    assert!(editor.redo().is_err());
    assert_eq!(
        *scopes.lock().unwrap(),
        vec![true, false, false, true, true, false]
    );
    assert_eq!(editor.document(), &doc);
    assert_eq!(editor.revision(), rev);
    assert_eq!(editor.history_stats(), history);
}
