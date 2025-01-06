struct Uniforms {
    position: vec2<f32>,
    bounds: vec4<f32>,  // x, y, width, height
}
@group(0) @binding(0) var<uniform> uniforms: Uniforms;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var output: VertexOutput;
    let line_width = 2.0;
    
    // Calculate positions for a thin vertical line
    var x = uniforms.position.x;
    var y = uniforms.bounds.y;
    
    switch vertex_index {
        case 0u: { output.position = vec4<f32>(x - line_width/2.0, y, 0.0, 1.0); }
        case 1u: { output.position = vec4<f32>(x + line_width/2.0, y, 0.0, 1.0); }
        case 2u: { output.position = vec4<f32>(x - line_width/2.0, y + uniforms.bounds.w, 0.0, 1.0); }
        case 3u: { output.position = vec4<f32>(x + line_width/2.0, y + uniforms.bounds.w, 0.0, 1.0); }
        default: { output.position = vec4<f32>(0.0); }
    }
    return output;
}

@fragment
fn fs_main() -> @location(0) vec4<f32> {
    return vec4<f32>(1.0, 1.0, 1.0, 0.8);
}