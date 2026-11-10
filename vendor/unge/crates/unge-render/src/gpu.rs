use crate::{Quad, Scene};
use unge_core::Viewport;
use wgpu::util::DeviceExt;

pub struct GpuRenderer {
    text: crate::text::TextRenderer,
    pipeline: wgpu::RenderPipeline,
    camera: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    instances: wgpu::Buffer,
    capacity: usize,
    count: u32,
    prepared: bool,
    background: [f32; 4],
    previous_quads: Vec<Quad>,
    upload_stats: crate::UploadStats,
}
impl GpuRenderer {
    pub fn upload_stats(&self) -> crate::UploadStats {
        self.upload_stats
    }
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        Self::with_font_system(device, format, cosmic_text::FontSystem::new())
    }
    /// Inject a host font database for portable, licensed bundled fonts.
    pub fn with_font_system(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        fonts: cosmic_text::FontSystem,
    ) -> Self {
        let text = crate::text::TextRenderer::new(device, format, fonts);
        let camera = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("UNGE camera"),
            contents: bytemuck::cast_slice(&[0.0f32; 12]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera.as_entire_binding(),
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("UNGE graph"),
            source: wgpu::ShaderSource::Wgsl(include_str!("graph.wgsl").into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("UNGE instanced graph"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Quad>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![0=>Float32x4,1=>Float32x4,2=>Float32x4],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview: None,
            cache: None,
        });
        let capacity = 256;
        let instances = Self::buffer(device, capacity);
        Self {
            text,
            pipeline,
            camera,
            bind_group,
            instances,
            capacity,
            count: 0,
            prepared: false,
            previous_quads: Vec::new(),
            upload_stats: crate::UploadStats::default(),
            background: crate::Theme::Dark.palette().background.linear(),
        }
    }
    pub fn text_stats(&self) -> crate::TextStats {
        self.text.stats
    }
    pub fn set_font_system(&mut self, fonts: cosmic_text::FontSystem) {
        self.prepared = false;
        self.text.set_fonts(fonts);
    }
    fn buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("UNGE instance buffer"),
            size: (capacity * std::mem::size_of::<Quad>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &Scene,
        viewport: Viewport,
    ) -> unge_core::Result<()> {
        self.prepare_sized(
            device,
            queue,
            scene,
            viewport,
            viewport.size.map(|v| v.round().max(1.0) as u32),
        )
    }
    /// Physical target size enables crisp text on HiDPI surfaces.
    pub fn prepare_sized(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &Scene,
        viewport: Viewport,
        size: [u32; 2],
    ) -> unge_core::Result<()> {
        self.prepared = false;
        self.count = 0;
        viewport.validate()?;
        if size.contains(&0)
            || size
                .iter()
                .any(|v| *v > device.limits().max_texture_dimension_2d)
        {
            return Err(unge_core::Error::Invalid("invalid text target size".into()));
        }
        if scene
            .background
            .iter()
            .chain(scene.grid_color.iter())
            .any(|c| !c.is_finite() || !(0.0..=1.0).contains(c))
        {
            return Err(unge_core::Error::Invalid("invalid scene colours".into()));
        }
        self.background = scene.background;
        self.text.prepare(queue, scene, viewport, size)?;
        let bytes = scene
            .quads
            .len()
            .checked_mul(std::mem::size_of::<Quad>())
            .ok_or_else(|| unge_core::Error::Invalid("scene too large".into()))?;
        if bytes as u64 > device.limits().max_buffer_size || scene.quads.len() > u32::MAX as usize {
            return Err(unge_core::Error::Invalid(
                "scene exceeds GPU buffer limit".into(),
            ));
        }
        let reallocated = scene.quads.len() > self.capacity;
        if reallocated {
            self.capacity = scene
                .quads
                .len()
                .next_power_of_two()
                .min((device.limits().max_buffer_size as usize) / std::mem::size_of::<Quad>());
            self.instances = Self::buffer(device, self.capacity);
        }
        self.count = scene.quads.len() as u32;
        let bytes: &[u8] = bytemuck::cast_slice(&scene.quads);
        let ranges = if reallocated && !bytes.is_empty() {
            std::iter::once(0..bytes.len()).collect()
        } else {
            crate::changed_ranges(
                bytemuck::cast_slice(&self.previous_quads),
                bytes,
                std::mem::size_of::<Quad>(),
            )
        };
        self.upload_stats = crate::UploadStats {
            quad_bytes: ranges.iter().map(|r| r.len()).sum(),
            quad_writes: ranges.len(),
            quad_count: scene.quads.len(),
        };
        for range in ranges {
            queue.write_buffer(&self.instances, range.start as u64, &bytes[range]);
        }
        self.previous_quads.clone_from(&scene.quads);
        queue.write_buffer(
            &self.camera,
            0,
            bytemuck::cast_slice(&[
                viewport.origin[0],
                viewport.origin[1],
                viewport.zoom,
                0.0,
                viewport.size[0],
                viewport.size[1],
                0.0,
                0.0,
                scene.grid_color[0],
                scene.grid_color[1],
                scene.grid_color[2],
                scene.grid_color[3],
            ]),
        );
        self.prepared = true;
        Ok(())
    }
    /// Geometry and glyph batches share a pass and preserve node paint order.
    pub fn render(&self, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView) {
        self.render_with_timestamps(encoder, target, None);
    }
    /// Host-provided timestamp queries for explicit profiling. Normal rendering
    /// uses no queries. The device must support the requested query feature.
    pub fn render_with_timestamps(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        timestamp_writes: Option<wgpu::RenderPassTimestampWrites<'_>>,
    ) {
        self.render_pass(encoder, target, timestamp_writes, None);
    }
    /// Draw into a top-left physical region. The caller must keep it within the
    /// target and prepare using this same size. The rest of the target is cleared.
    pub fn render_in_region(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        size: [u32; 2],
    ) {
        self.render_pass(encoder, target, None, Some(size));
    }
    fn render_pass(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        timestamp_writes: Option<wgpu::RenderPassTimestampWrites<'_>>,
        region: Option<[u32; 2]>,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("UNGE graph pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: f64::from(self.background[0]),
                        g: f64::from(self.background[1]),
                        b: f64::from(self.background[2]),
                        a: f64::from(self.background[3]),
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes,
            occlusion_query_set: None,
        });
        if !self.prepared {
            return;
        }
        if let Some([width, height]) = region {
            if width == 0 || height == 0 {
                return;
            }
            pass.set_viewport(0., 0., width as f32, height as f32, 0., 1.);
            pass.set_scissor_rect(0, 0, width, height);
        }
        let mut first = 0;
        for (after, range) in &self.text.batches {
            self.geometry(&mut pass, first..*after);
            self.text.render(&mut pass, range.clone());
            first = *after;
        }
        self.geometry(&mut pass, first..self.count);
    }
    fn geometry(&self, pass: &mut wgpu::RenderPass<'_>, range: std::ops::Range<u32>) {
        if range.is_empty() {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.instances.slice(..));
        pass.draw(0..6, range);
    }
}
