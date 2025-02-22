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
@fragment
fn fs_main(@location(0) tex_coords: vec2<f32>) -> @location(0) vec4<f32> {
    // Texture size and dimension checks
    let tex_size = textureDimensions(input_texture);
    
    // Extensive diagnostic logging
    if (tex_size.x == 0u || tex_size.y == 0u) {
        return vec4<f32>(1.0, 0.0, 0.0, 1.0); // Red for invalid texture size
    }

    // Safe UV clamping
    let safe_uv = clamp(tex_coords, vec2<f32>(0.0), vec2<f32>(1.0));
    
    // Sample the texture
    let color = textureSample(input_texture, s_sampler, safe_uv);
    
    // Detailed color diagnostics
    let r = color.r;
    let g = color.g;
    let b = color.b;
    let color_intensity = length(color.rgb);

    // Diagnostic for zero values
    if (r == 0.0 && g == 0.0 && b == 0.0) {
        // More informative diagnostic for zero values
        return vec4<f32>(
            0.5,  // Mid-tone red
            0.0,  // No green
            0.5,  // Mid-tone blue
            1.0
        );
    }

    // Intensity check with more nuanced output
    if (color_intensity < 0.001) {
        // Showcase actual color values
        return vec4<f32>(
            abs(r) * 5.0,   // Amplified red
            abs(g) * 5.0,   // Amplified green
            abs(b) * 5.0,   // Amplified blue
            1.0
        );
    }

    // Normal processing
    let processed_color = apply_upscale(color, safe_uv);
    
    return processed_color;
}

fn apply_upscale(color: vec4<f32>, tex_coords: vec2<f32>) -> vec4<f32> {
    // Color processing with additional safety checks
    let processed_color = apply_color_processing(color);
    
    let tex_size = textureDimensions(input_texture);
    let pixel_size = 1.0 / vec2<f32>(tex_size);
    
    // Sample neighboring pixels with boundary checks
    let l = safe_texture_sample(input_texture, s_sampler, tex_coords + vec2<f32>(-pixel_size.x, 0.0));
    let r = safe_texture_sample(input_texture, s_sampler, tex_coords + vec2<f32>(pixel_size.x, 0.0));
    let t = safe_texture_sample(input_texture, s_sampler, tex_coords + vec2<f32>(0.0, -pixel_size.y));
    let b = safe_texture_sample(input_texture, s_sampler, tex_coords + vec2<f32>(0.0, pixel_size.y));

    // Edge detection with additional robustness
    let luma_c = get_luma(processed_color);
    let luma_l = get_luma(l);
    let luma_r = get_luma(r);
    let luma_t = get_luma(t);
    let luma_b = get_luma(b);

    let grad_x = abs(luma_r - luma_l);
    let grad_y = abs(luma_t - luma_b);

    let edge_threshold = 0.05;
    let edge = sqrt(grad_x * grad_x + grad_y * grad_y);

    var output_color = processed_color;
    
    if (edge > edge_threshold) {
        // Edge enhancement with clamping
        let enhance_factor = 1.2;
        output_color = vec4<f32>(
            min(output_color.r * enhance_factor, 1.0),
            min(output_color.g * enhance_factor, 1.0),
            min(output_color.b * enhance_factor, 1.0),
            output_color.a
        );
    }

    // Final adjustments with additional safety
    let gamma = 0.95;
    let brightness = 1.05;
    
    return vec4<f32>(
        clamp(pow(output_color.r * brightness, gamma), 0.0, 1.0),
        clamp(pow(output_color.g * brightness, gamma), 0.0, 1.0),
        clamp(pow(output_color.b * brightness, gamma), 0.0, 1.0),
        output_color.a
    );
}

// Safe texture sampling to prevent out-of-bounds access
fn safe_texture_sample(tex: texture_2d<f32>, s: sampler, uv: vec2<f32>) -> vec4<f32> {
    let tex_size = textureDimensions(tex);
    let safe_uv = clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0));
    return textureSample(tex, s, safe_uv);
}