@group(0) @binding(0) var input_texture: texture_2d<f32>;
@group(0) @binding(1) var s_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var output: VertexOutput;
    
    // Use switch for positions
    switch(vertex_index) {
        case 0u: {
            output.position = vec4<f32>(-1.0, -1.0, 0.0, 1.0);
            output.tex_coords = vec2<f32>(0.0, 1.0);
        }
        case 1u: {
            output.position = vec4<f32>(1.0, -1.0, 0.0, 1.0);
            output.tex_coords = vec2<f32>(1.0, 1.0);
        }
        case 2u: {
            output.position = vec4<f32>(-1.0, 1.0, 0.0, 1.0);
            output.tex_coords = vec2<f32>(0.0, 0.0);
        }
        case 3u: {
            output.position = vec4<f32>(-1.0, 1.0, 0.0, 1.0);
            output.tex_coords = vec2<f32>(0.0, 0.0);
        }
        case 4u: {
            output.position = vec4<f32>(1.0, -1.0, 0.0, 1.0);
            output.tex_coords = vec2<f32>(1.0, 1.0);
        }
        default: {
            output.position = vec4<f32>(1.0, 1.0, 0.0, 1.0);
            output.tex_coords = vec2<f32>(1.0, 0.0);
        }
    }
    
    return output;
}

@fragment
fn fs_main(@location(0) tex_coords: vec2<f32>) -> @location(0) vec4<f32> {
    let color = textureSample(input_texture, s_sampler, tex_coords);
    let gray = dot(color.rgb, vec3<f32>(0.299, 0.587, 0.114));
    return vec4<f32>(gray, gray, gray, color.a);
}