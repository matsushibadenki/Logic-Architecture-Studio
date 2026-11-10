struct Metrics { size: vec2<f32>, atlas_size: f32, padding: f32 };
@group(0) @binding(0) var<uniform> metrics: Metrics;
@group(0) @binding(1) var atlas: texture_2d<f32>;
@group(0) @binding(2) var atlas_sampler: sampler;
struct Vertex {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};
@vertex fn vs(@builtin(vertex_index) index: u32,
    @location(0) rect: vec4<f32>, @location(1) uv: vec4<f32>,
    @location(2) color: vec4<f32>) -> Vertex {
    let corners = array<vec2<f32>, 6>(vec2(0.,0.),vec2(1.,0.),vec2(0.,1.),vec2(0.,1.),vec2(1.,0.),vec2(1.,1.));
    let p = corners[index];
    var out: Vertex;
    out.position = vec4((rect.xy + p * rect.zw) / metrics.size * vec2(2.,-2.) + vec2(-1.,1.), 0., 1.);
    out.uv = (uv.xy + p * uv.zw) / metrics.atlas_size;
    out.color = color;
    return out;
}
@fragment fn fs(v: Vertex) -> @location(0) vec4<f32> {
    return vec4(v.color.rgb, v.color.a * textureSample(atlas, atlas_sampler, v.uv).r);
}
