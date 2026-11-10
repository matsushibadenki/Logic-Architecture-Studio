use std::collections::BTreeSet;
use unge_core::*;
use unge_interaction::{CablePreview, Preview};
use unge_render::*;

fn document() -> Document {
    let mut editor = Editor::new(Document::default(), 0).unwrap();
    editor
        .execute(Command::AddNode {
            node: Node {
                id: Id::from_u128(1),
                type_id: "number".into(),
                inputs: vec![],
                outputs: vec![Port {
                    name: "value".into(),
                    data_type: DataType::Float,
                    cardinality: Cardinality::Single,
                    required: false,
                }],
                properties: Properties::new(),
            },
            rect: Rect {
                x: 20.,
                y: 20.,
                ..Rect::default()
            },
        })
        .unwrap();
    editor.document().clone()
}
fn viewport() -> Viewport {
    Viewport {
        origin: [0., 0.],
        zoom: 1.,
        size: [256., 256.],
    }
}
fn scene(theme: Theme, preview: &Preview) -> Scene {
    SceneIndex::new(&document())
        .scene_with_theme(
            viewport(),
            &BTreeSet::from([Id::from_u128(1)]),
            preview,
            &LabelCatalog::new(),
            Locale::En,
            theme,
        )
        .unwrap()
}
#[test]
fn theme_changes_all_presentation_colors_without_changing_geometry_or_labels() {
    let preview = Preview {
        marquee: Some(Rect {
            x: 210.,
            y: 130.,
            width: 20.,
            height: 20.,
        }),
        cable: Some(CablePreview {
            from: [200., 65.],
            to: [220., 80.],
            valid: false,
        }),
        ..Default::default()
    };
    let dark = scene(Theme::Dark, &preview);
    let light = scene(Theme::Light, &preview);
    assert_eq!(dark.quads.len(), light.quads.len());
    assert_eq!(
        (dark.visible_nodes, dark.visible_edges),
        (light.visible_nodes, light.visible_edges)
    );
    assert_ne!(dark.background, light.background);
    assert_ne!(dark.grid_color, light.grid_color);
    for (a, b) in dark.quads.iter().zip(&light.quads) {
        assert_eq!(a.rect, b.rect);
        assert_eq!(a.params, b.params);
        assert_ne!(a.color, b.color);
    }
    for (a, b) in dark.labels.iter().zip(&light.labels) {
        assert_eq!(a.text, b.text);
        assert_eq!(a.rect.x, b.rect.x);
        assert_eq!(a.rect.y, b.rect.y);
        assert_eq!(a.after_quad, b.after_quad);
        assert_ne!(a.color, b.color);
    }
    let palette = Theme::Light.palette();
    assert!(
        light
            .quads
            .iter()
            .any(|q| q.color == palette.accent.linear())
    );
    assert!(light.quads.iter().any(|q| q.color == palette.node.linear()));
    assert!(
        light
            .quads
            .iter()
            .any(|q| q.color == palette.border.linear())
    );
    assert_eq!(light.labels[0].color, palette.text.linear());
    assert_eq!(light.labels[1].color, palette.muted.linear());
    assert!(
        light
            .quads
            .iter()
            .any(|q| q.color == palette.cable_invalid.linear())
    );
    let mut valid = preview;
    valid.cable.as_mut().unwrap().valid = true;
    assert!(
        scene(Theme::Light, &valid)
            .quads
            .iter()
            .any(|q| q.color == palette.cable_valid.linear())
    );
    assert_eq!(
        SceneIndex::new(&document())
            .scene(viewport(), &BTreeSet::new())
            .unwrap()
            .background,
        Theme::Dark.palette().background.linear()
    );
}

fn pixels(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    renderer: &mut GpuRenderer,
    scene: &Scene,
) -> Vec<u8> {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("theme pixels"),
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
    renderer.prepare(device, queue, scene, viewport()).unwrap();
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
        .map_async(wgpu::MapMode::Read, move |result| tx.send(result).unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    rx.recv().unwrap().unwrap();
    let bytes = buffer.slice(..).get_mapped_range().to_vec();
    buffer.unmap();
    bytes
}
fn assert_color(bytes: &[u8], x: usize, y: usize, color: ThemeColor) {
    let i = (y * 256 + x) * 4;
    for (actual, expected) in bytes[i..i + 4]
        .iter()
        .zip(color.linear().map(|v| (v * 255.).round() as u8))
    {
        assert!(
            actual.abs_diff(expected) <= 2,
            "pixel {x}/{y}: {:?} expected {:?}",
            &bytes[i..i + 4],
            color.linear()
        );
    }
}
#[test]
#[ignore = "requires a working GPU adapter and fonts; run explicitly on a desktop"]
fn gpu_switches_background_grid_nodes_ports_and_text_without_stale_resources() {
    pollster::block_on(async {
        let adapter = wgpu::Instance::default()
            .request_adapter(&Default::default())
            .await
            .unwrap();
        let (device, queue) = adapter.request_device(&Default::default()).await.unwrap();
        device.push_error_scope(wgpu::ErrorFilter::Validation);
        let mut renderer = GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm);
        let mut entries = None;
        for theme in [Theme::Dark, Theme::Light, Theme::Dark] {
            let mut scene = scene(theme, &Preview::default());
            let with_text = pixels(&device, &queue, &mut renderer, &scene);
            let palette = theme.palette();
            assert_color(&with_text, 230, 230, palette.background);
            // The grid is antialiased; pixel centres blend grid and background.
            let grid_pixel = &with_text[(240 * 256 + 240) * 4..(240 * 256 + 240) * 4 + 3];
            let background = palette
                .background
                .linear()
                .map(|v| (v * 255.).round() as u8);
            let grid = palette.grid.linear().map(|v| (v * 255.).round() as u8);
            for i in 0..3 {
                assert!(grid_pixel[i] >= grid[i].min(background[i]).saturating_sub(1));
                assert!(grid_pixel[i] <= grid[i].max(background[i]).saturating_add(1));
            }
            assert!(grid_pixel[2].abs_diff(background[2]) >= 2);
            assert_color(&with_text, 100, 95, palette.node);
            assert_color(&with_text, 200, 65, palette.border);
            assert_eq!(renderer.text_stats().missing_glyphs, 0);
            if let Some(n) = entries {
                assert_eq!(renderer.text_stats().atlas_entries, n);
            } else {
                entries = Some(renderer.text_stats().atlas_entries);
            }
            scene.labels.clear();
            let without_text = pixels(&device, &queue, &mut renderer, &scene);
            let mut changed = 0;
            for y in 24..43 {
                for x in 32..125 {
                    let i = (y * 256 + x) * 4;
                    if with_text[i] != without_text[i] {
                        changed += 1;
                        match theme {
                            Theme::Dark => assert!(with_text[i] > without_text[i]),
                            Theme::Light => assert!(with_text[i] < without_text[i]),
                        }
                    }
                }
            }
            assert!(changed > 50, "readable title pixels in {theme:?}");
            // Clear colour must also change when no geometry is present.
            scene.quads.clear();
            let empty = pixels(&device, &queue, &mut renderer, &scene);
            assert_color(&empty, 60, 60, palette.background);
        }
        let mut invalid = scene(Theme::Light, &Preview::default());
        invalid.background[0] = f32::NAN;
        assert!(
            renderer
                .prepare(&device, &queue, &invalid, viewport())
                .is_err()
        );
        assert!(device.pop_error_scope().await.is_none());
    });
}
