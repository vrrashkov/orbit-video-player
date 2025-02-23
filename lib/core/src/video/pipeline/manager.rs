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
    line::LinePipeline,
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
    line_pipeline: LinePipeline,
    pub texture_manager: TextureManager,
    pub effect_manager: EffectManager,
    format: wgpu::TextureFormat,
    videos: BTreeMap<u64, VideoEntry>,
    pub effects_added: bool,
}

impl VideoPipelineManager {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let state = PipelineState::default();
        let video_pipeline = VideoPipeline::new(device, format);
        let line_pipeline = LinePipeline::new(device, format);
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
            line_pipeline,
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
        render_target_width: f32,  // New variable
        render_target_height: f32, // New variable
        texture_width: f32,        // New variable
        texture_height: f32,       // New variable
    ) {
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
            "Setting stretched viewport with x: {}, y: {}, width: {}, height: {}",
            clip.x as f32, clip.y as f32, corrected_width, corrected_height
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

        for effect_entry in &mut self.effect_manager.effects {
            effect_entry.state.prepare(&mut effect_entry.effect, queue);

            if let Some(uniforms) = &effect_entry.effect.uniforms {
                let comparison_position = uniforms.get_float("comparison_position").unwrap_or(0.5);
                let comparison_enabled = uniforms
                    .get_uint("comparison_enabled")
                    .map(|v| v != 0)
                    .unwrap_or(false);

                if comparison_enabled {
                    self.line_pipeline
                        .prepare(queue, bounds, comparison_position);
                }
            }
        }

        // if let Some(video) = self.videos.get(&video_id) {
        //     for effect_entry in &mut self.effect_manager.effects {
        //         effect_entry.state.prepare(&mut effect_entry.effect, queue);
        //         if let Err(e) =
        //             effect_entry
        //                 .state
        //                 .update_for_frame(device, &mut effect_entry.effect, video)
        //         {
        //             println!("Failed to update effect bind group: {:?}", e);
        //         }
        //         if let Some(uniforms) = &effect_entry.effect.uniforms {
        //             let comparison_position =
        //                 uniforms.get_float("comparison_position").unwrap_or(0.5);
        //             let comparison_enabled = uniforms
        //                 .get_uint("comparison_enabled")
        //                 .map(|v| v != 0)
        //                 .unwrap_or(false);

        //             if comparison_enabled {
        //                 self.line_pipeline
        //                     .prepare(queue, bounds, comparison_position);
        //             }
        //         }
        //     }
        // }
    }

    pub fn draw(
        &self,
        target: &wgpu::TextureView,
        encoder: &mut wgpu::CommandEncoder,
        clip: &iced::Rectangle<u32>,
        video_id: u64,
    ) {
        println!("Starting draw method");

        if let Some(video) = self.videos.get(&video_id) {
            println!("Found video with ID: {}", video_id);
            // Early return for empty effect manager
            if self.effect_manager.is_empty() {
                println!("Effect manager is empty, drawing basic video");
                self.video_pipeline.draw(target, encoder, clip, video);
                return;
            }

            // Validate pipeline configuration
            if self.texture_manager.len() <= self.effect_manager.len()
                || self.effect_manager.bind_groups().len() != self.effect_manager.len()
            {
                println!("Invalid texture/bind group configuration, drawing basic video");
                self.video_pipeline.draw(target, encoder, clip, video);
                return;
            }

            println!("Input texture formats:");
            println!("Y texture: {:?}", video.texture_y.format());
            println!("UV texture: {:?}", video.texture_uv.format());

            // Debug pipeline state
            println!("Effect chain state:");
            println!("  Number of effects: {}", self.effect_manager.len());
            println!(
                "  Number of bind groups: {}",
                self.effect_manager.bind_groups().len()
            );
            println!(
                "  Number of intermediate textures: {}",
                self.texture_manager.len()
            );
            // First pass: YUV to RGB conversion
            // if let Some(input_texture) = self.texture_manager.get_texture(0) {
            // println!("Intermediate texture: {:?}", input_texture.format());
            // println!("First pass format info:");
            // println!("  Input Y format: {:?}", video.texture_y.format());
            // println!("  Input UV format: {:?}", video.texture_uv.format());
            // println!(
            //     "  Intermediate texture format: {:?}",
            //     input_texture.format()
            // );

            // if let Some(first_view) = self.texture_manager.create_texture_view(0) {
            // Draw YUV video to first intermediate texture
            // self.video_pipeline
            //     .draw_clear(&first_view, encoder, clip, video);

            // Create all texture views upfront
            let mut views = Vec::new();
            for i in 0..self.effect_manager.len() {
                println!("Processing effect {}:", i);
                println!(
                    "  Effect name: {}",
                    self.effect_manager.effects[i].effect.name
                );
                println!(
                    "  Layout ID: {:?}",
                    self.effect_manager.effects[i]
                        .effect
                        .bind_group_layout
                        .global_id()
                );
                println!("  Using bind group index: {}", i);
                if let Some(texture) = self.texture_manager.get_texture(i) {
                    println!("Creating view for texture {}:", i);
                    println!("  Texture format: {:?}", texture.format());

                    if let Some(view) = self.texture_manager.create_texture_view(i) {
                        views.push(view);
                    } else {
                        println!("Failed to create view for texture {}", i);
                        return;
                    }
                } else {
                    println!("Failed to get texture {}", i);
                    return;
                }
            }

            // Calculate the render target and texture dimensions
            let render_target_width = clip.width as f32;
            let render_target_height = clip.height as f32;

            let texture_width = video.texture_y.size().width as f32;
            let texture_height = video.texture_y.size().height as f32;

            // Process effects chain
            for i in 0..self.effect_manager.len() {
                if let Some(input_tex) = self.texture_manager.get_texture(i) {
                    println!("Processing effect {}:", i);
                    println!(
                        "  Effect name: {}",
                        self.effect_manager.effects[i].effect.name
                    );
                    println!(
                        "  Effect layout ID: {:?}",
                        self.effect_manager.effects[i]
                            .effect
                            .bind_group_layout
                            .global_id()
                    );
                    println!(
                        "  Bind group ID: {:?}",
                        self.effect_manager.bind_groups()[i].global_id()
                    );

                    let output_view = if i == self.effect_manager.len() - 1 {
                        target
                    } else {
                        &views[i + 1]
                    };

                    // Ensure bind group matches the effect
                    if self.effect_manager.effects[i].effect.name == "yuv_to_rgb" {
                        println!("  Verifying YUV to RGB bindings");
                    } else if self.effect_manager.effects[i].effect.name == "effect" {
                        println!("  Verifying Upscale bindings");
                    }

                    let bind_group = self.effect_manager.effects[i]
                        .effect
                        .get_bind_group()
                        .expect("Bind group should be updated in prepare");

                    self.apply_effect(
                        encoder,
                        &self.effect_manager.effects[i].effect,
                        bind_group, // Make sure this matches
                        output_view,
                        input_tex.as_ref(),
                        clip,
                        i < self.effect_manager.len() - 1,
                        render_target_width,
                        render_target_height,
                        texture_width,
                        texture_height,
                    );
                }
                println!("Effect {} complete", i);
            }

            // Handle comparison line
            let mut draw_comparison_line = false;
            if let Some(effect_entry) = self.effect_manager.effects.last() {
                if let Some(uniforms) = &effect_entry.effect.uniforms {
                    let comparison_enabled = uniforms
                        .get_uint("comparison_enabled")
                        .map(|v| v != 0)
                        .unwrap_or(false);

                    if comparison_enabled {
                        println!("Will draw comparison line");
                        draw_comparison_line = true;
                    }
                }
            }

            if draw_comparison_line {
                println!("Drawing comparison line");
                self.line_pipeline.draw(encoder, target, clip);
            }
            // } else {
            //     println!("Failed to create first view");
            //     self.video_pipeline.draw(target, encoder, clip, video);
            // }
            // } else {
            //     println!("Failed to get first intermediate texture");
            //     self.video_pipeline.draw(target, encoder, clip, video);
            // }
        } else {
            println!("Video not found with ID: {}", video_id);
        }
    }

    // pub fn add_effect(
    //     &mut self,
    //     device: &wgpu::Device,
    //     queue: &wgpu::Queue,
    //     shader_effect: ShaderEffect,
    //     state: Box<dyn Effect>,
    // ) -> anyhow::Result<()> {
    //     println!("Effect Addition Diagnostics:");
    //     println!("  Current video entries: {}", self.videos.len());
    //     if let Some(video) = self.videos.values().next() {
    //         println!("  Y Texture Details:");
    //         println!("    Format: {:?}", video.texture_y.format());
    //         println!("    Size: {:?}", video.texture_y.size());

    //         println!("  UV Texture Details:");
    //         println!("    Format: {:?}", video.texture_uv.format());
    //         println!("    Size: {:?}", video.texture_uv.size());
    //         let extent = video.texture_y.size();
    //         let texture_size = wgpu::Extent3d {
    //             width: extent.width,
    //             height: extent.height,
    //             depth_or_array_layers: 1,
    //         };

    //         println!("Creating effect with texture size: {:?}", texture_size);
    //         let previous_format = if self.effect_manager.is_empty() {
    //             // If there are no previous effects, assume the format is the same as the video format
    //             if let Some(video) = self.videos.values().next() {
    //                 video.texture_y.format()
    //             } else {
    //                 // Default to a fallback format if no videos are available
    //                 wgpu::TextureFormat::Bgra8UnormSrgb
    //             }
    //         } else {
    //             // Get the format of the last intermediate texture
    //             self.texture_manager
    //                 .get_texture(self.effect_manager.len() - 1)
    //                 .unwrap()
    //                 .format()
    //         };
    //         let required_format = shader_effect.get_format();

    //         println!("required_format: {:?}", &required_format);
    //         println!("previous_format: {:?}", &previous_format);
    //         if &previous_format != required_format {
    //             println!("1111111");
    //             // Add the appropriate conversion effect based on the previous and required formats
    //             match (previous_format, required_format) {
    //                 (wgpu::TextureFormat::Bgra8UnormSrgb, wgpu::TextureFormat::Rgba8UnormSrgb) => {
    //                     // Add YUV to RGB conversion effect
    //                     println!("222222");
    //                     let mut yuv_to_rgb_effect = YuvToRgbEffect::new(
    //                         0, // BT.709 color space
    //                         wgpu::TextureFormat::Bgra8UnormSrgb,
    //                     );
    //                     let yuv_to_rgb_shader_effect = yuv_to_rgb_effect.add(device, queue);
    //                     self.effect_manager
    //                         .add_effect(yuv_to_rgb_shader_effect, Box::new(yuv_to_rgb_effect));
    //                 }
    //                 _ => {
    //                     println!("33333");
    //                     // Unsupported format conversion
    //                     return Err(anyhow::anyhow!(
    //                         "Unsupported format conversion: {:?} to {:?}",
    //                         previous_format,
    //                         required_format
    //                     ));
    //                 }
    //             }
    //         }

    //         // Resize textures first
    //         self.texture_manager.resize_intermediate_textures(
    //             device,
    //             texture_size,
    //             self.effect_manager.len() + 1,
    //         );

    //         // Get input texture view with explicit descriptor

    //         if let Some(texture) = self.texture_manager.get_texture(self.effect_manager.len()) {
    //             if let Some(input_view) = self
    //                 .texture_manager
    //                 .create_texture_view(self.effect_manager.len())
    //             {
    //                 // Create bind group with new texture view
    //                 let bind_group = state.create_bind_group(
    //                     device,
    //                     &shader_effect,
    //                     vec![&input_view],
    //                     vec![texture],
    //                 )?;

    //                 // Add effect and bind group
    //                 self.effect_manager.add_effect(shader_effect, state);
    //                 self.effect_manager.add_bind_group(bind_group);
    //             }
    //         }

    //         println!("Added effect. Current state:");
    //         println!("  Effects count: {}", self.effect_manager.len());
    //         println!(
    //             "  Bind groups count: {}",
    //             self.effect_manager.bind_groups.len()
    //         );
    //         println!("  Intermediate textures: {}", self.texture_manager.len());
    //     }

    //     Ok(())
    // }

    pub fn add_effect(
        &mut self,
        update: bool,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        shader_effect: &mut ShaderEffect,
        mut shader_effect_type: Box<dyn Effect + Send + Sync>,
    ) -> anyhow::Result<()> {
        println!("Effect Addition Diagnostics:");

        let effect_count = self.effect_manager.effects.len();

        println!("effect_count {}", effect_count);
        // Process each index
        for i in 0..=effect_count {
            println!("indexxxxxxx {}", i);
            if i == 0 {
                self.handle_first_video(
                    update,
                    i,
                    device,
                    queue,
                    shader_effect,
                    &mut shader_effect_type,
                )?;
            } else {
                println!("testtttttt 2222222");
                self.handle_subsequent_effect(
                    update,
                    i,
                    device,
                    queue,
                    shader_effect,
                    &mut shader_effect_type,
                )?;
            }
        }

        Ok(())
    }
    fn handle_first_video(
        &mut self,
        update: bool,
        index: usize,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        shader_effect: &mut ShaderEffect,
        _shader_effect_type: &mut Box<dyn Effect + Send + Sync>,
    ) -> anyhow::Result<()> {
        let (size, views, format) = {
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

            (size, vec![y_view, uv_view], format)
        };

        println!("testtttttt");
        self.process_effect(
            update,
            index,
            device,
            queue,
            shader_effect,
            &views,
            format,
            size,
        )
    }
    fn handle_subsequent_effect(
        &mut self,
        update: bool,
        index: usize,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        shader_effect: &mut ShaderEffect,
        shader_effect_type: &mut Box<dyn Effect + Send + Sync>,
    ) -> anyhow::Result<()> {
        // Get input texture from previous effect
        let input_texture = self
            .texture_manager
            .get_texture(index - 1) // Use previous index for input
            .ok_or_else(|| anyhow::anyhow!("No input texture available"))?;

        let input_view = self
            .texture_manager
            .get_texture_view(index - 1) // Use previous index for input
            .ok_or_else(|| anyhow::anyhow!("No input texture view available"))?;

        // Get output texture for current effect
        let output_texture = self
            .texture_manager
            .get_texture(index) // Current index for output
            .ok_or_else(|| anyhow::anyhow!("No output texture available"))?;

        // Create texture lists for update_for_frame
        let texture_views = vec![input_view];
        let textures = vec![input_texture.as_ref()];

        // Update the effect's bind group
        println!("update_for_frame shader_effect name {}", shader_effect.name);
        shader_effect_type.update_for_frame(device, shader_effect, &texture_views, &textures)?;

        let format = output_texture.format();
        let texture_size = wgpu::Extent3d {
            width: output_texture.size().width,
            height: output_texture.size().height,
            depth_or_array_layers: 1,
        };

        self.process_effect(
            update,
            index,
            device,
            queue,
            shader_effect,
            &texture_views,
            format,
            texture_size,
        )
    }

    fn process_effect(
        &mut self,
        update: bool,
        index: usize,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        shader_effect: &mut ShaderEffect,
        previous_texture_views: &[TextureView],
        previous_format: TextureFormat,
        texture_size: wgpu::Extent3d,
    ) -> anyhow::Result<()> {
        let effect_count = self.effect_manager.len();
        let required_format = shader_effect.get_format().to_owned();

        if previous_format != required_format {
            match (previous_format, required_format) {
                (wgpu::TextureFormat::R8Unorm, wgpu::TextureFormat::Bgra8UnormSrgb) => {
                    let mut yuv_to_rgb_effect =
                        YuvToRgbEffect::new(0, wgpu::TextureFormat::Bgra8UnormSrgb);
                    let mut yuv_shader = yuv_to_rgb_effect.add(device, queue);
                    let yuv_layout = &yuv_shader.bind_group_layout;

                    self.texture_manager
                        .resize_intermediate_textures(device, texture_size, index);

                    let output_texture = self
                        .texture_manager
                        .get_texture(index)
                        .ok_or_else(|| anyhow::anyhow!("Failed to get output texture"))?;

                    let yuv_bind_group = yuv_to_rgb_effect.create_bind_group(
                        device,
                        &yuv_shader,
                        previous_texture_views,
                        &vec![output_texture.as_ref()],
                    )?;

                    println!("Adding YUV to RGB conversion effect");
                    println!("  Layout ID: {:?}", yuv_layout.global_id());

                    // let layout_id = yuv_shader.bind_group_layout.global_id();
                    yuv_shader.update_bind_group(yuv_bind_group);

                    println!("updateupdate {}", update);
                    if !update {
                        println!("Adding YUV to RGB conversion effect CREATE");
                        self.effect_manager
                            .add_effect(yuv_shader, Box::new(yuv_to_rgb_effect));
                    }
                    // self.effect_manager
                    //     .add_bind_group(yuv_bind_group, layout_id);
                }
                _ => {
                    return Err(anyhow::anyhow!(
                        "Unsupported format conversion: {:?} to {:?}",
                        previous_format,
                        required_format
                    ))
                }
            }
        }

        self.texture_manager.resize_intermediate_textures(
            device,
            texture_size,
            self.effect_manager.len() + 1,
        );

        let current_layout = &shader_effect.bind_group_layout;

        println!("Creating current effect bind group");
        println!("  Layout ID: {:?}", current_layout.global_id());

        println!("Final state:");
        println!("  Effects count: {}", self.effect_manager.len());
        println!("  Intermediate textures: {}", self.texture_manager.len());

        Ok(())
    }

    // pub fn add_effect(
    //     &mut self,
    //     device: &wgpu::Device,
    //     queue: &wgpu::Queue,
    //     shader_effect: ShaderEffect,
    // ) -> anyhow::Result<()> {
    //     println!("Effect Addition Diagnostics:");

    //     println!(
    //         "Adding effect with layout ID: {:?}",
    //         shader_effect.bind_group_layout.global_id()
    //     );
    //     let effect_count = self.effect_manager.len();
    //     let (previous_texture, texture_size) = if effect_count == 0 {
    //         // Video texture case
    //         if let Some(video) = self.videos.values().next() {
    //             (
    //                 vec![&video.texture_y, &video.texture_uv],
    //                 wgpu::Extent3d {
    //                     width: video.texture_y.size().width,
    //                     height: video.texture_y.size().height,
    //                     depth_or_array_layers: 1,
    //                 },
    //             )
    //         } else {
    //             return Err(anyhow::anyhow!("No video available"));
    //         }
    //     } else {
    //         // Effect texture case
    //         let last_texture = self
    //             .texture_manager
    //             .get_texture(effect_count - 1)
    //             .ok_or_else(|| anyhow::anyhow!("No texture available"))?;

    //         // Clone the Arc to extend its lifetime
    //         let last_texture = last_texture.clone();
    //         (
    //             vec![&last_texture],
    //             wgpu::Extent3d {
    //                 width: last_texture.size().width,
    //                 height: last_texture.size().height,
    //                 depth_or_array_layers: 1,
    //             },
    //         )
    //     };

    //     let previous_format = previous_texture[0].format();
    //     let required_format = shader_effect.get_format().to_owned();

    //     if previous_format != required_format {
    //         match (previous_format, required_format) {
    //             (wgpu::TextureFormat::R8Unorm, wgpu::TextureFormat::Bgra8UnormSrgb) => {
    //                 let mut yuv_to_rgb_effect =
    //                     YuvToRgbEffect::new(0, wgpu::TextureFormat::Bgra8UnormSrgb);
    //                 let yuv_shader = yuv_to_rgb_effect.add(device, queue);
    //                 let yuv_layout = &yuv_shader.bind_group_layout;

    //                 self.texture_manager.resize_intermediate_textures(
    //                     device,
    //                     texture_size,
    //                     effect_count + 1,
    //                 );

    //                 let y_view = previous_texture[0].create_view(&Default::default());
    //                 let uv_view = previous_texture[1].create_view(&Default::default());

    //                 let output_texture = self
    //                     .texture_manager
    //                     .get_texture(self.effect_manager.len())
    //                     .ok_or_else(|| anyhow::anyhow!("Failed to get output texture"))?;

    //                 let yuv_bind_group = yuv_to_rgb_effect.create_bind_group(
    //                     device,
    //                     &yuv_shader,
    //                     vec![&y_view, &uv_view],
    //                     vec![output_texture.as_ref()], // Use as_ref() to get &Texture from Arc<Texture>
    //                 )?;

    //                 println!("Adding YUV to RGB conversion effect");
    //                 println!("  Layout ID: {:?}", yuv_layout.global_id());

    //                 let layout_id = yuv_shader.bind_group_layout.global_id();

    //                 self.effect_manager
    //                     .add_effect(yuv_shader, Box::new(yuv_to_rgb_effect));
    //                 self.effect_manager
    //                     .add_bind_group(yuv_bind_group, layout_id);
    //             }
    //             _ => {
    //                 return Err(anyhow::anyhow!(
    //                     "Unsupported format conversion: {:?} to {:?}",
    //                     previous_format,
    //                     required_format
    //                 ))
    //             }
    //         }
    //     }

    //     self.texture_manager.resize_intermediate_textures(
    //         device,
    //         texture_size,
    //         self.effect_manager.len() + 1,
    //     );

    //     let main_layout = &shader_effect.bind_group_layout;

    //     let texture = self
    //         .texture_manager
    //         .get_texture(self.effect_manager.len())
    //         .ok_or_else(|| anyhow::anyhow!("Failed to get texture"))?;

    //     let prev_texture = self
    //         .texture_manager
    //         .get_texture(self.effect_manager.len() - 1)
    //         .ok_or_else(|| anyhow::anyhow!("Failed to get previous texture"))?;

    //     let input_view = prev_texture.create_view(&Default::default());

    //     println!("Creating main effect bind group");
    //     println!("  Layout ID: {:?}", main_layout.global_id());

    //     println!("Final state:");
    //     println!("  Effects count: {}", self.effect_manager.len());
    //     println!("  Intermediate textures: {}", self.texture_manager.len());

    //     Ok(())
    // }
}
