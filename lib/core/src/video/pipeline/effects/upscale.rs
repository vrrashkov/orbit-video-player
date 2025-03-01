use super::Effect;
use crate::video::{
    pipeline::manager::{VideoEntry, VideoPipelineManager},
    shader::{ShaderEffectBuilder, ShaderUniforms, UniformValue},
    ShaderEffect,
};
use iced_wgpu::wgpu::{self, Texture, TextureView};
use std::{fs, num::NonZero};

#[derive(Clone, Debug)]
pub struct UpscaleEffect {
    pub state: UpscaleEffectState,
    pub format: wgpu::TextureFormat,
}

#[derive(Clone, Debug)]
pub struct UpscaleEffectState {
    pub comparison_enabled: bool,
    pub comparison_position: f32,
    pub color_threshold: f32,
    pub color_blend_mode: f32,
}

impl Default for UpscaleEffectState {
    fn default() -> Self {
        Self {
            comparison_enabled: false,
            comparison_position: 0.5,
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

        // Add all the uniforms we currently use
        shader_uniforms.set_uniform(
            "comparison_enabled",
            UniformValue::Uint(self.state.comparison_enabled as u32),
        );
        shader_uniforms.set_uniform(
            "comparison_position",
            UniformValue::Float(self.state.comparison_position),
        );
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
        println!("Shader source loaded: {}", shader_source.len() > 0);

        // Create bind group layout
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
                        min_binding_size: Some(NonZero::new(16).unwrap()), // size of ShaderUniforms
                    },
                    count: None,
                },
            ],
        });

        println!("Created bind group layout");

        println!("  Current format SHADER: {:?}", self.format);
        let shader_effect = ShaderEffectBuilder::new("upscale")
            .with_shader_source(shader_source.into())
            .with_bind_group_layout(bind_group_layout)
            .with_uniforms(shader_uniforms)
            .build(device, queue, self.format);

        println!("Created shader effect");
        shader_effect
    }

    fn prepare(&mut self, effect: &mut ShaderEffect, queue: &iced_wgpu::wgpu::Queue) {
        if let Some(uniforms) = &mut effect.uniforms {
            uniforms.set_uniform(
                "comparison_enabled",
                UniformValue::Uint(self.state.comparison_enabled as u32),
            );
            uniforms.set_uniform(
                "comparison_position",
                UniformValue::Float(self.state.comparison_position),
            );
            uniforms.set_uniform(
                "color_threshold",
                UniformValue::Float(self.state.color_threshold),
            );
            uniforms.set_uniform(
                "color_blend_mode",
                UniformValue::Float(self.state.color_blend_mode),
            );

            dbg!("self.state", &self.state);
            uniforms.validate_layout();
            uniforms.update_buffer(queue);
        }
    }
    // fn update_for_frame(
    //     &mut self,
    //     device: &wgpu::Device,
    //     effect: &mut ShaderEffect,
    //     video: &VideoEntry,
    // ) -> anyhow::Result<()> {
    //     let bind_group = self.create_bind_group(
    //         device,
    //         effect,
    //         vec![
    //             video.texture_y.create_view(&Default::default()),
    //             video.texture_uv.create_view(&Default::default()),
    //         ],
    //         vec![&video.texture_y, &video.texture_uv],
    //     )?;

    //     // Update the shader effect's bind group
    //     effect.update_bind_group(bind_group);
    //     Ok(())
    // }
    fn update_for_frame(
        &mut self,
        device: &wgpu::Device,
        effect: &mut ShaderEffect,
        texture_view_list: &[TextureView],
        texture_list: &[&Texture],
    ) -> anyhow::Result<()> {
        println!("update_for_frame UPSCALE");
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
        let input_texture_view = if let Some(value) = texture_view_list.get(0) {
            value
        } else {
            return Err(anyhow::anyhow!("Pleaase provide texture_view_list"));
        };

        let input_texture = if let Some(value) = texture_list.get(0) {
            value
        } else {
            return Err(anyhow::anyhow!("Pleaase provide texture_list"));
        };
        println!(
            "Creating upscale bind group with texture view ptr: {:p}",
            input_texture_view
        );
        println!("Texture dimensions: {:?}", input_texture.size());
        println!("Texture format: {:?}", input_texture.format());
        println!("Creating Upscale bind group with layout:");
        println!("  Expected bindings: [Texture, Sampler, Uniform]");
        println!("  Layout ID: {:?}", effect.bind_group_layout.global_id());
        println!("Creating bind group:");
        println!("  Sampler: {:?}", effect.sampler);
        println!(
            "  Uniform buffer size: {:?}",
            effect.uniforms.as_ref().unwrap().buffer().size()
        );
        println!("Creating bind group for effect:");
        println!("  Input texture dimensions: {:?}", input_texture.size());
        println!("  Input texture format: {:?}", input_texture.format());

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

        println!("Created bind group");
        Ok(bind_group)
    }

    fn update_comparison(&mut self, comparison_enabled: bool, comparison_position: f32) {
        self.state.comparison_enabled = comparison_enabled;
        self.state.comparison_position = comparison_position;
    }

    fn clone_box(&self) -> Box<dyn Effect> {
        Box::new(self.clone())
    }
}
