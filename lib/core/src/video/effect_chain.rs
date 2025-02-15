use iced_wgpu::primitive::Primitive;
use iced_wgpu::wgpu;
use std::{
    collections::{btree_map::Entry, BTreeMap},
    num::NonZero,
    sync::atomic::{AtomicUsize, Ordering},
};

use super::ShaderEffect;

pub struct EffectChain {
    pub effects: Vec<ShaderEffect>,
    pub bind_groups: Vec<wgpu::BindGroup>,
}

impl EffectChain {
    pub fn new() -> Self {
        Self {
            effects: Vec::new(),
            bind_groups: Vec::new(),
        }
    }

    pub fn add_effect(&mut self, effect: ShaderEffect) {
        self.effects.push(effect);
    }

    pub fn add_bind_group(&mut self, bind_group: wgpu::BindGroup) {
        self.bind_groups.push(bind_group);
    }

    pub fn effects(&self) -> &[ShaderEffect] {
        &self.effects
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
    pub fn effects_mut(&mut self) -> &mut [ShaderEffect] {
        &mut self.effects
    }
}
