struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
}

struct ShaderUniforms {
    comparison_enabled: u32,
    comparison_position: f32,
}

@group(0) @binding(0) var input_texture: texture_2d<f32>;
@group(0) @binding(1) var s_sampler: sampler;
@group(0) @binding(2) var<uniform> uniforms: ShaderUniforms;

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
    let color = textureSample(input_texture, s_sampler, tex_coords);
    
    if uniforms.comparison_enabled == 1u {
        // Only apply grayscale to right side of split
        if tex_coords.x > uniforms.comparison_position {
            let gray = dot(color.rgb, vec3<f32>(0.299, 0.587, 0.114));
            return vec4<f32>(gray, gray, gray, color.a);
        }
        // Return original color for left side
        return color;
    }
    
    // Apply grayscale to whole texture when comparison is disabled
    let gray = dot(color.rgb, vec3<f32>(0.299, 0.587, 0.114));
    return vec4<f32>(gray, gray, gray, color.a);
}