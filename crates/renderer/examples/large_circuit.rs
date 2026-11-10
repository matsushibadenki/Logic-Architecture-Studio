//! Reproducible offscreen benchmark. No user workspace or native UI is changed.
#[path = "../tests/support/mod.rs"]
mod support;
use serde_json::json;
use std::{collections::BTreeSet, time::Instant};
use studio_renderer::{project, retained::RetainedProjection};
use unge_core::{Locale, Viewport};
use unge_render::{FrameProfiler, GpuRenderer, SceneIndex};

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.
}
fn summary(mut v: Vec<f64>) -> serde_json::Value {
    v.sort_by(f64::total_cmp);
    json!({"p50":v[v.len()/2],"p95":v[(v.len()*95/100).min(v.len()-1)]})
}
fn main() {
    pollster::block_on(async {
        let adapter = wgpu::Instance::default()
            .request_adapter(&Default::default())
            .await
            .expect("GPU adapter required");
        let features = adapter.features() & wgpu::Features::TIMESTAMP_QUERY;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                required_features: features,
                ..Default::default()
            })
            .await
            .unwrap();
        device.push_error_scope(wgpu::ErrorFilter::Validation);
        let size = [1600, 1000];
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("large circuit benchmark"),
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let target = texture.create_view(&Default::default());
        let mut results = vec![];
        for count in [100, 500, 2000] {
            for mode in ["overview", "detail"] {
                let mut doc = support::circuit(count);
                doc.validate(false).unwrap();
                let mid = count / 2;
                let view = Viewport {
                    origin: if mode == "detail" {
                        [doc.nodes[mid].x - 400., doc.nodes[mid].y - 300.]
                    } else {
                        [-30., -30.]
                    },
                    size: size.map(|n| n as f32),
                    zoom: if mode == "detail" { 0.8 } else { 0.1 },
                };
                let mut retained = RetainedProjection::new(&doc).unwrap();
                let mut gpu = GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8UnormSrgb);
                let mut profiler = FrameProfiler::new(&device);
                let selected = BTreeSet::new();
                let initial = retained
                    .index
                    .scene_with_labels(
                        view,
                        &selected,
                        &Default::default(),
                        &retained.labels,
                        Locale::En,
                    )
                    .unwrap();
                profiler
                    .measure(&device, &queue, &mut gpu, &target, &initial, view, size)
                    .unwrap();
                let (mut full, mut delta, mut prepare, mut completed, mut pass) =
                    (vec![], vec![], vec![], vec![], vec![]);
                let mut last = serde_json::Value::Null;
                for sample in 0..35 {
                    doc.nodes[mid].x += if sample % 2 == 0 { 18. } else { -18. };
                    let t = Instant::now();
                    let (fresh, labels, _) = project(&doc).unwrap();
                    let scene = SceneIndex::new(&fresh)
                        .scene_with_labels(
                            view,
                            &selected,
                            &Default::default(),
                            &labels,
                            Locale::En,
                        )
                        .unwrap();
                    let full_ms = ms(t);
                    let t = Instant::now();
                    retained.sync(&doc).unwrap();
                    let incremental = retained
                        .index
                        .scene_with_labels(
                            view,
                            &selected,
                            &Default::default(),
                            &retained.labels,
                            Locale::En,
                        )
                        .unwrap();
                    let delta_ms = ms(t);
                    assert_eq!(
                        bytemuck::cast_slice::<_, u8>(&scene.quads),
                        bytemuck::cast_slice::<_, u8>(&incremental.quads)
                    );
                    assert_eq!(scene.labels, incremental.labels);
                    let timing = profiler
                        .measure(&device, &queue, &mut gpu, &target, &incremental, view, size)
                        .unwrap();
                    if sample >= 5 {
                        full.push(full_ms);
                        delta.push(delta_ms);
                        prepare.push(timing.prepare_ms);
                        completed.push(timing.completed_frame_ms);
                        if let Some(p) = timing.gpu_pass_ms {
                            pass.push(p);
                        }
                    }
                    last = json!({"visible_nodes":incremental.visible_nodes,"visible_edges":incremental.visible_edges,"geometry":incremental.geometry_stats,"quad_upload":gpu.upload_stats(),"full_quad_bytes":incremental.quads.len()*std::mem::size_of::<unge_render::Quad>(),"glyph_upload_bytes":gpu.text_stats().uploaded_bytes});
                }
                results.push(json!({"nodes":count,"edges":doc.edges.len(),"view":mode,"samples":30,"full_projection_and_scene_ms":summary(full),"delta_projection_and_scene_ms":summary(delta),"gpu_prepare_ms":summary(prepare),"completed_offscreen_frame_ms":summary(completed),"gpu_pass_ms":if pass.is_empty() {serde_json::Value::Null} else {summary(pass)},"last_frame":last}));
            }
        }
        assert!(device.pop_error_scope().await.is_none());
        println!("{}",serde_json::to_string_pretty(&json!({"adapter":format!("{:?}",adapter.get_info()),"profile":if cfg!(debug_assertions) {"debug"} else {"release"},"physical_size":size,"warmup_frames":5,"results":results})).unwrap());
    });
}
