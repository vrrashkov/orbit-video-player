use iced_wgpu::wgpu::{self, TextureFormat, TextureView};
use std::{
    collections::{BTreeMap, HashMap},
    ops::Deref,
    sync::{atomic::AtomicUsize, Arc},
};

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
pub struct VideoEntry {
    pub texture_y: wgpu::Texture,
    pub texture_uv: wgpu::Texture,
    pub instances: wgpu::Buffer,
    pub bg0: wgpu::BindGroup,
    pub alive: bool,

    pub prepare_index: AtomicUsize,
    pub render_index: AtomicUsize,
    pub aligned_uniform_size: usize,
}
pub struct VideoPipelineManager {
    state: PipelineState,
    video_pipeline: VideoPipeline,
    pub texture_manager: TextureManager,
    pub effect_manager: EffectManager,
    format: wgpu::TextureFormat,
    videos: BTreeMap<u64, VideoEntry>,
    pub effects_added: bool,
}
pub struct TextureInfo {
    pub views: Vec<wgpu::TextureView>,
    pub format: wgpu::TextureFormat,
    pub size: wgpu::Extent3d,
}
impl VideoPipelineManager {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let state = PipelineState::default();
        let video_pipeline = VideoPipeline::new(device, format);
        let mut texture_manager = TextureManager::new(format);

        let effect_manager = EffectManager::new();
        if !texture_manager.validate_formats() {
            println!("WARNING: Format inconsistency detected");
        }
        // Create initial textures
        let initial_size = wgpu::Extent3d {
            width: 1, // Will be resized on first video
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

    fn resize_intermediate_textures(&mut self, device: &wgpu::Device, size: wgpu::Extent3d) {
        println!("Resizing intermediate textures with size: {:?}", size);
        self.texture_manager.resize_intermediate_textures(
            device,
            size,
            self.effect_manager.len() + 1, // Make sure we have enough textures
        );
    }
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
        println!("Texture dimensions: {:?}", output.size());
        println!("Clip rectangle: {:?}", clip);
        println!("Effect bind group details:");
        println!("  Output texture format: {:?}", output.format());
        println!(
            "  Bind group layout ID: {:?}",
            effect.bind_group_layout.global_id()
        );
        println!("Applying effect with:");
        println!("  Effect name: {}", effect.name);
        println!(
            "  Effect bind group layout ID: {:?}",
            effect.bind_group_layout.global_id()
        );
        println!("  Using bind group with shader: {}", effect.name);
        // Debug the uniforms if they exist
        if let Some(uniforms) = &effect.uniforms {
            uniforms.debug_print_values();
        }

        // Calculate scaling factors based on render target and texture dimensions
        let scale_x = render_target_width / texture_width;
        let scale_y = render_target_height / texture_height;

        // Calculate corrected width and height for stretching the texture
        let clip_width = clip.width as f32;
        let clip_height = clip.height as f32;

        let corrected_width = clip_width * scale_x;
        let corrected_height = clip_height * scale_y;

        // Update the effect pass by passing the adjusted values to the RenderPasses function
        println!(
            "Setting stretched viewport with x: {}, y: {}, texture_width: {}, texture_height: {}",
            clip.x as f32, clip.y as f32, texture_width, texture_height
        );
        println!(
            "Setting stretched viewport with x: {}, y: {}, render_target_width: {}, render_target_height: {}",
            clip.x as f32, clip.y as f32, render_target_width, render_target_height
        );
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

    pub fn cleanup(&mut self) {
        let ids: Vec<_> = self
            .videos
            .iter()
            .filter_map(|(id, entry)| (!entry.alive).then_some(*id))
            .collect();

        for id in ids {
            if let Some(video) = self.videos.remove(&id) {
                video.texture_y.destroy();
                video.texture_uv.destroy();
                video.instances.destroy();
            }
        }
    }

    pub fn get_video(&self, video_id: u64) -> Option<&VideoEntry> {
        self.videos.get(&video_id)
    }

    pub fn has_effects(&self) -> bool {
        !self.effect_manager.is_empty()
    }

    pub fn clear_effects(&mut self) {
        self.effect_manager.clear();
    }

    pub fn update_state(&mut self, new_state: PipelineState) {
        self.state = new_state;
    }

    pub fn get_state(&self) -> &PipelineState {
        &self.state
    }

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
            let size = wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            };

            let needs_resize = if let Some(texture) = self.texture_manager.get_texture(0) {
                let current_size = texture.size();
                current_size.width != width || current_size.height != height
            } else {
                true
            };

            if needs_resize {
                self.resize_intermediate_textures(device, size);
            }
        }

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
    fn prepare_comparison_effect(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        println!("Running prepare_comparison_effect");
        // Find the comparison effect
        for i in 0..self.effect_manager.len() {
            if self.effect_manager.effects[i].effect.name == "comparison" {
                println!("Found comparison effect (#{}) with RGB source", i);
                println!("Preparing comparison effect (#{}) with RGB source", i);

                // Always use RGB texture (output of YUV to RGB) as the original
                let rgb_texture = match self.texture_manager.get_texture(0) {
                    Some(texture) => texture,
                    None => {
                        println!("Error: No RGB texture available for comparison");
                        return;
                    }
                };

                // Get the processed result (last effect's output)
                let processed_index = i - 1;
                let processed_texture = match self.texture_manager.get_texture(processed_index) {
                    Some(texture) => texture,
                    None => {
                        println!("Error: No processed texture at index {}", processed_index);
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
                    println!("Error updating comparison effect: {:?}", e);
                } else {
                    println!("Successfully updated comparison effect");
                }

                break;
            }
        }
    }
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
                    println!("Error updating first effect: {:?}", e);
                }
            }
            if self.effect_manager.len() > 1 {
                println!("Output texture from YUV to RGB:");
                if let Some(rgb_texture) = self.texture_manager.get_texture(0) {
                    println!(
                        "  Format: {:?}, Size: {:?}",
                        rgb_texture.format(),
                        rgb_texture.size()
                    );

                    // Verify this is being passed to the upscale effect
                    println!("Input texture for Upscale effect:");
                    // Check if these match
                }
            }
            if let Some(input_texture) = self.texture_manager.get_texture(0) {
                println!("Upscale input texture details:");
                println!("  Texture size: {:?}", input_texture.size());
                println!("  Texture format: {:?}", input_texture.format());

                // Try to read back a few pixels to check if it has content
                // (This might not be possible with wgpu directly, but worth checking)
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
                        println!("Error updating effect {}: {:?}", i, e);
                    } else {
                        println!("Successfully updated effect {}", i);
                    }
                } else {
                    println!("Failed to get input texture for effect {}", i);
                }
            }
            if self.effect_manager.len() > 1 {
                // Update any comparison effects with the correct textures
                self.prepare_comparison_effect(device, queue);
            }
            // After updating all effects, ensure the next frame uses the correct textures
            self.update_effect_uniforms(queue);
        }
        println!("Textures after preparing YUV to RGB:");
        self.texture_manager.debug_print_state();
    }
    pub fn has_effect(&self, name: &str) -> bool {
        self.effect_manager
            .effects
            .iter()
            .any(|e| e.effect.name == name)
    }

    // Remove effect by name
    pub fn remove_effect(&mut self, name: &str) {
        if self.has_effect(name) {
            println!("Removing effect: {}", name);
            self.effect_manager
                .effects
                .retain(|e| e.effect.name != name);
        }
    }
    fn debug_effect_chain(&self) {
        println!("\nEffect Chain Debug:");
        println!("Total effects: {}", self.effect_manager.len());

        for (i, effect_entry) in self.effect_manager.effects.iter().enumerate() {
            println!("Effect {}: {}", i, effect_entry.effect.name);
            println!("  Format: {:?}", effect_entry.effect.format);

            if let Some(bind_group) = effect_entry.effect.get_bind_group() {
                println!("  Bind group ID: {:?}", bind_group.global_id());
            } else {
                println!("  NO BIND GROUP");
            }

            if let Some(uniforms) = &effect_entry.effect.uniforms {
                println!("  Uniforms:");
                uniforms.debug_print_values();
            }
        }
        println!();
    }
    // Add this helper method to update uniforms for all effects
    fn update_effect_uniforms(&mut self, queue: &wgpu::Queue) {
        for effect_entry in &mut self.effect_manager.effects {
            effect_entry.state.prepare(&mut effect_entry.effect, queue);
        }
    }

    pub fn draw(
        &self,
        target: &wgpu::TextureView,
        encoder: &mut wgpu::CommandEncoder,
        clip: &iced::Rectangle<u32>,
        video_id: u64,
    ) {
        println!("\nStarting draw method for frame {}", video_id);
        self.debug_effect_chain();

        // Add debug logs here
        println!("\nSize Debug Information:");
        println!("UI clip bounds: {}x{}", clip.width, clip.height);

        if let Some(video) = self.videos.get(&video_id) {
            println!(
                "Video source size: {}x{}",
                video.texture_y.size().width,
                video.texture_y.size().height
            );

            // Log all intermediate texture sizes
            for i in 0..self.texture_manager.len() {
                if let Some(texture) = self.texture_manager.get_texture(i) {
                    println!(
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

                println!("clip {:?}", &clip);
                println!("render_target_width {}", &render_target_width);
                println!("render_target_height {}", &render_target_height);
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
                self.video_pipeline.draw(target, encoder, clip, video);
            }
        }
    }

    // Add this helper method to properly process the effect chain
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
            println!("Not enough textures for effect chain");
            return;
        }

        // Create all views upfront
        let mut views = Vec::new();
        for i in 0..self.effect_manager.len() {
            if let Some(view) = self.texture_manager.create_texture_view(i) {
                views.push(view);
            } else {
                println!("Failed to create view {}", i);
                return;
            }
        }

        // For each effect in the chain
        for i in 0..self.effect_manager.len() {
            let effect = &self.effect_manager.effects[i].effect;
            let bind_group = effect.get_bind_group().expect("Bind group should exist");

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

            println!(
                "Effect {} ({}): Reading from texture {}, writing to {}",
                i,
                effect.name,
                input_index,
                if i == self.effect_manager.len() - 1 {
                    "screen".to_string()
                } else {
                    i.to_string()
                }
            );

            // Get the texture for this effect
            let input_texture = self
                .texture_manager
                .get_texture(input_index)
                .expect(&format!("Texture {} should exist", input_index));

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

    pub fn add_effect(
        &mut self,
        update: bool,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        shader_effect: ShaderEffect,
        mut shader_effect_type: Box<dyn Effect + Send + Sync>,
    ) -> anyhow::Result<()> {
        println!("Effect Addition Diagnostics:");

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

        println!("Final state:");
        println!("  Effects count: {}", self.effect_manager.len());
        println!("  Intermediate textures: {}", self.texture_manager.len());

        Ok(())
    }
    fn update_existing_effects(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> anyhow::Result<()> {
        let effect_count = self.effect_manager.effects.len();
        println!("Updating {} existing effects", effect_count);

        // First, we'll store all the Arc<Texture> objects we need to keep alive
        let mut arc_textures = Vec::new();

        for i in 0..effect_count {
            println!(
                "Updating effect #{} ({})",
                i, self.effect_manager.effects[i].effect.name
            );

            // Get input texture and view
            let input_index = if i == 0 { 0 } else { i - 1 };
            println!("  Using input index: {}", input_index);

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
                        println!(
                            "  Found input texture with format: {:?}, size: {:?}",
                            texture.format(),
                            texture.size()
                        );
                        // Store the Arc to keep it alive
                        arc_textures.push(texture.clone());
                        arc_textures.last().unwrap()
                    }
                    None => {
                        println!("  ERROR: No input texture at index {}", input_index);
                        return Err(anyhow::anyhow!("No input texture at index {}", input_index));
                    }
                };

                let input_view = input_texture.create_view(&Default::default());
                println!("  Successfully created input view");

                // Get a reference to the texture inside the Arc
                (vec![input_view], vec![input_texture.as_ref()])
            };

            println!(
                "  Calling update_for_frame for '{}' effect with {} texture views",
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
                Ok(_) => println!("  Successfully updated effect bind group"),
                Err(e) => {
                    println!("  ERROR updating effect bind group: {:?}", e);
                    return Err(e);
                }
            }
        }

        println!("Successfully updated all existing effects");
        Ok(())
    }
    fn initialize_effect(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        shader_effect: &mut ShaderEffect,
        shader_effect_type: &mut dyn Effect,
    ) -> anyhow::Result<()> {
        let effect_count = self.effect_manager.effects.len();
        println!(
            "Initializing new effect (current effect count: {})",
            effect_count
        );
        println!("  Effect name: {}", shader_effect.name);

        // Get source textures and format
        let (input_views, input_format, texture_size) = if effect_count == 0 {
            println!("  First effect - getting textures from video");
            let result = self.get_video_textures()?;
            println!(
                "  Got {} video texture views with format {:?}, size {:?}",
                result.0.len(),
                result.1,
                result.2
            );
            result
        } else {
            println!("  Subsequent effect - getting textures from previous effect");
            let result = self.get_previous_effect_texture(effect_count - 1)?;
            println!(
                "  Got {} previous effect texture views with format {:?}, size {:?}",
                result.0.len(),
                result.1,
                result.2
            );
            result
        };

        // Check if format conversion is needed
        let required_format = shader_effect.get_format().to_owned();
        println!(
            "  Effect requires format: {:?}, input format is: {:?}",
            required_format, input_format
        );

        if input_format != required_format {
            println!(
                "  Format conversion needed from {:?} to {:?}",
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
            println!("  Format conversion effect added successfully");
        } else {
            println!("  No format conversion needed");
        }

        // Resize textures for the new effect
        println!("  Resizing intermediate textures for new effect");
        self.texture_manager.resize_intermediate_textures(
            device,
            texture_size,
            self.effect_manager.len() + 1,
        );

        // Get the input for the current effect (which might be after conversion)
        let input_index = if input_format != required_format {
            // If we added a conversion effect, use its output
            println!("  Using conversion effect output at index {}", effect_count);
            effect_count
        } else {
            // Otherwise use the previous texture
            let idx = effect_count.saturating_sub(1);
            println!("  Using previous texture at index {}", idx);
            idx
        };

        // Update this effect's bind group
        println!("  Getting input texture at index {}", input_index);
        let input_texture = self
            .texture_manager
            .get_texture(input_index)
            .ok_or_else(|| {
                println!("  ERROR: No input texture found at index {}", input_index);
                anyhow::anyhow!("No input texture at index {}", input_index)
            })?;
        println!(
            "  Found input texture with format {:?}, size {:?}",
            input_texture.format(),
            input_texture.size()
        );

        println!("  Getting input texture view at index {}", input_index);
        let input_view = self
            .texture_manager
            .get_texture_view(input_index)
            .ok_or_else(|| {
                println!("  ERROR: No input view found at index {}", input_index);
                anyhow::anyhow!("No input view at index {}", input_index)
            })?;
        println!("  Got input texture view successfully");

        // Create texture lists
        let texture_views = vec![input_view];
        let textures = vec![input_texture.as_ref()];
        println!(
            "  Created texture lists with {} views and {} textures",
            texture_views.len(),
            textures.len()
        );

        // Update the shader effect's bind group
        println!("  Calling update_for_frame on shader effect");
        match shader_effect_type.update_for_frame(device, shader_effect, &texture_views, &textures)
        {
            Ok(_) => println!("  Successfully updated shader effect bind group"),
            Err(e) => {
                println!("  ERROR updating shader effect bind group: {:?}", e);
                return Err(e);
            }
        }

        println!("  Effect initialization completed successfully");
        Ok(())
    }
    fn get_video_textures(
        &self,
    ) -> anyhow::Result<(Vec<TextureView>, TextureFormat, wgpu::Extent3d)> {
        let video = self
            .videos
            .values()
            .next()
            .ok_or_else(|| anyhow::anyhow!("No video available"))?;

        let size = wgpu::Extent3d {
            width: video.texture_y.size().width,
            height: video.texture_y.size().height,
            depth_or_array_layers: 1,
        };

        let format = video.texture_y.format();
        let y_view = video.texture_y.create_view(&Default::default());
        let uv_view = video.texture_uv.create_view(&Default::default());

        Ok((vec![y_view, uv_view], format, size))
    }

    fn get_previous_effect_texture(
        &self,
        index: usize,
    ) -> anyhow::Result<(Vec<TextureView>, TextureFormat, wgpu::Extent3d)> {
        let texture = self
            .texture_manager
            .get_texture(index)
            .ok_or_else(|| anyhow::anyhow!("No texture at index {}", index))?;

        let view = self
            .texture_manager
            .get_texture_view(index)
            .ok_or_else(|| anyhow::anyhow!("No texture view at index {}", index))?;

        let size = wgpu::Extent3d {
            width: texture.size().width,
            height: texture.size().height,
            depth_or_array_layers: 1,
        };

        let format = texture.format();

        Ok((vec![view], format, size))
    }
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
        println!(
            "Adding format conversion from {:?} to {:?}",
            input_format, required_format
        );
        println!("  Received {} input views", input_views.len());
        println!("  Output index: {}", output_index);
        println!("  Texture size: {:?}", texture_size);

        match (input_format, required_format) {
            (wgpu::TextureFormat::R8Unorm, wgpu::TextureFormat::Bgra8UnormSrgb) => {
                println!("  Creating YUV to RGB converter");
                let mut yuv_effect = YuvToRgbEffect::new(0, wgpu::TextureFormat::Bgra8UnormSrgb);
                let mut yuv_shader = yuv_effect.add(device, queue);
                println!(
                    "  Created YUV shader effect with layout ID: {:?}",
                    yuv_shader.bind_group_layout.global_id()
                );

                // Ensure we have enough textures
                println!("  Resizing intermediate textures for conversion");
                self.texture_manager.resize_intermediate_textures(
                    device,
                    texture_size,
                    output_index + 1,
                );

                // Create bind group
                println!("  Getting output texture at index {}", output_index);
                let output_texture =
                    self.texture_manager
                        .get_texture(output_index)
                        .ok_or_else(|| {
                            println!(
                                "  ERROR: Failed to get output texture at index {}",
                                output_index
                            );
                            anyhow::anyhow!("Failed to get output texture")
                        })?;
                println!(
                    "  Found output texture with format {:?}, size {:?}",
                    output_texture.format(),
                    output_texture.size()
                );

                println!("  Creating bind group for YUV to RGB effect");
                println!(
                    "  Using {} input views and 1 output texture",
                    input_views.len()
                );
                let bind_group = match yuv_effect.create_bind_group(
                    device,
                    &yuv_shader,
                    input_views,
                    &vec![output_texture.as_ref()],
                ) {
                    Ok(bg) => {
                        println!("  Successfully created bind group");
                        bg
                    }
                    Err(e) => {
                        println!("  ERROR creating bind group: {:?}", e);
                        return Err(e);
                    }
                };

                println!("  Updating YUV shader bind group");
                yuv_shader.update_bind_group(bind_group);

                // Add to manager
                println!("  Adding YUV to RGB effect to manager");
                self.effect_manager
                    .add_effect(yuv_shader, Box::new(yuv_effect));
                println!("  YUV to RGB conversion effect added successfully");

                Ok(())
            }
            _ => {
                println!("  ERROR: Unsupported format conversion");
                Err(anyhow::anyhow!(
                    "Unsupported format conversion: {:?} to {:?}",
                    input_format,
                    required_format
                ))
            }
        }
    }
}
