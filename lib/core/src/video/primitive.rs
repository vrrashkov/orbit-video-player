use ffmpeg_next::color::{self, Space};
use iced_wgpu::primitive::Primitive;
use iced_wgpu::wgpu;
use std::{
    cell::RefCell,
    collections::{btree_map::Entry, BTreeMap},
    num::NonZero,
    rc::Rc,
    sync::atomic::{AtomicUsize, Ordering},
};

use crate::video::pipeline::effects::{
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

        // For testing
        if current_frame >= 3 {
            // 0-indexed, so 1 means 2 frames
            // std::process::exit(0); // Forcefully exit the program
        }
        dbg!("testttttt 1111");
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

        // Check if effects have already been added
        if !pipeline_manager.effects_added {
            // ADD EFFECTS AFTER FRAME
            println!("Creating upscale effect");
            let mut upscale_effect = UpscaleEffect {
                state: UpscaleEffectState {
                    comparison_enabled: self.comparison_enabled,
                    comparison_position: self.comparison_position,
                    color_threshold: 1.,
                    color_blend_mode: 0.5,
                },
                format,
            };

            println!("Adding shader effect");
            let upscale_shader_effect = upscale_effect.add(device, queue);
            println!("Effect created successfully");

            pipeline_manager
                .add_effect(device, queue, upscale_shader_effect)
                .unwrap();
            ///// END EFFECTS
            pipeline_manager.effects_added = true;
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

        // Update and prepare effects
        for effect in &mut pipeline_manager.effect_manager.effects {
            effect
                .state
                .as_mut()
                .update_comparison(self.comparison_enabled, self.comparison_position);
            effect.state.as_mut().prepare(&mut effect.effect, queue);
        }

        // Debug prints
        println!("Effects count: {}", pipeline_manager.has_effects());
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
