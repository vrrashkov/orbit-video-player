use ffmpeg_next::color::{self, Space};
use iced_wgpu::primitive::Primitive;
use iced_wgpu::wgpu;
use std::{
    cell::RefCell,
    collections::{btree_map::Entry, BTreeMap},
    num::NonZero,
    ops::Deref,
    rc::Rc,
    sync::atomic::{AtomicUsize, Ordering},
};

use crate::video::pipeline::effects::{
    comparison::ComparisonEffect,
    upscale::{UpscaleEffect, UpscaleEffectState},
    yuv_to_rgb::YuvToRgbEffect,
    Effect,
};

use super::pipeline::manager::VideoPipelineManager;
#[derive(Debug, Clone)]
pub struct VideoPrimitive {
    video_id: u64,
    alive: bool,
    frame: Vec<u8>,
    size: (u32, u32),
    upload_frame: bool,
    color_space: Space,
    comparison_enabled: bool,
    comparison_position: f32,
}

impl VideoPrimitive {
    pub fn new(
        video_id: u64,
        alive: bool,
        frame: Vec<u8>,
        size: (u32, u32),
        upload_frame: bool,
        color_space: Space,
    ) -> Self {
        VideoPrimitive {
            video_id,
            alive,
            frame,
            size,
            upload_frame,
            color_space,
            comparison_enabled: false,
            comparison_position: 0.5,
        }
    }

    pub fn with_comparison(mut self, enabled: bool) -> Self {
        self.comparison_enabled = enabled;
        self
    }

    pub fn with_comparison_position(mut self, position: f32) -> Self {
        self.comparison_position = position.clamp(0.0, 1.0);
        self
    }
}
static FRAME_COUNT: AtomicUsize = AtomicUsize::new(0);
impl Primitive for VideoPrimitive {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        storage: &mut iced_wgpu::primitive::Storage,
        bounds: &iced::Rectangle,
        viewport: &iced_wgpu::graphics::Viewport,
    ) {
        let current_frame = FRAME_COUNT.fetch_add(1, Ordering::SeqCst);

        let has_manager = storage.has::<VideoPipelineManager>();
        dbg!("Has VideoPipelineManager:", has_manager);

        if !has_manager {
            dbg!("Creating new pipeline manager");
            let pipeline_manager = VideoPipelineManager::new(device, format);
            storage.store(pipeline_manager);
        }

        let pipeline_manager = storage.get_mut::<VideoPipelineManager>().unwrap();

        if self.upload_frame {
            pipeline_manager.upload_frame(
                device,
                queue,
                self.video_id,
                self.size.0,
                self.size.1,
                self.frame.as_slice(),
                self.alive,
            );
        }

        // Create a list of effects that should be active based on current settings
        let mut desired_effects = Vec::new();

        // Always add base effects
        if !pipeline_manager.has_effect("upscale") {
            desired_effects.push((
                "upscale",
                Box::new(UpscaleEffect {
                    state: UpscaleEffectState {
                        color_threshold: 1.0,
                        color_blend_mode: 0.5,
                    },
                    format,
                }) as Box<dyn Effect + Send + Sync>,
            ));
        }

        // Add conditional effects
        if self.comparison_enabled && !pipeline_manager.has_effect("comparison") {
            desired_effects.push((
                "comparison",
                Box::new(ComparisonEffect {
                    line_position: self.comparison_position,
                    format,
                }) as Box<dyn Effect + Send + Sync>,
            ));
        }

        // Apply desired effects
        // Instead of iterating with references
        for (name, mut effect) in desired_effects {
            println!("Adding effect: {}", name);
            let shader_effect = effect.add(device, queue);

            pipeline_manager
                .add_effect(false, device, queue, shader_effect, effect)
                .unwrap();
        }

        // Remove effects that should no longer be active
        if !self.comparison_enabled {
            pipeline_manager.remove_effect("comparison");
        }

        // Update effect parameters for those that are active
        for effect in &mut pipeline_manager.effect_manager.effects {
            match effect.effect.name.as_str() {
                "comparison" => {
                    effect
                        .state
                        .as_mut()
                        .update_comparison(true, self.comparison_position);
                }
                // Can add other conditional effect parameters here
                _ => {}
            }
        }

        let physical_size = viewport.physical_size();
        // Resize textures to match viewport
        let size = wgpu::Extent3d {
            width: physical_size.width,
            height: physical_size.height,
            depth_or_array_layers: 1,
        };
        pipeline_manager
            .texture_manager
            .resize_intermediate_textures(device, size, pipeline_manager.effect_manager.len() + 1);

        // Prepare for rendering
        pipeline_manager.prepare(
            device,
            queue,
            self.video_id,
            &(*bounds
                * iced::Transformation::orthographic(
                    physical_size.width as _,
                    physical_size.height as _,
                )),
            self.color_space,
        );

        pipeline_manager.effects_added = true;

        // Debug prints
        println!("Effects count: {}", pipeline_manager.effect_manager.len());
        println!(
            "Texture manager size: {}",
            pipeline_manager.texture_manager.len()
        );
        println!("Comparison enabled: {}", self.comparison_enabled);
    }

    fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        storage: &iced_wgpu::primitive::Storage,
        target: &wgpu::TextureView,
        clip_bounds: &iced::Rectangle<u32>,
    ) {
        let pipeline_manager = storage.get::<VideoPipelineManager>().unwrap();
        pipeline_manager.draw(target, encoder, clip_bounds, self.video_id);
    }
}
