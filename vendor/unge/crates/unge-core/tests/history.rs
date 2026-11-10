use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use unge_core::*;

fn node() -> Node {
    Node {
        id: Id::new_v4(),
        type_id: "test".into(),
        inputs: vec![],
        outputs: vec![],
        properties: Properties::new(),
    }
}
fn add(node: Node) -> Command {
    Command::AddNode {
        node,
        rect: Rect::default(),
    }
}
fn set(id: Id, value: &str) -> Command {
    Command::SetProperty {
        id,
        key: "text".into(),
        value: Some(value.into()),
    }
}
fn editor(bytes: usize, steps: usize) -> Editor {
    Editor::with_history_limits(
        Document::default(),
        HistoryLimits {
            max_bytes: bytes,
            max_steps: steps,
        },
    )
    .unwrap()
}
#[test]
fn byte_budget_evicts_oldest_and_limits_undo_and_redo_together() {
    let mut e = editor(500, 100);
    let n = node();
    let id = n.id;
    e.execute(add(n)).unwrap();
    for value in ["a", "b", "c", "d", "e", "f", "g", "h"] {
        e.execute(set(id, &value.repeat(70))).unwrap();
        assert!(e.history_stats().bytes <= 500);
    }
    let count = e.history_stats().undo_steps;
    assert!((1..8).contains(&count));
    for _ in 0..count {
        assert!(e.undo().unwrap());
        assert!(e.history_stats().bytes <= 500);
    }
    assert!(!e.undo().unwrap());
    assert_eq!(e.history_stats().redo_steps, count);
    for _ in 0..count {
        assert!(e.redo().unwrap());
    }
    assert_eq!(
        e.document().graph().nodes()[&id].properties["text"],
        "h".repeat(70)
    );
    assert!(e.history_stats().bytes <= 500);
}
#[test]
fn oversized_inverse_is_a_history_barrier_and_future_edits_remain_undoable() {
    let mut e = editor(500, 100);
    let n = node();
    let id = n.id;
    e.execute(add(n)).unwrap();
    e.execute(set(id, &"界".repeat(1000))).unwrap();
    // The deletion's inverse contains the large old value. Do not undo past it.
    e.execute(set(id, "small")).unwrap();
    assert_eq!(
        e.history_stats(),
        HistoryStats {
            undo_steps: 0,
            redo_steps: 0,
            bytes: 0
        }
    );
    assert!(!e.undo().unwrap());
    assert_eq!(
        e.document().graph().nodes()[&id].properties["text"],
        "small"
    );
    e.execute(set(id, "next")).unwrap();
    assert!(e.undo().unwrap());
    assert_eq!(
        e.document().graph().nodes()[&id].properties["text"],
        "small"
    );
}
#[test]
fn inverse_can_grow_during_undo_and_must_still_obey_budget() {
    let mut e = editor(100, 4);
    let mut n = node();
    n.properties.insert("text".into(), "x".repeat(2000).into());
    e.execute(add(n)).unwrap(); // RemoveNode inverse is small.
    assert_eq!(e.history_stats().undo_steps, 1);
    assert!(e.undo().unwrap()); // AddNode inverse is now too large for redo.
    assert_eq!(e.history_stats().bytes, 0);
    assert!(!e.redo().unwrap());
    assert!(e.document().graph().nodes().is_empty());
}
#[test]
fn new_edit_clears_redo_bytes_and_zero_limits_disable_history() {
    for (bytes, steps) in [(0, 10), (1000, 0)] {
        let mut e = editor(bytes, steps);
        e.execute(add(node())).unwrap();
        assert_eq!(e.history_stats().bytes, 0);
        assert!(!e.undo().unwrap());
    }
    let mut e = editor(2000, 2);
    let n = node();
    let id = n.id;
    e.execute(add(n)).unwrap();
    e.execute(set(id, "first")).unwrap();
    e.undo().unwrap();
    assert_eq!(e.history_stats().redo_steps, 1);
    e.execute(set(id, "other")).unwrap();
    assert_eq!(e.history_stats().redo_steps, 0);
    assert_eq!(e.history_stats().undo_steps, 2);
    assert!(!e.redo().unwrap());
    e.undo().unwrap();
    e.undo().unwrap();
    assert!(!e.undo().unwrap());
}
#[test]
fn validator_failure_preserves_document_revision_and_both_history_stacks() {
    // Simulates a rejecting host validator, including during undo and redo.
    struct Gate(Arc<AtomicBool>);
    impl DocumentValidator for Gate {
        fn validate(&self, _: &Document) -> Result<()> {
            if self.0.load(Ordering::SeqCst) {
                Err(Error::Invalid("blocked".into()))
            } else {
                Ok(())
            }
        }
    }
    let blocked = Arc::new(AtomicBool::new(false));
    let mut e = editor(5000, 10)
        .with_validator(Arc::new(Gate(blocked.clone())))
        .unwrap();
    let n = node();
    let id = n.id;
    e.execute(add(n)).unwrap();
    e.execute(set(id, "first")).unwrap();
    e.undo().unwrap();
    let before = e.document().clone();
    let revision = e.revision();
    let stats = e.history_stats();
    blocked.store(true, Ordering::SeqCst);
    assert!(e.execute(set(id, "rejected")).is_err());
    assert!(e.undo().is_err());
    assert!(e.redo().is_err());
    assert_eq!(e.document(), &before);
    assert_eq!(e.revision(), revision);
    assert_eq!(e.history_stats(), stats);
    blocked.store(false, Ordering::SeqCst);
    assert!(e.redo().unwrap());
    assert_eq!(
        e.document().graph().nodes()[&id].properties["text"],
        "first"
    );
}
