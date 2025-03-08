use super::Effect;
use crate::video::{
    pipeline::manager::{VideoEntry, VideoPipelineManager},
    shader::{ShaderEffectBuilder, ShaderUniforms, UniformValue},
    ShaderEffect,
};
use iced_wgpu::wgpu::{self, Texture, TextureView};
use std::{fs, num::NonZero};
use tracing::{debug, info, trace};

#[derive(Clone, Debug)]
pub struct UpscaleEffect {
    pub state: UpscaleEffectState,
    pub format: wgpu::TextureFormat,
}

#[derive(Clone, Debug)]
pub struct UpscaleEffectState {
    pub color_threshold: f32,
    pub color_blend_mode: f32,
}

impl Default for UpscaleEffectState {
    fn default() -> Self {
        Self {
            color_threshold: 0.05,
            color_blend_mode: 2.0,
        }
    }
}

impl Effect for UpscaleEffect {
    fn add(
        &mut self,
        device: &iced_wgpu::wgpu::Device,
        queue: &iced_wgpu::wgpu::Queue,
    ) -> ShaderEffect {
        // Create uniform buffer with initial values
        let mut shader_uniforms = ShaderUniforms::new(device, 2);

        shader_uniforms.set_uniform(
            "color_threshold",
            UniformValue::Float(self.state.color_threshold),
        );
        shader_uniforms.set_uniform(
            "color_blend_mode",
            UniformValue::Float(self.state.color_blend_mode),
        );

        // Update the buffer with initial values
        shader_uniforms.update_buffer(queue);

        let shader_source = include_str!("../../../../../../assets/shaders/upscale_v1.wgsl");
        debug!("Shader source loaded: {} bytes", shader_source.len());

        // Create bind group layout with 3 entries:
        // 1. Input texture (binding 0)
        // 2. Sampler (binding 1)
        // 3. Uniform buffer for shader parameters (binding 2)
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("upscale_bind_group_layout"),
            entries: &[
                // @binding(0) var input_texture: texture_2d<f32>
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // @binding(1) var s_sampler: sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                // @binding(2) var<uniform> uniforms: ShaderUniforms
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: Some(NonZero::new(16).unwrap()), // size of ShaderUniforms (2 floats × 4 bytes each + padding = 16 bytes)
                    },
                    count: None,
                },
            ],
        });

        debug!("Using texture format for shader: {:?}", self.format);

        // Build the shader effect with all necessary components
        let shader_effect = ShaderEffectBuilder::new("upscale")
            .with_shader_source(shader_source.into())
            .with_bind_group_layout(bind_group_layout)
            .with_uniforms(shader_uniforms)
            .build(device, queue, self.format);

        debug!("Shader effect created successfully");
        shader_effect
    }

    fn prepare(&mut self, effect: &mut ShaderEffect, queue: &iced_wgpu::wgpu::Queue) {
        // Update uniform values if they've changed since last frame
        if let Some(uniforms) = &mut effect.uniforms {
            uniforms.set_uniform(
                "color_threshold",
                UniformValue::Float(self.state.color_threshold),
            );
            uniforms.set_uniform(
                "color_blend_mode",
                UniformValue::Float(self.state.color_blend_mode),
            );

            trace!(
                "Updated uniforms - threshold: {}, blend mode: {}",
                self.state.color_threshold,
                self.state.color_blend_mode
            );

            // Ensure all required uniforms are present
            uniforms.validate_layout();
            // Push uniform data to GPU
            uniforms.update_buffer(queue);
        }
    }

    fn update_for_frame(
        &mut self,
        device: &wgpu::Device,
        effect: &mut ShaderEffect,
        texture_view_list: &[TextureView],
        texture_list: &[&Texture],
    ) -> anyhow::Result<()> {
        trace!("Updating upscale effect for new frame");

        // Create a bind group connecting the shader to input texture and uniforms
        let bind_group = self.create_bind_group(device, effect, texture_view_list, texture_list)?;

        // Update the shader effect's bind group
        effect.update_bind_group(bind_group);
        Ok(())
    }

    fn create_bind_group(
        &self,
        device: &wgpu::Device,
        effect: &ShaderEffect,
        texture_view_list: &[iced_wgpu::wgpu::TextureView],
        texture_list: &[&iced_wgpu::wgpu::Texture],
    ) -> anyhow::Result<wgpu::BindGroup> {
        // Get the input texture view (required)
        let input_texture_view = if let Some(value) = texture_view_list.get(0) {
            value
        } else {
            return Err(anyhow::anyhow!("No texture view provided"));
        };

        // Get the input texture (for debug info)
        let input_texture = if let Some(value) = texture_list.get(0) {
            value
        } else {
            return Err(anyhow::anyhow!("No texture provided"));
        };

        // Log detailed texture information for debugging purposes
        debug!(
            "Creating upscale bind group with texture: dims={:?}, format={:?}",
            input_texture.size(),
            input_texture.format()
        );

        // Assemble the bind group with all required resources
        // This connects our shader to:
        // 1. The input texture (binding 0)
        // 2. A sampler for texture access (binding 1)
        // 3. The uniform buffer with effect parameters (binding 2)
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("upscale_bind_group"),
            layout: &effect.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(input_texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&effect.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: effect
                        .uniforms
                        .as_ref()
                        .unwrap()
                        .buffer()
                        .as_entire_binding(),
                },
            ],
        });

        trace!("Bind group created successfully");
        Ok(bind_group)
    }

    fn update_comparison(&mut self, comparison_enabled: bool, comparison_position: f32) {
        // No-op for this effect
    }

    fn clone_box(&self) -> Box<dyn Effect> {
        Box::new(self.clone())
    }
}
