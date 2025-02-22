use iced_wgpu::wgpu;
use std::{
    collections::{BTreeMap, HashMap},
    sync::atomic::AtomicUsize,
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
            if let Some(input_texture) = self.texture_manager.get_texture(0) {
                println!("Intermediate texture: {:?}", input_texture.format());
                println!("First pass format info:");
                println!("  Input Y format: {:?}", video.texture_y.format());
                println!("  Input UV format: {:?}", video.texture_uv.format());
                println!(
                    "  Intermediate texture format: {:?}",
                    input_texture.format()
                );

                if let Some(first_view) = self.texture_manager.create_texture_view(0) {
                    // Draw YUV video to first intermediate texture
                    self.video_pipeline
                        .draw_clear(&first_view, encoder, clip, video);

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

                            self.apply_effect(
                                encoder,
                                &self.effect_manager.effects[i].effect,
                                &self.effect_manager.bind_groups()[i], // Make sure this matches
                                output_view,
                                input_tex,
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
                } else {
                    println!("Failed to create first view");
                    self.video_pipeline.draw(target, encoder, clip, video);
                }
            } else {
                println!("Failed to get first intermediate texture");
                self.video_pipeline.draw(target, encoder, clip, video);
            }
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
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        mut shader_effect: ShaderEffect,
        state: Box<dyn Effect>,
    ) -> anyhow::Result<()> {
        println!("Effect Addition Diagnostics:");

        println!(
            "Adding effect with layout ID: {:?}",
            shader_effect.bind_group_layout.global_id()
        );
        // Get current format and texture size
        let (previous_format, texture_size) = if self.effect_manager.is_empty() {
            if let Some(video) = self.videos.values().next() {
                (
                    video.texture_y.format(),
                    wgpu::Extent3d {
                        width: video.texture_y.size().width,
                        height: video.texture_y.size().height,
                        depth_or_array_layers: 1,
                    },
                )
            } else {
                return Err(anyhow::anyhow!("No video available"));
            }
        } else {
            let last_texture = self
                .texture_manager
                .get_texture(self.effect_manager.len() - 1)
                .unwrap();
            (
                last_texture.format(),
                wgpu::Extent3d {
                    width: last_texture.size().width,
                    height: last_texture.size().height,
                    depth_or_array_layers: 1,
                },
            )
        };

        let required_format = shader_effect.get_format().to_owned();

        // Handle YUV to RGB conversion if needed
        if previous_format != required_format {
            match (previous_format, required_format) {
                (wgpu::TextureFormat::R8Unorm, wgpu::TextureFormat::Bgra8UnormSrgb) => {
                    // Create YUV to RGB conversion effect
                    let mut yuv_to_rgb_effect =
                        YuvToRgbEffect::new(0, wgpu::TextureFormat::Bgra8UnormSrgb);
                    let yuv_shader = yuv_to_rgb_effect.add(device, queue);

                    // Store the layout from the shader effect
                    let yuv_layout = &yuv_shader.bind_group_layout;

                    // Prepare textures for YUV conversion
                    self.texture_manager.resize_intermediate_textures(
                        device,
                        texture_size,
                        self.effect_manager.len() + 1,
                    );

                    // Get video for YUV conversion
                    let video =
                        self.videos.values().next().ok_or_else(|| {
                            anyhow::anyhow!("No video available for YUV conversion")
                        })?;

                    // Create views for Y and UV textures
                    let y_view = video.texture_y.create_view(&Default::default());
                    let uv_view = video.texture_uv.create_view(&Default::default());

                    // Get output texture for YUV conversion
                    let output_texture = self
                        .texture_manager
                        .get_texture(self.effect_manager.len())
                        .ok_or_else(|| anyhow::anyhow!("Failed to get output texture"))?;

                    // Create bind group using the saved layout
                    let yuv_bind_group = yuv_to_rgb_effect.create_bind_group(
                        device,
                        &yuv_shader, // Pass shader with the original layout
                        vec![&y_view, &uv_view],
                        vec![output_texture],
                    )?;

                    println!("Adding YUV to RGB conversion effect");
                    println!("  Layout ID: {:?}", yuv_layout.global_id());

                    // Add YUV conversion effect and bind group
                    let layout_id = yuv_shader.bind_group_layout.global_id();

                    self.effect_manager
                        .add_effect(yuv_shader, Box::new(yuv_to_rgb_effect));
                    self.effect_manager
                        .add_bind_group(yuv_bind_group, layout_id);
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

        // Add the main effect
        self.texture_manager.resize_intermediate_textures(
            device,
            texture_size,
            self.effect_manager.len() + 1,
        );

        // Store the main effect layout
        let main_layout = &shader_effect.bind_group_layout;

        // Get input and output textures for the main effect
        let texture = self
            .texture_manager
            .get_texture(self.effect_manager.len())
            .ok_or_else(|| anyhow::anyhow!("Failed to get texture"))?;

        let prev_texture = self
            .texture_manager
            .get_texture(self.effect_manager.len() - 1)
            .ok_or_else(|| anyhow::anyhow!("Failed to get previous texture"))?;

        let input_view = prev_texture.create_view(&Default::default());

        println!("Creating main effect bind group");
        println!("  Layout ID: {:?}", main_layout.global_id());

        // // ADD MORE EFFECTS
        // // Create bind group using the saved layout
        // let bind_group = state.create_bind_group(
        //     device,
        //     &shader_effect, // Pass shader with the original layout
        //     vec![&input_view],
        //     vec![texture],
        // )?;

        // // Add main effect and bind group
        // let layout_id = shader_effect.bind_group_layout.global_id();

        // self.effect_manager.add_effect(shader_effect, state);
        // self.effect_manager.add_bind_group(bind_group, layout_id);
        // // END DD MORE EFFECTS
        println!("Final state:");
        println!("  Effects count: {}", self.effect_manager.len());
        println!("  Intermediate textures: {}", self.texture_manager.len());

        Ok(())
    }
}
