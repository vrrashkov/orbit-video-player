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
    let uv = textureSample(tex_uv, s, in.uv * 0.5);
    
    // BT.709 conversion for full range YUV
    let kr = 0.2126;
    let kb = 0.0722;
    let kg = 1.0 - kr - kb;
    
    let y_norm = y;
    let u_norm = uv.r - 0.5;
    let v_norm = uv.g - 0.5;
    
    let r = y_norm + (2.0 * (1.0 - kr)) * v_norm;
    let g = y_norm - (2.0 * (1.0 - kr) * kr / kg) * v_norm - (2.0 * (1.0 - kb) * kb / kg) * u_norm;
    let b = y_norm + (2.0 * (1.0 - kb)) * u_norm;
    
    return vec4<f32>(
        clamp(r, 0.0, 1.0),
        clamp(g, 0.0, 1.0),
        clamp(b, 0.0, 1.0),
        1.0
    );
}