use std::collections::HashMap;

use crate::video::{shader::ShaderUniforms, ShaderEffect};

use iced_wgpu::primitive::Primitive;
use iced_wgpu::wgpu;
use std::{
    collections::{btree_map::Entry, BTreeMap},
    num::NonZero,
    sync::atomic::{AtomicUsize, Ordering},
};

use super::manager::VideoPipelineManager;

pub mod upscale;
pub mod yuv_to_rgb;

pub trait Effect: Send + Sync {
    fn add(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) -> ShaderEffect;
    fn prepare(&mut self, effect: &mut ShaderEffect, queue: &wgpu::Queue);
    fn create_bind_group(
        &self,
        device: &wgpu::Device,
        effect: &ShaderEffect,
        input_texture_view: Vec<&wgpu::TextureView>,
        input_texture: Vec<&wgpu::Texture>,
    ) -> anyhow::Result<wgpu::BindGroup>;
    fn update_comparison(&mut self, comparison_enabled: bool, comparison_position: f32);
    fn clone_box(&self) -> Box<dyn Effect>;
}
pub struct EffectManager {
    pub effects: Vec<(ShaderEffect, Box<dyn Effect>)>,
    pub bind_groups: Vec<wgpu::BindGroup>,
}

impl EffectManager {
    pub fn new() -> Self {
        Self {
            effects: Vec::new(),
            bind_groups: Vec::new(),
        }
    }

    pub fn add_effect(&mut self, effect: ShaderEffect, state: Box<dyn Effect>) {
        self.effects.push((effect, state));
    }

    pub fn add_bind_group(&mut self, bind_group: wgpu::BindGroup) {
        self.bind_groups.push(bind_group);
    }

    pub fn bind_groups(&self) -> &[wgpu::BindGroup] {
        &self.bind_groups
    }

    pub fn clear_bind_groups(&mut self) {
        self.bind_groups.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.effects.is_empty()
    }

    pub fn len(&self) -> usize {
        self.effects.len()
    }
    pub fn clear(&mut self) {
        self.effects.clear();
    }
}
