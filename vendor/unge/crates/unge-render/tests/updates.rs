use std::collections::BTreeSet;
use unge_core::*;
use unge_render::*;
fn editor() -> Editor {
    let port = Port {
        name: "p".into(),
        data_type: DataType::Float,
        cardinality: Cardinality::Multiple,
        required: false,
    };
    let mut commands: Vec<_> = (1..=180)
        .map(|n| Command::AddNode {
            node: Node {
                id: Id::from_u128(n),
                type_id: "test".into(),
                inputs: vec![port.clone()],
                outputs: vec![port.clone()],
                properties: Properties::new(),
            },
            rect: Rect {
                x: (n % 10) as f32 * 250.0,
                y: (n / 10) as f32 * 150.0,
                ..Rect::default()
            },
        })
        .collect();
    for n in 1..180 {
        commands.push(Command::Connect {
            edge: Edge {
                id: Id::from_u128(1000 + n),
                from: Endpoint {
                    node: Id::from_u128(n),
                    port: "p".into(),
                },
                to: Endpoint {
                    node: Id::from_u128(n + 1),
                    port: "p".into(),
                },
            },
        });
    }
    let mut editor = Editor::new(Document::default(), 256).unwrap();
    editor.execute(Command::Batch { commands }).unwrap();
    editor
}
fn compare(index: &SceneIndex, document: &Document) {
    let rebuilt = SceneIndex::new(document);
    for (origin, zoom) in [
        ([0.0, 0.0], 1.0),
        ([4000.0, -1000.0], 1.0),
        ([0.0, 0.0], 0.02),
    ] {
        let view = Viewport {
            origin,
            zoom,
            size: [800.0, 600.0],
        };
        let selection = BTreeSet::from([Id::from_u128(1)]);
        let a = index.scene(view, &selection).unwrap();
        let b = rebuilt.scene(view, &selection).unwrap();
        assert_eq!(
            bytemuck::cast_slice::<_, u8>(&a.quads),
            bytemuck::cast_slice::<_, u8>(&b.quads)
        );
        assert_eq!(format!("{:?}", a.labels), format!("{:?}", b.labels));
        assert_eq!(
            (a.visible_nodes, a.visible_edges),
            (b.visible_nodes, b.visible_edges)
        );
        let preview = unge_interaction::Preview {
            placement: [(
                Id::from_u128(2),
                Rect {
                    x: 4100.0,
                    y: -500.0,
                    ..Rect::default()
                },
            )]
            .into(),
            ..Default::default()
        };
        let a = index
            .scene_with_preview(view, &selection, &preview)
            .unwrap();
        let b = rebuilt
            .scene_with_preview(view, &selection, &preview)
            .unwrap();
        assert_eq!(
            bytemuck::cast_slice::<_, u8>(&a.quads),
            bytemuck::cast_slice::<_, u8>(&b.quads)
        );
        assert_eq!(format!("{:?}", a.labels), format!("{:?}", b.labels));
        assert_eq!(
            index.spatial_index().query(view.world_rect()),
            rebuilt.spatial_index().query(view.world_rect())
        );
    }
}
#[test]
fn repeated_moves_and_overlay_rebuild_match_fresh_indexes() {
    let mut editor = editor();
    let mut index = SceneIndex::new(editor.document());
    let mut rebuilds = 0;
    for n in 1..=150 {
        let command = Command::MoveNode {
            id: Id::from_u128(n),
            rect: Rect {
                x: 5000.0 + n as f32 * 12.0,
                y: -500.0,
                ..Rect::default()
            },
        };
        let changes = SceneChanges::from_command(&command);
        editor.execute(command).unwrap();
        if index.update(editor.document(), &changes) == SceneUpdate::Rebuilt {
            rebuilds += 1;
        }
        if n % 25 == 0 {
            compare(&index, editor.document());
        }
    }
    assert!(rebuilds > 0);
    for x in [0.0, 9000.0, -1000.0, 200.0] {
        let command = Command::MoveNode {
            id: Id::from_u128(1),
            rect: Rect {
                x,
                ..Rect::default()
            },
        };
        let changes = SceneChanges::from_command(&command);
        editor.execute(command).unwrap();
        assert_eq!(
            index.update(editor.document(), &changes),
            SceneUpdate::Incremental
        );
        compare(&index, editor.document());
    }
}
#[test]
fn batch_final_placement_properties_groups_and_topology_match_rebuild() {
    let mut editor = editor();
    let mut index = SceneIndex::new(editor.document());
    let id = Id::from_u128(1);
    let edit = Command::Batch {
        commands: vec![
            Command::MoveNode {
                id,
                rect: Rect {
                    x: -700.0,
                    ..Rect::default()
                },
            },
            Command::MoveNode {
                id,
                rect: Rect {
                    x: 4500.0,
                    ..Rect::default()
                },
            },
            Command::SetProperty {
                id,
                key: "value".into(),
                value: Some(serde_json::json!(42)),
            },
        ],
    };
    let changes = SceneChanges::from_command(&edit);
    editor.execute(edit).unwrap();
    assert_eq!(
        index.update(editor.document(), &changes),
        SceneUpdate::Incremental
    );
    compare(&index, editor.document());
    let property = Command::SetProperty {
        id,
        key: "value".into(),
        value: None,
    };
    let changes = SceneChanges::from_command(&property);
    editor.execute(property).unwrap();
    assert_eq!(
        index.update(editor.document(), &changes),
        SceneUpdate::Unchanged
    );
    compare(&index, editor.document());
    let group_id = Id::from_u128(5000);
    let group = Command::SetGroup {
        id: group_id,
        group: Some(Group {
            id: group_id,
            label: "Group".into(),
            nodes: BTreeSet::from([id]),
        }),
    };
    let changes = SceneChanges::from_command(&group);
    editor.execute(group).unwrap();
    assert_eq!(
        index.update(editor.document(), &changes),
        SceneUpdate::Incremental
    );
    compare(&index, editor.document());
    for command in [
        Command::Disconnect {
            id: Id::from_u128(1001),
        },
        Command::RemoveNode { id },
    ] {
        let changes = SceneChanges::from_command(&command);
        editor.execute(command).unwrap();
        assert_eq!(
            index.update(editor.document(), &changes),
            SceneUpdate::Incremental
        );
        compare(&index, editor.document());
    }
}

#[test]
fn high_degree_moves_refit_without_truncating_incident_edges() {
    let mut editor = editor();
    let commands = (1..180)
        .flat_map(|n| {
            [
                Command::Disconnect {
                    id: Id::from_u128(1000 + n),
                },
                Command::Connect {
                    edge: Edge {
                        id: Id::from_u128(1000 + n),
                        from: Endpoint {
                            node: Id::from_u128(1),
                            port: "p".into(),
                        },
                        to: Endpoint {
                            node: Id::from_u128(n + 1),
                            port: "p".into(),
                        },
                    },
                },
            ]
        })
        .collect();
    editor.execute(Command::Batch { commands }).unwrap();
    let mut index = SceneIndex::new(editor.document());
    let command = Command::MoveNode {
        id: Id::from_u128(1),
        rect: Rect {
            x: 5000.0,
            y: -500.0,
            ..Default::default()
        },
    };
    let changes = SceneChanges::from_command(&command);
    editor.execute(command).unwrap();
    assert_eq!(
        index.update(editor.document(), &changes),
        SceneUpdate::Incremental
    );
    compare(&index, editor.document());
}

fn apply_and_compare(editor: &mut Editor, index: &mut SceneIndex, command: Command) {
    let changes = SceneChanges::from_command(&command);
    editor.execute(command).unwrap();
    assert_eq!(
        index.update(editor.document(), &changes),
        SceneUpdate::Incremental
    );
    compare(index, editor.document());
    for _ in 0..2 {
        let changes = SceneChanges::from_command(editor.undo_command().unwrap());
        assert!(editor.undo().unwrap());
        assert_eq!(
            index.update(editor.document(), &changes),
            SceneUpdate::Incremental
        );
        compare(index, editor.document());
        let changes = SceneChanges::from_command(editor.redo_command().unwrap());
        assert!(editor.redo().unwrap());
        assert_eq!(
            index.update(editor.document(), &changes),
            SceneUpdate::Incremental
        );
        compare(index, editor.document());
    }
}
#[test]
fn topology_history_restores_implicit_edges_groups_and_updates_incidence() {
    let mut editor = editor();
    let mut index = SceneIndex::new(editor.document());
    let id = Id::from_u128(1);
    let group_id = Id::from_u128(5000);
    apply_and_compare(
        &mut editor,
        &mut index,
        Command::SetGroup {
            id: group_id,
            group: Some(Group {
                id: group_id,
                label: "Group 日本語 组".into(),
                nodes: [id, Id::from_u128(2)].into(),
            }),
        },
    );
    apply_and_compare(&mut editor, &mut index, Command::RemoveNode { id });
    // Restore the removed node before rewiring its edge to different endpoints.
    let changes = SceneChanges::from_command(editor.undo_command().unwrap());
    editor.undo().unwrap();
    index.update(editor.document(), &changes);
    let edge_id = Id::from_u128(1001);
    apply_and_compare(
        &mut editor,
        &mut index,
        Command::Batch {
            commands: vec![
                Command::Disconnect { id: edge_id },
                Command::Connect {
                    edge: Edge {
                        id: edge_id,
                        from: Endpoint {
                            node: Id::from_u128(3),
                            port: "p".into(),
                        },
                        to: Endpoint {
                            node: Id::from_u128(90),
                            port: "p".into(),
                        },
                    },
                },
            ],
        },
    );
    for n in [1, 3, 90] {
        apply_and_compare(
            &mut editor,
            &mut index,
            Command::MoveNode {
                id: Id::from_u128(n),
                rect: Rect {
                    x: 4500.,
                    y: -600.,
                    ..Rect::default()
                },
            },
        );
    }
    apply_and_compare(
        &mut editor,
        &mut index,
        Command::RemoveNode {
            id: Id::from_u128(3),
        },
    );
}
#[test]
fn recreated_ids_use_final_ports_labels_and_geometry() {
    let mut editor = editor();
    let mut index = SceneIndex::new(editor.document());
    let id = Id::from_u128(1);
    let mut node = editor.document().graph().nodes()[&id].clone();
    node.type_id = "replacement".into();
    let mut port = node.outputs[0].clone();
    port.name = "extra".into();
    node.outputs.insert(0, port);
    let edge = editor.document().graph().edges()[&Id::from_u128(1001)].clone();
    apply_and_compare(
        &mut editor,
        &mut index,
        Command::Batch {
            commands: vec![
                Command::RemoveNode { id },
                Command::AddNode {
                    node,
                    rect: Rect {
                        x: 5000.,
                        y: -700.,
                        width: 300.,
                        height: 180.,
                    },
                },
                Command::Connect { edge },
            ],
        },
    );
    let property = Command::SetProperty {
        id,
        key: "value".into(),
        value: Some(42.into()),
    };
    let changes = SceneChanges::from_command(&property);
    editor.execute(property).unwrap();
    assert_eq!(
        index.update(editor.document(), &changes),
        SceneUpdate::Unchanged
    );
    let changes = SceneChanges::from_command(editor.undo_command().unwrap());
    editor.undo().unwrap();
    assert_eq!(
        index.update(editor.document(), &changes),
        SceneUpdate::Unchanged
    );
    compare(&index, editor.document());
}
#[test]
fn insert_remove_churn_and_failed_batch_preserve_final_scene() {
    let mut editor = editor();
    let mut index = SceneIndex::new(editor.document());
    let template = editor.document().graph().nodes()[&Id::from_u128(1)].clone();
    let mut rebuilds = 0;
    for n in 300..450 {
        let mut node = template.clone();
        node.id = Id::from_u128(n);
        for command in [
            Command::AddNode {
                node,
                rect: Rect::default(),
            },
            Command::RemoveNode {
                id: Id::from_u128(n),
            },
        ] {
            let changes = SceneChanges::from_command(&command);
            editor.execute(command).unwrap();
            if index.update(editor.document(), &changes) == SceneUpdate::Rebuilt {
                rebuilds += 1;
            }
        }
        if n % 25 == 0 {
            compare(&index, editor.document());
        }
    }
    assert!(rebuilds > 0);
    let revision = editor.revision();
    let bad = Command::Batch {
        commands: vec![
            Command::RemoveNode {
                id: Id::from_u128(1),
            },
            Command::RemoveNode {
                id: Id::from_u128(999),
            },
        ],
    };
    assert!(editor.execute(bad).is_err());
    assert_eq!(editor.revision(), revision);
    compare(&index, editor.document());
    let command = Command::Batch {
        commands: (3..=140)
            .map(|n| Command::RemoveNode {
                id: Id::from_u128(n),
            })
            .collect(),
    };
    let changes = SceneChanges::from_command(&command);
    editor.execute(command).unwrap();
    assert_eq!(
        index.update(editor.document(), &changes),
        SceneUpdate::Rebuilt
    );
    compare(&index, editor.document());
    let changes = SceneChanges::from_command(editor.undo_command().unwrap());
    editor.undo().unwrap();
    assert_eq!(
        index.update(editor.document(), &changes),
        SceneUpdate::Rebuilt
    );
    compare(&index, editor.document());
}

#[test]
fn bulk_moves_with_overlays_groups_nested_batches_and_history_match_rebuilds() {
    let mut editor = editor();
    let group = Id::from_u128(9000);
    editor
        .execute(Command::SetGroup {
            id: group,
            group: Some(Group {
                id: group,
                label: "Bulk 日本語 组".into(),
                nodes: (1..=160).map(Id::from_u128).collect(),
            }),
        })
        .unwrap();
    let mut index = SceneIndex::new(editor.document());
    // Existing sparse overlays must not hide geometry after refitting.
    apply_and_compare(
        &mut editor,
        &mut index,
        Command::MoveNode {
            id: Id::from_u128(1),
            rect: Rect {
                x: -1000.,
                ..Rect::default()
            },
        },
    );
    let commands: Vec<_> = (1..=160)
        .map(|n| Command::MoveNode {
            id: Id::from_u128(n),
            rect: Rect {
                x: if n % 2 == 0 { 5000. } else { -800. },
                y: -500.,
                width: 220.,
                height: 120.,
            },
        })
        .collect();
    apply_and_compare(
        &mut editor,
        &mut index,
        Command::Batch {
            commands: vec![
                Command::Batch { commands },
                Command::MoveNode {
                    id: Id::from_u128(1),
                    rect: Rect {
                        x: 4500.,
                        y: -500.,
                        ..Rect::default()
                    },
                },
                Command::SetProperty {
                    id: Id::from_u128(1),
                    key: "value".into(),
                    value: Some(42.into()),
                },
            ],
        },
    );
    let fresh = SceneIndex::new(editor.document());
    for r in editor.document().placement().values() {
        let point = [r.x + 1., r.y + 1.];
        assert_eq!(
            index.spatial_index().hit_test(point),
            fresh.spatial_index().hit_test(point)
        );
    }
    // Structural edits mixed with a large move retain the old full rebuild path.
    let command = Command::Batch {
        commands: vec![
            Command::Batch {
                commands: (1..=160)
                    .map(|n| Command::MoveNode {
                        id: Id::from_u128(n),
                        rect: Rect::default(),
                    })
                    .collect(),
            },
            Command::RemoveNode {
                id: Id::from_u128(180),
            },
        ],
    };
    let changes = SceneChanges::from_command(&command);
    editor.execute(command).unwrap();
    assert_eq!(
        index.update(editor.document(), &changes),
        SceneUpdate::Rebuilt
    );
    compare(&index, editor.document());
}

#[test]
fn bulk_move_limits_and_overlay_only_nodes_fall_back_to_complete_geometry() {
    let mut editor = editor();
    let template = editor.document().graph().nodes()[&Id::from_u128(1)].clone();
    editor
        .execute(Command::Batch {
            commands: (181..=1200)
                .map(|n| {
                    let mut node = template.clone();
                    node.id = Id::from_u128(n);
                    Command::AddNode {
                        node,
                        rect: Rect::default(),
                    }
                })
                .collect(),
        })
        .unwrap();
    let mut index = SceneIndex::new(editor.document());
    for (count, expected) in [
        (1024, SceneUpdate::Incremental),
        (1025, SceneUpdate::Rebuilt),
    ] {
        let command = Command::Batch {
            commands: (1..=count)
                .map(|n| Command::MoveNode {
                    id: Id::from_u128(n),
                    rect: Rect {
                        x: 5000.,
                        y: -500.,
                        ..Rect::default()
                    },
                })
                .collect(),
        };
        let changes = SceneChanges::from_command(&command);
        editor.execute(command).unwrap();
        assert_eq!(index.update(editor.document(), &changes), expected);
        compare(&index, editor.document());
    }
    let mut node = template;
    node.id = Id::from_u128(1300);
    apply_and_compare(
        &mut editor,
        &mut index,
        Command::AddNode {
            node,
            rect: Rect::default(),
        },
    );
    let command = Command::Batch {
        commands: (1..=159)
            .chain([1300])
            .map(|n| Command::MoveNode {
                id: Id::from_u128(n),
                rect: Rect {
                    x: -700.,
                    ..Rect::default()
                },
            })
            .collect(),
    };
    let changes = SceneChanges::from_command(&command);
    editor.execute(command).unwrap();
    assert_eq!(
        index.update(editor.document(), &changes),
        SceneUpdate::Rebuilt
    );
    compare(&index, editor.document());
}

#[test]
fn incident_edge_limit_rebuilds_without_dropping_connections() {
    let mut editor = editor();
    let template = editor.document().graph().nodes()[&Id::from_u128(1)].clone();
    let mut commands: Vec<_> = (181..=4300)
        .map(|n| {
            let mut node = template.clone();
            node.id = Id::from_u128(n);
            Command::AddNode {
                node,
                rect: Rect {
                    x: n as f32 * 3.,
                    y: 0.,
                    ..Rect::default()
                },
            }
        })
        .collect();
    commands.extend((1..180).map(|n| Command::Disconnect {
        id: Id::from_u128(1000 + n),
    }));
    commands.extend((2..=4300).map(|n| Command::Connect {
        edge: Edge {
            id: Id::from_u128(10000 + n),
            from: Endpoint {
                node: Id::from_u128(1),
                port: "p".into(),
            },
            to: Endpoint {
                node: Id::from_u128(n),
                port: "p".into(),
            },
        },
    }));
    editor.execute(Command::Batch { commands }).unwrap();
    let mut index = SceneIndex::new(editor.document());
    let command = Command::MoveNode {
        id: Id::from_u128(1),
        rect: Rect {
            x: 5000.,
            y: -500.,
            ..Rect::default()
        },
    };
    let changes = SceneChanges::from_command(&command);
    editor.execute(command).unwrap();
    assert_eq!(
        index.update(editor.document(), &changes),
        SceneUpdate::Rebuilt
    );
    compare(&index, editor.document());
    let changes = SceneChanges::from_command(editor.undo_command().unwrap());
    editor.undo().unwrap();
    assert_eq!(
        index.update(editor.document(), &changes),
        SceneUpdate::Rebuilt
    );
    compare(&index, editor.document());
}

#[test]
fn timed_scene_matches_normal_output_and_rejects_invalid_views() {
    let editor = editor();
    let index = SceneIndex::new(editor.document());
    let selection = [Id::from_u128(1)].into();
    let preview = unge_interaction::Preview {
        placement: [
            (
                Id::from_u128(1),
                Rect {
                    x: 4500.,
                    y: -500.,
                    ..Rect::default()
                },
            ),
            (
                Id::from_u128(2),
                Rect {
                    x: 4400.,
                    y: -500.,
                    ..Rect::default()
                },
            ),
        ]
        .into(),
        marquee: Some(Rect {
            x: 4000.,
            y: -700.,
            width: 800.,
            height: 400.,
        }),
        ..Default::default()
    };
    let mut catalog = LabelCatalog::new();
    catalog.insert(
        "test".into(),
        NodeLabels {
            tone: NodeTone::Rose,
            ..Default::default()
        },
    );
    for theme in [Theme::Dark, Theme::Light] {
        for locale in [Locale::En, Locale::Ja, Locale::ZhCn] {
            for zoom in [0.02, 1., 16.] {
                let view = Viewport {
                    origin: [4000., -700.],
                    zoom,
                    size: [800., 600.],
                };
                let normal = index
                    .scene_with_theme(view, &selection, &preview, &catalog, locale, theme)
                    .unwrap();
                let (timed, t) = index
                    .scene_with_theme_timed(view, &selection, &preview, &catalog, locale, theme)
                    .unwrap();
                assert_eq!(
                    bytemuck::cast_slice::<_, u8>(&normal.quads),
                    bytemuck::cast_slice::<_, u8>(&timed.quads)
                );
                assert_eq!(
                    format!("{:?}", normal.labels),
                    format!("{:?}", timed.labels)
                );
                assert_eq!(
                    (
                        normal.visible_nodes,
                        normal.visible_edges,
                        normal.visible_groups
                    ),
                    (
                        timed.visible_nodes,
                        timed.visible_edges,
                        timed.visible_groups
                    )
                );
                let phases = [
                    t.groups_ms,
                    t.edge_query_ms,
                    t.edge_geometry_ms,
                    t.node_query_ms,
                    t.node_geometry_ms,
                    t.overlays_ms,
                ];
                assert!(phases.iter().all(|x| x.is_finite() && *x >= 0.));
                assert!(
                    t.total_ms.is_finite() && t.total_ms >= phases.iter().sum::<f64>() - 0.000001
                );
            }
        }
    }
    assert!(
        index
            .scene_with_theme_timed(
                Viewport {
                    origin: [0., 0.],
                    zoom: 0.,
                    size: [800., 600.]
                },
                &selection,
                &preview,
                &catalog,
                Locale::En,
                Theme::Dark
            )
            .is_err()
    );
}

#[test]
fn retained_edge_metadata_tracks_source_recreation_and_live_catalog_changes() {
    let mut editor = editor();
    let mut index = SceneIndex::new(editor.document());
    let id = Id::from_u128(1);
    let mut node = editor.document().graph().nodes()[&id].clone();
    node.type_id = "replaced".into();
    let edge = editor.document().graph().edges()[&Id::from_u128(1001)].clone();
    apply_and_compare(
        &mut editor,
        &mut index,
        Command::Batch {
            commands: vec![
                Command::RemoveNode { id },
                Command::AddNode {
                    node,
                    rect: Rect {
                        x: 5000.,
                        y: -500.,
                        width: 300.,
                        height: 200.,
                    },
                },
                Command::Connect { edge },
            ],
        },
    );
    for tone in [NodeTone::Rose, NodeTone::Mint, NodeTone::Neutral] {
        let catalog = [
            (
                "test".into(),
                NodeLabels {
                    tone: NodeTone::Blue,
                    ..Default::default()
                },
            ),
            (
                "replaced".into(),
                NodeLabels {
                    tone,
                    ..Default::default()
                },
            ),
        ]
        .into();
        let view = Viewport {
            origin: [0., 0.],
            zoom: 0.02,
            size: [800., 600.],
        };
        let a = index
            .scene_with_theme(
                view,
                &BTreeSet::new(),
                &Default::default(),
                &catalog,
                Locale::En,
                Theme::Light,
            )
            .unwrap();
        let b = SceneIndex::new(editor.document())
            .scene_with_theme(
                view,
                &BTreeSet::new(),
                &Default::default(),
                &catalog,
                Locale::En,
                Theme::Light,
            )
            .unwrap();
        assert_eq!(
            bytemuck::cast_slice::<_, u8>(&a.quads),
            bytemuck::cast_slice::<_, u8>(&b.quads)
        );
    }
}
