// Readback helper adapted from UNGE's gallery (MIT); see THIRD_PARTY_NOTICES.md.
use std::{collections::BTreeSet, io::Write};
use unge_core::{Locale, Viewport};
use unge_render::{GpuRenderer, Scene, SceneIndex, Theme};
#[test]
#[ignore = "Requires a real GPU and system fonts"]
fn delta_gpu_uploads_match_fresh_buffers_after_edits_and_reallocation() {
    pollster::block_on(async {
        let adapter = wgpu::Instance::default()
            .request_adapter(&Default::default())
            .await
            .unwrap();
        let (device, queue) = adapter.request_device(&Default::default()).await.unwrap();
        device.push_error_scope(wgpu::ErrorFilter::Validation);
        let mut doc = design_core::Document::demo();
        let mut cache = studio_renderer::retained::RetainedProjection::new(&doc).unwrap();
        let mut gpu = GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8UnormSrgb);
        let view = Viewport {
            origin: [-60., -60.],
            zoom: 1.,
            size: [1280., 800.],
        };
        for step in 0..7 {
            match step {
                1 => doc.nodes.update("add", |n| n.x += 20.).unwrap(),
                2 => doc.nodes.iter_mut().find(|n| n.id == "add").unwrap().name = "sum".into(),
                4 => {
                    for i in 0..40 {
                        let mut n = doc.nodes[0].clone();
                        n.id = format!("extra{i}");
                        n.name = n.id.clone();
                        n.x = 100.;
                        n.y = 100.;
                        doc.nodes.push(n);
                    }
                }
                5 => doc = design_core::Document::demo(),
                _ => {}
            }
            cache.sync(&doc).unwrap();
            let selection = if step == 3 {
                BTreeSet::from([cache.ids["add"]])
            } else {
                BTreeSet::new()
            };
            let scene = cache
                .index
                .scene_with_theme(
                    view,
                    &selection,
                    &Default::default(),
                    &cache.labels,
                    Locale::En,
                    Theme::Dark,
                )
                .unwrap();
            let actual = render(&device, &queue, &mut gpu, &scene, view);
            let (fresh, labels, _) = studio_renderer::project(&doc).unwrap();
            let expected = SceneIndex::new(&fresh)
                .scene_with_theme(
                    view,
                    &selection,
                    &Default::default(),
                    &labels,
                    Locale::En,
                    Theme::Dark,
                )
                .unwrap();
            let mut fresh_gpu = GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8UnormSrgb);
            assert_eq!(
                actual,
                render(&device, &queue, &mut fresh_gpu, &expected, view),
                "step {step}"
            );
            if step == 6 {
                assert_eq!(gpu.upload_stats().quad_bytes, 0);
                assert_eq!(gpu.text_stats().uploaded_bytes, 0);
            }
        }
        assert!(device.pop_error_scope().await.is_none());
    });
}
#[test]
#[ignore = "Requires a real GPU and system CJK fonts"]
fn native_circuit_renders_three_languages() {
    pollster::block_on(async {
        let adapter = wgpu::Instance::default()
            .request_adapter(&Default::default())
            .await
            .expect("GPU adapter");
        let (device, queue) = adapter.request_device(&Default::default()).await.unwrap();
        device.push_error_scope(wgpu::ErrorFilter::Validation);
        let (document, catalog, ids) =
            studio_renderer::project(&design_core::Document::demo()).unwrap();
        let index = SceneIndex::new(&document);
        let mut renderer = GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8UnormSrgb);
        let viewport = Viewport {
            origin: [-60., -60.],
            zoom: 1.,
            size: [1280., 800.],
        };
        for (locale, name) in [(Locale::En, "en"), (Locale::Ja, "ja"), (Locale::ZhCn, "zh")] {
            let scene = index
                .scene_with_theme(
                    viewport,
                    &BTreeSet::from([ids["add"]]),
                    &Default::default(),
                    &catalog,
                    locale,
                    Theme::Dark,
                )
                .unwrap();
            let pixels = render(&device, &queue, &mut renderer, &scene, viewport);
            assert_eq!(
                renderer.text_stats().missing_glyphs,
                0,
                "Missing glyphs: {name}"
            );
            assert!(
                pixels
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .filter(|p| p[0] > 70 || p[1] > 70 || p[2] > 70)
                    .count()
                    > 10000,
                "Circuit must contain visible geometry and text"
            );
            if let Ok(dir) = std::env::var("LAS_GPU_SNAPSHOT") {
                std::fs::create_dir_all(&dir).unwrap();
                let mut f = std::fs::File::create(format!("{dir}/circuit-{name}.ppm")).unwrap();
                write!(f, "P6\n1280 800\n255\n").unwrap();
                for p in pixels.as_chunks::<4>().0.iter() {
                    f.write_all(&p[..3]).unwrap();
                }
            }
        }
        assert!(device.pop_error_scope().await.is_none());
    });
}
fn render(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    renderer: &mut GpuRenderer,
    scene: &Scene,
    view: Viewport,
) -> Vec<u8> {
    let size = wgpu::Extent3d {
        width: 1280,
        height: 800,
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("node gallery"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    renderer.prepare(device, queue, scene, view).unwrap();
    let mut encoder = device.create_command_encoder(&Default::default());
    renderer.render(&mut encoder, &texture.create_view(&Default::default()));
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 1280 * 800 * 4,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(1280 * 4),
                rows_per_image: Some(800),
            },
        },
        size,
    );
    queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    rx.recv().unwrap().unwrap();
    let pixels = buffer.slice(..).get_mapped_range().to_vec();
    buffer.unmap();
    pixels
}
