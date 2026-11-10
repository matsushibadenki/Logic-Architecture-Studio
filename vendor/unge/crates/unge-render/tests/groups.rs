use std::collections::BTreeSet;
use unge_core::*;
use unge_interaction::Preview;
use unge_render::*;
fn editor() -> Editor {
    let mut editor = Editor::new(Document::default(), 64).unwrap();
    for (id, x, y) in [(1, 40.0, 80.0), (2, 300.0, 250.0)] {
        editor
            .execute(Command::AddNode {
                node: Node {
                    id: Id::from_u128(id),
                    type_id: "test".into(),
                    inputs: vec![],
                    outputs: vec![],
                    properties: Default::default(),
                },
                rect: Rect {
                    x,
                    y,
                    ..Default::default()
                },
            })
            .unwrap();
    }
    editor
}
fn group(id: u128, label: &str, nodes: &[u128]) -> Command {
    Command::SetGroup {
        id: Id::from_u128(id),
        group: Some(Group {
            id: Id::from_u128(id),
            label: label.into(),
            nodes: nodes.iter().map(|n| Id::from_u128(*n)).collect(),
        }),
    }
}
fn view() -> Viewport {
    Viewport {
        origin: [0.0, 0.0],
        zoom: 1.0,
        size: [600.0, 400.0],
    }
}
#[test]
fn frames_titles_order_and_lod_follow_members() {
    let mut editor = editor();
    editor
        .execute(group(10, "グループ 组 Group", &[1, 2]))
        .unwrap();
    editor.execute(group(11, "empty", &[])).unwrap();
    let index = SceneIndex::new(editor.document());
    let scene = index.scene(view(), &BTreeSet::new()).unwrap();
    assert_eq!(scene.visible_groups, 1);
    assert_eq!(scene.quads[1].rect, [260.0, 196.0, 472.0, 320.0]);
    assert_eq!(scene.labels[0].text, "グループ 组 Group");
    assert_eq!(scene.labels[0].after_quad, 3);
    assert!(scene.labels[1].after_quad > scene.labels[0].after_quad);
    let overview = index
        .scene(
            Viewport {
                zoom: 0.1,
                ..view()
            },
            &BTreeSet::new(),
        )
        .unwrap();
    assert_eq!(overview.visible_groups, 1);
    assert!(overview.labels.is_empty());
    // Frame intersects the view even though every member is outside it.
    let enclosed = index
        .scene(
            Viewport {
                origin: [230.0, 190.0],
                size: [30.0, 20.0],
                ..view()
            },
            &BTreeSet::new(),
        )
        .unwrap();
    assert_eq!((enclosed.visible_nodes, enclosed.visible_groups), (0, 1));
}
#[test]
fn group_edits_move_previews_and_deletion_match_fresh_indexes() {
    let mut editor = editor();
    let mut index = SceneIndex::new(editor.document());
    let command = group(10, "First", &[1]);
    let changes = SceneChanges::from_command(&command);
    editor.execute(command).unwrap();
    assert_eq!(
        index.update(editor.document(), &changes),
        SceneUpdate::Incremental
    );
    let outside = Viewport {
        origin: [4000.0, 0.0],
        ..view()
    };
    assert_eq!(
        index
            .scene(outside, &BTreeSet::new())
            .unwrap()
            .visible_groups,
        0
    );
    let mut preview = Preview::default();
    preview.placement.insert(
        Id::from_u128(1),
        Rect {
            x: 4100.0,
            y: 80.0,
            ..Default::default()
        },
    );
    assert_eq!(
        index
            .scene_with_preview(outside, &BTreeSet::new(), &preview)
            .unwrap()
            .visible_groups,
        1
    );
    assert_eq!(
        index
            .scene_with_preview(view(), &BTreeSet::new(), &preview)
            .unwrap()
            .visible_groups,
        0
    );
    for command in [
        group(10, "Renamed 组", &[1, 2]),
        Command::MoveNode {
            id: Id::from_u128(1),
            rect: Rect {
                x: 4100.0,
                y: 80.0,
                ..Default::default()
            },
        },
        Command::RemoveNode {
            id: Id::from_u128(2),
        },
        Command::SetGroup {
            id: Id::from_u128(10),
            group: None,
        },
    ] {
        let changes = SceneChanges::from_command(&command);
        editor.execute(command).unwrap();
        index.update(editor.document(), &changes);
        let rebuilt = SceneIndex::new(editor.document());
        for viewport in [view(), outside] {
            let a = index.scene(viewport, &BTreeSet::new()).unwrap();
            let b = rebuilt.scene(viewport, &BTreeSet::new()).unwrap();
            // Cache hit counters may differ; rendered content must not.
            assert_eq!(
                bytemuck::cast_slice::<_, u8>(&a.quads),
                bytemuck::cast_slice::<_, u8>(&b.quads)
            );
            assert_eq!(a.labels, b.labels);
            assert_eq!(
                (a.visible_nodes, a.visible_edges, a.visible_groups),
                (b.visible_nodes, b.visible_edges, b.visible_groups)
            );
            assert_eq!(a.background, b.background);
            assert_eq!(a.grid_color, b.grid_color);
        }
    }
    assert_eq!(
        index
            .scene(outside, &BTreeSet::new())
            .unwrap()
            .visible_groups,
        0
    );
}
#[test]
fn shared_members_themes_and_extreme_frames_are_safe() {
    let mut editor = editor();
    editor.execute(group(10, "A", &[1])).unwrap();
    editor.execute(group(11, "B", &[1, 2])).unwrap();
    let index = SceneIndex::new(editor.document());
    let dark = index.scene(view(), &BTreeSet::new()).unwrap();
    let light = index
        .scene_with_theme(
            view(),
            &BTreeSet::new(),
            &Preview::default(),
            &LabelCatalog::new(),
            Locale::ZhCn,
            Theme::Light,
        )
        .unwrap();
    assert_eq!((dark.visible_groups, light.visible_groups), (2, 2));
    assert_eq!(dark.quads[1].rect, light.quads[1].rect);
    assert_ne!(dark.quads[1].color, light.quads[1].color);
    for (id, x) in [(1, -600_000_000.0), (2, 600_000_000.0)] {
        editor
            .execute(Command::MoveNode {
                id: Id::from_u128(id),
                rect: Rect {
                    x,
                    ..Default::default()
                },
            })
            .unwrap();
    }
    let scene = SceneIndex::new(editor.document())
        .scene(view(), &BTreeSet::new())
        .unwrap();
    assert_eq!(scene.visible_groups, 0);
    assert!(scene.quads.iter().flat_map(|q| q.rect).all(f32::is_finite));
}
