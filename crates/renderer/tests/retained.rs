mod support;
use design_core::{Document, Kind};
use std::collections::BTreeSet;
use studio_renderer::{project, retained::RetainedProjection};
use unge_core::{Locale, Viewport};
use unge_render::{Scene, SceneIndex, SceneUpdate, Theme};

fn equal(a: &Scene, b: &Scene) {
    assert_eq!(
        bytemuck::cast_slice::<_, u8>(&a.quads),
        bytemuck::cast_slice::<_, u8>(&b.quads)
    );
    assert_eq!(a.labels, b.labels);
    assert_eq!(
        (a.visible_nodes, a.visible_edges),
        (b.visible_nodes, b.visible_edges)
    );
}
fn compare(cache: &RetainedProjection, doc: &Document) {
    let (fresh, labels, _) = project(doc).unwrap();
    let index = SceneIndex::new(&fresh);
    for (zoom, locale, theme) in [
        (0.4, Locale::En, Theme::Dark),
        (1., Locale::Ja, Theme::Dark),
        (0.8, Locale::ZhCn, Theme::Light),
    ] {
        let view = Viewport {
            origin: [-30., -30.],
            size: [1600., 1000.],
            zoom,
        };
        let selected = BTreeSet::from([cache.ids[&doc.nodes[0].id]]);
        equal(
            &cache
                .index
                .scene_with_theme(
                    view,
                    &selected,
                    &Default::default(),
                    &cache.labels,
                    locale,
                    theme,
                )
                .unwrap(),
            &index
                .scene_with_theme(view, &selected, &Default::default(), &labels, locale, theme)
                .unwrap(),
        );
    }
}
#[test]
fn delta_matches_full_rebuild_across_edits_import_undo_and_reorder() {
    let base = Document::demo();
    let mut doc = base.clone();
    let mut cache = RetainedProjection::new(&doc).unwrap();
    compare(&cache, &doc);
    let id = cache.ids["add"];
    doc.nodes.iter_mut().find(|n| n.id == "add").unwrap().x += 20.;
    assert_eq!(cache.sync(&doc).unwrap(), SceneUpdate::Incremental);
    compare(&cache, &doc);
    doc.nodes.iter_mut().find(|n| n.id == "add").unwrap().name = "sum".into();
    cache.sync(&doc).unwrap();
    compare(&cache, &doc);
    doc.nodes.iter_mut().find(|n| n.id == "add").unwrap().kind = Kind::Sub;
    cache.sync(&doc).unwrap();
    compare(&cache, &doc);
    doc.edges[0].from = "b".into();
    cache.sync(&doc).unwrap();
    compare(&cache, &doc);
    let connected = doc.clone();
    let removed = doc.edges[0].id.clone();
    doc.edges.retain(|edge| edge.id != removed);
    cache.sync(&doc).unwrap();
    compare(&cache, &doc);
    doc = connected;
    cache.sync(&doc).unwrap();
    compare(&cache, &doc);
    doc.nodes.reverse();
    doc.edges.reverse();
    cache.sync(&doc).unwrap();
    compare(&cache, &doc);
    assert_eq!(cache.ids["add"], id);
    doc.nodes.retain(|n| n.id != "add");
    doc.edges.retain(|e| e.from != "add" && e.to != "add");
    cache.sync(&doc).unwrap();
    compare(&cache, &doc);
    cache.sync(&base).unwrap();
    compare(&cache, &base);
    let mut imported = support::circuit(200);
    cache.sync(&imported).unwrap();
    compare(&cache, &imported);
    for n in &mut imported.nodes {
        n.width = 16;
    }
    cache.sync(&imported).unwrap();
    compare(&cache, &imported);
    cache.sync(&base).unwrap();
    compare(&cache, &base);
}
#[test]
fn large_single_move_regenerates_only_node_and_incident_edges() {
    let mut doc = support::circuit(2000);
    doc.validate(false).unwrap();
    let mut cache = RetainedProjection::new(&doc).unwrap();
    let view = Viewport {
        origin: [-30., -30.],
        size: [1600., 1000.],
        zoom: 0.1,
    };
    let draw = |c: &RetainedProjection| {
        c.index
            .scene_with_labels(
                view,
                &BTreeSet::new(),
                &Default::default(),
                &c.labels,
                Locale::En,
            )
            .unwrap()
    };
    draw(&cache);
    let moved_id = doc.nodes[1000].id.clone();
    doc.nodes.update(&moved_id, |n| n.x += 18.).unwrap();
    assert_eq!(cache.sync(&doc).unwrap(), SceneUpdate::Incremental);
    let scene = draw(&cache);
    assert_eq!(scene.geometry_stats.rebuilt_nodes, 1);
    assert_eq!(scene.geometry_stats.rebuilt_edges, 4);
    assert_eq!(scene.geometry_stats.reused_nodes, 1999);
    let (fresh, labels, _) = project(&doc).unwrap();
    equal(
        &scene,
        &SceneIndex::new(&fresh)
            .scene_with_labels(
                view,
                &BTreeSet::new(),
                &Default::default(),
                &labels,
                Locale::En,
            )
            .unwrap(),
    );
    let idle = draw(&cache);
    assert_eq!(idle.geometry_stats.rebuilt_nodes, 0);
    assert_eq!(idle.geometry_stats.rebuilt_edges, 0);
}

#[test]
fn feedback_lanes_match_during_drag_commit_and_rebuild() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../../examples/logic-gates.json")).unwrap();
    let mut doc: Document = serde_json::from_value(fixture["document"].clone()).unwrap();
    doc.nodes
        .retain(|n| ["g_and", "g_xor", "g_nand"].contains(&n.id.as_str()));
    for (id, x, y) in [
        ("g_and", 1100., 0.),
        ("g_xor", 400., 400.),
        ("g_nand", 0., 400.),
    ] {
        doc.nodes
            .update(id, |n| {
                n.x = x;
                n.y = y;
            })
            .unwrap();
    }
    doc.edges = [("outer", "g_and", "a"), ("inner", "g_xor", "b")]
        .map(|(id, from, port)| design_core::Edge {
            source_port: "y".into(),
            id: id.into(),
            from: from.into(),
            to: "g_nand".into(),
            port: port.into(),
        })
        .to_vec()
        .into();
    let mut cache = RetainedProjection::new(&doc).unwrap();
    let view = Viewport {
        origin: [-100., -100.],
        size: [1600., 1100.],
        zoom: 0.5,
    };
    for moved in ["g_nand", "g_and", "g_xor"] {
        let n = doc.nodes.iter().find(|n| n.id == moved).unwrap();
        let mut preview = unge_interaction::Preview::default();
        preview.placement.insert(
            cache.ids[moved],
            unge_core::Rect {
                x: n.x + 24.,
                y: n.y + 24.,
                width: design_core::NODE_WIDTH,
                height: design_core::node_height(&n.kind),
            },
        );
        let during = cache
            .index
            .scene_with_labels(view, &BTreeSet::new(), &preview, &cache.labels, Locale::En)
            .unwrap();
        doc.nodes
            .update(moved, |n| {
                n.x += 24.;
                n.y += 24.;
            })
            .unwrap();
        assert_eq!(cache.sync(&doc).unwrap(), SceneUpdate::Incremental);
        let after = cache
            .index
            .scene_with_labels(
                view,
                &BTreeSet::new(),
                &Default::default(),
                &cache.labels,
                Locale::En,
            )
            .unwrap();
        equal(&during, &after);
        compare(&cache, &doc);
    }
}

#[test]
fn appearance_rebuilds_only_affected_nodes_and_source_wires() {
    let mut doc = Document::demo();
    let mut cache = RetainedProjection::new(&doc).unwrap();
    let draw = |cache: &RetainedProjection| {
        cache
            .index
            .scene_with_labels(
                Viewport {
                    origin: [-30., -30.],
                    size: [1600., 1000.],
                    zoom: 0.7,
                },
                &BTreeSet::new(),
                &Default::default(),
                &cache.labels,
                Locale::En,
            )
            .unwrap()
    };
    let before = draw(&cache);
    let colors = std::collections::BTreeMap::from([("add".into(), "#ff3344".into())]);
    cache.labels.get_mut("add").unwrap().state = Some("7".into());
    cache.set_node_colors(&colors);
    assert_eq!(cache.labels["add"].state.as_deref(), Some("7"));
    assert_eq!(cache.labels["add"].color, Some([255, 51, 68]));
    let colored = draw(&cache);
    assert_eq!(colored.geometry_stats.rebuilt_nodes, 1);
    assert_eq!(
        colored.geometry_stats.rebuilt_edges,
        doc.edges.iter().filter(|e| e.from == "add").count()
    );
    assert_ne!(
        bytemuck::cast_slice::<_, u8>(&before.quads),
        bytemuck::cast_slice::<_, u8>(&colored.quads)
    );
    assert_eq!(draw(&cache).geometry_stats.rebuilt_nodes, 0);
    doc.nodes.update("add", |n| n.x += 24.).unwrap();
    cache.sync(&doc).unwrap();
    assert_eq!(cache.labels["add"].color, Some([255, 51, 68]));
    doc.nodes.update("add", |n| n.name = "sum".into()).unwrap();
    cache.sync(&doc).unwrap();
    assert_eq!(cache.labels["add"].color, Some([255, 51, 68]));
    doc.nodes.update("add", |n| n.kind = Kind::Sub).unwrap();
    cache.sync(&doc).unwrap();
    assert_eq!(cache.labels["add"].color, None);
    cache.sync(&Document::demo()).unwrap();
    assert_eq!(cache.labels["add"].color, Some([255, 51, 68]));
    cache.set_node_colors(&Default::default());
    compare(&cache, &Document::demo());
}

#[test]
fn bit_node_configuration_changes_update_native_ports_and_labels() {
    let project: serde_json::Value =
        serde_json::from_str(include_str!("../../../examples/bit-nodes.json")).unwrap();
    let mut doc: Document = serde_json::from_value(project["document"].clone()).unwrap();
    doc.edges = vec![].into();
    let mut cache = RetainedProjection::new(&doc).unwrap();
    for (id, config) in [
        (
            "literal",
            design_core::NodeConfig::Constant {
                value: "0xCD".into(),
            },
        ),
        (
            "upper",
            design_core::NodeConfig::Slice {
                input_width: 16,
                lsb: 8,
            },
        ),
        (
            "joined",
            design_core::NodeConfig::Concat {
                high_width: 3,
                low_width: 5,
            },
        ),
    ] {
        doc.nodes.update(id, |n| n.config = config.clone()).unwrap();
        doc.validate(false).unwrap();
        cache.sync(&doc).unwrap();
        compare(&cache, &doc);
    }
}

#[test]
fn extension_configuration_updates_port_width_and_signed_labels() {
    let project: serde_json::Value =
        serde_json::from_str(include_str!("../../../examples/extension-nodes.json")).unwrap();
    let mut doc: Document = serde_json::from_value(project["document"].clone()).unwrap();
    doc.edges = vec![].into();
    let mut cache = RetainedProjection::new(&doc).unwrap();
    assert!(!cache.labels["signed_bits"].inputs["a"].en.ends_with(" s"));
    doc = doc.set_signed("signed_bits", true).unwrap();
    cache.sync(&doc).unwrap();
    compare(&cache, &doc);
    assert!(cache.labels["signed_bits"].outputs["y"].en.ends_with(" s"));
    assert!(!cache.labels["zero"].inputs["a"].en.ends_with(" s"));
    for (id, config) in [
        (
            "zero",
            design_core::NodeConfig::ZeroExtend { input_width: 1 },
        ),
        (
            "signed_bits",
            design_core::NodeConfig::SignExtend { input_width: 8 },
        ),
    ] {
        doc.nodes.update(id, |n| n.config = config).unwrap();
        doc.validate(false).unwrap();
        cache.sync(&doc).unwrap();
        compare(&cache, &doc);
    }
}

#[test]
fn gate_mode_invalidates_geometry_without_moving_ports_or_losing_updates() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../../examples/logic-gates.json")).unwrap();
    let mut doc: Document = serde_json::from_value(fixture["document"].clone()).unwrap();
    let mut cache = RetainedProjection::new(&doc).unwrap();
    let geometry = cache.document().clone();
    let view = Viewport {
        origin: [-30., -30.],
        size: [2000., 3000.],
        zoom: 1.,
    };
    let render = |c: &RetainedProjection, zoom| {
        c.index
            .scene_with_theme(
                Viewport { zoom, ..view },
                &Default::default(),
                &Default::default(),
                &c.labels,
                Locale::En,
                Theme::Dark,
            )
            .unwrap()
    };
    for zoom in [0.2, 0.35, 1.0] {
        cache.set_gate_symbols(false);
        let blocks = render(&cache, zoom);
        cache.set_gate_symbols(true);
        let gates = render(&cache, zoom);
        assert_eq!(cache.document(), &geometry);
        assert_eq!(blocks.visible_edges, gates.visible_edges);
        assert!(gates.quads.len() > blocks.quads.len());
        assert_eq!(
            cache.labels["g_nor"].gate,
            Some(unge_render::GateSymbol::Nor)
        );
        cache.set_gate_symbols(false);
        equal(&blocks, &render(&cache, zoom));
    }
    cache.set_gate_symbols(true);
    doc.nodes.iter_mut().find(|n| n.id == "g_or").unwrap().kind = Kind::Nor;
    cache.sync(&doc).unwrap();
    assert_eq!(
        cache.labels["g_or"].gate,
        Some(unge_render::GateSymbol::Nor)
    );
    cache.labels.get_mut("g_or").unwrap().state = Some("255".into());
    let scene = render(&cache, 1.0);
    assert!(scene.labels.iter().any(|l| l.text == "255"));
}

#[test]
fn light_overview_gates_have_no_filled_tile_even_when_selected() {
    let doc: Document = serde_json::from_value(serde_json::json!({
        "name":"GateOutline", "nodes":[{"id":"g","name":"NOR","kind":"nor","width":1,"x":0,"y":0}], "edges":[]
    })).unwrap();
    let mut cache = RetainedProjection::new(&doc).unwrap();
    cache.set_gate_symbols(true);
    for selected in [false, true] {
        let selection = if selected {
            BTreeSet::from([cache.ids["g"]])
        } else {
            BTreeSet::new()
        };
        for theme in [Theme::Light, Theme::Dark] {
            let scene = cache
                .index
                .scene_with_theme(
                    Viewport {
                        origin: [-10., -10.],
                        size: [400., 400.],
                        zoom: 0.25,
                    },
                    &selection,
                    &Default::default(),
                    &cache.labels,
                    Locale::Ja,
                    theme,
                )
                .unwrap();
            let filled_tile = scene
                .quads
                .iter()
                .any(|q| q.params[2] == 0. && q.rect[2] > 200. && q.rect[3] > 200.);
            assert_eq!(filled_tile, theme == Theme::Dark);
            assert_eq!(scene.visible_nodes, 1);
        }
    }
}

#[test]
fn light_overview_leads_reach_every_port_with_screen_space_width() {
    for kind in ["and", "or", "xor", "not", "nand", "nor", "xnor"] {
        let doc: Document = serde_json::from_value(serde_json::json!({
            "name":"GateLeads", "nodes":[{"id":"g","name":"gate","kind":kind,"width":1,"x":0,"y":0}], "edges":[]
        })).unwrap();
        let mut cache = RetainedProjection::new(&doc).unwrap();
        cache.set_gate_symbols(true);
        for zoom in [0.02, 0.1, 0.2, 0.349] {
            let scene = cache
                .index
                .scene_with_theme(
                    Viewport {
                        origin: [-10., -10.],
                        size: [400., 400.],
                        zoom,
                    },
                    &Default::default(),
                    &Default::default(),
                    &cache.labels,
                    Locale::En,
                    Theme::Light,
                )
                .unwrap();
            let inputs = if kind == "not" {
                vec![144.]
            } else {
                vec![96., 192.]
            };
            for (x, y) in inputs.into_iter().map(|y| (0., y)).chain([(288., 144.)]) {
                // A horizontal line (not a port circle or vertical frame edge)
                // must continue inward from each unchanged hit-test anchor.
                assert!(
                    scene.quads.iter().any(|q| {
                        q.params[2] == -1.
                            && q.params[0].abs() < 0.001
                            && (q.rect[1] - y).abs() < 0.001
                            && q.rect[2] > q.rect[3] + 10.
                            && q.rect[3] * zoom >= 1.49
                            && q.rect[0] - q.rect[2] / 2. <= x
                            && q.rect[0] + q.rect[2] / 2. >= x
                    }),
                    "{kind} at zoom {zoom}, port {x},{y}"
                );
            }
        }
    }
}

#[test]
fn hidden_gate_frames_keep_geometry_and_restore_when_toggled() {
    let doc: Document = serde_json::from_value(serde_json::json!({
        "name":"HiddenFrame", "nodes":[{"id":"g","name":"NOT","kind":"not","width":1,"x":0,"y":0}], "edges":[]
    })).unwrap();
    let mut cache = RetainedProjection::new(&doc).unwrap();
    cache.set_gate_symbols(true);
    let geometry = cache.document().clone();
    for theme in [Theme::Light, Theme::Dark] {
        for zoom in [0.2, 0.5, 1.] {
            let draw = |c: &RetainedProjection, selected| {
                c.index
                    .scene_with_theme(
                        Viewport {
                            origin: [-10., -10.],
                            size: [600., 600.],
                            zoom,
                        },
                        &if selected {
                            BTreeSet::from([c.ids["g"]])
                        } else {
                            BTreeSet::new()
                        },
                        &Default::default(),
                        &c.labels,
                        Locale::En,
                        theme,
                    )
                    .unwrap()
            };
            cache.set_hide_gate_frames(false);
            let framed = draw(&cache, false);
            cache.set_hide_gate_frames(true);
            for selected in [false, true] {
                let scene = draw(&cache, selected);
                assert_eq!(cache.document(), &geometry);
                assert!(!scene
                    .quads
                    .iter()
                    .any(|q| q.params[2] == 0. && q.rect[2] > 200.));
                if zoom >= 0.35 {
                    assert!(scene.labels.iter().any(|l| l.text.starts_with("NOT")));
                }
            }
            cache.set_hide_gate_frames(false);
            equal(&framed, &draw(&cache, false));
        }
    }
    cache.set_hide_gate_frames(true);
    let mut changed = doc.clone();
    changed
        .nodes
        .update("g", |n| n.name = "inverter".into())
        .unwrap();
    cache.sync(&changed).unwrap();
    assert!(cache.labels["g"].hide_frame);
}

#[test]
fn nets_and_gate_leads_use_identical_strokes_for_bits_and_buses() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../../examples/logic-gates.json")).unwrap();
    for bits in [1, 8, 64] {
        let mut doc: Document = serde_json::from_value(fixture["document"].clone()).unwrap();
        for node in &mut doc.nodes {
            node.width = bits;
        }
        let mut cache = RetainedProjection::new(&doc).unwrap();
        cache.set_gate_symbols(true);
        cache.set_hide_gate_frames(true);
        for zoom in [0.02, 0.2, 0.349, 0.35, 0.7, 1., 2.] {
            for theme in [Theme::Light, Theme::Dark] {
                let scene = cache
                    .index
                    .scene_with_theme(
                        Viewport {
                            origin: [-30., -30.],
                            size: [2500., 6000.],
                            zoom,
                        },
                        &Default::default(),
                        &Default::default(),
                        &cache.labels,
                        Locale::En,
                        theme,
                    )
                    .unwrap();
                // Long capsule quads are wires/straight gate segments; circular
                // ports, grid, node fills and glyphs are intentionally excluded.
                let strokes: Vec<_> = scene
                    .quads
                    .iter()
                    .filter(|q| q.params[2] == -1. && q.rect[2] > q.rect[3] + 10.)
                    .collect();
                assert!(strokes.len() > 20);
                let expected = strokes[0].rect[3];
                assert!(expected * zoom >= 1.49);
                assert!(
                    strokes.iter().all(|q| (q.rect[3] - expected).abs() < 0.001),
                    "bits={bits}, zoom={zoom}, theme={theme:?}"
                );
            }
        }
    }
}

#[test]
fn moving_unconnected_obstacle_reroutes_wire_during_preview_and_commit() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../../examples/logic-gates.json")).unwrap();
    let mut doc: Document = serde_json::from_value(fixture["document"].clone()).unwrap();
    doc.nodes
        .retain(|n| ["g_and", "g_xor", "g_not"].contains(&n.id.as_str()));
    for (id, x, y) in [
        ("g_and", 0., 0.),
        ("g_xor", 1000., 0.),
        ("g_not", 500., 500.),
    ] {
        doc.nodes
            .update(id, |n| {
                n.x = x;
                n.y = y;
            })
            .unwrap();
    }
    doc.edges = vec![design_core::Edge {
        source_port: "y".into(),
        id: "test_wire".into(),
        from: "g_and".into(),
        to: "g_xor".into(),
        port: "a".into(),
    }]
    .into();
    let original = doc.clone();
    let mut cache = RetainedProjection::new(&doc).unwrap();
    let view = Viewport {
        origin: [-100., -100.],
        size: [1800., 1200.],
        zoom: 0.7,
    };
    let draw = |c: &RetainedProjection, p: &unge_interaction::Preview| {
        c.index
            .scene_with_labels(view, &BTreeSet::new(), p, &c.labels, Locale::En)
            .unwrap()
    };
    let before = draw(&cache, &Default::default());
    let mut preview = unge_interaction::Preview::default();
    preview.placement.insert(
        cache.ids["g_not"],
        unge_core::Rect {
            x: 500.,
            y: 0.,
            width: 288.,
            height: 288.,
        },
    );
    let during = draw(&cache, &preview);
    assert_eq!(
        during.geometry_stats.rebuilt_edges, 1,
        "non-incident wire must reroute"
    );
    assert_ne!(
        bytemuck::cast_slice::<_, u8>(&before.quads),
        bytemuck::cast_slice::<_, u8>(&during.quads)
    );
    doc.nodes.update("g_not", |n| n.y = 0.).unwrap();
    cache.sync(&doc).unwrap();
    equal(&during, &draw(&cache, &Default::default()));
    compare(&cache, &doc);
    cache.sync(&original).unwrap();
    equal(&before, &draw(&cache, &Default::default()));
}

#[test]
fn module_projection_preserves_multiple_typed_output_anchors() {
    let (doc, instance) = Document::demo()
        .group_module(&["add".into(), "bit_and".into()], "Arithmetic")
        .unwrap();
    let (projected, labels, ids) = project(&doc).unwrap();
    let module = &projected.graph().nodes()[&ids[&instance]];
    assert_eq!(module.inputs.len(), 2);
    assert_eq!(module.outputs.len(), 2);
    assert_eq!(labels[&instance].outputs.len(), 2);
    let outputs: BTreeSet<_> = projected
        .graph()
        .edges()
        .values()
        .filter(|e| e.from.node == ids[&instance])
        .map(|e| e.from.port.clone())
        .collect();
    assert_eq!(outputs, BTreeSet::from(["out0".into(), "out1".into()]));
    let mut retained = RetainedProjection::new(&Document::demo()).unwrap();
    retained.sync(&doc).unwrap();
    compare(&retained, &doc);
}
