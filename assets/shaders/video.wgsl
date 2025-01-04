struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

struct Uniforms {
    rect: vec4<f32>,
    color_space: u32,  
}

@group(0) @binding(0)
var tex_y: texture_2d<f32>;

@group(0) @binding(1)
var tex_uv: texture_2d<f32>;

@group(0) @binding(2)
var s: sampler;

@group(0) @binding(3)
var<uniform> uniforms: Uniforms;

@vertex
fn vs_main(@builtin(vertex_index) in_vertex_index: u32) -> VertexOutput {
    var quad = array<vec4<f32>, 6>(
        vec4<f32>(uniforms.rect.xy, 0.0, 0.0),
        vec4<f32>(uniforms.rect.zy, 1.0, 0.0),
        vec4<f32>(uniforms.rect.xw, 0.0, 1.0),
        vec4<f32>(uniforms.rect.zy, 1.0, 0.0),
        vec4<f32>(uniforms.rect.zw, 1.0, 1.0),
        vec4<f32>(uniforms.rect.xw, 0.0, 1.0),
    );

    var out: VertexOutput;
    out.uv = quad[in_vertex_index].zw;
    out.position = vec4<f32>(quad[in_vertex_index].xy, 1.0, 1.0);
    return out;
}
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Sample Y and UV planes
    let y = textureSample(tex_y, s, in.uv).r;
    let uv = textureSample(tex_uv, s, in.uv).rg;
    
    // YUV is in MPEG range (limited)
    let y_range = (y - 16.0/255.0) * (255.0/219.0);
    
    // UV values are packed in RG channels, need to recenter around 0
    let u = (uv.r - 128.0/255.0) * (255.0/224.0);
    let v = (uv.g - 128.0/255.0) * (255.0/224.0);

    // BT.709 matrix (standard HDTV)
    let r = y_range + 1.5748 * v;
    let g = y_range - 0.1873 * u - 0.4681 * v;
    let b = y_range + 1.8556 * u;
    
    return vec4<f32>(
        clamp(r, 0.0, 1.0),
        clamp(g, 0.0, 1.0),
        clamp(b, 0.0, 1.0),
        1.0
    );
}