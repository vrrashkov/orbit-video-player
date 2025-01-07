struct Uniforms {
    position: f32,     // x position in clip space (-1 to 1)
    bounds: vec4<f32>, // x, y, width, height
    line_width: f32,   // width in pixels
    _pad: vec3<f32>,   // padding
}

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    // Calculate position in normalized device coordinates
    let line_x = uniforms.position;
    let width = uniforms.line_width / uniforms.bounds.z; // Convert to NDC
    
    var positions = array<vec2<f32>, 6>(
        // First triangle
        vec2<f32>(line_x - width, -1.0),
        vec2<f32>(line_x + width, -1.0),
        vec2<f32>(line_x - width,  1.0),
        // Second triangle
        vec2<f32>(line_x + width, -1.0),
        vec2<f32>(line_x + width,  1.0),
        vec2<f32>(line_x - width,  1.0)
    );

    var output: VertexOutput;
    output.position = vec4<f32>(positions[vertex_index], 0.0, 1.0);
    return output;
}

@fragment
fn fs_main() -> @location(0) vec4<f32> {
    return vec4<f32>(1.0, 1.0, 1.0, 0.8); // Semi-transparent white
}