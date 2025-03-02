use super::Effect;
use crate::video::{
    pipeline::manager::{VideoEntry, VideoPipelineManager},
    shader::{ShaderEffectBuilder, ShaderUniforms, UniformValue},
    ShaderEffect,
};
use iced_wgpu::wgpu::{self, Texture, TextureView};
use std::{fs, num::NonZero};

#[derive(Clone, Debug)]
pub struct ComparisonEffect {
    pub line_position: f32, // 0.0 to 1.0 for split position
    pub format: wgpu::TextureFormat,
}

impl Effect for ComparisonEffect {
    fn add(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) -> ShaderEffect {
        // Create uniforms for the line position
        let mut shader_uniforms = ShaderUniforms::new(device, 3);
        shader_uniforms.set_uniform("line_position", UniformValue::Float(self.line_position));
        shader_uniforms.update_buffer(queue);

        // - binding 0: original video texture
        // - binding 1: processed result texture
        // - binding 2: sampler
        // - binding 3: uniforms (line position)
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("comparison_bind_group_layout"),
            entries: &[
                // Original texture
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
                // Processed texture
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // Sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                // Uniforms
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: Some(NonZero::new(4).unwrap()),
                    },
                    count: None,
                },
            ],
        });

        let shader_source = include_str!("../../../../../../assets/shaders/comparison.wgsl");
        // Create shader effect
        ShaderEffectBuilder::new("comparison")
            .with_shader_source(shader_source.into())
            .with_bind_group_layout(bind_group_layout)
            .with_uniforms(shader_uniforms)
            .build(device, queue, self.format)
    }
    fn update_for_frame(
        &mut self,
        device: &wgpu::Device,
        effect: &mut ShaderEffect,
        texture_view_list: &[TextureView],
        texture_list: &[&Texture],
    ) -> anyhow::Result<()> {
        // If we're in initial setup and only received one texture
        if texture_view_list.len() < 2 {
            println!("Comparison effect needs two textures but only received {}. Creating temporary bind group.", texture_view_list.len());

            if texture_view_list.is_empty() {
                return Err(anyhow::anyhow!(
                    "No textures provided for comparison effect"
                ));
            }

            // During setup, we'll just use the same texture for both input and output
            // This temporary bind group will be replaced by prepare_comparison_effect later
            let texture_view = &texture_view_list[0];

            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("initial_comparison_bind_group"),
                layout: &effect.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(texture_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(texture_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&effect.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: effect
                            .uniforms
                            .as_ref()
                            .unwrap()
                            .buffer()
                            .as_entire_binding(),
                    },
                ],
            });

            effect.update_bind_group(bind_group);
            println!("Created initial bind group for comparison effect (will be updated later)");
            return Ok(());
        }

        // Normal case - we have both textures
        let bind_group = self.create_bind_group(device, effect, texture_view_list, texture_list)?;
        effect.update_bind_group(bind_group);

        Ok(())
    }

    fn prepare(&mut self, effect: &mut ShaderEffect, queue: &wgpu::Queue) {
        // Update uniforms if needed
        if let Some(uniforms) = &mut effect.uniforms {
            // Update the line position uniform
            uniforms.set_uniform("line_position", UniformValue::Float(self.line_position));
            uniforms.update_buffer(queue);
        }
    }

    fn create_bind_group(
        &self,
        device: &wgpu::Device,
        effect: &ShaderEffect,
        texture_view_list: &[TextureView],
        texture_list: &[&Texture],
    ) -> anyhow::Result<wgpu::BindGroup> {
        if texture_view_list.len() < 2 {
            return Err(anyhow::anyhow!(
                "Comparison effect requires two texture views: original and processed"
            ));
        }

        // First texture should be original, second is processed
        let original_view = &texture_view_list[0];
        let processed_view = &texture_view_list[1];

        // Debug info
        println!("Creating comparison bind group");
        println!("  Original texture format: {:?}", texture_list[0].format());
        println!("  Processed texture format: {:?}", texture_list[1].format());

        // Create bind group with both textures
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("comparison_bind_group"),
            layout: &effect.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(original_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(processed_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&effect.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: effect
                        .uniforms
                        .as_ref()
                        .unwrap()
                        .buffer()
                        .as_entire_binding(),
                },
            ],
        });

        Ok(bind_group)
    }
    fn update_comparison(&mut self, _enable: bool, position: f32) {
        self.line_position = position;
    }
    fn clone_box(&self) -> Box<dyn Effect> {
        Box::new(self.clone())
    }
}
