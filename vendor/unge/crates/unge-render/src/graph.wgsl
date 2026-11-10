struct Camera { origin_zoom: vec4<f32>, size: vec4<f32>, grid_color: vec4<f32> };
@group(0) @binding(0) var<uniform> camera: Camera;
struct Output {
    @builtin(position) position: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) half_size: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) params: vec4<f32>,
    @location(4) world: vec2<f32>,
};
@vertex fn vs(@builtin(vertex_index) i: u32, @location(0) rect: vec4<f32>, @location(1) color: vec4<f32>, @location(2) params: vec4<f32>) -> Output {
    let corners = array<vec2<f32>,6>(vec2(-1.,-1.),vec2(1.,-1.),vec2(-1.,1.),vec2(-1.,1.),vec2(1.,-1.),vec2(1.,1.));
    let local = corners[i]*rect.zw*0.5;
    let c = cos(params.x); let s = sin(params.x);
    let world = rect.xy+vec2(c*local.x-s*local.y,s*local.x+c*local.y);
    let screen = (world-camera.origin_zoom.xy)*camera.origin_zoom.z;
    var out: Output;
    out.position = vec4(screen.x/camera.size.x*2.-1.,1.-screen.y/camera.size.y*2.,0.,1.);
    out.local=local; out.half_size=rect.zw*0.5; out.color=color; out.params=params; out.world=world;
    return out;
}
@fragment fn fs(in: Output) -> @location(0) vec4<f32> {
    if in.params.z > 0.5 {
        let spacing = select(24.,120.,camera.origin_zoom.z < 0.3);
        let cell = abs(fract(in.world/spacing-0.5)-0.5)*spacing;
        let line = 1.-smoothstep(0.55,1.35,length(cell)*camera.origin_zoom.z);
        return vec4(mix(in.color.rgb,camera.grid_color.rgb,line*camera.grid_color.a),in.color.a);
    }
    let radius = min(in.params.y,min(in.half_size.x,in.half_size.y));
    let q = abs(in.local)-in.half_size+vec2(radius);
    let distance = length(max(q,vec2(0.)))+min(max(q.x,q.y),0.)-radius;
    let feather = max(1./camera.origin_zoom.z,in.params.w);
    let alpha = 1.-smoothstep(-feather,0.,distance);
    return vec4(in.color.rgb,in.color.a*alpha);
}
