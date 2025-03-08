use std::collections::HashMap;

use crate::video::{shader::ShaderUniforms, ShaderEffect};

use iced_wgpu::wgpu::{self, Texture};
use iced_wgpu::{primitive::Primitive, wgpu::TextureView};
use std::{
    collections::{btree_map::Entry, BTreeMap},
    num::NonZero,
    sync::atomic::{AtomicUsize, Ordering},
};
use tracing::{debug, info, trace};

use super::manager::{VideoEntry, VideoPipelineManager};

pub mod comparison;
pub mod upscale;
pub mod yuv_to_rgb;

/// Trait defining the interface for all video effects
///
/// All effects must implement this trait to be used in the effect pipeline,
/// providing methods for initialization, updating, and rendering.
pub trait Effect: Send + Sync {
    /// Initialize and create a shader effect with the necessary resources
    fn add(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) -> ShaderEffect;

    /// Update effect parameters before rendering
    fn prepare(&mut self, effect: &mut ShaderEffect, queue: &wgpu::Queue);

    /// Create a bind group connecting textures and uniforms to the shader
    fn create_bind_group(
        &self,
        device: &wgpu::Device,
        effect: &ShaderEffect,
        texture_view_list: &[TextureView],
        texture_list: &[&Texture],
    ) -> anyhow::Result<wgpu::BindGroup>;

    /// Update comparison mode parameters
    fn update_comparison(&mut self, comparison_enabled: bool, comparison_position: f32);

    /// Create a clone of this effect
    fn clone_box(&self) -> Box<dyn Effect>;

    /// Update the effect for the current frame with the given textures
    fn update_for_frame(
        &mut self,
        device: &wgpu::Device,
        effect: &mut ShaderEffect,
        texture_view_list: &[TextureView],
        texture_list: &[&Texture],
    ) -> anyhow::Result<()>;
}

/// Represents a single effect instance in the effect chain
pub struct EffectEntry {
    pub effect: ShaderEffect,
    pub state: Box<dyn Effect + Send + Sync>,
    pub get_from_video: bool, // Flag indicating if this effect uses video textures as input
}

/// Manages a chain of video effects that can be applied sequentially
pub struct EffectManager {
    pub effects: Vec<EffectEntry>,
}

impl EffectManager {
    pub fn new() -> Self {
        Self {
            effects: Vec::new(),
        }
    }

    /// Add a new effect to the end of the effect chain
    pub fn add_effect(&mut self, effect: ShaderEffect, state: Box<dyn Effect + Send + Sync>) {
        debug!(
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

        // Log the current effect chain for debugging
        debug!("Current Effect Chain:");
        for (i, effect_entry) in self.effects.iter().enumerate() {
            debug!(
                "{}. Effect '{}' (Layout ID: {:?})",
                i + 1,
                effect_entry.effect.name,
                effect_entry.effect.bind_group_layout.global_id()
            );

            // Log bind group info if available
            if let Some(bind_group) = effect_entry.effect.get_bind_group() {
                debug!("   Bind Group ID: {:?}", bind_group.global_id());
            }
        }
        debug!("Total effects: {}", self.effects.len());
    }

    /// Get all bind groups from the effect chain for rendering
    pub fn bind_groups(&self) -> Vec<&wgpu::BindGroup> {
        let groups = self
            .effects
            .iter()
            .filter_map(|e| e.effect.get_bind_group())
            .collect::<Vec<_>>();

        trace!("Collecting {} bind groups for rendering", groups.len());

        // Log detailed bind group information at trace level
        for (i, group) in groups.iter().enumerate() {
            trace!("  Effect {}: Bind group ID: {:?}", i, group.global_id());
        }

        groups
    }

    /// Check if the effect chain is empty
    pub fn is_empty(&self) -> bool {
        self.effects.is_empty()
    }

    /// Get the number of effects in the chain
    pub fn len(&self) -> usize {
        self.effects.len()
    }

    /// Clear all effects from the chain
    pub fn clear(&mut self) {
        debug!("Clearing all effects from the effect chain");
        self.effects.clear();
    }
}
