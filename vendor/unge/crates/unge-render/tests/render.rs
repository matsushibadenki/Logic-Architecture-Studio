use std::collections::BTreeSet;
use unge_core::*;
use unge_render::*;
fn document() -> Document {
    let mut editor = Editor::new(Document::default(), 0).unwrap();
    editor
        .execute(Command::AddNode {
            node: Node {
                id: Id::new_v4(),
                type_id: "test".into(),
                inputs: vec![],
                outputs: vec![],
                properties: Properties::new(),
            },
            rect: Rect {
                x: 20.,
                y: 20.,
                width: 180.,
                height: 90.,
            },
        })
        .unwrap();
    editor.document().clone()
}
#[test]
fn culls_nodes_and_reduces_ports_at_low_zoom() {
    let index = SceneIndex::new(&document());
    let view = Viewport {
        origin: [0., 0.],
        zoom: 1.,
        size: [256., 256.],
    };
    assert_eq!(
        index.scene(view, &BTreeSet::new()).unwrap().visible_nodes,
        1
    );
    assert_eq!(
        index
            .scene(
                Viewport {
                    origin: [1000., 1000.],
                    ..view
                },
                &BTreeSet::new()
            )
            .unwrap()
            .visible_nodes,
        0
    );
    assert!(
        index
            .scene(Viewport { zoom: 0., ..view }, &BTreeSet::new())
            .is_err()
    );
}
#[test]
fn edges_crossing_viewport_survive_endpoint_culling() {
    let port = Port {
        name: "p".into(),
        data_type: DataType::Float,
        cardinality: Cardinality::Single,
        required: true,
    };
    let a = Node {
        id: Id::new_v4(),
        type_id: "test".into(),
        inputs: vec![port.clone()],
        outputs: vec![port],
        properties: Properties::new(),
    };
    let b = Node {
        id: Id::new_v4(),
        ..a.clone()
    };
    let (aid, bid) = (a.id, b.id);
    let mut editor = Editor::new(Document::default(), 0).unwrap();
    editor
        .execute(Command::Batch {
            commands: vec![
                Command::AddNode {
                    node: a,
                    rect: Rect {
                        x: -400.,
                        y: 10.,
                        width: 180.,
                        height: 90.,
                    },
                },
                Command::AddNode {
                    node: b,
                    rect: Rect {
                        x: 400.,
                        y: 10.,
                        width: 180.,
                        height: 90.,
                    },
                },
                Command::Connect {
                    edge: Edge {
                        id: Id::new_v4(),
                        from: Endpoint {
                            node: aid,
                            port: "p".into(),
                        },
                        to: Endpoint {
                            node: bid,
                            port: "p".into(),
                        },
                    },
                },
            ],
        })
        .unwrap();
    let scene = SceneIndex::new(editor.document())
        .scene(
            Viewport {
                origin: [0., 0.],
                zoom: 1.,
                size: [200., 200.],
            },
            &BTreeSet::new(),
        )
        .unwrap();
    assert_eq!(scene.visible_nodes, 0);
    assert_eq!(scene.visible_edges, 1);
    let index = SceneIndex::new(editor.document());
    let mut preview = unge_interaction::Preview::default();
    for id in [aid, bid] {
        preview.placement.insert(
            id,
            Rect {
                y: 400.,
                ..editor.document().placement()[&id]
            },
        );
    }
    let viewport = Viewport {
        origin: [0., 0.],
        zoom: 1.,
        size: [200., 200.],
    };
    assert_eq!(
        index
            .scene_with_preview(viewport, &BTreeSet::new(), &preview)
            .unwrap()
            .visible_edges,
        0
    );
    assert_eq!(
        index
            .scene_with_preview(
                Viewport {
                    origin: [0., 400.],
                    ..viewport
                },
                &BTreeSet::new(),
                &preview
            )
            .unwrap()
            .visible_edges,
        1
    );
}
#[test]
#[ignore = "requires a working GPU adapter; run explicitly on a desktop"]
fn gpu_renders_pixels_without_validation_errors() {
    pollster::block_on(async {
        let instance = wgpu::Instance::default();
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .expect("GPU adapter required");
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .unwrap();
        device.push_error_scope(wgpu::ErrorFilter::Validation);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: 256,
                height: 256,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let mut renderer = GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm);
        let viewport = Viewport {
            origin: [0., 0.],
            zoom: 1.,
            size: [256., 256.],
        };
        let mut grouped = Editor::new(document(), 0).unwrap();
        let id = *grouped.document().graph().nodes().keys().next().unwrap();
        let group_id = Id::new_v4();
        grouped
            .execute(Command::SetGroup {
                id: group_id,
                group: Some(Group {
                    id: group_id,
                    label: "Group".into(),
                    nodes: BTreeSet::from([id]),
                }),
            })
            .unwrap();
        let document = grouped.document().clone();
        let id = *document.graph().nodes().keys().next().unwrap();
        let mut preview = unge_interaction::Preview::default();
        preview.placement.insert(
            id,
            Rect {
                x: 100.,
                y: 120.,
                ..Rect::default()
            },
        );
        preview.marquee = Some(Rect {
            x: 220.,
            y: 10.,
            width: 20.,
            height: 30.,
        });
        preview.cable = Some(unge_interaction::CablePreview {
            from: [20.0, 180.0],
            to: [200.0, 230.0],
            valid: true,
        });
        let scene = SceneIndex::new(&document)
            .scene_with_preview(viewport, &BTreeSet::from([id]), &preview)
            .unwrap();
        assert_eq!(scene.visible_groups, 1);
        renderer.prepare(&device, &queue, &scene, viewport).unwrap();
        let mut encoder = device.create_command_encoder(&Default::default());
        renderer.render(&mut encoder, &texture.create_view(&Default::default()));
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 256 * 256 * 4,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(1024),
                    rows_per_image: Some(256),
                },
            },
            wgpu::Extent3d {
                width: 256,
                height: 256,
                depth_or_array_layers: 1,
            },
        );
        queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                tx.send(result).unwrap();
            });
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        rx.recv().unwrap().unwrap();
        let pixels = buffer.slice(..).get_mapped_range();
        let center = &pixels[(160 * 256 + 160) * 4..(160 * 256 + 160) * 4 + 4];
        let expected = (Theme::Dark.palette().node.linear()[0] * 255.).round() as u8;
        assert!(
            center[0].abs_diff(expected) <= 1,
            "node body pixel: {center:?}"
        );
        assert_eq!(center[3], 255);
        // Symmetric cubic passes through (110, 205). Check an actual curved
        // stroke independently of the node body and grid pixels.
        let cable = &pixels[(205 * 256 + 110) * 4..(205 * 256 + 110) * 4 + 4];
        assert!(cable[1] > center[1] + 50, "curve midpoint pixel: {cable:?}");

        let background = &pixels[(60 * 256 + 60) * 4..(60 * 256 + 60) * 4 + 4];
        assert!(background[0] < 18);
        let fill = &pixels[(100 * 256 + 90) * 4..(100 * 256 + 90) * 4 + 4];
        let palette = Theme::Dark.palette();
        for axis in 0..3 {
            let expected =
                ((palette.background.linear()[axis] + palette.node.linear()[axis]) * 0.5 * 255.0)
                    .round() as u8;
            assert!(fill[axis].abs_diff(expected) <= 1, "group fill: {fill:?}");
        }
        let title_pixels = (82..104)
            .flat_map(|y| (96..160).map(move |x| (y * 256 + x) * 4))
            .filter(|offset| pixels[*offset] > 100)
            .count();
        assert!(title_pixels > 10, "group title pixels: {title_pixels}");
        let baseline = pixels.to_vec();
        drop(pixels);
        buffer.unmap();
        // Same world coordinates in a smaller graph region must not stretch.
        // The inspector area is cleared and receives no grid, glyphs or geometry.
        renderer
            .prepare_sized(
                &device,
                &queue,
                &scene,
                Viewport {
                    size: [128., 256.],
                    ..viewport
                },
                [128, 256],
            )
            .unwrap();
        let mut encoder = device.create_command_encoder(&Default::default());
        renderer.render_in_region(
            &mut encoder,
            &texture.create_view(&Default::default()),
            [128, 256],
        );
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(1024),
                    rows_per_image: Some(256),
                },
            },
            wgpu::Extent3d {
                width: 256,
                height: 256,
                depth_or_array_layers: 1,
            },
        );
        queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        rx.recv().unwrap().unwrap();
        let pixels = buffer.slice(..).get_mapped_range();
        for y in 0..256 {
            for x in 0..256 {
                let offset = (y * 256 + x) * 4;
                for channel in 0..4 {
                    let expected = if x < 128 {
                        baseline[offset + channel]
                    } else {
                        (scene.background[channel] * 255.).round() as u8
                    };
                    assert!(
                        pixels[offset + channel].abs_diff(expected) <= 1,
                        "region pixel {x},{y} channel {channel}"
                    );
                }
            }
        }
        drop(pixels);
        buffer.unmap();
        assert!(device.pop_error_scope().await.is_none());
    });
}

#[test]
fn preview_reuses_committed_index_without_ghost_nodes_and_validates_geometry() {
    let doc = document();
    let id = *doc.graph().nodes().keys().next().unwrap();
    let index = SceneIndex::new(&doc);
    let viewport = Viewport {
        origin: [0., 0.],
        zoom: 1.,
        size: [256., 256.],
    };
    let mut preview = unge_interaction::Preview::default();
    preview.placement.insert(
        id,
        Rect {
            x: 1000.,
            y: 1000.,
            ..Rect::default()
        },
    );
    assert_eq!(
        index
            .scene_with_preview(viewport, &BTreeSet::new(), &preview)
            .unwrap()
            .visible_nodes,
        0
    );
    assert_eq!(
        index
            .scene_with_preview(
                Viewport {
                    origin: [1000., 1000.],
                    ..viewport
                },
                &BTreeSet::new(),
                &preview
            )
            .unwrap()
            .visible_nodes,
        1
    );
    assert_eq!(
        index
            .scene(viewport, &BTreeSet::new())
            .unwrap()
            .visible_nodes,
        1
    );
    assert_eq!(doc.placement()[&id].x, 20.);
    preview.placement.get_mut(&id).unwrap().x = f32::NAN;
    assert!(
        index
            .scene_with_preview(viewport, &BTreeSet::new(), &preview)
            .is_err()
    );
}

#[test]
fn labels_localize_without_changing_ports_follow_previews_and_obey_lod() {
    let mut editor = Editor::new(document(), 0).unwrap();
    let mut node = editor
        .document()
        .graph()
        .nodes()
        .values()
        .next()
        .unwrap()
        .clone();
    let id = node.id;
    editor.execute(Command::RemoveNode { id }).unwrap();
    let port = |name: &str| Port {
        name: name.into(),
        data_type: DataType::Float,
        cardinality: Cardinality::Single,
        required: false,
    };
    node.inputs = vec![port("a"), port("b")];
    node.outputs = vec![port("value")];
    editor
        .execute(Command::AddNode {
            node,
            rect: Rect::default(),
        })
        .unwrap();
    let index = SceneIndex::new(editor.document());
    let catalog = [(
        "test".into(),
        NodeLabels {
            title: LabelText {
                en: "Sum".into(),
                ja: "加算".into(),
                zh_cn: "加法".into(),
            },
            inputs: [(
                "a".into(),
                LabelText {
                    en: "Input A".into(),
                    ja: "入力 A".into(),
                    zh_cn: "输入 A".into(),
                },
            )]
            .into(),
            ..Default::default()
        },
    )]
    .into();
    let view = Viewport {
        origin: [0., 0.],
        zoom: 1.,
        size: [512., 512.],
    };
    let mut preview = unge_interaction::Preview::default();
    preview.placement.insert(
        id,
        Rect {
            x: 80.,
            y: 50.,
            ..Rect::default()
        },
    );
    for (locale, title, input) in [
        (Locale::En, "Sum", "Input A"),
        (Locale::Ja, "加算", "入力 A"),
        (Locale::ZhCn, "加法", "输入 A"),
    ] {
        let scene = index
            .scene_with_labels(view, &BTreeSet::new(), &preview, &catalog, locale)
            .unwrap();
        assert_eq!(scene.labels.len(), 4);
        assert_eq!(scene.labels[0].text, title);
        assert_eq!(scene.labels[1].text, input);
        assert_eq!(scene.labels[2].text, "b");
        assert_eq!(scene.labels[3].text, "value");
        assert!(scene.labels[3].right_aligned);
        assert_eq!(scene.labels[0].rect.x, 92.);
        assert!(
            scene
                .labels
                .iter()
                .all(|l| l.after_quad == scene.quads.len())
        );
    }
    assert_eq!(
        index
            .scene(Viewport { zoom: 0.5, ..view }, &BTreeSet::new())
            .unwrap()
            .labels
            .len(),
        1
    );
    assert_eq!(
        index
            .scene(Viewport { zoom: 0.65, ..view }, &BTreeSet::new())
            .unwrap()
            .labels
            .len(),
        1
    );
    assert!(
        index
            .scene(
                Viewport {
                    origin: [2000., 2000.],
                    ..view
                },
                &BTreeSet::new()
            )
            .unwrap()
            .labels
            .is_empty()
    );
    assert_eq!(editor.document().graph().nodes()[&id].inputs[0].name, "a");
}

#[test]
fn label_metadata_is_bounded_and_stacking_matches_node_order() {
    let mut editor = Editor::new(document(), 0).unwrap();
    let first = editor
        .document()
        .graph()
        .nodes()
        .values()
        .next()
        .unwrap()
        .clone();
    editor
        .execute(Command::AddNode {
            node: Node {
                id: Id::new_v4(),
                ..first
            },
            rect: Rect::default(),
        })
        .unwrap();
    let catalog = [(
        "test".into(),
        NodeLabels {
            title: LabelText {
                en: format!("line\n{}", "数".repeat(1000)),
                ..Default::default()
            },
            ..Default::default()
        },
    )]
    .into();
    let scene = SceneIndex::new(editor.document())
        .scene_with_labels(
            Viewport {
                origin: [0., 0.],
                zoom: 1.,
                size: [256., 256.],
            },
            &BTreeSet::new(),
            &Default::default(),
            &catalog,
            Locale::En,
        )
        .unwrap();
    assert_eq!(scene.labels.len(), 2);
    assert_eq!(scene.labels[0].text.chars().count(), 256);
    assert!(!scene.labels[0].text.contains('\n'));
    assert!(scene.labels[0].after_quad < scene.labels[1].after_quad);
}

#[test]
#[ignore = "requires a GPU and system fonts covering English, Japanese and Simplified Chinese"]
fn gpu_text_clips_stacks_and_renders_cjk_at_two_pixel_densities() {
    pollster::block_on(async {
        let adapter = wgpu::Instance::default()
            .request_adapter(&Default::default())
            .await
            .unwrap();
        let (device, queue) = adapter.request_device(&Default::default()).await.unwrap();
        device.push_error_scope(wgpu::ErrorFilter::Validation);
        let mut renderer = GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm);
        let mut editor = Editor::new(Document::default(), 0).unwrap();
        for (id, x) in [(1, 20.), (2, 100.)] {
            editor
                .execute(Command::AddNode {
                    node: Node {
                        id: Id::from_u128(id),
                        type_id: "test".into(),
                        inputs: vec![],
                        outputs: vec![],
                        properties: Properties::new(),
                    },
                    rect: Rect {
                        x,
                        y: 20.,
                        width: 180.,
                        height: 90.,
                    },
                })
                .unwrap();
        }
        let view = Viewport {
            origin: [0., 0.],
            zoom: 1.,
            size: [256., 256.],
        };
        let mut scene = SceneIndex::new(editor.document())
            .scene(view, &BTreeSet::new())
            .unwrap();
        scene.labels.truncate(1);
        scene.labels[0].text = "MMMMMMMMMMMMMMMMMMMM".into();
        scene.labels.push(TextLabel {
            text: "数値 加算".into(),
            rect: Rect {
                x: 20.,
                y: 140.,
                width: 180.,
                height: 22.,
            },
            font_size: 16.,
            right_aligned: false,
            color: [1.; 4],
            after_quad: scene.quads.len(),
        });
        scene.labels.push(TextLabel {
            text: "输入 数值".into(),
            rect: Rect {
                x: 20.,
                y: 175.,
                width: 180.,
                height: 22.,
            },
            ..scene.labels[1].clone()
        });
        // Check clipping independently of occlusion, with an intentionally narrow box.
        scene.labels.push(TextLabel {
            text: "MMMMMMMMM".into(),
            rect: Rect {
                x: 20.,
                y: 210.,
                width: 25.,
                height: 22.,
            },
            ..scene.labels[1].clone()
        });
        for density in [1u32, 2] {
            let side = 256 * density;
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d {
                    width: side,
                    height: side,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            renderer
                .prepare_sized(&device, &queue, &scene, view, [side, side])
                .unwrap();
            assert_eq!(
                renderer.text_stats().missing_glyphs,
                0,
                "install CJK fonts for this desktop test"
            );
            assert!(renderer.text_stats().visible_glyphs > 15);
            let entries = renderer.text_stats().atlas_entries;
            // A repeated frame reuses both shaping and atlas entries.
            renderer
                .prepare_sized(&device, &queue, &scene, view, [side, side])
                .unwrap();
            assert_eq!(renderer.text_stats().atlas_entries, entries);
            let mut encoder = device.create_command_encoder(&Default::default());
            renderer.render(&mut encoder, &texture.create_view(&Default::default()));
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: u64::from(side * side * 4),
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            encoder.copy_texture_to_buffer(
                texture.as_image_copy(),
                wgpu::TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(side * 4),
                        rows_per_image: Some(side),
                    },
                },
                wgpu::Extent3d {
                    width: side,
                    height: side,
                    depth_or_array_layers: 1,
                },
            );
            queue.submit([encoder.finish()]);
            let (tx, rx) = std::sync::mpsc::channel();
            buffer
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |result| tx.send(result).unwrap());
            device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
            rx.recv().unwrap().unwrap();
            let pixels = buffer.slice(..).get_mapped_range();
            let bright = |x0, y0, x1, y1| -> usize {
                (y0 * density..y1 * density)
                    .flat_map(|y| {
                        (x0 * density..x1 * density).map(move |x| (y * side + x) as usize * 4)
                    })
                    .filter(|i| pixels[*i] > 120)
                    .count()
            };
            assert!(bright(33, 25, 95, 43) > 60, "English title visible");
            assert_eq!(
                bright(110, 25, 190, 43),
                0,
                "foreground node hides underlying title"
            );
            assert!(bright(20, 140, 150, 162) > 100, "Japanese glyphs visible");
            assert!(bright(20, 175, 150, 197) > 100, "Chinese glyphs visible");
            assert!(bright(20, 210, 45, 232) > 40, "narrow label visible");
            assert_eq!(bright(46, 210, 150, 232), 0, "text cannot escape clip rect");
            drop(pixels);
            buffer.unmap();
        }
        let mut invalid = scene;
        invalid.labels[0].after_quad = usize::MAX;
        assert!(renderer.prepare(&device, &queue, &invalid, view).is_err());
        assert!(device.pop_error_scope().await.is_none());
    });
}
