use super::Effect;
use crate::video::{
    pipeline::manager::{VideoEntry, VideoPipelineManager},
    shader::{ShaderEffectBuilder, ShaderUniforms, UniformValue},
    ShaderEffect,
};
use iced_wgpu::wgpu::{self, Texture, TextureView};
use std::{fs, num::NonZero};

#[derive(Clone, Debug)]
pub struct YuvToRgbEffect {
    pub color_space: u32, // 0 for BT.709, 1 for BT.601
    pub format: wgpu::TextureFormat,
}
impl Effect for YuvToRgbEffect {
    fn add(
        &mut self,
        device: &iced_wgpu::wgpu::Device,
        queue: &iced_wgpu::wgpu::Queue,
    ) -> ShaderEffect {
        // Create uniform buffer with color space
        let mut shader_uniforms = ShaderUniforms::new(device, 3); // Changed binding to 3 to match layout

        // Set color space uniform
        shader_uniforms.set_uniform("color_space", UniformValue::Uint(self.color_space));

        // Update the buffer with initial values
        shader_uniforms.update_buffer(queue);

        let shader_source = include_str!("../../../../../../assets/shaders/yuv_to_rgb.wgsl");
        println!(
            "YUV to RGB Shader source loaded: {}",
            shader_source.len() > 0
        );

        // Create bind group layout with more detailed logging
        println!("Creating YUV to RGB bind group layout");
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("yuv_to_rgb_bind_group_layout"),
            entries: &[
                // Y texture
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
                // UV texture
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

        println!(
            "Created YUV to RGB bind group layout with ID: {:?}",
            bind_group_layout.global_id()
        );

        // Store the layout for later comparison
        let layout_id = bind_group_layout.global_id();

        let shader_effect = ShaderEffectBuilder::new("yuv_to_rgb")
            .with_shader_source(shader_source.into())
            .with_bind_group_layout(bind_group_layout)
            .with_uniforms(shader_uniforms)
            .build(device, queue, self.format);

        println!(
            "Created shader effect with layout ID: {:?}",
            shader_effect.bind_group_layout.global_id()
        );

        // Verify layout preservation
        assert_eq!(
            layout_id,
            shader_effect.bind_group_layout.global_id(),
            "Bind group layout ID changed during shader effect creation"
        );

        shader_effect
    }
    // In the update_for_frame method of YuvToRgbEffect
    fn update_for_frame(
        &mut self,
        device: &wgpu::Device,
        effect: &mut ShaderEffect,
        texture_view_list: &[TextureView],
        texture_list: &[&Texture],
    ) -> anyhow::Result<()> {
        // Check if we have enough views
        if texture_view_list.len() < 2 {
            return Err(anyhow::anyhow!("Not enough texture views"));
        }

        // Create a completely new bind group with the current frame's textures
        let bind_group = self.create_bind_group(device, effect, texture_view_list, texture_list)?;

        // Replace the old bind group entirely, don't try to update it
        effect.update_bind_group(bind_group);
        println!("YUV to RGB effect bind group completely replaced");

        Ok(())
    }
    fn create_bind_group(
        &self,
        device: &wgpu::Device,
        effect: &ShaderEffect,
        texture_view_list: &[iced_wgpu::wgpu::TextureView],
        texture_list: &[&iced_wgpu::wgpu::Texture],
    ) -> anyhow::Result<wgpu::BindGroup> {
        effect.debug_layout();

        // Check if we have the expected number of textures
        if texture_view_list.len() < 2 {
            println!(
                "WARNING: YUV to RGB effect received only {} texture views",
                texture_view_list.len()
            );
            println!("This might cause a pink screen if not handled properly");

            // Check formats to see what we're dealing with
            if texture_list.len() > 0 {
                println!("Input texture format: {:?}", texture_list[0].format());

                // If input format is already RGB/BGRA, we might need to pass through directly
                if texture_list[0].format() == wgpu::TextureFormat::Bgra8UnormSrgb {
                    println!("Input is already in BGRA format - attempting to create compatible bind group");

                    // Create a dummy texture for the second binding to make shader happy
                    // Or use the same texture for both bindings
                    let y_texture_view = &texture_view_list[0];

                    // Special case: create bind group that should handle RGB input
                    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("rgb_input_bind_group"),
                        layout: &effect.bind_group_layout,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureView(y_texture_view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::TextureView(y_texture_view), // Use same texture for UV
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

                    println!("Created RGB-compatible bind group");

                    // You might need to modify the shader or set a special uniform to handle RGB input
                    return Ok(bind_group);
                }
            }

            return Err(anyhow::anyhow!(
                "Not enough texture views for YUV to RGB conversion"
            ));
        }

        // Regular case with separate Y and UV textures
        let y_texture_view = &texture_view_list[0];
        let uv_texture_view = &texture_view_list[1];

        println!(
            "Creating bind group with effect layout ID: {:?}",
            effect.bind_group_layout.global_id()
        );

        println!("Y texture format: {:?}", texture_list[0].format());
        if texture_list.len() > 1 {
            println!("UV texture format: {:?}", texture_list[1].format());
        }

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("yuv_to_rgb_bind_group"),
            layout: &effect.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&y_texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&uv_texture_view),
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

        println!(
            "Created standard YUV bind group with layout ID: {:?}",
            &effect.bind_group_layout.global_id()
        );

        Ok(bind_group)
    }

    fn prepare(&mut self, effect: &mut ShaderEffect, queue: &iced_wgpu::wgpu::Queue) {
        if let Some(uniforms) = &mut effect.uniforms {
            uniforms.set_uniform("color_space", UniformValue::Uint(self.color_space));
            uniforms.update_buffer(queue);
        }
    }
    fn update_comparison(&mut self, _: bool, _: f32) {
        // No comparison functionality needed for YUV to RGB conversion
    }

    fn clone_box(&self) -> Box<dyn Effect> {
        Box::new(self.clone())
    }
}

impl YuvToRgbEffect {
    pub fn new(color_space: u32, format: wgpu::TextureFormat) -> Self {
        Self {
            color_space,
            format,
        }
    }
}
