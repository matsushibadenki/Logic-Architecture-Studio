use crate::{GpuRenderer, Scene};
use std::time::Instant;
use unge_core::Viewport;

#[derive(Debug, Clone, serde::Serialize)]
pub struct SurfaceFrameTimings {
    pub acquire_ms: f64,
    pub frame: crate::FrameTimings,
    /// CPU present call only. Does not measure compositor/display scanout.
    pub present_call_ms: f64,
    pub total_ms: f64,
}

/// CPU timings for the normal queued rendering path. No GPU completion wait,
/// timestamp query or readback is added. Acquisition/submit/present may still
/// block internally for driver backpressure or VSync.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct CpuSurfaceFrameTimings {
    pub acquire_ms: f64,
    pub prepare_ms: f64,
    pub encode_submit_ms: f64,
    /// CPU call only, not compositor/display completion.
    pub present_call_ms: f64,
    pub total_ms: f64,
}

fn elapsed_ms(start: Option<Instant>) -> f64 {
    start.map_or(0., |start| start.elapsed().as_secs_f64() * 1000.)
}

/// Native surface owned by the Rust application, independent of any WebView.
pub struct SurfaceRenderer {
    surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    renderer: GpuRenderer,
    suspended: bool,
    profiler: Option<crate::FrameProfiler>,
    adapter_info: wgpu::AdapterInfo,
}
impl SurfaceRenderer {
    pub async fn new(
        target: impl Into<wgpu::SurfaceTarget<'static>>,
        size: [u32; 2],
    ) -> Result<Self, String> {
        Self::create(target, size, false).await
    }
    /// Explicit diagnostic constructor; requests optional GPU timestamp support.
    pub async fn new_profiled(
        target: impl Into<wgpu::SurfaceTarget<'static>>,
        size: [u32; 2],
    ) -> Result<Self, String> {
        Self::create(target, size, true).await
    }
    async fn create(
        target: impl Into<wgpu::SurfaceTarget<'static>>,
        size: [u32; 2],
        profiling: bool,
    ) -> Result<Self, String> {
        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(target).map_err(|e| e.to_string())?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
            })
            .await
            .map_err(|e| e.to_string())?;
        let adapter_info = adapter.get_info();
        let required_features = if profiling {
            adapter.features() & wgpu::Features::TIMESTAMP_QUERY
        } else {
            wgpu::Features::empty()
        };
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                required_features,
                ..Default::default()
            })
            .await
            .map_err(|e| e.to_string())?;
        if size
            .iter()
            .any(|v| *v > device.limits().max_texture_dimension_2d)
        {
            return Err("surface size exceeds GPU limits".into());
        }
        let config = surface
            .get_default_config(&adapter, size[0].max(1), size[1].max(1))
            .ok_or("surface has no supported configuration")?;
        surface.configure(&device, &config);
        let renderer = GpuRenderer::new(&device, config.format);
        let profiler = profiling.then(|| crate::FrameProfiler::new(&device));
        Ok(Self {
            profiler,
            adapter_info,
            surface,
            device,
            queue,
            config,
            renderer,
            suspended: size.contains(&0),
        })
    }
    pub fn adapter_info(&self) -> &wgpu::AdapterInfo {
        &self.adapter_info
    }
    pub fn present_mode(&self) -> wgpu::PresentMode {
        self.config.present_mode
    }
    /// Blocking native diagnostic. None means a suspended/lost/timeout frame,
    /// which must be excluded from successful-frame statistics.
    pub fn draw_profiled(
        &mut self,
        scene: &Scene,
        viewport: Viewport,
    ) -> Result<Option<SurfaceFrameTimings>, String> {
        if self.profiler.is_none() {
            return Err("use SurfaceRenderer::new_profiled for diagnostics".into());
        }
        let total = Instant::now();
        if self.suspended {
            return Ok(None);
        }
        let frame = match self.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.config);
                return Ok(None);
            }
            Err(wgpu::SurfaceError::Timeout) => return Ok(None),
            Err(error) => return Err(error.to_string()),
        };
        let acquire_ms = total.elapsed().as_secs_f64() * 1000.;
        let timing = self.profiler.as_mut().unwrap().measure(
            &self.device,
            &self.queue,
            &mut self.renderer,
            &frame.texture.create_view(&Default::default()),
            scene,
            viewport,
            [self.config.width, self.config.height],
        )?;
        let present = Instant::now();
        frame.present();
        Ok(Some(SurfaceFrameTimings {
            acquire_ms,
            frame: timing,
            present_call_ms: present.elapsed().as_secs_f64() * 1000.,
            total_ms: total.elapsed().as_secs_f64() * 1000.,
        }))
    }
    pub fn text_stats(&self) -> crate::TextStats {
        self.renderer.text_stats()
    }
    /// Replaces font selection and invalidates layout/atlas caches.
    pub fn set_font_system(&mut self, fonts: cosmic_text::FontSystem) {
        self.renderer.set_font_system(fonts);
    }
    pub fn resize(&mut self, size: [u32; 2]) -> Result<(), String> {
        self.suspended = size.contains(&0);
        if self.suspended {
            return Ok(());
        }
        if size
            .iter()
            .any(|v| *v > self.device.limits().max_texture_dimension_2d)
        {
            return Err("surface size exceeds GPU limits".into());
        }
        if self.config.width != size[0] || self.config.height != size[1] {
            self.config.width = size[0];
            self.config.height = size[1];
            self.surface.configure(&self.device, &self.config);
        }
        Ok(())
    }
    /// Physical pixels are supplied by the host. Skip minimized and timeout frames.
    pub fn draw(&mut self, scene: &Scene, viewport: Viewport) -> Result<bool, String> {
        self.draw_region(scene, viewport, [self.config.width, self.config.height])
    }
    /// Keep the Surface at the whole window size, rendering only in its top-left
    /// region. Useful for an opaque child WebView inspector beside the graph.
    pub fn draw_region(
        &mut self,
        scene: &Scene,
        viewport: Viewport,
        graph_size: [u32; 2],
    ) -> Result<bool, String> {
        self.draw_region_impl::<false>(scene, viewport, graph_size)
            .map(|sample| sample.is_some())
    }
    /// Opt-in CPU measurement of the same path as `draw`. A skipped frame is
    /// None, never a zero-duration sample. Works with the ordinary constructor.
    pub fn draw_timed(
        &mut self,
        scene: &Scene,
        viewport: Viewport,
    ) -> Result<Option<CpuSurfaceFrameTimings>, String> {
        self.draw_region_timed(scene, viewport, [self.config.width, self.config.height])
    }
    /// Region variant for hosts with an opaque child WebView beside the graph.
    pub fn draw_region_timed(
        &mut self,
        scene: &Scene,
        viewport: Viewport,
        graph_size: [u32; 2],
    ) -> Result<Option<CpuSurfaceFrameTimings>, String> {
        self.draw_region_impl::<true>(scene, viewport, graph_size)
    }
    fn draw_region_impl<const TIMED: bool>(
        &mut self,
        scene: &Scene,
        viewport: Viewport,
        graph_size: [u32; 2],
    ) -> Result<Option<CpuSurfaceFrameTimings>, String> {
        let total = TIMED.then(Instant::now);
        if graph_size[0] > self.config.width || graph_size[1] > self.config.height {
            return Err("graph region exceeds surface size".into());
        }
        if self.suspended || graph_size.contains(&0) {
            return Ok(None);
        }
        let frame = match self.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.config);
                return Ok(None);
            }
            Err(wgpu::SurfaceError::Timeout) => return Ok(None),
            Err(error) => return Err(error.to_string()),
        };
        let acquire_ms = elapsed_ms(total);
        let prepare = TIMED.then(Instant::now);
        self.renderer
            .prepare_sized(&self.device, &self.queue, scene, viewport, graph_size)
            .map_err(|e| e.to_string())?;
        let prepare_ms = elapsed_ms(prepare);
        let encode = TIMED.then(Instant::now);
        let view = frame.texture.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        self.renderer
            .render_in_region(&mut encoder, &view, graph_size);
        self.queue.submit([encoder.finish()]);
        let encode_submit_ms = elapsed_ms(encode);
        let present = TIMED.then(Instant::now);
        frame.present();
        Ok(Some(CpuSurfaceFrameTimings {
            acquire_ms,
            prepare_ms,
            encode_submit_ms,
            present_call_ms: elapsed_ms(present),
            total_ms: elapsed_ms(total),
        }))
    }
}
