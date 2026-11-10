use crate::{Scene, TextLabel};
use cosmic_text::{
    Attrs, Buffer, CacheKey, FontSystem, LayoutGlyph, Metrics, Shaping, SwashCache, SwashContent,
    Wrap,
};
use std::{
    collections::{HashMap, VecDeque},
    ops::Range,
    sync::Arc,
};
use unge_core::{Error, Result, Viewport};

const ATLAS_SIZE: u32 = 2048;
const MAX_LAYOUTS: usize = 1024;
const MAX_GLYPHS: usize = 65_536;
const MAX_ATLAS_ENTRIES: usize = 16_384;
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GlyphInstance {
    rect: [f32; 4],
    uv: [f32; 4],
    color: [f32; 4],
}
#[derive(Clone, Copy)]
struct AtlasGlyph {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    left: i32,
    top: i32,
}
#[derive(Default)]
struct Shelf {
    x: u32,
    y: u32,
    height: u32,
}
impl Shelf {
    fn allocate(&mut self, width: u32, height: u32) -> Option<[u32; 2]> {
        // One transparent texel of padding on each side avoids filtering neighbours.
        let (w, h) = (width.checked_add(2)?, height.checked_add(2)?);
        if w > ATLAS_SIZE || h > ATLAS_SIZE {
            return None;
        }
        if self.x + w > ATLAS_SIZE {
            self.y += self.height;
            self.x = 0;
            self.height = 0;
        }
        if self.y + h > ATLAS_SIZE {
            return None;
        }
        let position = [self.x + 1, self.y + 1];
        self.x += w;
        self.height = self.height.max(h);
        Some(position)
    }
}
struct Line {
    glyphs: Vec<LayoutGlyph>,
    baseline: f32,
    width: f32,
}
/// Diagnostics for the last successfully prepared frame.
#[derive(Debug, Clone, Copy, Default)]
pub struct TextStats {
    pub visible_glyphs: usize,
    pub missing_glyphs: usize,
    pub cached_layouts: usize,
    pub atlas_entries: usize,
    pub uploaded_bytes: usize,
    pub upload_writes: usize,
}
type LayoutKey = (String, u32);
pub(crate) struct TextRenderer {
    pub stats: TextStats,
    fonts: FontSystem,
    swash: SwashCache,
    layouts: HashMap<LayoutKey, Arc<Line>>,
    order: VecDeque<LayoutKey>,
    glyphs: HashMap<CacheKey, Option<AtlasGlyph>>,
    shelf: Shelf,
    atlas: wgpu::Texture,
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    metrics: wgpu::Buffer,
    instances: wgpu::Buffer,
    previous_instances: Vec<GlyphInstance>,
    // One range per node, interleaved with its geometry to preserve stacking.
    pub batches: Vec<(u32, Range<u32>)>,
}
fn invalid(message: &str) -> Error {
    Error::Invalid(message.into())
}
impl TextRenderer {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, fonts: FontSystem) -> Self {
        let atlas = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("UNGE glyph atlas (4 MiB)"),
            size: wgpu::Extent3d {
                width: ATLAS_SIZE,
                height: ATLAS_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let metrics = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("UNGE text metrics"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            min_filter: wgpu::FilterMode::Linear,
            mag_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: metrics.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(
                        &atlas.create_view(&Default::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("UNGE text"),
            source: wgpu::ShaderSource::Wgsl(include_str!("text.wgsl").into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("UNGE glyph instances"),
            layout: Some(
                &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: None,
                    bind_group_layouts: &[&layout],
                    push_constant_ranges: &[],
                }),
            ),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<GlyphInstance>() as u64,
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
        let instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("UNGE glyph instances (3 MiB)"),
            size: (MAX_GLYPHS * std::mem::size_of::<GlyphInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            stats: TextStats::default(),
            fonts,
            swash: SwashCache::new(),
            layouts: HashMap::new(),
            order: VecDeque::new(),
            glyphs: HashMap::new(),
            shelf: Shelf::default(),
            atlas,
            pipeline,
            bind_group,
            metrics,
            instances,
            previous_instances: Vec::new(),
            batches: Vec::new(),
        }
    }
    pub fn set_fonts(&mut self, fonts: FontSystem) {
        self.stats = TextStats::default();
        self.fonts = fonts;
        self.swash = SwashCache::new();
        self.layouts.clear();
        self.order.clear();
        self.clear_atlas();
        self.batches.clear();
    }
    fn clear_atlas(&mut self) {
        self.glyphs.clear();
        self.shelf = Shelf::default();
    }
    fn line(&mut self, label: &TextLabel) -> Arc<Line> {
        let key = (label.text.clone(), label.font_size.to_bits());
        if let Some(line) = self.layouts.get(&key) {
            return line.clone();
        }
        let mut buffer = Buffer::new(
            &mut self.fonts,
            Metrics::new(label.font_size, label.font_size * 1.3),
        );
        buffer.set_wrap(Wrap::None);
        buffer.set_text(&label.text, &Attrs::new(), Shaping::Advanced, None);
        // cosmic-text setters defer shaping; finish layout before caching glyphs.
        buffer.shape_until_scroll(&mut self.fonts, false);
        let line = Arc::new(
            buffer
                .layout_runs()
                .next()
                .map(|run| Line {
                    glyphs: run.glyphs.to_vec(),
                    baseline: run.line_y,
                    width: run.line_w,
                })
                .unwrap_or(Line {
                    glyphs: Vec::new(),
                    baseline: 0.0,
                    width: 0.0,
                }),
        );
        if self.layouts.len() >= MAX_LAYOUTS
            && let Some(old) = self.order.pop_front()
        {
            self.layouts.remove(&old);
        }
        self.order.push_back(key.clone());
        self.layouts.insert(key, line.clone());
        line
    }
    fn glyph(&mut self, queue: &wgpu::Queue, key: CacheKey) -> Result<Option<AtlasGlyph>> {
        if let Some(glyph) = self.glyphs.get(&key) {
            return Ok(*glyph);
        }
        if self.glyphs.len() >= MAX_ATLAS_ENTRIES {
            return Err(invalid("glyph atlas entry limit exceeded"));
        }
        // The GPU atlas is the raster cache; do not keep duplicate CPU bitmaps.
        let Some(image) = self.swash.get_image_uncached(&mut self.fonts, key) else {
            self.glyphs.insert(key, None);
            return Ok(None);
        };
        let p = image.placement;
        if p.width == 0 || p.height == 0 {
            self.glyphs.insert(key, None);
            return Ok(None);
        }
        let [x, y] = self
            .shelf
            .allocate(p.width, p.height)
            .ok_or_else(|| invalid("glyph atlas full"))?;
        let stride = p.width as usize + 2;
        let mut pixels = vec![0u8; stride * (p.height as usize + 2)];
        for row in 0..p.height as usize {
            for col in 0..p.width as usize {
                let i = row * p.width as usize + col;
                pixels[(row + 1) * stride + col + 1] = match image.content {
                    SwashContent::Mask => image.data[i],
                    // Labels are monochrome, including colour-font silhouettes.
                    SwashContent::Color => image.data[i * 4 + 3],
                    SwashContent::SubpixelMask => image.data[i * 4..i * 4 + 3]
                        .iter()
                        .copied()
                        .max()
                        .unwrap_or(0),
                };
            }
        }
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.atlas,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: x - 1,
                    y: y - 1,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            &pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(p.width + 2),
                rows_per_image: Some(p.height + 2),
            },
            wgpu::Extent3d {
                width: p.width + 2,
                height: p.height + 2,
                depth_or_array_layers: 1,
            },
        );
        let glyph = AtlasGlyph {
            x,
            y,
            width: p.width,
            height: p.height,
            left: p.left,
            top: p.top,
        };
        self.glyphs.insert(key, Some(glyph));
        Ok(Some(glyph))
    }
    pub fn prepare(
        &mut self,
        queue: &wgpu::Queue,
        scene: &Scene,
        view: Viewport,
        size: [u32; 2],
    ) -> Result<()> {
        self.batches.clear();
        self.stats = TextStats::default();
        if scene.labels.len() > 16_384 {
            return Err(invalid("scene exceeds 16384 text labels"));
        }
        let scale = size[0] as f32 / view.size[0] * view.zoom;
        let scale_y = size[1] as f32 / view.size[1] * view.zoom;
        if !scale.is_finite() || scale <= 0.0 || (scale - scale_y).abs() > scale * 0.01 {
            return Err(invalid("text target must preserve viewport aspect ratio"));
        }
        let mut previous = 0;
        for label in &scene.labels {
            if !label.rect.valid()
                || !label.font_size.is_finite()
                || !(1.0..=512.0).contains(&label.font_size)
                || label.text.chars().count() > 256
                || label
                    .text
                    .chars()
                    .any(|c| c.is_control() || c == '\u{2028}' || c == '\u{2029}')
                || label.after_quad < previous
                || label.after_quad > scene.quads.len()
                || label
                    .color
                    .iter()
                    .any(|c| !c.is_finite() || !(0.0..=1.0).contains(c))
            {
                return Err(invalid("invalid text label"));
            }
            previous = label.after_quad;
        }
        // Atlas recycling invalidates UVs. Rebuild the entire frame once after a reset.
        let instances = match self.build(queue, scene, view, size, scale) {
            Ok(instances) => instances,
            Err(_) => {
                self.clear_atlas();
                self.batches.clear();
                self.build(queue, scene, view, size, scale)
                    .inspect_err(|_| self.batches.clear())?
            }
        };
        self.stats.visible_glyphs = instances.len();
        self.stats.cached_layouts = self.layouts.len();
        self.stats.atlas_entries = self.glyphs.len();
        queue.write_buffer(
            &self.metrics,
            0,
            bytemuck::cast_slice(&[size[0] as f32, size[1] as f32, ATLAS_SIZE as f32, 0.0]),
        );
        let bytes: &[u8] = bytemuck::cast_slice(&instances);
        let ranges = crate::changed_ranges(
            bytemuck::cast_slice(&self.previous_instances),
            bytes,
            std::mem::size_of::<GlyphInstance>(),
        );
        self.stats.uploaded_bytes = ranges.iter().map(|r| r.len()).sum();
        self.stats.upload_writes = ranges.len();
        for range in ranges {
            queue.write_buffer(&self.instances, range.start as u64, &bytes[range]);
        }
        self.previous_instances = instances;
        Ok(())
    }
    fn build(
        &mut self,
        queue: &wgpu::Queue,
        scene: &Scene,
        view: Viewport,
        size: [u32; 2],
        scale: f32,
    ) -> Result<Vec<GlyphInstance>> {
        let mut instances = Vec::new();
        self.stats.missing_glyphs = 0;
        for label in &scene.labels {
            let line = self.line(label);
            if line.glyphs.is_empty() {
                self.stats.missing_glyphs +=
                    label.text.chars().filter(|c| !c.is_whitespace()).count();
            }
            let left = (label.rect.x - view.origin[0]) * scale;
            let top = (label.rect.y - view.origin[1]) * scale;
            let clip = [
                left.max(0.0),
                top.max(0.0),
                (left + label.rect.width * scale).min(size[0] as f32),
                (top + label.rect.height * scale).min(size[1] as f32),
            ];
            if clip[0] >= clip[2] || clip[1] >= clip[3] {
                continue;
            }
            let align = if label.right_aligned {
                (label.rect.width - line.width).max(0.0) * scale
            } else {
                0.0
            };
            let raster_scale = scale.min(512.0 / label.font_size);
            let magnify = scale / raster_scale;
            let start = instances.len() as u32;
            for glyph in &line.glyphs {
                // Cull before rasterizing long labels clipped to a narrow box.
                if left + align + (glyph.x + glyph.w) * scale < clip[0] - 512.0
                    || left + align + glyph.x * scale > clip[2] + 512.0
                {
                    continue;
                }
                if glyph.glyph_id == 0 {
                    self.stats.missing_glyphs += 1;
                }
                let physical = glyph.physical((0.0, line.baseline * raster_scale), raster_scale);
                let Some(g) = self.glyph(queue, physical.cache_key)? else {
                    continue;
                };
                let x = left + align + (physical.x as f32 + g.left as f32) * magnify;
                let y = top + (physical.y as f32 - g.top as f32) * magnify;
                let x0 = x.max(clip[0]);
                let y0 = y.max(clip[1]);
                let x1 = (x + g.width as f32 * magnify).min(clip[2]);
                let y1 = (y + g.height as f32 * magnify).min(clip[3]);
                if x0 >= x1 || y0 >= y1 {
                    continue;
                }
                if instances.len() >= MAX_GLYPHS {
                    return Err(invalid("scene exceeds 65536 visible glyphs"));
                }
                instances.push(GlyphInstance {
                    rect: [x0, y0, x1 - x0, y1 - y0],
                    uv: [
                        g.x as f32 + (x0 - x) / magnify,
                        g.y as f32 + (y0 - y) / magnify,
                        (x1 - x0) / magnify,
                        (y1 - y0) / magnify,
                    ],
                    color: label.color,
                });
            }
            let end = instances.len() as u32;
            if end > start {
                if let Some((after, range)) = self.batches.last_mut()
                    && *after == label.after_quad as u32
                {
                    range.end = end;
                } else {
                    self.batches.push((label.after_quad as u32, start..end));
                }
            }
        }
        Ok(instances)
    }
    pub fn render(&self, pass: &mut wgpu::RenderPass<'_>, range: Range<u32>) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.instances.slice(..));
        pass.draw(0..6, range);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shelf_packs_with_padding_and_reports_exhaustion() {
        let mut shelf = Shelf::default();
        assert_eq!(shelf.allocate(2046, 20), Some([1, 1]));
        assert_eq!(shelf.allocate(10, 10), Some([1, 23]));
        assert_eq!(shelf.allocate(2047, 1), None);
        assert_eq!(shelf.allocate(2046, 2046), None);
    }
}
