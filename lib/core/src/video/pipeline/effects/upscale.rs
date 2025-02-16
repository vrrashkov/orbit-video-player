use std::fs;

use super::Effect;
use crate::video::{
    pipeline::manager::VideoPipelineManager,
    shader::{ShaderEffectBuilder, ShaderUniforms, UniformValue},
    ShaderEffect,
};
use iced_wgpu::wgpu;
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

        let shader_effect = ShaderEffectBuilder::new("effect")
            .with_shader_source(
                include_str!("../../../../../../assets/shaders/upscale_v1.wgsl").into(),
            )
            .build(device, queue, self.format);

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

    fn update_comparison(&mut self, comparison_enabled: bool, comparison_position: f32) {
        self.state.comparison_enabled = comparison_enabled;
        self.state.comparison_position = comparison_position;
    }

    fn clone_box(&self) -> Box<dyn Effect> {
        Box::new(self.clone())
    }
}
