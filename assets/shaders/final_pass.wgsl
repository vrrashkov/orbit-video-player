struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

struct Uniforms {
    rect: vec4<f32>,  // Add this to match your YUV shader
}

@group(0) @binding(0)
var t_texture: texture_2d<f32>;
@group(0) @binding(1)
var s_sampler: sampler;
@group(0) @binding(2)  // Add uniform binding
var<uniform> uniforms: Uniforms;

@vertex
fn vs_main(@builtin(vertex_index) vert_idx: u32) -> VertexOutput {
    // Use the same quad layout as your YUV shader
    var quad = array<vec4<f32>, 6>(
        vec4<f32>(uniforms.rect.xy, 0.0, 0.0),
        vec4<f32>(uniforms.rect.zy, 1.0, 0.0),
        vec4<f32>(uniforms.rect.xw, 0.0, 1.0),
        vec4<f32>(uniforms.rect.zy, 1.0, 0.0),
        vec4<f32>(uniforms.rect.zw, 1.0, 1.0),
        vec4<f32>(uniforms.rect.xw, 0.0, 1.0),
    );

    var out: VertexOutput;
    out.uv = quad[vert_idx].zw;
    out.position = vec4<f32>(quad[vert_idx].xy, 0.0, 1.0);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(t_texture, s_sampler, in.uv);
    return vec4<f32>(color.rgb, 1.0);
}