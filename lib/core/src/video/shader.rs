use iced_wgpu::wgpu;
use indexmap::IndexMap;
use std::{collections::HashMap, num::NonZero, ops::Index};

// Custom uniform type to support different uniform data types
#[derive(Clone, Debug)]
pub enum UniformValue {
    Float(f32),
    Int(i32),
    Uint(u32),
    Vec2([f32; 2]),
    Vec3([f32; 3]),
    Vec4([f32; 4]),
    Mat3([[f32; 3]; 3]),
    Mat4([[f32; 4]; 4]),
}

#[derive(Debug)]
pub struct ShaderUniforms {
    buffer: wgpu::Buffer,
    pub values: IndexMap<String, UniformValue>,
    layout_entry: wgpu::BindGroupLayoutEntry,
}
impl ShaderUniforms {
    pub fn get_float(&self, name: &str) -> Option<f32> {
        self.values.get(name).and_then(|value| match value {
            UniformValue::Float(v) => Some(*v),
            _ => None,
        })
    }

    pub fn get_uint(&self, name: &str) -> Option<u32> {
        self.values.get(name).and_then(|value| match value {
            UniformValue::Uint(v) => Some(*v),
            _ => None,
        })
    }
    pub fn debug_print_values(&self) {
        println!("Current uniform values:");
        let mut sorted_values: Vec<_> = self.values.iter().collect();
        sorted_values.sort_by_key(|(name, _)| *name);

        for (name, value) in sorted_values {
            println!("  {}: {:?}", name, value);
        }
    }
    pub fn validate_layout(&self) {
        let mut offset = 0;
        let sorted_values: Vec<_> = self.values.iter().collect();
        // sorted_values.sort_by_key(|(name, _)| *name);

        println!("\nUniform layout validation:");
        for (name, value) in &sorted_values {
            while offset % 4 != 0 {
                offset += 1;
            }
            println!("  {} at offset {}, size {}", name, offset, value.size());
            offset += value.size();
        }
        println!("Total size (before alignment): {}", offset);
        println!("Aligned size: {}\n", (offset + 15) & !15);
    }
}
// First, add methods to calculate uniform sizes
impl UniformValue {
    pub fn size(&self) -> usize {
        match self {
            UniformValue::Float(_) | UniformValue::Int(_) | UniformValue::Uint(_) => 4,
            UniformValue::Vec2(_) => 8,
            UniformValue::Vec3(_) => 12,
            UniformValue::Vec4(_) => 16,
            UniformValue::Mat3(_) => 36,
            UniformValue::Mat4(_) => 64,
        }
    }

    pub fn as_bytes(&self) -> Vec<u8> {
        match self {
            UniformValue::Float(v) => bytemuck::cast_slice(&[*v]).to_vec(),
            UniformValue::Int(v) => bytemuck::cast_slice(&[*v]).to_vec(),
            UniformValue::Uint(v) => bytemuck::cast_slice(&[*v]).to_vec(),
            UniformValue::Vec2(v) => bytemuck::cast_slice(v).to_vec(),
            UniformValue::Vec3(v) => bytemuck::cast_slice(v).to_vec(),
            UniformValue::Vec4(v) => bytemuck::cast_slice(v).to_vec(),
            UniformValue::Mat3(v) => bytemuck::cast_slice(v).to_vec(),
            UniformValue::Mat4(v) => bytemuck::cast_slice(v).to_vec(),
        }
    }
}

impl ShaderUniforms {
    pub fn new(device: &wgpu::Device, binding: u32) -> Self {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shader_uniforms_buffer"),
            size: 256, // Fixed size buffer that can hold several uniforms
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let layout_entry = wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };

        Self {
            buffer,
            values: IndexMap::new(),
            layout_entry,
        }
    }
    pub fn update_buffer(&self, queue: &wgpu::Queue) {
        let mut data = Vec::new();
        let mut offset = 0;

        // Values will be in insertion order
        for (name, value) in &self.values {
            while offset % 4 != 0 {
                data.push(0);
                offset += 1;
            }

            let value_data = value.as_bytes();
            data.extend_from_slice(&value_data);
            offset += value_data.len();

            println!(
                "Added uniform {} at offset {}: {:?}",
                name,
                offset - value_data.len(),
                value
            );
        }

        while data.len() % 16 != 0 {
            data.push(0);
        }

        println!("Final buffer size: {}, data: {:?}", data.len(), data);
        queue.write_buffer(&self.buffer, 0, &data);
    }
    pub fn set_uniform(&mut self, name: &str, value: UniformValue) {
        self.values.insert(name.to_string(), value);
    }

    pub fn buffer(&self) -> &wgpu::Buffer {
        &self.buffer
    }
}

pub struct ShaderEffectBuilder {
    name: String,
    shader_source: String,
    uniforms: Option<ShaderUniforms>,
    pending_uniforms: HashMap<String, UniformValue>,
    texture_bindings: Vec<wgpu::BindGroupLayoutEntry>,
    bind_group_layout: Option<wgpu::BindGroupLayout>,
    sampler_bindings: Vec<wgpu::BindGroupLayoutEntry>,
}

impl ShaderEffectBuilder {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            shader_source: String::new(),
            uniforms: None,
            bind_group_layout: None,
            pending_uniforms: HashMap::new(),
            texture_bindings: Vec::new(),
            sampler_bindings: Vec::new(),
        }
    }

    pub fn with_shader_source(mut self, source: &str) -> Self {
        self.shader_source = source.to_string();
        self
    }
    pub fn with_uniform(mut self, name: &str, value: UniformValue) -> Self {
        self.pending_uniforms.insert(name.to_string(), value);
        self
    }

    pub fn with_bind_group_layout(mut self, layout: wgpu::BindGroupLayout) -> Self {
        self.bind_group_layout = Some(layout);
        self
    }
    pub fn with_uniforms(mut self, uniforms: ShaderUniforms) -> Self {
        self.uniforms = Some(uniforms);
        self
    }

    pub fn with_texture_binding(mut self, binding: u32) -> Self {
        self.texture_bindings.push(wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        });
        self
    }

    // In the ShaderEffectBuilder's build method, modify the uniforms handling:
    pub fn build(
        self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
    ) -> ShaderEffect {
        // Create sampler first
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some(&format!("{}_sampler", self.name)),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            lod_min_clamp: 0.0,
            lod_max_clamp: 1.0,
            compare: None,
            anisotropy_clamp: 1,
            border_color: None,
        });

        // Create uniforms
        let uniforms = self
            .uniforms
            .or_else(|| Some(ShaderUniforms::new(device, 2)));

        // Create bind group layout
        // let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        //     label: Some(&format!("{}_bind_group_layout", self.name)),
        //     entries: &[
        //         wgpu::BindGroupLayoutEntry {
        //             binding: 0,
        //             visibility: wgpu::ShaderStages::FRAGMENT,
        //             ty: wgpu::BindingType::Texture {
        //                 sample_type: wgpu::TextureSampleType::Float { filterable: true },
        //                 view_dimension: wgpu::TextureViewDimension::D2,
        //                 multisampled: false,
        //             },
        //             count: None,
        //         },
        //         wgpu::BindGroupLayoutEntry {
        //             binding: 1,
        //             visibility: wgpu::ShaderStages::FRAGMENT,
        //             ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        //             count: None,
        //         },
        //         wgpu::BindGroupLayoutEntry {
        //             binding: 2,
        //             visibility: wgpu::ShaderStages::FRAGMENT,
        //             ty: wgpu::BindingType::Buffer {
        //                 ty: wgpu::BufferBindingType::Uniform,
        //                 has_dynamic_offset: false,
        //                 min_binding_size: Some(NonZero::new(16).unwrap()),
        //             },
        //             count: None,
        //         },
        //     ],
        // });

        println!("Created bind group layout");

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(&format!("{}_shader", self.name)),
            source: wgpu::ShaderSource::Wgsl(self.shader_source.into()),
        });

        let bind_layout_group = self.bind_group_layout.unwrap();
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some(&format!("{}_pipeline_layout", self.name)),
            bind_group_layouts: &[&bind_layout_group],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(&format!("{}_pipeline", self.name)),
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
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        ShaderEffect {
            name: self.name,
            pipeline,
            bind_group_layout: bind_layout_group,
            format,
            uniforms,
            sampler,
        }
    }
}

// Enhanced shader effect struct
pub struct ShaderEffect {
    pub name: String,
    pub pipeline: wgpu::RenderPipeline,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub uniforms: Option<ShaderUniforms>,
    pub sampler: wgpu::Sampler,
    pub format: wgpu::TextureFormat,
}

impl ShaderEffect {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn pipeline(&self) -> &wgpu::RenderPipeline {
        &self.pipeline
    }

    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }
    pub fn get_format(&self) -> &wgpu::TextureFormat {
        &self.format
    }

    pub fn update_uniform(&mut self, name: &str, value: UniformValue, queue: &wgpu::Queue) {
        if let Some(uniforms) = &mut self.uniforms {
            uniforms.set_uniform(name, value);
            uniforms.update_buffer(queue);
        }
    }
}
