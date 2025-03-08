use super::Effect;
use crate::video::{
    pipeline::manager::{VideoEntry, VideoPipelineManager},
    shader::{ShaderEffectBuilder, ShaderUniforms, UniformValue},
    ShaderEffect,
};
use iced_wgpu::wgpu::{self, Texture, TextureView};
use std::{fs, num::NonZero};
use tracing::{debug, error, info, warn};

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
        // Binding is 3 to match the shader layout
        let mut shader_uniforms = ShaderUniforms::new(device, 3);

        // Set color space uniform
        shader_uniforms.set_uniform("color_space", UniformValue::Uint(self.color_space));
        shader_uniforms.update_buffer(queue);

        let shader_source = include_str!("../../../../../../assets/shaders/yuv_to_rgb.wgsl");
        debug!("YUV to RGB Shader loaded: {} bytes", shader_source.len());

        // Create bind group layout for YUV conversion
        // This layout defines how shader accesses textures and uniform data
        debug!("Creating YUV to RGB bind group layout");
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("yuv_to_rgb_bind_group_layout"),
            entries: &[
                // Y texture (luma component)
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
                // UV texture (chroma components)
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
                // Sampler for texture access
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                // Uniforms buffer (contains color space info)
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

        debug!(
            "Created bind group layout with ID: {:?}",
            bind_group_layout.global_id()
        );

        // Store the layout ID for debugging and validation
        let layout_id = bind_group_layout.global_id();

        // Build the shader effect with our layout and uniforms
        let shader_effect = ShaderEffectBuilder::new("yuv_to_rgb")
            .with_shader_source(shader_source.into())
            .with_bind_group_layout(bind_group_layout)
            .with_uniforms(shader_uniforms)
            .build(device, queue, self.format);

        debug!(
            "Created shader effect with layout ID: {:?}",
            shader_effect.bind_group_layout.global_id()
        );

        // Verify layout preservation (sanity check)
        assert_eq!(
            layout_id,
            shader_effect.bind_group_layout.global_id(),
            "Bind group layout ID changed during shader effect creation"
        );

        shader_effect
    }

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

        // YUV conversion requires new bind groups for each frame
        // as the texture views change with each video frame
        let bind_group = self.create_bind_group(device, effect, texture_view_list, texture_list)?;

        // Replace the old bind group entirely
        effect.update_bind_group(bind_group);
        debug!("YUV to RGB effect bind group replaced for new frame");

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

        // Handling insufficient textures case (expected: Y and UV)
        if texture_view_list.len() < 2 {
            warn!(
                "YUV to RGB effect received only {} texture views (expected 2)",
                texture_view_list.len()
            );
            warn!("This might cause a pink screen if not handled properly");

            // Check if we have any textures and examine their format
            if !texture_list.is_empty() {
                let input_format = texture_list[0].format();
                debug!("Input texture format: {:?}", input_format);

                // Special handling for already RGB/BGRA formatted input
                // This is a fallback for when we receive RGB data instead of YUV
                if input_format == wgpu::TextureFormat::Bgra8UnormSrgb {
                    info!("Input is already in BGRA format - creating compatible bind group");

                    // Use same texture for both Y and UV to satisfy binding requirements
                    let texture_view = &texture_view_list[0];

                    // Create a bind group that can handle RGB input
                    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("rgb_input_bind_group"),
                        layout: &effect.bind_group_layout,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureView(texture_view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::TextureView(texture_view), // Same view for UV
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

                    debug!("Created RGB-compatible bind group for fallback handling");
                    return Ok(bind_group);
                }
            }

            return Err(anyhow::anyhow!(
                "Not enough texture views for YUV to RGB conversion"
            ));
        }

        // Normal case: Create bind group with separate Y and UV textures
        let y_texture_view = &texture_view_list[0];
        let uv_texture_view = &texture_view_list[1];

        // Log texture formats for debugging
        debug!("Y texture format: {:?}", texture_list[0].format());
        if texture_list.len() > 1 {
            debug!("UV texture format: {:?}", texture_list[1].format());
        }

        // Create bind group with all required resources for YUV to RGB conversion
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("yuv_to_rgb_bind_group"),
            layout: &effect.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(y_texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(uv_texture_view),
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

        debug!("Created standard YUV-to-RGB bind group");
        Ok(bind_group)
    }

    fn prepare(&mut self, effect: &mut ShaderEffect, queue: &iced_wgpu::wgpu::Queue) {
        // Update color space uniform if needed
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
