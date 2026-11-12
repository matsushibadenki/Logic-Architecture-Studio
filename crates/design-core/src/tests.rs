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
    assert_eq!(first.node("add").unwrap().x, 400.);
    let value = serde_json::to_value(&w).unwrap();
    assert!(value["document"]["nodes"].is_array());
    assert!(value["redo"][0]["nodes"].is_array());
    let decoded: Workspace = serde_json::from_value(value).unwrap();
    assert_eq!(w, decoded);
}

#[test]
fn unsigned_inputs_reject_implicit_truncation_but_arithmetic_wraps() {
    let doc = Document::demo();
    let inputs = BTreeMap::from([
        ("a".into(), u64::from(u32::MAX)),
        ("b".into(), 1),
        ("op".into(), 0),
    ]);
    assert_eq!(doc.evaluate(&inputs).unwrap()["result"], 0);
    let mut bad = inputs.clone();
    bad.insert("a".into(), u64::from(u32::MAX) + 1);
    assert!(doc.evaluate(&bad).unwrap_err().contains("unsigned 32-bit"));
    bad = inputs;
    bad.insert("op".into(), 4);
    assert!(doc.evaluate(&bad).unwrap_err().contains("unsigned 2-bit"));
}

#[test]
fn auto_arrange_avoids_overlap_with_square_and_tall_nodes_in_same_column() {
    let mut workspace = Workspace::default();
    Arc::make_mut(&mut workspace.document).nodes.push(Node {
        id: "extra_mux".into(),
        name: "extra_mux".into(),
        kind: Kind::Mux,
        width: 32,
        x: 0.,
        y: 0.,
    });
    workspace
        .dispatch(
            "layout.auto_arrange",
            json!({"expected_revision":0}),
            "test",
        )
        .unwrap();
    for (index, a) in workspace.document.nodes.iter().enumerate() {
        for b in workspace.document.nodes.iter().skip(index + 1) {
            assert!(
                a.x + NODE_WIDTH <= b.x
                    || b.x + NODE_WIDTH <= a.x
                    || a.y + node_height(&a.kind) <= b.y
                    || b.y + node_height(&b.kind) <= a.y,
                "{} overlaps {}",
                a.id,
                b.id
            );
        }
    }
    workspace.document.validate(false).unwrap();
}

#[test]
fn multiple_selection_survives_group_move_undo_and_preferences() {
    let mut w = Workspace::default();
    let before = w.document.clone();
    w.dispatch(
        "view.select_nodes",
        json!({"expected_revision":0,"ids":["b","a"]}),
        "test",
    )
    .unwrap();
    assert_eq!(w.selected_nodes(), vec!["a", "b"]);
    assert_eq!(w.revision, 0);
    for bad in [json!(["a", "missing"]), json!(["a", "a"])] {
        assert!(w
            .dispatch(
                "view.select_nodes",
                json!({"expected_revision":0,"ids":bad}),
                "test"
            )
            .is_err());
        assert_eq!(w.selected_nodes(), vec!["a", "b"]);
    }
    let ops = json!([{"op":"update_node","id":"a","x":80,"y":100}, {"op":"update_node","id":"b","x":80,"y":420}]);
    w.dispatch("design.apply_patch", patch(&w, ops), "test")
        .unwrap();
    assert_eq!(w.revision, 1);
    w.dispatch("view.preferences", json!({"locale":"ja"}), "test")
        .unwrap();
    let persisted: Workspace = serde_json::from_value(serde_json::to_value(&w).unwrap()).unwrap();
    assert_eq!(persisted.selected_nodes(), vec!["a", "b"]);
    w.dispatch("design.undo", json!({"expected_revision":1}), "test")
        .unwrap();
    assert_eq!(w.document, before);
    assert_eq!(w.selected_nodes(), vec!["a", "b"]);
    assert!(w
        .dispatch(
            "view.select_nodes",
            json!({"expected_revision":0,"ids":[]}),
            "test"
        )
        .is_err());
    w.dispatch(
        "view.select_nodes",
        json!({"expected_revision":2,"ids":[]}),
        "test",
    )
    .unwrap();
    assert!(w.selected_nodes().is_empty());
    assert!(w.selected.is_none());
    // Workspaces saved before multiple selection still restore their focused node.
    let mut old = serde_json::to_value(Workspace::default()).unwrap();
    old.as_object_mut().unwrap().remove("selection");
    assert_eq!(
        serde_json::from_value::<Workspace>(old)
            .unwrap()
            .selected_nodes(),
        vec!["add"]
    );
}

#[test]
fn appearance_preferences_are_atomic_and_independent_of_design_history() {
    let mut w = Workspace::default();
    let document = w.document.clone();
    let history = w.history.clone();
    w.dispatch_view(
        "view.preferences",
        json!({"locale":"zh","node_colors":{"add":"#FF1234"}}),
        "test",
    )
    .unwrap();
    assert_eq!(w.node_colors["add"], "#ff1234");
    assert_eq!(w.node_colors["input"], default_node_colors()["input"]);
    let before = serde_json::to_value(&w).unwrap();
    for args in [
        json!({"locale":"ja","node_colors":{"add":"#fff"}}),
        json!({"locale":"ja","node_colors":{"unknown":"#123456"}}),
        json!({"locale":"ja","reset_colors":true,"node_colors":{"add":"#é1234"}}),
        json!({"locale":"invalid","node_colors":{"add":"#123456"}}),
        json!({"locale":"ja","node_colors":{"add":42}}),
    ] {
        assert!(w.dispatch_view("view.preferences", args, "test").is_err());
        assert_eq!(serde_json::to_value(&w).unwrap(), before);
    }
    let restored: Workspace = serde_json::from_value(before.clone()).unwrap();
    assert_eq!(restored.node_colors, w.node_colors);
    let mut legacy = before;
    legacy.as_object_mut().unwrap().remove("node_colors");
    assert_eq!(
        serde_json::from_value::<Workspace>(legacy)
            .unwrap()
            .node_colors,
        default_node_colors()
    );
    w.dispatch_view(
        "view.preferences",
        json!({"node_colors":{"add":null,"register":"#123456"}}),
        "test",
    )
    .unwrap();
    assert_eq!(w.node_colors["add"], default_node_colors()["add"]);
    assert_eq!(w.node_colors["register"], "#123456");
    w.dispatch_view("view.preferences", json!({"reset_colors":true}), "test")
        .unwrap();
    assert_eq!(w.node_colors, default_node_colors());
    assert_eq!(w.revision, 0);
    assert!(Arc::ptr_eq(&w.document, &document));
    assert_eq!(
        serde_json::to_value(&w.history).unwrap(),
        serde_json::to_value(history).unwrap()
    );
}
