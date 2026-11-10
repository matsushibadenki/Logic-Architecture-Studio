use std::collections::BTreeSet;
use unge_core::*;
use unge_render::*;
#[test]
fn role_colors_follow_cables_and_port_centres_without_mutating_document() {
    let mut editor = Editor::new(Document::default(), 0).unwrap();
    let port = Port {
        name: "image".into(),
        data_type: DataType::Image,
        cardinality: Cardinality::Single,
        required: false,
    };
    for (n, x) in [(1, 30.), (2, 340.)] {
        editor
            .execute(Command::AddNode {
                node: Node {
                    id: Id::from_u128(n),
                    type_id: "image".into(),
                    inputs: vec![port.clone()],
                    outputs: vec![port.clone()],
                    properties: Properties::new(),
                },
                rect: Rect {
                    x,
                    y: 40.,
                    width: 224.,
                    height: 128.,
                },
            })
            .unwrap();
    }
    editor
        .execute(Command::Connect {
            edge: Edge {
                id: Id::from_u128(3),
                from: Endpoint {
                    node: Id::from_u128(1),
                    port: "image".into(),
                },
                to: Endpoint {
                    node: Id::from_u128(2),
                    port: "image".into(),
                },
            },
        })
        .unwrap();
    let index = SceneIndex::new(editor.document());
    let catalog = LabelCatalog::from([(
        "image".into(),
        NodeLabels {
            tone: NodeTone::Violet,
            ..Default::default()
        },
    )]);
    for theme in [Theme::Dark, Theme::Light] {
        let view = Viewport {
            origin: [0., 0.],
            size: [640., 320.],
            zoom: 1.,
        };
        let scene = index
            .scene_with_theme(
                view,
                &BTreeSet::from([Id::from_u128(1)]),
                &Default::default(),
                &catalog,
                Locale::Ja,
                theme,
            )
            .unwrap();
        let tone = theme.palette().tone(NodeTone::Violet).linear();
        assert_eq!((scene.visible_nodes, scene.visible_edges), (2, 1));
        // The first geometry after the grid is an outgoing cable segment.
        assert_eq!(scene.quads[1].color, tone);
        for rect in editor.document().placement().values() {
            for output in [false, true] {
                let p = port_anchor(*rect, 0, 1, output);
                assert!(
                    scene
                        .quads
                        .iter()
                        .any(|q| q.rect == [p[0], p[1], 12., 12.] && q.color == tone)
                );
            }
        }
        assert!(scene.quads.iter().any(|q| q.params[3] > 0.));
        for width in [320., 375., 414., 768.] {
            let overview = index
                .scene_with_theme(
                    Viewport {
                        zoom: 0.25,
                        size: [width, 320.],
                        ..view
                    },
                    &BTreeSet::new(),
                    &Default::default(),
                    &catalog,
                    Locale::En,
                    theme,
                )
                .unwrap();
            assert!(overview.quads.iter().all(|q| q.params[3] == 0.));
            assert!(overview.labels.is_empty());
        }
    }
    assert_eq!(editor.document().placement()[&Id::from_u128(1)].x, 30.);
}

#[test]
fn dense_port_labels_stay_below_the_header() {
    for count in [3, 6, 12] {
        let mut editor = Editor::new(Document::default(), 0).unwrap();
        editor
            .execute(Command::AddNode {
                node: Node {
                    id: Id::from_u128(1),
                    type_id: "Dense node".into(),
                    inputs: (0..count)
                        .map(|i| Port {
                            name: format!("Port {i}"),
                            data_type: DataType::Float,
                            cardinality: Cardinality::Single,
                            required: false,
                        })
                        .collect(),
                    outputs: vec![],
                    properties: Properties::new(),
                },
                rect: Rect {
                    x: 20.,
                    y: 20.,
                    width: 224.,
                    height: 128.,
                },
            })
            .unwrap();
        let scene = SceneIndex::new(editor.document())
            .scene(
                Viewport {
                    origin: [0., 0.],
                    size: [320., 320.],
                    zoom: 1.,
                },
                &BTreeSet::new(),
            )
            .unwrap();
        assert_eq!(scene.labels[0].text, "Dense node");
        for label in scene.labels.iter().skip(1) {
            assert!(label.rect.y >= 54., "port label overlaps the header");
            assert!(label.rect.y + label.rect.height <= 148.);
        }
    }
}
