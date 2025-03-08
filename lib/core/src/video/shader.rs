use iced_wgpu::wgpu;
use indexmap::IndexMap;
use std::{collections::HashMap, num::NonZero, ops::Index};
use tracing::{debug, info, trace, warn};

/// Represents different types of uniform values that can be used in shaders
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

/// Manages uniform values and their buffer for shader effects
#[derive(Debug)]
pub struct ShaderUniforms {
    buffer: wgpu::Buffer,
    pub values: IndexMap<String, UniformValue>,
    layout_entry: wgpu::BindGroupLayoutEntry,
}

impl ShaderUniforms {
    /// Retrieve a float uniform value by name
    pub fn get_float(&self, name: &str) -> Option<f32> {
        self.values.get(name).and_then(|value| match value {
            UniformValue::Float(v) => Some(*v),
            _ => None,
        })
    }

    /// Retrieve a uint uniform value by name
    pub fn get_uint(&self, name: &str) -> Option<u32> {
        self.values.get(name).and_then(|value| match value {
            UniformValue::Uint(v) => Some(*v),
            _ => None,
        })
    }

    /// Print all uniform values for debugging
    pub fn debug_print_values(&self) {
        debug!("Current uniform values:");
        let mut sorted_values: Vec<_> = self.values.iter().collect();
        sorted_values.sort_by_key(|(name, _)| *name);

        for (name, value) in sorted_values {
            debug!("  {}: {:?}", name, value);
        }
    }

    /// Validate the memory layout of uniform values
    pub fn validate_layout(&self) {
        let mut offset = 0;
        let sorted_values: Vec<_> = self.values.iter().collect();

        trace!("Uniform layout validation:");
        for (name, value) in &sorted_values {
            // Ensure alignment
            while offset % 4 != 0 {
                offset += 1;
            }
            trace!("  {} at offset {}, size {}", name, offset, value.size());
            offset += value.size();
        }
        trace!("Total size (before alignment): {}", offset);
        trace!("Aligned size: {}", (offset + 15) & !15);
    }
}

impl UniformValue {
    /// Get the size in bytes of this uniform value
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

    /// Convert the uniform value to a byte representation
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
    /// Create a new uniform buffer with the specified binding point
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

    /// Update the GPU buffer with the current uniform values
    pub fn update_buffer(&self, queue: &wgpu::Queue) {
        let mut data = Vec::new();
        let mut offset = 0;

        // Process values in insertion order
        for (name, value) in &self.values {
            // Ensure 4-byte alignment for each value
            while offset % 4 != 0 {
                data.push(0);
                offset += 1;
            }

            let value_data = value.as_bytes();
            data.extend_from_slice(&value_data);
            offset += value_data.len();

            trace!(
                "Added uniform {} at offset {}: {:?}",
                name,
                offset - value_data.len(),
                value
            );
        }

        // Ensure 16-byte alignment for the overall buffer
        while data.len() % 16 != 0 {
            data.push(0);
        }

        trace!("Final uniform buffer size: {}", data.len());
        queue.write_buffer(&self.buffer, 0, &data);
    }

    /// Set or update a uniform value
    pub fn set_uniform(&mut self, name: &str, value: UniformValue) {
        self.values.insert(name.to_string(), value);
    }

    /// Get the underlying GPU buffer
    pub fn buffer(&self) -> &wgpu::Buffer {
        &self.buffer
    }
}

/// Builder for creating shader effects with a fluent API
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
    /// Create a new shader effect builder with the given name
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

    /// Set the WGSL shader source code
    pub fn with_shader_source(mut self, source: &str) -> Self {
        self.shader_source = source.to_string();
        self
    }

    /// Add a uniform value to be included in the shader
    pub fn with_uniform(mut self, name: &str, value: UniformValue) -> Self {
        self.pending_uniforms.insert(name.to_string(), value);
        self
    }

    /// Set a custom bind group layout
    pub fn with_bind_group_layout(mut self, layout: wgpu::BindGroupLayout) -> Self {
        self.bind_group_layout = Some(layout);
        self
    }

    /// Set a pre-configured uniform buffer
    pub fn with_uniforms(mut self, uniforms: ShaderUniforms) -> Self {
        self.uniforms = Some(uniforms);
        self
    }

    /// Add a texture binding at the specified binding point
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

    /// Build the shader effect with all configured options
    pub fn build(
        self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
    ) -> ShaderEffect {
        let bind_layout_group = self
            .bind_group_layout
            .expect("Bind group layout must be provided");

        let original_layout_id = bind_layout_group.global_id();
        debug!(
            "Building effect '{}' with layout ID: {:?}",
            self.name, original_layout_id
        );

        // Create texture sampler
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some(&format!("{}_sampler", self.name)),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            lod_min_clamp: 0.0,
            lod_max_clamp: 1.0,
            compare: None,
            anisotropy_clamp: 1,
            border_color: None,
        });

        // Use provided uniforms or create new ones
        let uniforms = self
            .uniforms
            .or_else(|| Some(ShaderUniforms::new(device, 2)));

        // Create shader module from source
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(&format!("{}_shader", self.name)),
            source: wgpu::ShaderSource::Wgsl(self.shader_source.into()),
        });

        // Create pipeline layout using the bind group layout
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some(&format!("{}_pipeline_layout", self.name)),
            bind_group_layouts: &[&bind_layout_group],
            push_constant_ranges: &[],
        });

        // Create render pipeline with alpha blending
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(&format!("{}_pipeline", self.name)),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main", // Standard entry point for vertex shader
                buffers: &[],           // No vertex buffers needed for fullscreen quad
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main", // Standard entry point for fragment shader
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState {
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
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        debug!(
            "Created pipeline with layout ID: {:?}",
            bind_layout_group.global_id()
        );

        // Assemble the final shader effect
        let effect = ShaderEffect {
            name: self.name,
            pipeline,
            bind_group_layout: bind_layout_group,
            format,
            uniforms,
            sampler,
            current_bind_group: None,
        };

        // Ensure layout ID hasn't changed during the process
        if original_layout_id != effect.bind_group_layout.global_id() {
            warn!(
                "Layout ID changed during ShaderEffect creation: {:?} -> {:?}",
                original_layout_id,
                effect.bind_group_layout.global_id()
            );
        }

        effect
    }
}

/// Represents a GPU shader effect with all associated resources
pub struct ShaderEffect {
    pub name: String,
    pub pipeline: wgpu::RenderPipeline,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub uniforms: Option<ShaderUniforms>,
    pub sampler: wgpu::Sampler,
    pub format: wgpu::TextureFormat,
    pub current_bind_group: Option<wgpu::BindGroup>,
}

impl ShaderEffect {
    /// Get the name of this shader effect
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the render pipeline for this effect
    pub fn pipeline(&self) -> &wgpu::RenderPipeline {
        &self.pipeline
    }

    /// Update the bind group used by this effect
    pub fn update_bind_group(&mut self, bind_group: wgpu::BindGroup) {
        trace!("Updating bind group for effect '{}'", self.name);
        self.current_bind_group = Some(bind_group);
    }

    /// Get the current bind group if one is set
    pub fn get_bind_group(&self) -> Option<&wgpu::BindGroup> {
        self.current_bind_group.as_ref()
    }

    /// Get the bind group layout for this effect
    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }

    /// Get the texture format used by this effect
    pub fn get_format(&self) -> &wgpu::TextureFormat {
        &self.format
    }

    /// Update a single uniform value and sync to GPU
    pub fn update_uniform(&mut self, name: &str, value: UniformValue, queue: &wgpu::Queue) {
        if let Some(uniforms) = &mut self.uniforms {
            trace!(
                "Updating uniform '{}' for effect '{}': {:?}",
                name,
                self.name,
                value
            );
            uniforms.set_uniform(name, value);
            uniforms.update_buffer(queue);
        } else {
            warn!(
                "Attempted to update uniform '{}' but effect '{}' has no uniforms",
                name, self.name
            );
        }
    }

    /// Print debug information about the bind group layout
    pub fn debug_layout(&self) {
        debug!(
            "ShaderEffect '{}' layout ID: {:?}",
            self.name,
            self.bind_group_layout.global_id()
        );
    }
}
