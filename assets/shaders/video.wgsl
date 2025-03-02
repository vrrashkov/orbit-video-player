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
    
    let r = y_range + 1.5748 * v_range;
    let g = y_range - 0.1873 * u_range - 0.4681 * v_range;
    let b = y_range + 1.8556 * u_range;
    
    return clamp(vec3<f32>(r, g, b), vec3<f32>(0.0), vec3<f32>(1.0));
}

fn convert_yuv_bt601(y: f32, u: f32, v: f32) -> vec3<f32> {
    let y_range = (y - 16.0/255.0) * (255.0/219.0);
    let u_range = (u - 128.0/255.0) * (255.0/224.0);
    let v_range = (v - 128.0/255.0) * (255.0/224.0);
    
    let r = y_range + 1.402 * v_range;
    let g = y_range - 0.344 * u_range - 0.714 * v_range;
    let b = y_range + 1.772 * u_range;
    
    return clamp(vec3<f32>(r, g, b), vec3<f32>(0.0), vec3<f32>(1.0));
}

@vertex
fn vs_main(@builtin(vertex_index) in_vertex_index: u32) -> VertexOutput {
    // Define a full-screen quad in normalized device coordinates (-1 to 1)
    var positions = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0),  // bottom left
        vec2<f32>(1.0, -1.0),   // bottom right
        vec2<f32>(-1.0, 1.0),   // top left
        vec2<f32>(1.0, -1.0),   // bottom right
        vec2<f32>(1.0, 1.0),    // top right
        vec2<f32>(-1.0, 1.0)    // top left
    );
    
    // Define UVs for the quad (0 to 1)
    var uvs = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 1.0),  // bottom left
        vec2<f32>(1.0, 1.0),  // bottom right
        vec2<f32>(0.0, 0.0),  // top left
        vec2<f32>(1.0, 1.0),  // bottom right
        vec2<f32>(1.0, 0.0),  // top right
        vec2<f32>(0.0, 0.0)   // top left
    );

    var out: VertexOutput;
    out.uv = uvs[in_vertex_index];
    out.position = vec4<f32>(positions[in_vertex_index], 0.0, 1.0);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Texture size diagnostics
    let y_tex_size = textureDimensions(tex_y);
    let uv_tex_size = textureDimensions(tex_uv);
    
    // Validate texture dimensions
    if (y_tex_size.x == 0u || y_tex_size.y == 0u || 
        uv_tex_size.x == 0u || uv_tex_size.y == 0u) {
        return vec4<f32>(1.0, 0.0, 0.0, 1.0); // Red for invalid texture size
    }

    // Safe UV clamping
    let safe_uv = clamp(in.uv, vec2<f32>(0.0), vec2<f32>(1.0));
    
    // Sample Y and UV planes
    let y = textureSample(tex_y, s, safe_uv).r;
    let uv = textureSample(tex_uv, s, safe_uv).rg;
    
    // Validate input values
    if (y < 0.0 || y > 1.0 || 
        uv.r < 0.0 || uv.r > 1.0 || 
        uv.g < 0.0 || uv.g > 1.0) {
        return vec4<f32>(0.0, 1.0, 0.0, 1.0); // Green for invalid input values
    }
    
    // Convert YUV to RGB
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
    
    // Create final color
    let final_color = vec4<f32>(rgb, 1.0);
    
    // Validate output color
    if (length(final_color.rgb) < 0.001) {
        return vec4<f32>(0.0, 0.0, 1.0, 1.0); // Blue for zero-intensity conversion
    }
    
    return final_color;
}