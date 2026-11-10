use std::collections::BTreeSet;
use unge_core::*;
use unge_render::*;

#[test]
#[ignore = "requires a real desktop GPU"]
fn profiler_reuses_queries_and_handles_devices_without_timestamps() {
    pollster::block_on(async {
        let adapter = wgpu::Instance::default()
            .request_adapter(&Default::default())
            .await
            .unwrap();
        for features in [
            wgpu::Features::empty(),
            adapter.features() & wgpu::Features::TIMESTAMP_QUERY,
        ] {
            let (device, queue) = adapter
                .request_device(&wgpu::DeviceDescriptor {
                    required_features: features,
                    ..Default::default()
                })
                .await
                .unwrap();
            device.push_error_scope(wgpu::ErrorFilter::Validation);
            let size = [256, 128];
            let target = device
                .create_texture(&wgpu::TextureDescriptor {
                    label: None,
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
                })
                .create_view(&Default::default());
            let viewport = Viewport {
                origin: [0., 0.],
                zoom: 1.,
                size: [256., 128.],
            };
            let scene = SceneIndex::new(&Document::default())
                .scene(viewport, &BTreeSet::new())
                .unwrap();
            let mut renderer = GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8UnormSrgb);
            let mut profiler = FrameProfiler::new(&device);
            let mut valid_timestamps = 0;
            for _ in 0..8 {
                let times = profiler
                    .measure(
                        &device,
                        &queue,
                        &mut renderer,
                        &target,
                        &scene,
                        viewport,
                        size,
                    )
                    .unwrap();
                for value in [
                    times.prepare_ms,
                    times.encode_submit_ms,
                    times.completion_wait_ms,
                    times.completed_frame_ms,
                ] {
                    assert!(value.is_finite() && value >= 0.);
                }
                assert!(times.completed_frame_ms >= times.prepare_ms + times.encode_submit_ms);
                if features.is_empty() {
                    assert!(times.gpu_pass_ms.is_none());
                }
                if let Some(value) = times.gpu_pass_ms {
                    valid_timestamps += 1;
                    assert!(value.is_finite() && value >= 0.);
                }
            }
            if !features.is_empty() {
                assert_eq!(
                    valid_timestamps, 8,
                    "reused queries returned invalid timestamps"
                );
            }
            assert!(
                profiler
                    .measure(
                        &device,
                        &queue,
                        &mut renderer,
                        &target,
                        &scene,
                        Viewport {
                            zoom: f32::NAN,
                            ..viewport
                        },
                        size
                    )
                    .is_err()
            );
            profiler
                .measure(
                    &device,
                    &queue,
                    &mut renderer,
                    &target,
                    &scene,
                    viewport,
                    size,
                )
                .unwrap();
            assert!(device.pop_error_scope().await.is_none());
        }
    });
}
