@group(0) @binding(0) var video_texture: texture_2d<f32>;
@group(0) @binding(1) var video_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
}

fn detect_edges(tex_coords: vec2<f32>) -> vec4<f32> {
    let dims = textureDimensions(video_texture);
    let pixel_size = vec2<f32>(1.0 / f32(dims.x), 1.0 / f32(dims.y));
    
    // Sobel operator for edge detection
    let current = textureSample(video_texture, video_sampler, tex_coords).rgb;
    let left = textureSample(video_texture, video_sampler, tex_coords - vec2<f32>(pixel_size.x, 0.0)).rgb;
    let right = textureSample(video_texture, video_sampler, tex_coords + vec2<f32>(pixel_size.x, 0.0)).rgb;
    let top = textureSample(video_texture, video_sampler, tex_coords - vec2<f32>(0.0, pixel_size.y)).rgb;
    let bottom = textureSample(video_texture, video_sampler, tex_coords + vec2<f32>(0.0, pixel_size.y)).rgb;
    
    let edge_x = length(right - left);
    let edge_y = length(bottom - top);
    
    return vec4<f32>(vec3<f32>(sqrt(edge_x * edge_x + edge_y * edge_y)), 1.0);
}

@vertex
fn vs_main(@builtin(vertex_index) in_vertex_index: u32) -> VertexOutput {
    var pos = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(1.0, -1.0),
        vec2<f32>(-1.0, 1.0),
        vec2<f32>(-1.0, 1.0),
        vec2<f32>(1.0, -1.0),
        vec2<f32>(1.0, 1.0)
    );
    
    var tex_coords = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(1.0, 0.0)
    );

    var output: VertexOutput;
    output.position = vec4<f32>(pos[in_vertex_index], 0.0, 1.0);
    output.tex_coords = tex_coords[in_vertex_index];
    return output;
}

@fragment
fn fs_main(@location(0) tex_coords: vec2<f32>) -> @location(0) vec4<f32> {
    // Split screen at x = 0.5
    if (tex_coords.x < 0.5) {
        return textureSample(video_texture, video_sampler, tex_coords);
    } else {
        return detect_edges(tex_coords);
    }
}