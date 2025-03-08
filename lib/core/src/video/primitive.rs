use ffmpeg_next::color::{self, Space};
use iced_wgpu::primitive::Primitive;
use iced_wgpu::wgpu;
use std::{
    collections::HashMap,
    sync::atomic::{AtomicUsize, Ordering},
};
use tracing::{debug, info, trace, warn};

use crate::video::pipeline::effects::{
    comparison::ComparisonEffect,
    upscale::{UpscaleEffect, UpscaleEffectState},
    yuv_to_rgb::YuvToRgbEffect,
    Effect,
};

use super::pipeline::manager::VideoPipelineManager;

/// A primitive for rendering video content in the iced UI framework
///
/// This primitive handles video frame display, shader effects processing,
/// and comparison mode for visual effect evaluation.
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
    shader_selections: HashMap<String, bool>,
}

impl VideoPrimitive {
    /// Create a new video primitive
    pub fn new(
        video_id: u64,
        alive: bool,
        frame: Vec<u8>,
        size: (u32, u32),
        upload_frame: bool,
        color_space: Space,
    ) -> Self {
        let shader_selections = HashMap::new();
        VideoPrimitive {
            video_id,
            alive,
            frame,
            size,
            upload_frame,
            color_space,
            comparison_enabled: false,
            comparison_position: 0.5,
            shader_selections,
        }
    }

    /// Set which shader effects should be active
    pub fn with_shader_selections(mut self, selections: HashMap<String, bool>) -> Self {
        self.shader_selections = selections;
        self
    }

    /// Enable or disable comparison mode
    pub fn with_comparison(mut self, enabled: bool) -> Self {
        self.comparison_enabled = enabled;
        self
    }

    /// Set the position of the comparison slider (0.0-1.0)
    pub fn with_comparison_position(mut self, position: f32) -> Self {
        self.comparison_position = position.clamp(0.0, 1.0);
        self
    }
}

// Global counter to track prepare calls for debugging
static FRAME_COUNT: AtomicUsize = AtomicUsize::new(0);

impl Primitive for VideoPrimitive {
    /// Prepare the video for rendering
    ///
    /// This method:
    /// 1. Initializes the video pipeline if needed
    /// 2. Uploads new frame data if available
    /// 3. Manages active shader effects based on current selections
    /// 4. Updates effect parameters
    /// 5. Resizes intermediate textures to match the viewport
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
        trace!(
            "Preparing video frame {}: id={}",
            current_frame,
            self.video_id
        );

        // Create pipeline manager if it doesn't exist yet
        let has_manager = storage.has::<VideoPipelineManager>();
        trace!("Pipeline manager exists: {}", has_manager);

        if !has_manager {
            debug!("Creating new video pipeline manager");
            let pipeline_manager = VideoPipelineManager::new(device, format);
            storage.store(pipeline_manager);
        }

        let pipeline_manager = storage.get_mut::<VideoPipelineManager>().unwrap();

        // Upload new frame data if requested
        if self.upload_frame {
            debug!(
                "Uploading frame: id={}, size={}x{}, data_len={}",
                self.video_id,
                self.size.0,
                self.size.1,
                self.frame.len()
            );

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

        // Add upscale effect if selected and not already active
        if *self.shader_selections.get("upscale").unwrap_or(&false)
            && !pipeline_manager.has_effect("upscale")
        {
            debug!("Adding upscale effect (selected but not yet active)");
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

        // Add comparison effect if needed
        if self.comparison_enabled && !pipeline_manager.has_effect("comparison") {
            debug!("Adding comparison effect (enabled but not yet active)");
            desired_effects.push((
                "comparison",
                Box::new(ComparisonEffect {
                    line_position: self.comparison_position,
                    format,
                }) as Box<dyn Effect + Send + Sync>,
            ));
        }

        // Add all desired effects to the pipeline
        for (name, mut effect) in desired_effects {
            debug!("Initializing effect: {}", name);
            let shader_effect = effect.add(device, queue);

            if let Err(e) = pipeline_manager.add_effect(false, device, queue, shader_effect, effect)
            {
                warn!("Failed to add effect {}: {}", name, e);
            }
        }

        // Remove effects that should no longer be active
        if !self.comparison_enabled && pipeline_manager.has_effect("comparison") {
            debug!("Removing comparison effect (no longer enabled)");
            pipeline_manager.remove_effect("comparison");
        }

        // Handle shader toggles
        for (name, enabled) in &self.shader_selections {
            // If the shader is disabled in selections but exists in the pipeline, remove it
            if !enabled && pipeline_manager.has_effect(name) {
                debug!("Removing effect: {} (disabled in shader selections)", name);
                pipeline_manager.remove_effect(name);
            }
        }

        // Update parameters for active effects
        for effect in &mut pipeline_manager.effect_manager.effects {
            match effect.effect.name.as_str() {
                "comparison" => {
                    trace!(
                        "Updating comparison effect: position={}",
                        self.comparison_position
                    );
                    effect
                        .state
                        .as_mut()
                        .update_comparison(true, self.comparison_position);
                }
                // Can add other effect parameter updates here
                _ => {}
            }
        }

        // Resize intermediate textures to match viewport size
        let physical_size = viewport.physical_size();
        let size = wgpu::Extent3d {
            width: physical_size.width,
            height: physical_size.height,
            depth_or_array_layers: 1,
        };

        trace!(
            "Resizing intermediate textures: {}x{}, count={}",
            size.width,
            size.height,
            pipeline_manager.effect_manager.len() + 1
        );

        pipeline_manager
            .texture_manager
            .resize_intermediate_textures(device, size, pipeline_manager.effect_manager.len() + 1);

        // Prepare the pipeline for rendering with current parameters
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

        trace!(
            "Video prepared: effects={}, textures={}, comparison={}",
            pipeline_manager.effect_manager.len(),
            pipeline_manager.texture_manager.len(),
            self.comparison_enabled
        );
    }

    /// Render the video to the target texture
    ///
    /// This method delegates to the pipeline manager to perform the actual rendering
    fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        storage: &iced_wgpu::primitive::Storage,
        target: &wgpu::TextureView,
        clip_bounds: &iced::Rectangle<u32>,
    ) {
        trace!("Rendering video {}: clip={:?}", self.video_id, clip_bounds);

        if let Some(pipeline_manager) = storage.get::<VideoPipelineManager>() {
            pipeline_manager.draw(target, encoder, clip_bounds, self.video_id);
        } else {
            warn!("Attempted to render without pipeline manager");
        }
    }
}
