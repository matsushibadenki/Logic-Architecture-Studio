use super::*;
fn patch(w: &Workspace, operations: Value) -> Value {
    json!({"expected_revision":w.revision,"label":"test","operations":operations})
}
#[test]
fn demo_contract_and_hdl() {
    let doc = Document::demo();
    let r = verify(&doc, 0, &alu_vectors(&doc).unwrap()).unwrap();
    assert_eq!(r.passed, 320);
    assert!(doc.rtl().unwrap().contains("assign n_add = n_a + n_b;"));
}
#[test]
fn atomic_patch_and_conflict() {
    let mut w = Workspace::default();
    let before = w.document.clone();
    let p = patch(&w, json!([{"op":"update_node","id":"add","width":8}]));
    assert!(w.dispatch("design.apply_patch", p, "test").is_err());
    assert_eq!(w.document, before);
    assert_eq!(w.revision, 0);
    let p = patch(&w, json!([{"op":"update_node","id":"add","name":"sum"}]));
    w.dispatch("design.apply_patch", p.clone(), "test").unwrap();
    assert!(w.dispatch("design.apply_patch", p, "test").is_err());
    assert_eq!(w.revision, 1);
}
#[test]
fn cycles_and_multiple_drivers_rejected() {
    let mut doc = Document::demo();
    doc.edges.push(Edge {
        id: "bad".into(),
        from: "add".into(),
        to: "add".into(),
        port: "a".into(),
    });
    assert!(doc.validate(false).is_err());
    doc.edges.retain(|e| e.id != "a_add");
    assert!(doc.validate(false).unwrap_err().contains("cycle"));
}
#[test]
fn undo_redo_revisions_monotonic() {
    let mut w = Workspace::default();
    let p = patch(&w, json!([{"op":"remove_node","id":"result"}]));
    w.dispatch("design.apply_patch", p, "test").unwrap();
    w.dispatch("design.undo", json!({"expected_revision":1}), "test")
        .unwrap();
    assert_eq!(w.document.nodes.len(), 8);
    w.dispatch("design.redo", json!({"expected_revision":2}), "test")
        .unwrap();
    assert_eq!(w.revision, 3);
    assert_eq!(w.document.nodes.len(), 7);
}
#[test]
fn mutation_fails_independent_contract() {
    let mut doc = Document::demo();
    doc.nodes.iter_mut().find(|n| n.id == "add").unwrap().kind = Kind::Sub;
    let r = verify(&doc, 1, &alu_vectors(&doc).unwrap()).unwrap();
    assert!(r.passed < r.total);
}
#[test]
fn sixty_four_bits_preserved() {
    let mut d = Document::demo();
    for n in &mut d.nodes {
        if n.width == 32 {
            n.width = 64;
        }
    }
    let r = verify(&d, 0, &alu_vectors(&d).unwrap()).unwrap();
    assert_eq!(r.passed, r.total);
}
#[test]
fn proposal_does_not_mutate() {
    let mut w = Workspace::default();
    let p = patch(&w, json!([{"op":"update_node","id":"add","name":"sum"}]));
    w.dispatch("design.propose_patch", p, "mcp").unwrap();
    assert_eq!(w.revision, 0);
    assert_eq!(w.document.node("add").unwrap().name, "add");
}
#[test]
fn incomplete_design_cannot_export() {
    let mut d = Document::demo();
    d.edges.pop();
    assert!(d.validate(false).is_ok());
    assert!(d.rtl().is_err());
}
#[test]
fn experiments_are_isolated_and_stale_adoption_fails() {
    let mut w = Workspace::default();
    w.dispatch(
        "experiment.create",
        json!({"expected_revision":0,"name":"test","variants":[{"name":"base","operations":[]}]}),
        "test",
    )
    .unwrap();
    w.dispatch("experiment.run", json!({"id":"exp_1"}), "test")
        .unwrap();
    assert_eq!(w.revision, 0);
    w.dispatch(
        "layout.auto_arrange",
        json!({"expected_revision":0}),
        "test",
    )
    .unwrap();
    assert!(w
        .dispatch(
            "experiment.adopt",
            json!({"expected_revision":1,"id":"exp_1","candidate_id":"candidate_0"}),
            "test"
        )
        .is_err());
}

#[test]
fn mcp_schema_rejects_malformed_nested_patch() {
    let schema = protocol::tools()
        .into_iter()
        .find(|t| t["name"] == "design.apply_patch")
        .unwrap()["inputSchema"]
        .clone();
    let good = json!({"expected_revision":0,"label":"connect","operations":[{"op":"connect","edge":{"id":"e","from":"a","to":"add","port":"a"}}]});
    assert!(protocol::validate(&schema, &good).is_ok());
    let mut bad = good.clone();
    bad["operations"][0]["edge"]["from"] = json!(12);
    assert!(protocol::validate(&schema, &bad).is_err());
    let mut bad = good;
    bad["operations"][0]["edge"]["port"] = json!("unknown");
    assert!(protocol::validate(&schema, &bad).is_err());
}

#[test]
fn project_import_is_validated_revision_checked_and_undoable() {
    let mut w = Workspace::default();
    let original = w.document.clone();
    let exported = w.dispatch("project.export", json!({}), "test").unwrap();
    let mut changed = exported.clone();
    changed["document"]["name"] = json!("Imported");
    w.dispatch(
        "project.import",
        json!({"expected_revision":0,"project":changed}),
        "test",
    )
    .unwrap();
    assert_eq!(w.document.name, "Imported");
    assert!(w
        .dispatch(
            "project.import",
            json!({"expected_revision":0,"project":exported}),
            "test"
        )
        .is_err());
    let before = w.document.clone();
    assert!(w
        .dispatch(
            "project.import",
            json!({"expected_revision":1,"project":{"schema":99}}),
            "test"
        )
        .is_err());
    assert_eq!(before, w.document);
    w.dispatch("design.undo", json!({"expected_revision":1}), "test")
        .unwrap();
    assert_eq!(w.document, original);
}
#[test]
fn combined_history_limit_keeps_nearest_snapshots() {
    let mut w = Workspace::default();
    for i in 0..120 {
        let mut d = Document::demo();
        d.name = format!("Version{i}");
        w.undo.push(Arc::new(d));
    }
    w.limit_history();
    assert_eq!(w.undo.len(), 100);
    assert_eq!(w.undo.first().unwrap().name, "Version20");
    assert_eq!(w.undo.last().unwrap().name, "Version119");
}

#[test]
fn compact_position_path_is_atomic_revision_checked_and_reloadable() {
    let mut w = Workspace::default();
    for operations in [
        json!([{"op":"update_node","id":"add","x":100001}]),
        json!([{"op":"update_node","id":"add","x":99},{"op":"update_node","id":"missing","y":1}]),
    ] {
        let before = w.clone();
        let p = patch(&w, operations);
        assert!(w.dispatch_view("design.apply_patch", p, "test").is_err());
        assert_eq!(w, before);
    }
    for i in 0..110 {
        let p = patch(&w, json!([{"op":"update_node","id":"add","x":i,"y":-i}]));
        let ack = w
            .dispatch_view("design.apply_patch", p.clone(), "test")
            .unwrap();
        assert_eq!(ack, json!({"revision":i+1,"committed":true}));
        assert!(w.dispatch_view("design.apply_patch", p, "test").is_err());
        assert!(w.document.validate(false).is_ok());
        let mut counted = w.clone();
        counted.limit_history();
        assert_eq!(counted.undo, w.undo);
        assert_eq!(counted.redo, w.redo);
    }
    assert_eq!(w.undo.len(), 100);
    let mut reloaded: Workspace =
        serde_json::from_value(serde_json::to_value(&w).unwrap()).unwrap();
    for method in ["design.undo", "design.undo", "design.redo"] {
        let args = json!({"expected_revision":w.revision});
        w.dispatch_view(method, args.clone(), "test").unwrap();
        reloaded.dispatch_view(method, args, "test").unwrap();
        assert_eq!(w, reloaded);
    }
}

#[test]
fn transaction_clones_share_snapshots_and_edits_keep_history_immutable() {
    let mut w = Workspace::default();
    let first = w.document.clone();
    let p = patch(&w, json!([{"op":"update_node","id":"add","x":444}]));
    w.dispatch_view("design.apply_patch", p, "test").unwrap();
    assert!(Arc::ptr_eq(&first, &w.undo[0]));
    let transaction = w.clone();
    assert!(Arc::ptr_eq(&transaction.document, &w.document));
    assert!(Arc::ptr_eq(&transaction.undo[0], &w.undo[0]));
    let current = w.document.clone();
    w.dispatch_view("design.undo", json!({"expected_revision":1}), "test")
        .unwrap();
    assert!(Arc::ptr_eq(&w.document, &first));
    assert!(Arc::ptr_eq(&w.redo[0], &current));
    assert_eq!(transaction.document.node("add").unwrap().x, 444.);
    assert_eq!(first.node("add").unwrap().x, 310.);
    let value = serde_json::to_value(&w).unwrap();
    assert!(value["document"]["nodes"].is_array());
    assert!(value["redo"][0]["nodes"].is_array());
    let decoded: Workspace = serde_json::from_value(value).unwrap();
    assert_eq!(w, decoded);
}
