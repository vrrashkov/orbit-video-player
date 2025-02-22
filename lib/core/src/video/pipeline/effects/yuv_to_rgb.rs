use super::Effect;
use crate::video::{
    pipeline::manager::VideoPipelineManager,
    shader::{ShaderEffectBuilder, ShaderUniforms, UniformValue},
    ShaderEffect,
};
use iced_wgpu::wgpu;
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
        let mut shader_uniforms = ShaderUniforms::new(device, 1);

        // Set color space uniform
        shader_uniforms.set_uniform("color_space", UniformValue::Uint(self.color_space));

        // Update the buffer with initial values
        shader_uniforms.update_buffer(queue);

        let shader_source = include_str!("../../../../../../assets/shaders/yuv_to_rgb.wgsl");
        println!(
            "YUV to RGB Shader source loaded: {}",
            shader_source.len() > 0
        );

        // Create bind group layout for YUV to RGB conversion
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
                // Sampler (changed to match shader)
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

        let shader_effect = ShaderEffectBuilder::new("yuv_to_rgb")
            .with_shader_source(shader_source.into())
            .with_bind_group_layout(bind_group_layout)
            .with_uniforms(shader_uniforms)
            .build(device, queue, self.format);

        shader_effect
    }

    fn prepare(&mut self, effect: &mut ShaderEffect, queue: &iced_wgpu::wgpu::Queue) {
        if let Some(uniforms) = &mut effect.uniforms {
            uniforms.set_uniform("color_space", UniformValue::Uint(self.color_space));
            uniforms.update_buffer(queue);
        }
    }

    fn create_bind_group(
        &self,
        device: &wgpu::Device,
        effect: &ShaderEffect,
        input_texture_view_list: Vec<&wgpu::TextureView>,
        input_texture_list: Vec<&wgpu::Texture>,
    ) -> anyhow::Result<wgpu::BindGroup> {
        let y_texture_view = if let Some(value) = input_texture_view_list.get(0) {
            value
        } else {
            return Err(anyhow::anyhow!("Pleaase provide y_texture_view"));
        };
        let uv_texture_view = if let Some(value) = input_texture_view_list.get(1) {
            value
        } else {
            return Err(anyhow::anyhow!("Pleaase provide uv_texture_view"));
        };

        let y_input_texture = if let Some(value) = input_texture_list.get(0) {
            value
        } else {
            return Err(anyhow::anyhow!("Pleaase provide input_texture_list"));
        };
        let uv_input_texture = if let Some(value) = input_texture_list.get(1) {
            value
        } else {
            return Err(anyhow::anyhow!("Pleaase provide input_texture_list"));
        };

        Ok(device.create_bind_group(&wgpu::BindGroupDescriptor {
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
        }))
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
