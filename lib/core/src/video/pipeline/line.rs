use iced_wgpu::wgpu;

use super::PipelineConfig;
// Line-specific uniforms
#[repr(C, align(16))] // Add explicit alignment
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LineUniforms {
    pub position: f32,    // 4 bytes
    pub _pad1: [f32; 3],  // 12 bytes padding for alignment
    pub bounds: [f32; 4], // 16 bytes (vec4 in shader)
    pub line_width: f32,  // 4 bytes
    pub _pad2: [f32; 7],  // 28 bytes padding to reach 64 total
}
pub struct LinePipeline {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,
    config: PipelineConfig,
}

impl LinePipeline {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let config = PipelineConfig {
            format,
            blend_state: Some(wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::SrcAlpha,
                    dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                    operation: wgpu::BlendOperation::Add,
                },
            }),
            ..Default::default()
        };

        let bind_group_layout = Self::create_bind_group_layout(device);
        let pipeline = Self::create_pipeline(device, &config, &bind_group_layout);
        let uniform_buffer = Self::create_uniform_buffer(device);
        let bind_group = Self::create_bind_group(device, &bind_group_layout, &uniform_buffer);

        Self {
            pipeline,
            bind_group_layout,
            bind_group,
            uniform_buffer,
            config,
        }
    }

    fn create_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("line_bind_group_layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        })
    }

    fn create_bind_group(
        device: &wgpu::Device,
        bind_group_layout: &wgpu::BindGroupLayout,
        uniform_buffer: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("line_bind_group"),
            layout: bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        })
    }

    fn create_pipeline(
        device: &wgpu::Device,
        config: &PipelineConfig,
        bind_group_layout: &wgpu::BindGroupLayout,
    ) -> wgpu::RenderPipeline {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("line_shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../../../assets/shaders/split_line.wgsl").into(),
            ),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("line_pipeline_layout"),
            bind_group_layouts: &[bind_group_layout],
            push_constant_ranges: &[],
        });

        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("line_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: config.blend_state,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: config.sample_count,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
        })
    }

    pub fn create_uniform_buffer(device: &wgpu::Device) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("line_uniform_buffer"),
            size: 64, // Explicit size of 64 bytes
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    pub fn update_uniforms(
        &self,
        queue: &wgpu::Queue,
        buffer: &wgpu::Buffer,
        uniforms: &LineUniforms,
    ) {
        queue.write_buffer(buffer, 0, bytemuck::cast_slice(&[*uniforms]));
    }

    pub fn draw(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        clip: &iced::Rectangle<u32>,
    ) {
        println!("Drawing line: clip = {:?}", clip);
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("line_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        pass.set_viewport(
            clip.x as f32,
            clip.y as f32,
            clip.width as f32,
            clip.height as f32,
            0.0,
            1.0,
        );

        pass.set_scissor_rect(clip.x, clip.y, clip.width, clip.height);
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.draw(0..6, 0..1);
    }

    pub fn prepare(&self, queue: &wgpu::Queue, bounds: &iced::Rectangle, position: f32) {
        let uniforms = LineUniforms {
            position: position * 2.0 - 1.0,
            _pad1: [0.0; 3],
            bounds: [bounds.x, bounds.y, bounds.width, bounds.height],
            line_width: 2.0,
            _pad2: [0.0; 7],
        };

        println!("Line Uniform Debug:");
        println!("  Position: {}", uniforms.position);
        println!(
            "  Bounds: [{}, {}, {}, {}]",
            uniforms.bounds[0], uniforms.bounds[1], uniforms.bounds[2], uniforms.bounds[3]
        );
        println!("  Line width: {}", uniforms.line_width);
        println!("  Total size: {}", std::mem::size_of::<LineUniforms>());

        self.update_uniforms(queue, &self.uniform_buffer, &uniforms);
    }
}
