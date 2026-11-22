//! Reproducible offscreen benchmark. No user workspace or native UI is changed.
#[path = "../tests/support/mod.rs"]
mod support;
use serde_json::json;
use std::{
    collections::BTreeSet,
    time::{Duration, Instant},
};
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
#[cfg(target_os = "macos")]
fn thread_cpu_time_ns() -> u64 {
    unsafe extern "C" {
        fn clock_gettime_nsec_np(clock_id: std::ffi::c_int) -> u64;
    }
    // Darwin's CLOCK_THREAD_CPUTIME_ID measures only this thread while running.
    const CLOCK_THREAD_CPUTIME_ID: std::ffi::c_int = 16;
    unsafe { clock_gettime_nsec_np(CLOCK_THREAD_CPUTIME_ID) }
}

#[cfg(not(target_os = "macos"))]
fn thread_cpu_time_ns() -> u64 {
    0
}

fn main() {
    let stress = std::env::args().any(|arg| arg == "--stress");
    let replay = std::env::args().any(|arg| arg == "--drag-replay");
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
        let counts: &[usize] = if stress || replay {
            &[2000]
        } else {
            &[100, 500, 2000]
        };
        for &count in counts {
            for mode in ["overview", "detail"] {
                let mut doc = support::circuit(count);
                if replay && mode != "overview" {
                    continue;
                }
                if stress || replay {
                    // Keep the same edge count while feeding one input to every adder.
                    for edge in doc.edges.iter_mut().filter(|edge| edge.port == "b") {
                        edge.from = "n0".into();
                    }
                }
                doc.validate(false).unwrap();
                let moved = if stress || replay {
                    vec![
                        "n0".to_owned(),
                        "n512".to_owned(),
                        "n1024".to_owned(),
                        "n1536".to_owned(),
                    ]
                } else {
                    vec![doc.nodes[count / 2].id.clone()]
                };
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
                if replay {
                    let period = Duration::from_micros(4_000);
                    let path: Vec<_> = (0..250)
                        .map(|i| {
                            let phase = (i % 100) as f32;
                            let x = 50. - (phase - 50.).abs();
                            let mut preview = unge_interaction::Preview::default();
                            for name in &moved {
                                let id = retained.ids[name];
                                let mut rect = *retained.document().placement().get(&id).unwrap();
                                rect.x += x * 1.5;
                                rect.y += x * 0.75;
                                preview.placement.insert(id, rect);
                            }
                            preview
                        })
                        .collect();
                    let selection = moved.iter().map(|name| retained.ids[name]).collect();
                    for preview in path.iter().take(5) {
                        let scene = retained
                            .index
                            .scene_with_labels(
                                view,
                                &selection,
                                preview,
                                &retained.labels,
                                Locale::En,
                            )
                            .unwrap();
                        profiler
                            .measure(&device, &queue, &mut gpu, &target, &scene, view, size)
                            .unwrap();
                    }
                    let start = Instant::now();
                    let mut next = 0;
                    let (mut lag, mut scene_ms, mut routing) = (vec![], vec![], vec![]);
                    let mut coalesced = 0;
                    while next < path.len() {
                        let deadline = start + period * next as u32;
                        if let Some(wait) = deadline.checked_duration_since(Instant::now()) {
                            std::thread::sleep(wait);
                        }
                        let available = ((start.elapsed().as_micros() / period.as_micros())
                            as usize)
                            .min(path.len() - 1)
                            .max(next);
                        coalesced += available - next;
                        let (scene, stages) = retained
                            .index
                            .scene_with_theme_timed(
                                view,
                                &selection,
                                &path[available],
                                &retained.labels,
                                Locale::En,
                                unge_render::Theme::default(),
                            )
                            .unwrap();
                        profiler
                            .measure(&device, &queue, &mut gpu, &target, &scene, view, size)
                            .unwrap();
                        lag.push(ms(start + period * available as u32));
                        scene_ms.push(stages.total_ms);
                        routing.push(stages.edge_query_ms);
                        next = available + 1;
                    }
                    results.push(json!({"view":mode,"nodes":count,"edges":doc.edges.len(),"moved":moved,"input_hz":250,"input_events":path.len(),"coalesced":coalesced,"frames":lag.len(),"latest_input_to_gpu_complete_ms":summary(lag),"scene_assembly_ms":summary(scene_ms),"preview_routing_and_edge_query_ms":summary(routing)}));
                    continue;
                }
                let (mut full, mut delta, mut prepare, mut completed, mut pass) =
                    (vec![], vec![], vec![], vec![], vec![]);
                let (mut sync, mut assembly) = (vec![], vec![]);
                let (mut edge_geometry, mut node_geometry, mut edge_query, mut node_query) =
                    (vec![], vec![], vec![], vec![]);
                let mut slow_frames = Vec::new();
                let mut cpu_assembly = Vec::new();
                let mut last = serde_json::Value::Null;
                for sample in 0..35 {
                    let offset = if sample % 2 == 0 { 18. } else { -18. };
                    for id in &moved {
                        doc.nodes.update(id, |node| node.x += offset).unwrap();
                    }
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
                    let sync_ms = ms(t);
                    let assembly_start = Instant::now();
                    let cpu_start = thread_cpu_time_ns();
                    let (incremental, stages) = retained
                        .index
                        .scene_with_theme_timed(
                            view,
                            &selected,
                            &Default::default(),
                            &retained.labels,
                            Locale::En,
                            unge_render::Theme::default(),
                        )
                        .unwrap();
                    let cpu_end = thread_cpu_time_ns();
                    let assembly_ms = ms(assembly_start);
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
                        sync.push(sync_ms);
                        assembly.push(assembly_ms);
                        let cpu_ms = (cpu_start != 0 && cpu_end >= cpu_start)
                            .then(|| (cpu_end - cpu_start) as f64 / 1_000_000.);
                        cpu_assembly.extend(cpu_ms);
                        slow_frames.push((assembly_ms, sample, stages.clone(), cpu_ms));
                        edge_geometry.push(stages.edge_geometry_ms);
                        node_geometry.push(stages.node_geometry_ms);
                        edge_query.push(stages.edge_query_ms);
                        node_query.push(stages.node_query_ms);
                        prepare.push(timing.prepare_ms);
                        completed.push(timing.completed_frame_ms);
                        if let Some(p) = timing.gpu_pass_ms {
                            pass.push(p);
                        }
                    }
                    last = json!({"visible_nodes":incremental.visible_nodes,"visible_edges":incremental.visible_edges,"geometry":incremental.geometry_stats,"quad_upload":gpu.upload_stats(),"full_quad_bytes":incremental.quads.len()*std::mem::size_of::<unge_render::Quad>(),"glyph_upload_bytes":gpu.text_stats().uploaded_bytes});
                }
                slow_frames.sort_unstable_by(|a, b| b.0.total_cmp(&a.0));
                let slow_frames: Vec<_> = slow_frames.into_iter().take(3)
                    .map(|(assembly_ms, sample, stages, cpu_ms)| json!({"sample": sample, "assembly_ms": assembly_ms, "thread_cpu_ms":cpu_ms, "stages": stages}))
                    .collect();
                results.push(json!({"slow_frames":slow_frames,"scene_thread_cpu_ms":(!cpu_assembly.is_empty()).then(|| summary(cpu_assembly)),"nodes":count,"edges":doc.edges.len(),"view":mode,"samples":30,"workload":if stress {"wide_fanout_multiblock"} else {"single_move"},"moved_nodes":moved.len(),"delta_sync_ms":summary(sync),"scene_assembly_ms":summary(assembly),"edge_geometry_ms":summary(edge_geometry),"node_geometry_ms":summary(node_geometry),"edge_query_ms":summary(edge_query),"node_query_ms":summary(node_query),"full_projection_and_scene_ms":summary(full),"delta_projection_and_scene_ms":summary(delta),"gpu_prepare_ms":summary(prepare),"completed_offscreen_frame_ms":summary(completed),"gpu_pass_ms":if pass.is_empty() {serde_json::Value::Null} else {summary(pass)},"last_frame":last}));
            }
        }
        assert!(device.pop_error_scope().await.is_none());
        println!("{}",serde_json::to_string_pretty(&json!({"adapter":format!("{:?}",adapter.get_info()),"profile":if cfg!(debug_assertions) {"debug"} else {"release"},"physical_size":size,"warmup_frames":5,"results":results})).unwrap());
    });
}
