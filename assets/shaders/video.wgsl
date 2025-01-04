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

fn convert_yuv_bt709(y: f32, u: f32, v: f32) -> vec3<f32> {
    let y_range = (y - 16.0/255.0) * (255.0/219.0);
    let u_range = (u - 128.0/255.0) * (255.0/224.0);
    let v_range = (v - 128.0/255.0) * (255.0/224.0);
    
    return vec3<f32>(
        y_range + 1.5748 * v_range,
        y_range - 0.1873 * u_range - 0.4681 * v_range,
        y_range + 1.8556 * u_range
    );
}

fn convert_yuv_bt601(y: f32, u: f32, v: f32) -> vec3<f32> {
    let y_range = (y - 16.0/255.0) * (255.0/219.0);
    let u_range = (u - 128.0/255.0) * (255.0/224.0);
    let v_range = (v - 128.0/255.0) * (255.0/224.0);
    
    return vec3<f32>(
        y_range + 1.402 * v_range,
        y_range - 0.344 * u_range - 0.714 * v_range,
        y_range + 1.772 * u_range
    );
}

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
    let y = textureSample(tex_y, s, in.uv).r;
    let uv = textureSample(tex_uv, s, in.uv).rg;
    
    var rgb: vec3<f32>;
    switch uniforms.color_space {
        case 0u: { // BT.709
            rgb = convert_yuv_bt709(y, uv.r, uv.g);
        }
        case 1u: { // BT.601
            rgb = convert_yuv_bt601(y, uv.r, uv.g);
        }
        default: { // Fallback to BT.709
            rgb = convert_yuv_bt709(y, uv.r, uv.g);
        }
    }
    
    return vec4<f32>(clamp(rgb, vec3(0.0), vec3(1.0)), 1.0);
}