struct YUVToRGBUniforms {
    color_space: u32, // 0 for BT.709, 1 for BT.601, etc.
}

@group(0) @binding(0) var input_texture_y: texture_2d<f32>;
@group(0) @binding(1) var input_texture_uv: texture_2d<f32>;
@group(0) @binding(2) var s_sampler: sampler;
@group(0) @binding(3) var<uniform> uniforms: YUVToRGBUniforms;
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
}
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
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var pos = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>( 1.0,  1.0),
    );

    var tex_coords = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(1.0, 0.0),
    );

    var output: VertexOutput;
    output.position = vec4<f32>(pos[vertex_index], 0.0, 1.0);
    output.tex_coords = tex_coords[vertex_index];
    return output;
}
@fragment
fn fs_main(@location(0) tex_coords: vec2<f32>) -> @location(0) vec4<f32> {
    // Sample Y and UV planes
    let y = textureSample(input_texture_y, s_sampler, tex_coords).r;
    let uv = textureSample(input_texture_uv, s_sampler, tex_coords).rg;
    
    // Convert based on color space
    var rgb: vec3<f32>;
    switch (uniforms.color_space) {
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
    
    // Return as RGB texture
    return vec4<f32>(
        clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)), 
        1.0
    );
}