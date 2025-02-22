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
    let y = textureSample(input_texture_y, s_sampler, tex_coords).r;
    let uv = textureSample(input_texture_uv, s_sampler, tex_coords).rg;
    
    // Debug out-of-range values with different colors
    if (y < 16.0/255.0) {
        return vec4<f32>(1.0, 0.0, 0.0, 1.0);  // Red for too-dark Y
    }
    if (y > 235.0/255.0) {
        return vec4<f32>(0.0, 1.0, 0.0, 1.0);  // Green for too-bright Y
    }
    
    // Check UV components separately
    let uv_min = 16.0/255.0;
    let uv_max = 240.0/255.0;
    if (uv.r < uv_min || uv.r > uv_max || uv.g < uv_min || uv.g > uv_max) {
        return vec4<f32>(0.0, 0.0, 1.0, 1.0);  // Blue for invalid UV
    }

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
    
    // Debug conversion output
    if (rgb.r < 0.0 || rgb.g < 0.0 || rgb.b < 0.0) {
        return vec4<f32>(1.0, 0.5, 0.0, 1.0);  // Orange for negative RGB
    }
    if (rgb.r > 1.0 || rgb.g > 1.0 || rgb.b > 1.0) {
        return vec4<f32>(0.5, 0.0, 1.0, 1.0);  // Purple for >1.0 RGB
    }
    if (rgb.r == 0.0 && rgb.g == 0.0 && rgb.b == 0.0) {
        return vec4<f32>(0.5, 0.5, 0.5, 1.0);  // Gray for zero RGB
    }

    return vec4<f32>(
        clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)), 
        1.0
    );
}