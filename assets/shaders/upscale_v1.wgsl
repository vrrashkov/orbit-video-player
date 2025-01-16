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

fn get_luma(color: vec4<f32>) -> f32 {
    return dot(color.rgb, vec3<f32>(0.299, 0.587, 0.114));
}

fn apply_upscale(color: vec4<f32>, tex_coords: vec2<f32>) -> vec4<f32> {
    let tex_size = textureDimensions(input_texture);
    let pixel_size = 1.0 / vec2<f32>(tex_size);
    
    let l = textureSample(input_texture, s_sampler, tex_coords + vec2<f32>(-pixel_size.x, 0.0));
    let r = textureSample(input_texture, s_sampler, tex_coords + vec2<f32>(pixel_size.x, 0.0));
    let t = textureSample(input_texture, s_sampler, tex_coords + vec2<f32>(0.0, -pixel_size.y));
    let b = textureSample(input_texture, s_sampler, tex_coords + vec2<f32>(0.0, pixel_size.y));

    // Edge detection
    let luma_c = get_luma(color);
    let luma_l = get_luma(l);
    let luma_r = get_luma(r);
    let luma_t = get_luma(t);
    let luma_b = get_luma(b);

    let grad_x = abs(luma_r - luma_l);
    let grad_y = abs(luma_t - luma_b);

    let strength = 1.0;
    let edge_threshold = 0.05;
    let edge = sqrt(grad_x * grad_x + grad_y * grad_y);

    var output_color = color;
    
    if (edge > edge_threshold) {
        // Edge enhancement
        let enhance_factor = 1.2;  // Increase contrast at edges
        output_color = vec4<f32>(
            output_color.r * enhance_factor,
            output_color.g * enhance_factor,
            output_color.b * enhance_factor,
            output_color.a
        );
    }

    // Final adjustments
    let gamma = 0.95;
    let brightness = 1.05;
    
    return vec4<f32>(
        pow(output_color.r * brightness, gamma),
        pow(output_color.g * brightness, gamma),
        pow(output_color.b * brightness, gamma),
        output_color.a
    );
}

@fragment
fn fs_main(@location(0) tex_coords: vec2<f32>) -> @location(0) vec4<f32> {
    let color = textureSample(input_texture, s_sampler, tex_coords);
    
    if uniforms.comparison_enabled == 1u {
        // Only apply upscaling to left side of split
        if tex_coords.x > uniforms.comparison_position {
            return color;  // Original on right side
        }
        // Return upscaled version for left side
        return apply_upscale(color, tex_coords);
    }
    
    // Apply upscaling to whole texture when comparison is disabled
    return apply_upscale(color, tex_coords);
}