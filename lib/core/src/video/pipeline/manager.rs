use iced_wgpu::wgpu::{self, TextureFormat, TextureView};
use std::{
    collections::{BTreeMap, HashMap},
    ops::Deref,
    sync::{atomic::AtomicUsize, Arc},
};
use tracing::{debug, error, info, trace, warn};

use crate::video::{
    pipeline::effects::yuv_to_rgb::YuvToRgbEffect,
    render_passes::RenderPasses,
    shader::{ShaderEffectBuilder, ShaderUniforms, UniformValue},
    texture_manager::TextureManager,
    ShaderEffect,
};

use super::{
    effects::{Effect, EffectManager},
    state::PipelineState,
    video::VideoPipeline,
};

/// Represents a single video entry with associated GPU resources
pub struct VideoEntry {
    pub texture_y: wgpu::Texture,  // Y plane texture
    pub texture_uv: wgpu::Texture, // UV plane texture (chroma)
    pub instances: wgpu::Buffer,   // Uniform buffer for rendering
    pub bg0: wgpu::BindGroup,      // Bind group connecting textures and uniforms
    pub alive: bool,               // Whether this video is still active

    pub prepare_index: AtomicUsize,  // Current prepare buffer index
    pub render_index: AtomicUsize,   // Current render buffer index
    pub aligned_uniform_size: usize, // Size of each uniform entry, aligned to GPU requirements
}

/// Main manager for video pipelines and effect chains
pub struct VideoPipelineManager {
    state: PipelineState,
    video_pipeline: VideoPipeline,
    pub texture_manager: TextureManager,
    pub effect_manager: EffectManager,
    format: wgpu::TextureFormat,
    videos: BTreeMap<u64, VideoEntry>,
    pub effects_added: bool,
}

/// Contains information about a texture for effect processing
pub struct TextureInfo {
    pub views: Vec<wgpu::TextureView>,
    pub format: wgpu::TextureFormat,
    pub size: wgpu::Extent3d,
}

impl VideoPipelineManager {
    /// Create a new video pipeline manager with the specified texture format
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let state = PipelineState::default();
        let video_pipeline = VideoPipeline::new(device, format);
        let mut texture_manager = TextureManager::new(format);
        let effect_manager = EffectManager::new();

        if !texture_manager.validate_formats() {
            warn!("Format inconsistency detected in texture manager");
        }

        // Create initial textures with minimal size (will be resized later)
        let initial_size = wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        };
        texture_manager.resize_intermediate_textures(device, initial_size, 1);

        Self {
            state,
            video_pipeline,
            texture_manager,
            effect_manager,
            format,
            videos: BTreeMap::new(),
            effects_added: false,
        }
    }

    /// Resize intermediate textures based on video dimensions
    pub fn resize_for_effects(&mut self, device: &wgpu::Device) {
        if let Some(video) = self.videos.values().next() {
            let extent = video.texture_y.size();
            let size = wgpu::Extent3d {
                width: extent.width,
                height: extent.height,
                depth_or_array_layers: 1,
            };
            self.resize_intermediate_textures(device, size);
        }
    }

    /// Resize intermediate textures to the specified size
    fn resize_intermediate_textures(&mut self, device: &wgpu::Device, size: wgpu::Extent3d) {
        debug!(
            "Resizing intermediate textures to {}x{}",
            size.width, size.height
        );

        self.texture_manager.resize_intermediate_textures(
            device,
            size,
            self.effect_manager.len() + 1, // Ensure we have enough textures
        );
    }

    /// Apply a shader effect to an input texture and write to output texture
    fn apply_effect(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        effect: &ShaderEffect,
        bind_group: &wgpu::BindGroup,
        output_view: &wgpu::TextureView,
        output: &wgpu::Texture,
        clip: &iced::Rectangle<u32>,
        clear: bool,
        render_target_width: f32,
        render_target_height: f32,
        texture_width: f32,
        texture_height: f32,
    ) {
        trace!(
            "Applying effect '{}': output_format={:?}, dims={}x{}",
            effect.name,
            output.format(),
            output.size().width,
            output.size().height
        );

        trace!(
            "Effect bind group layout ID: {:?}",
            effect.bind_group_layout.global_id()
        );

        // Log uniform values at trace level
        if let Some(uniforms) = &effect.uniforms {
            uniforms.debug_print_values();
        }

        // Calculate scaling factors for stretching texture
        let scale_x = render_target_width / texture_width;
        let scale_y = render_target_height / texture_height;
        let clip_width = clip.width as f32;
        let clip_height = clip.height as f32;
        let corrected_width = clip_width * scale_x;
        let corrected_height = clip_height * scale_y;

        trace!(
            "Viewport setup: target={}x{}, texture={}x{}, clear={}",
            render_target_width,
            render_target_height,
            texture_width,
            texture_height,
            clear
        );

        // Apply the effect using the render passes utility
        RenderPasses::apply_effect(
            effect,
            encoder,
            bind_group,
            output_view,
            clip,
            clear,
            render_target_width,
            render_target_height,
            texture_width,
            texture_height,
        );
    }

    /// Remove inactive videos from memory
    pub fn cleanup(&mut self) {
        let ids: Vec<_> = self
            .videos
            .iter()
            .filter_map(|(id, entry)| (!entry.alive).then_some(*id))
            .collect();

        if !ids.is_empty() {
            debug!("Cleaning up {} inactive videos", ids.len());

            for id in ids {
                if let Some(video) = self.videos.remove(&id) {
                    trace!("Destroying resources for video {}", id);
                    video.texture_y.destroy();
                    video.texture_uv.destroy();
                    video.instances.destroy();
                }
            }
        }
    }

    /// Get a video entry by ID
    pub fn get_video(&self, video_id: u64) -> Option<&VideoEntry> {
        self.videos.get(&video_id)
    }

    /// Check if there are any active effects
    pub fn has_effects(&self) -> bool {
        !self.effect_manager.is_empty()
    }

    /// Remove all effects from the pipeline
    pub fn clear_effects(&mut self) {
        debug!("Clearing all effects from pipeline");
        self.effect_manager.clear();
    }

    /// Update the pipeline state
    pub fn update_state(&mut self, new_state: PipelineState) {
        self.state = new_state;
    }

    /// Get the current pipeline state
    pub fn get_state(&self) -> &PipelineState {
        &self.state
    }

    /// Upload a new video frame to GPU textures
    pub fn upload_frame(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        video_id: u64,
        width: u32,
        height: u32,
        frame_data: &[u8],
        alive: bool,
    ) {
        let is_new_video = !self.videos.contains_key(&video_id);

        if is_new_video {
            debug!(
                "Creating new video entry: id={}, size={}x{}",
                video_id, width, height
            );

            let size = wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            };

            // Check if we need to resize intermediate textures
            let needs_resize = if let Some(texture) = self.texture_manager.get_texture(0) {
                let current_size = texture.size();
                current_size.width != width || current_size.height != height
            } else {
                true
            };

            if needs_resize {
                debug!("Resizing intermediate textures for new video size");
                self.resize_intermediate_textures(device, size);
            }
        }

        // Upload frame data to GPU
        self.video_pipeline.upload(
            device,
            queue,
            video_id,
            alive,
            (width, height),
            frame_data,
            &mut self.videos,
        );
    }

    /// Prepare the comparison effect with original and processed textures
    fn prepare_comparison_effect(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        trace!("Preparing comparison effect");

        // Find the comparison effect
        for i in 0..self.effect_manager.len() {
            if self.effect_manager.effects[i].effect.name == "comparison" {
                debug!("Found comparison effect at position {}", i);

                // Use RGB texture (output of YUV to RGB) as the original
                let rgb_texture = match self.texture_manager.get_texture(0) {
                    Some(texture) => texture,
                    None => {
                        warn!("No RGB texture available for comparison");
                        return;
                    }
                };

                // Get the processed result (last effect's output)
                let processed_index = i - 1;
                let processed_texture = match self.texture_manager.get_texture(processed_index) {
                    Some(texture) => texture,
                    None => {
                        warn!(
                            "No processed texture available at index {}",
                            processed_index
                        );
                        return;
                    }
                };

                // Create views for both textures
                let rgb_view = rgb_texture.create_view(&Default::default());
                let processed_view = processed_texture.create_view(&Default::default());

                // Create vectors for the comparison effect
                let views = vec![rgb_view, processed_view];
                let textures = vec![rgb_texture.as_ref(), processed_texture.as_ref()];

                // Update the comparison effect with both textures
                let effect_entry = &mut self.effect_manager.effects[i];
                if let Err(e) = effect_entry.state.update_for_frame(
                    device,
                    &mut effect_entry.effect,
                    &views,
                    &textures,
                ) {
                    error!("Failed to update comparison effect: {}", e);
                } else {
                    trace!("Successfully updated comparison effect");
                }

                break;
            }
        }
    }

    /// Prepare the pipeline for rendering a frame
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        video_id: u64,
        bounds: &iced::Rectangle,
        color_space: ffmpeg_next::color::Space,
    ) {
        // Update video pipeline state
        self.video_pipeline.prepare(
            device,
            queue,
            video_id,
            bounds,
            color_space,
            &mut self.videos,
            &self.state,
        );

        if let Some(video) = self.videos.get(&video_id) {
            // Get video textures for the first effect (YUV to RGB)
            let y_view = video.texture_y.create_view(&Default::default());
            let uv_view = video.texture_uv.create_view(&Default::default());

            let textures = vec![&video.texture_y, &video.texture_uv];
            let views = vec![y_view, uv_view];

            // Update the first effect (YUV to RGB) with video textures
            if self.effect_manager.len() > 0 {
                let effect_entry = &mut self.effect_manager.effects[0];
                if let Err(e) = effect_entry.state.update_for_frame(
                    device,
                    &mut effect_entry.effect,
                    &views,
                    &textures,
                ) {
                    error!("Failed to update first effect: {}", e);
                }
            }

            // Log texture details for debugging
            if self.effect_manager.len() > 1 {
                if let Some(rgb_texture) = self.texture_manager.get_texture(0) {
                    trace!(
                        "YUV to RGB output: format={:?}, size={}x{}",
                        rgb_texture.format(),
                        rgb_texture.size().width,
                        rgb_texture.size().height
                    );
                }
            }

            // For subsequent effects, use the output from previous effect
            for i in 1..self.effect_manager.len() {
                // Get the output texture from the previous effect
                let prev_output_index = i - 1;

                // Get the texture from the TextureManager
                if let Some(input_texture) = self.texture_manager.get_texture(prev_output_index) {
                    // Create a view for this texture
                    let input_view = input_texture.create_view(&Default::default());

                    // Create vectors for the current effect
                    let current_views = vec![input_view];
                    let current_textures = vec![input_texture.as_ref()];

                    // Update this effect with the output from the previous effect
                    let effect_entry = &mut self.effect_manager.effects[i];
                    if let Err(e) = effect_entry.state.update_for_frame(
                        device,
                        &mut effect_entry.effect,
                        &current_views,
                        &current_textures,
                    ) {
                        error!("Failed to update effect {}: {}", i, e);
                    } else {
                        trace!("Updated effect {} successfully", i);
                    }
                } else {
                    warn!("No input texture available for effect {}", i);
                }
            }

            // Handle special case for comparison effect
            if self.effect_manager.len() > 1 {
                self.prepare_comparison_effect(device, queue);
            }

            // Update uniform values for all effects
            self.update_effect_uniforms(queue);
        }

        // Log texture state at trace level
        trace!("Texture state after preparing YUV to RGB:");
        self.texture_manager.debug_print_state();
    }

    /// Check if an effect with the given name exists
    pub fn has_effect(&self, name: &str) -> bool {
        self.effect_manager
            .effects
            .iter()
            .any(|e| e.effect.name == name)
    }

    /// Remove an effect by name
    pub fn remove_effect(&mut self, name: &str) {
        if self.has_effect(name) {
            debug!("Removing effect: {}", name);
            self.effect_manager
                .effects
                .retain(|e| e.effect.name != name);
        }
    }

    /// Log the current effect chain for debugging
    fn debug_effect_chain(&self) {
        debug!("Effect Chain: {} total effects", self.effect_manager.len());

        for (i, effect_entry) in self.effect_manager.effects.iter().enumerate() {
            debug!("Effect {}: {}", i, effect_entry.effect.name);
            debug!("  Format: {:?}", effect_entry.effect.format);

            if let Some(bind_group) = effect_entry.effect.get_bind_group() {
                debug!("  Bind group ID: {:?}", bind_group.global_id());
            } else {
                debug!("  NO BIND GROUP");
            }

            if let Some(uniforms) = &effect_entry.effect.uniforms {
                trace!("  Uniforms:");
                uniforms.debug_print_values();
            }
        }
    }

    /// Update uniforms for all effects
    fn update_effect_uniforms(&mut self, queue: &wgpu::Queue) {
        for effect_entry in &mut self.effect_manager.effects {
            effect_entry.state.prepare(&mut effect_entry.effect, queue);
        }
    }

    /// Draw the current frame to the target
    pub fn draw(
        &self,
        target: &wgpu::TextureView,
        encoder: &mut wgpu::CommandEncoder,
        clip: &iced::Rectangle<u32>,
        video_id: u64,
    ) {
        trace!("Drawing video frame for id={}", video_id);

        // Log effect chain and texture details at debug level
        self.debug_effect_chain();

        debug!("UI clip bounds: {}x{}", clip.width, clip.height);

        if let Some(video) = self.videos.get(&video_id) {
            debug!(
                "Video source size: {}x{}",
                video.texture_y.size().width,
                video.texture_y.size().height
            );

            // Log all intermediate texture sizes
            for i in 0..self.texture_manager.len() {
                if let Some(texture) = self.texture_manager.get_texture(i) {
                    debug!(
                        "Intermediate texture {}: {}x{}",
                        i,
                        texture.size().width,
                        texture.size().height
                    );
                }
            }
        }

        if let Some(video) = self.videos.get(&video_id) {
            // For each effect in the chain, ensure the video textures are properly bound
            if !self.effect_manager.is_empty() {
                // Get input texture dimensions
                let texture_width = video.texture_y.size().width as f32;
                let texture_height = video.texture_y.size().height as f32;

                // Get target dimensions
                let render_target_width = clip.width as f32;
                let render_target_height = clip.height as f32;

                trace!(
                    "Render dimensions: target={}x{}, texture={}x{}, clip={:?}",
                    render_target_width,
                    render_target_height,
                    texture_width,
                    texture_height,
                    clip
                );

                // Process the effect chain with the current frame's textures
                self.process_effect_chain(
                    encoder,
                    target,
                    clip,
                    video,
                    render_target_width,
                    render_target_height,
                    texture_width,
                    texture_height,
                );
            } else {
                // Fallback to basic video rendering if no effects
                trace!("No effects active, using basic video rendering");
                self.video_pipeline.draw(target, encoder, clip, video);
            }
        }
    }

    /// Process the entire effect chain for rendering
    fn process_effect_chain(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        clip: &iced::Rectangle<u32>,
        video: &VideoEntry,
        render_target_width: f32,
        render_target_height: f32,
        texture_width: f32,
        texture_height: f32,
    ) {
        // Ensure we have enough textures
        if self.texture_manager.len() <= self.effect_manager.len() {
            warn!(
                "Not enough textures for effect chain ({} textures, {} effects)",
                self.texture_manager.len(),
                self.effect_manager.len()
            );
            return;
        }

        // Create all views upfront
        let mut views = Vec::new();
        for i in 0..self.effect_manager.len() {
            if let Some(view) = self.texture_manager.create_texture_view(i) {
                views.push(view);
            } else {
                error!("Failed to create view for texture {}", i);
                return;
            }
        }

        // For each effect in the chain
        for i in 0..self.effect_manager.len() {
            let effect = &self.effect_manager.effects[i].effect;
            let bind_group = match effect.get_bind_group() {
                Some(bg) => bg,
                None => {
                    error!("Missing bind group for effect {} ({})", i, effect.name);
                    return;
                }
            };

            // Calculate input and output texture indices
            let input_index = if i == 0 {
                // First effect doesn't use intermediate textures as input
                // (Input comes directly from video textures in bind_group)
                0
            } else {
                // Other effects read from the output of the previous effect
                i - 1
            };

            // Last effect writes directly to screen, others to their intermediate texture
            let output_view = if i == self.effect_manager.len() - 1 {
                target
            } else {
                &views[i] // Effect i writes to texture i
            };

            debug!(
                "Effect {} ({}): Reading from texture {}, writing to {}",
                i,
                effect.name,
                input_index,
                if i == self.effect_manager.len() - 1 {
                    "screen".to_string()
                } else {
                    format!("texture {}", i)
                }
            );

            // Get the texture for this effect
            let input_texture = match self.texture_manager.get_texture(input_index) {
                Some(texture) => texture,
                None => {
                    error!("Missing input texture {} for effect {}", input_index, i);
                    return;
                }
            };

            // When rendering to an intermediate texture
            if i < self.effect_manager.len() - 1 {
                // Use the intermediate texture dimensions
                let intermediate_width = input_texture.size().width as f32;
                let intermediate_height = input_texture.size().height as f32;

                self.apply_effect(
                    encoder,
                    effect,
                    bind_group,
                    output_view,
                    input_texture.as_ref(),
                    clip,
                    true, // clear = true for intermediate
                    intermediate_width,
                    intermediate_height,
                    intermediate_width,
                    intermediate_height,
                );
            } else {
                // For the final render to the UI, use the clip dimensions
                self.apply_effect(
                    encoder,
                    effect,
                    bind_group,
                    output_view,
                    input_texture.as_ref(),
                    clip,
                    false, // clear = false for final
                    clip.width as f32,
                    clip.height as f32,
                    input_texture.size().width as f32,
                    input_texture.size().height as f32,
                );
            }
        }
    }

    /// Add a new effect to the pipeline or update existing effects
    pub fn add_effect(
        &mut self,
        update: bool,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        shader_effect: ShaderEffect,
        mut shader_effect_type: Box<dyn Effect + Send + Sync>,
    ) -> anyhow::Result<()> {
        debug!("Adding/updating effect: {}", shader_effect.name);

        if update {
            // Only update existing effects, don't add new ones
            self.update_existing_effects(device, queue)?;
        } else {
            // Add new effect
            let mut shader_effect_mut = shader_effect;
            self.initialize_effect(
                device,
                queue,
                &mut shader_effect_mut,
                &mut *shader_effect_type,
            )?;

            self.effect_manager
                .add_effect(shader_effect_mut, shader_effect_type);
        }

        debug!(
            "Current state: {} effects, {} textures",
            self.effect_manager.len(),
            self.texture_manager.len()
        );

        Ok(())
    }

    /// Update all existing effects with current textures
    fn update_existing_effects(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> anyhow::Result<()> {
        let effect_count = self.effect_manager.effects.len();
        debug!("Updating {} existing effects", effect_count);

        // First, we'll store all the Arc<Texture> objects we need to keep alive
        let mut arc_textures = Vec::new();

        for i in 0..effect_count {
            debug!(
                "Updating effect #{} ({})",
                i, self.effect_manager.effects[i].effect.name
            );

            // Get input texture and view
            let input_index = if i == 0 { 0 } else { i - 1 };
            trace!("Using input index: {}", input_index);

            // For the YUV to RGB effect (first effect), we need to handle video textures
            let (texture_views, textures) = if i == 0 && !self.videos.is_empty() {
                // Get video textures directly
                let video = self.videos.values().next().unwrap();

                let y_view = video.texture_y.create_view(&Default::default());
                let uv_view = video.texture_uv.create_view(&Default::default());

                // References to the actual textures
                (
                    vec![y_view, uv_view],
                    vec![&video.texture_y, &video.texture_uv],
                )
            } else {
                // Handle intermediate textures (stored as Arc)
                let input_texture = match self.texture_manager.get_texture(input_index) {
                    Some(texture) => {
                        trace!(
                            "Found input texture: format={:?}, size={}x{}",
                            texture.format(),
                            texture.size().width,
                            texture.size().height
                        );
                        // Store the Arc to keep it alive
                        arc_textures.push(texture.clone());
                        arc_textures.last().unwrap()
                    }
                    None => {
                        error!("No input texture available at index {}", input_index);
                        return Err(anyhow::anyhow!("No input texture at index {}", input_index));
                    }
                };

                let input_view = input_texture.create_view(&Default::default());
                trace!("Created input view successfully");

                // Get a reference to the texture inside the Arc
                (vec![input_view], vec![input_texture.as_ref()])
            };

            trace!(
                "Updating effect '{}' with {} texture views",
                self.effect_manager.effects[i].effect.name,
                texture_views.len()
            );

            let effect_entry = &mut self.effect_manager.effects[i];
            match effect_entry.state.update_for_frame(
                device,
                &mut effect_entry.effect,
                &texture_views,
                &textures,
            ) {
                Ok(_) => trace!("Updated effect bind group successfully"),
                Err(e) => {
                    error!("Failed to update effect bind group: {}", e);
                    return Err(e);
                }
            }
        }

        debug!("Successfully updated all existing effects");
        Ok(())
    }
    /// Initialize a new effect with appropriate textures and bind groups
    fn initialize_effect(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        shader_effect: &mut ShaderEffect,
        shader_effect_type: &mut dyn Effect,
    ) -> anyhow::Result<()> {
        let effect_count = self.effect_manager.effects.len();
        debug!(
            "Initializing new effect '{}' (current count: {})",
            shader_effect.name, effect_count
        );

        // Get source textures and format based on position in the effect chain
        let (input_views, input_format, texture_size) = if effect_count == 0 {
            // First effect uses video textures as input
            debug!("First effect - using video textures as input");
            let result = self.get_video_textures()?;
            trace!(
                "Got {} video texture views: format={:?}, size={}x{}",
                result.0.len(),
                result.1,
                result.2.width,
                result.2.height
            );
            result
        } else {
            // Subsequent effects use the output from previous effect
            debug!("Subsequent effect - using previous effect output as input");
            let result = self.get_previous_effect_texture(effect_count - 1)?;
            trace!(
                "Got {} texture views from previous effect: format={:?}, size={}x{}",
                result.0.len(),
                result.1,
                result.2.width,
                result.2.height
            );
            result
        };

        // Check if format conversion is needed between effects
        let required_format = shader_effect.get_format().to_owned();
        debug!(
            "Effect requires format: {:?}, input format is: {:?}",
            required_format, input_format
        );

        if input_format != required_format {
            debug!(
                "Format conversion needed: {:?} -> {:?}",
                input_format, required_format
            );

            // Add format conversion effect if needed
            self.add_format_conversion(
                device,
                queue,
                &input_views,
                input_format,
                required_format,
                texture_size,
                effect_count,
            )?;
            debug!("Format conversion effect added successfully");
        } else {
            debug!("No format conversion needed");
        }

        // Resize textures for the new effect
        debug!("Resizing intermediate textures for new effect");
        self.texture_manager.resize_intermediate_textures(
            device,
            texture_size,
            self.effect_manager.len() + 1,
        );

        // Get the input for the current effect (which might be after conversion)
        let input_index = if input_format != required_format {
            // If we added a conversion effect, use its output
            debug!("Using conversion effect output at index {}", effect_count);
            effect_count
        } else {
            // Otherwise use the previous texture
            let idx = effect_count.saturating_sub(1);
            debug!("Using previous texture at index {}", idx);
            idx
        };

        // Get the input texture for this effect
        trace!("Getting input texture at index {}", input_index);
        let input_texture = self
            .texture_manager
            .get_texture(input_index)
            .ok_or_else(|| {
                error!("No input texture found at index {}", input_index);
                anyhow::anyhow!("No input texture at index {}", input_index)
            })?;

        trace!(
            "Using input texture: format={:?}, size={}x{}",
            input_texture.format(),
            input_texture.size().width,
            input_texture.size().height
        );

        // Get the corresponding texture view
        trace!("Getting input texture view");
        let input_view = self
            .texture_manager
            .get_texture_view(input_index)
            .ok_or_else(|| {
                error!("No input view found at index {}", input_index);
                anyhow::anyhow!("No input view at index {}", input_index)
            })?;
        trace!("Input texture view created successfully");

        // Create texture lists for the effect
        let texture_views = vec![input_view];
        let textures = vec![input_texture.as_ref()];
        trace!(
            "Created texture lists with {} views and {} textures",
            texture_views.len(),
            textures.len()
        );

        // Update the shader effect's bind group with these textures
        debug!("Updating effect bind group with textures");
        match shader_effect_type.update_for_frame(device, shader_effect, &texture_views, &textures)
        {
            Ok(_) => debug!("Successfully updated shader effect bind group"),
            Err(e) => {
                error!("Failed to update shader effect bind group: {}", e);
                return Err(e);
            }
        }

        debug!("Effect initialization completed successfully");
        Ok(())
    }

    /// Get textures from the current video for use in effects
    fn get_video_textures(
        &self,
    ) -> anyhow::Result<(Vec<TextureView>, TextureFormat, wgpu::Extent3d)> {
        // Get the first available video
        let video = self
            .videos
            .values()
            .next()
            .ok_or_else(|| anyhow::anyhow!("No video available"))?;

        // Extract size from Y plane texture
        let size = wgpu::Extent3d {
            width: video.texture_y.size().width,
            height: video.texture_y.size().height,
            depth_or_array_layers: 1,
        };

        // Get format and create views
        let format = video.texture_y.format();
        let y_view = video.texture_y.create_view(&Default::default());
        let uv_view = video.texture_uv.create_view(&Default::default());

        trace!(
            "Retrieved video textures: format={:?}, size={}x{}",
            format,
            size.width,
            size.height
        );

        Ok((vec![y_view, uv_view], format, size))
    }

    /// Get texture from a previous effect's output
    fn get_previous_effect_texture(
        &self,
        index: usize,
    ) -> anyhow::Result<(Vec<TextureView>, TextureFormat, wgpu::Extent3d)> {
        // Get the texture from the manager
        let texture = self
            .texture_manager
            .get_texture(index)
            .ok_or_else(|| anyhow::anyhow!("No texture at index {}", index))?;

        // Get the corresponding view
        let view = self
            .texture_manager
            .get_texture_view(index)
            .ok_or_else(|| anyhow::anyhow!("No texture view at index {}", index))?;

        // Extract size and format
        let size = wgpu::Extent3d {
            width: texture.size().width,
            height: texture.size().height,
            depth_or_array_layers: 1,
        };

        let format = texture.format();

        trace!(
            "Retrieved previous effect texture at index {}: format={:?}, size={}x{}",
            index,
            format,
            size.width,
            size.height
        );

        Ok((vec![view], format, size))
    }

    /// Add a format conversion effect between incompatible effect formats
    fn add_format_conversion(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        input_views: &[TextureView],
        input_format: TextureFormat,
        required_format: TextureFormat,
        texture_size: wgpu::Extent3d,
        output_index: usize,
    ) -> anyhow::Result<()> {
        info!(
            "Adding format conversion: {:?} → {:?}",
            input_format, required_format
        );
        trace!(
            "Format conversion details: input_views={}, output_index={}, size={}x{}",
            input_views.len(),
            output_index,
            texture_size.width,
            texture_size.height
        );

        // Handle supported format conversions
        match (input_format, required_format) {
            // YUV to RGB conversion (common for video processing)
            (wgpu::TextureFormat::R8Unorm, wgpu::TextureFormat::Bgra8UnormSrgb) => {
                debug!("Creating YUV to RGB converter");

                // Create the YUV to RGB effect
                let mut yuv_effect = YuvToRgbEffect::new(0, wgpu::TextureFormat::Bgra8UnormSrgb);
                let mut yuv_shader = yuv_effect.add(device, queue);
                debug!(
                    "Created YUV shader effect with bind group layout ID: {:?}",
                    yuv_shader.bind_group_layout.global_id()
                );

                // Ensure we have enough textures for this conversion
                debug!("Preparing intermediate textures for format conversion");
                self.texture_manager.resize_intermediate_textures(
                    device,
                    texture_size,
                    output_index + 1,
                );

                // Get the output texture for the conversion effect
                debug!("Getting output texture at index {}", output_index);
                let output_texture =
                    self.texture_manager
                        .get_texture(output_index)
                        .ok_or_else(|| {
                            error!("Failed to get output texture at index {}", output_index);
                            anyhow::anyhow!("Failed to get output texture")
                        })?;

                trace!(
                    "Output texture: format={:?}, size={}x{}",
                    output_texture.format(),
                    output_texture.size().width,
                    output_texture.size().height
                );

                // Create bind group connecting input and output textures
                debug!("Creating bind group for YUV to RGB effect");
                let bind_group = match yuv_effect.create_bind_group(
                    device,
                    &yuv_shader,
                    input_views,
                    &vec![output_texture.as_ref()],
                ) {
                    Ok(bg) => {
                        debug!("Successfully created bind group");
                        bg
                    }
                    Err(e) => {
                        error!("Failed to create bind group: {}", e);
                        return Err(e);
                    }
                };

                // Update the effect with the bind group
                yuv_shader.update_bind_group(bind_group);

                // Add the conversion effect to the manager
                debug!("Adding YUV to RGB effect to pipeline");
                self.effect_manager
                    .add_effect(yuv_shader, Box::new(yuv_effect));

                info!("YUV to RGB conversion effect added successfully");
                Ok(())
            }
            // Add other format conversions here as needed
            _ => {
                error!(
                    "Unsupported format conversion: {:?} to {:?}",
                    input_format, required_format
                );
                Err(anyhow::anyhow!(
                    "Unsupported format conversion: {:?} to {:?}",
                    input_format,
                    required_format
                ))
            }
        }
    }
}
