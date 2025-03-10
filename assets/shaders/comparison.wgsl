struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

struct Uniforms {
    line_position: f32,
}

@group(0) @binding(0) var original_texture: texture_2d<f32>;
@group(0) @binding(1) var processed_texture: texture_2d<f32>;
@group(0) @binding(2) var texture_sampler: sampler;
@group(0) @binding(3) var<uniform> uniforms: Uniforms;

@vertex
fn vs_main(@builtin(vertex_index) vertex_idx: u32) -> VertexOutput {
    var positions = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>( 1.0,  1.0),
    );
    
    var uvs = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(1.0, 0.0),
    );
    
    var output: VertexOutput;
    output.position = vec4<f32>(positions[vertex_idx], 0.0, 1.0);
    output.uv = uvs[vertex_idx];
    return output;
}
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Calculate how close we are to the dividing line
    let line_distance = abs(in.uv.x - uniforms.line_position);
    
    // Sample the appropriate texture using ternary operator style
    let color = select(
        textureSample(processed_texture, texture_sampler, in.uv),
        textureSample(original_texture, texture_sampler, in.uv),
        in.uv.x < uniforms.line_position
    );
    
    // Create a line effect with a dark color with opacity
    if (line_distance < 0.002) {
        // Dark line color with opacity
        let line_color = vec3<f32>(0.1, 0.1, 0.1); // Almost black
        let opacity = 0.9;
        
        // Blend the line color with the original content
        return vec4<f32>(
            line_color.r * opacity + color.r * (1.0 - opacity),
            line_color.g * opacity + color.g * (1.0 - opacity),
            line_color.b * opacity + color.b * (1.0 - opacity),
            color.a
        );
    } else {
        return color;
    }
}