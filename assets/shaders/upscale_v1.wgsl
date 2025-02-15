struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
}

struct ShaderUniforms {
    comparison_enabled: u32,
    comparison_position: f32,
    color_threshold: f32, 
    color_blend_mode: u32, // 0: Sharp, 1: Soft Blend, 2: Adaptive
}

@group(0) @binding(0) var input_texture: texture_2d<f32>;
@group(0) @binding(1) var s_sampler: sampler;
@group(0) @binding(2) var<uniform> uniforms: ShaderUniforms;

fn rgb_to_lab(rgb: vec3<f32>) -> vec3<f32> {
    // Convert RGB to XYZ
    let m = mat3x3<f32>(
        0.4124564, 0.3575761, 0.1804375,
        0.2126729, 0.7151522, 0.0721750,
        0.0193339, 0.1191920, 0.9503041
    );
    let xyz = m * rgb;
    
    // XYZ to LAB conversion
    let white_point = vec3<f32>(0.95047, 1.0, 1.0880);
    let epsilon = 0.008856;
    let kappa = 903.3;
    
    var lab: vec3<f32>;
    
    let xyz_normalized = xyz / white_point;
    
    let fx = select(
        pow(xyz_normalized.x, 1.0 / 3.0),
        (7.787 * xyz_normalized.x) + (16.0 / 116.0),
        xyz_normalized.x > epsilon
    );
    let fy = select(
        pow(xyz_normalized.y, 1.0 / 3.0),
        (7.787 * xyz_normalized.y) + (16.0 / 116.0),
        xyz_normalized.y > epsilon
    );
    let fz = select(
        pow(xyz_normalized.z, 1.0 / 3.0),
        (7.787 * xyz_normalized.z) + (16.0 / 116.0),
        xyz_normalized.z > epsilon
    );
    
    lab.x = (116.0 * fy) - 16.0;
    lab.y = 500.0 * (fx - fy);
    lab.z = 200.0 * (fy - fz);
    
    return lab;
}
fn color_distance(color1: vec3<f32>, color2: vec3<f32>) -> f32 {
    let lab1 = rgb_to_lab(color1);
    let lab2 = rgb_to_lab(color2);
    
    return length(lab1 - lab2);
}
fn adaptive_color_blend(color1: vec4<f32>, color2: vec4<f32>) -> vec4<f32> {
    let distance = color_distance(color1.rgb, color2.rgb);
    // let blend_factor = smoothstep(0.0, uniforms.color_threshold, distance);
    let blend_factor = smoothstep(0.0, 2.5, distance);
    
    return mix(color1, color2, blend_factor);
}

fn apply_color_processing(color: vec4<f32>) -> vec4<f32> {
    switch uniforms.color_blend_mode {
        case 0u { // Sharp mode - Color Quantization
            // Quantize colors into discrete levels
            let quantization_levels = 8.0;
            return vec4<f32>(
                floor(color.r * quantization_levels) / quantization_levels,
                floor(color.g * quantization_levels) / quantization_levels,
                floor(color.b * quantization_levels) / quantization_levels,
                color.a
            );
        }
        case 1u { // Soft Blend mode - Color Smoothing
            // Apply a slight gaussian-like smoothing to color transitions
            let smoothing_factor = 0.1;
            return vec4<f32>(
                color.r * (1.0 - smoothing_factor) + smoothing_factor * 0.5,
                color.g * (1.0 - smoothing_factor) + smoothing_factor * 0.5,
                color.b * (1.0 - smoothing_factor) + smoothing_factor * 0.5,
                color.a
            );
        }
        case 2u { // Adaptive mode - Intelligent Color Normalization
            // Normalize colors while preserving relative differences
            let luma = dot(color.rgb, vec3<f32>(0.299, 0.587, 0.114));
            let color_intensity = length(color.rgb);
            let normalized_color = color.rgb / max(color_intensity, 0.001);
            
            return vec4<f32>(
                normalized_color * luma,
                color.a
            );
        }
        default { return color; }
    }
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

fn get_luma(color: vec4<f32>) -> f32 {
    return dot(color.rgb, vec3<f32>(0.299, 0.587, 0.114));
}

fn apply_upscale(color: vec4<f32>, tex_coords: vec2<f32>) -> vec4<f32> {
    let processed_color = apply_color_processing(color);
    let tex_size = textureDimensions(input_texture);
    let pixel_size = 1.0 / vec2<f32>(tex_size);
    
    let l = textureSample(input_texture, s_sampler, tex_coords + vec2<f32>(-pixel_size.x, 0.0));
    let r = textureSample(input_texture, s_sampler, tex_coords + vec2<f32>(pixel_size.x, 0.0));
    let t = textureSample(input_texture, s_sampler, tex_coords + vec2<f32>(0.0, -pixel_size.y));
    let b = textureSample(input_texture, s_sampler, tex_coords + vec2<f32>(0.0, pixel_size.y));

    // Edge detection
    let luma_c = get_luma(processed_color);
    let luma_l = get_luma(l);
    let luma_r = get_luma(r);
    let luma_t = get_luma(t);
    let luma_b = get_luma(b);

    let grad_x = abs(luma_r - luma_l);
    let grad_y = abs(luma_t - luma_b);

    let strength = 1.0;
    let edge_threshold = 0.05;
    let edge = sqrt(grad_x * grad_x + grad_y * grad_y);

    var output_color = processed_color;
    
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
    
    // Debug visualization
    if uniforms.comparison_enabled == 1u {
        // Add a visible split line
        let split_width = 0.005; // Make it wider for visibility
        if abs(tex_coords.x - uniforms.comparison_position) < split_width {
            return vec4<f32>(1.0, 0.0, 0.0, 1.0); // Bright red line
        }
        
        // Tint the sides slightly to see the split
        if tex_coords.x > uniforms.comparison_position {
            return color * vec4<f32>(1.0, 0.9, 0.9, 1.0); // Slight red tint for original
        } else {
            return apply_upscale(color, tex_coords) * vec4<f32>(0.9, 1.0, 0.9, 1.0); // Slight green tint for processed
        }
    }
    
    return apply_upscale(color, tex_coords);
}