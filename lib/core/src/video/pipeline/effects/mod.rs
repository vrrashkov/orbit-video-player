use std::collections::HashMap;

use crate::video::{shader::ShaderUniforms, ShaderEffect};

use iced_wgpu::wgpu::{self, Texture};
use iced_wgpu::{primitive::Primitive, wgpu::TextureView};
use std::{
    collections::{btree_map::Entry, BTreeMap},
    num::NonZero,
    sync::atomic::{AtomicUsize, Ordering},
};

use super::manager::{VideoEntry, VideoPipelineManager};

pub mod upscale;
pub mod yuv_to_rgb;

pub trait Effect: Send + Sync {
    fn add(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) -> ShaderEffect;
    fn prepare(&mut self, effect: &mut ShaderEffect, queue: &wgpu::Queue);
    fn create_bind_group(
        &self,
        device: &wgpu::Device,
        effect: &ShaderEffect,
        texture_view_list: &[TextureView],
        texture_list: &[&Texture],
    ) -> anyhow::Result<wgpu::BindGroup>;
    fn update_comparison(&mut self, comparison_enabled: bool, comparison_position: f32);
    fn clone_box(&self) -> Box<dyn Effect>;
    // fn update_for_frame(
    //     &mut self,
    //     device: &wgpu::Device,
    //     effect: &mut ShaderEffect,
    //     video: &VideoEntry,
    // ) -> anyhow::Result<()>;
    fn update_for_frame(
        &mut self,
        device: &wgpu::Device,
        effect: &mut ShaderEffect,
        texture_view_list: &[TextureView],
        texture_list: &[&Texture],
    ) -> anyhow::Result<()>;
}

pub struct EffectEntry {
    pub effect: ShaderEffect,
    pub state: Box<dyn Effect>,
    pub get_from_video: bool,
    // pub bind_group: Option<wgpu::BindGroup>,
    // pub layout_id: Option<wgpu::Id<wgpu::BindGroupLayout>>,
}

pub struct EffectManager {
    pub effects: Vec<EffectEntry>,
}
impl EffectManager {
    pub fn new() -> Self {
        Self {
            effects: Vec::new(),
        }
    }
    pub fn add_effect(&mut self, effect: ShaderEffect, state: Box<dyn Effect>) {
        println!(
            "Adding effect '{}' with layout ID: {:?}",
            effect.name,
            effect.bind_group_layout.global_id()
        );

        let entry = EffectEntry {
            effect,
            state,
            get_from_video: false,
        };
        self.effects.push(entry);

        // Print all effects after adding new one
        println!("\nCurrent Effect Chain:");
        for (i, effect_entry) in self.effects.iter().enumerate() {
            println!(
                "{}. Effect '{}' (Layout ID: {:?})",
                i + 1,
                effect_entry.effect.name,
                effect_entry.effect.bind_group_layout.global_id()
            );

            // Print bind group info if available
            if let Some(bind_group) = effect_entry.effect.get_bind_group() {
                println!("   Bind Group ID: {:?}", bind_group.global_id());
            }
        }
        println!("Total effects: {}\n", self.effects.len());
    }

    // pub fn add_bind_group(
    //     &mut self,
    //     bind_group: wgpu::BindGroup,
    //     layout_id: wgpu::Id<wgpu::BindGroupLayout>,
    // ) {
    //     if let Some(entry) = self.effects.last_mut() {
    //         let effect_layout_id = entry.effect.bind_group_layout.global_id();
    //         println!("Adding bind group for effect '{}':", entry.effect.name);
    //         println!(
    //             "  Effect/Layout ID (should match): {:?} == {:?}",
    //             effect_layout_id, layout_id
    //         );
    //         println!(
    //             "  Bind group ID (different by design): {:?}",
    //             bind_group.global_id()
    //         );

    //         // Only assert that layouts match
    //         assert_eq!(
    //             effect_layout_id, layout_id,
    //             "Layout ID mismatch when adding bind group"
    //         );

    //         entry.bind_group = Some(bind_group);
    //         entry.layout_id = Some(layout_id);
    //     }
    // }

    pub fn bind_groups(&self) -> Vec<&wgpu::BindGroup> {
        let groups = self
            .effects
            .iter()
            .filter_map(|e| e.effect.get_bind_group())
            .collect::<Vec<_>>();

        println!("Returning bind groups:");
        for (i, group) in groups.iter().enumerate() {
            println!("  Effect {}: Bind group ID: {:?}", i, group.global_id());
        }

        groups
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
