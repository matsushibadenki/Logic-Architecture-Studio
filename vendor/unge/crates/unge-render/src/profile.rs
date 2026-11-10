//! Explicit native diagnostic path. Never use blocking profiling in normal animation.
use crate::{GpuRenderer, Scene};
use std::time::{Duration, Instant};
use unge_core::Viewport;

#[derive(Debug, Clone, serde::Serialize)]
pub struct FrameTimings {
    /// CPU time for geometry upload, text shaping and atlas preparation.
    pub prepare_ms: f64,
    pub encode_submit_ms: f64,
    /// CPU wait for completion/readback; not GPU execution time.
    pub completion_wait_ms: f64,
    /// Render-pass timestamps only; excludes upload, presentation and CPU work.
    pub gpu_pass_ms: Option<f64>,
    /// Includes preparation, submission and completion/readback. No presentation.
    pub completed_frame_ms: f64,
}
struct Queries {
    set: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    readback: wgpu::Buffer,
}
/// Retains two timestamps and a 16-byte readback buffer. Uses the supplied device.
/// Enable TIMESTAMP_QUERY when requesting the device to get gpu_pass_ms.
/// Without it, GPU completion is still awaited and gpu_pass_ms is None.
pub struct FrameProfiler {
    queries: Option<Queries>,
}
impl FrameProfiler {
    pub fn new(device: &wgpu::Device) -> Self {
        let queries = device
            .features()
            .contains(wgpu::Features::TIMESTAMP_QUERY)
            .then(|| Queries {
                set: device.create_query_set(&wgpu::QuerySetDescriptor {
                    label: Some("UNGE frame timestamps"),
                    ty: wgpu::QueryType::Timestamp,
                    count: 2,
                }),
                resolve: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("UNGE timestamp resolve"),
                    size: 16,
                    usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                    mapped_at_creation: false,
                }),
                readback: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("UNGE timestamp readback"),
                    size: 16,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                }),
            });
        Self { queries }
    }
    /// Native-only blocking diagnostic with a five-second completion timeout.
    /// Device, queue, target, renderer and profiler must belong to the same device.
    #[allow(clippy::too_many_arguments)]
    pub fn measure(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        renderer: &mut GpuRenderer,
        target: &wgpu::TextureView,
        scene: &Scene,
        viewport: Viewport,
        physical_size: [u32; 2],
    ) -> Result<FrameTimings, String> {
        let total = Instant::now();
        renderer
            .prepare_sized(device, queue, scene, viewport, physical_size)
            .map_err(|e| e.to_string())?;
        let prepare_ms = total.elapsed().as_secs_f64() * 1000.;
        let start = Instant::now();
        let mut encoder = device.create_command_encoder(&Default::default());
        let timestamps = self
            .queries
            .as_ref()
            .map(|q| wgpu::RenderPassTimestampWrites {
                query_set: &q.set,
                beginning_of_pass_write_index: Some(0),
                end_of_pass_write_index: Some(1),
            });
        renderer.render_with_timestamps(&mut encoder, target, timestamps);
        let submission = queue.submit([encoder.finish()]);
        let encode_submit_ms = start.elapsed().as_secs_f64() * 1000.;
        let start = Instant::now();
        device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(Duration::from_secs(5)),
            })
            .map_err(|e| e.to_string())?;
        // Resolve after render completion in a separate submission. Some native
        // backends expose stale end-of-pass counters when resolving in the same
        // command buffer. Only the diagnostic path pays this synchronization cost.
        let receiver = if let Some(q) = &self.queries {
            let mut resolve = device.create_command_encoder(&Default::default());
            resolve.resolve_query_set(&q.set, 0..2, &q.resolve, 0);
            resolve.copy_buffer_to_buffer(&q.resolve, 0, &q.readback, 0, 16);
            let submission = queue.submit([resolve.finish()]);
            let (tx, rx) = std::sync::mpsc::sync_channel(1);
            q.readback
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |result| {
                    let _ = tx.send(result);
                });
            if let Err(error) = device.poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(Duration::from_secs(5).saturating_sub(start.elapsed())),
            }) {
                q.readback.unmap();
                return Err(error.to_string());
            }
            Some(rx)
        } else {
            None
        };
        let gpu_pass_ms = if let (Some(q), Some(rx)) = (&self.queries, receiver) {
            let mapped = rx
                .try_recv()
                .map_err(|e| e.to_string())
                .and_then(|r| r.map_err(|e| e.to_string()));
            if let Err(error) = mapped {
                q.readback.unmap();
                return Err(error);
            }
            let bytes = q.readback.slice(..).get_mapped_range();
            let begin = u64::from_ne_bytes(bytes[0..8].try_into().unwrap());
            let end = u64::from_ne_bytes(bytes[8..16].try_into().unwrap());
            drop(bytes);
            q.readback.unmap();
            // Invalid/disjoint samples are unavailable, never silently reported as zero.
            end.checked_sub(begin)
                .map(|ticks| ticks as f64 * f64::from(queue.get_timestamp_period()) / 1_000_000.)
        } else {
            None
        };
        Ok(FrameTimings {
            prepare_ms,
            encode_submit_ms,
            completion_wait_ms: start.elapsed().as_secs_f64() * 1000.,
            gpu_pass_ms,
            completed_frame_ms: total.elapsed().as_secs_f64() * 1000.,
        })
    }
}
