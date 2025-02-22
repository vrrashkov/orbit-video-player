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
}

impl VideoPipelineManager {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let state = PipelineState::default();
        let video_pipeline = VideoPipeline::new(device, format);
        let line_pipeline = LinePipeline::new(device, format);
        let texture_manager = TextureManager::new(format);
        let effect_manager = EffectManager::new();
        if !texture_manager.validate_formats() {
            println!("WARNING: Format inconsistency detected");
        }
        Self {
            state,
            video_pipeline,
            line_pipeline,
            texture_manager,
            effect_manager,
            format,
            videos: BTreeMap::new(),
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
    ) {
        println!("Applying effect with:");
        println!("  Output dimensions: {:?}", output.size());
        println!("  Output format: {:?}", output.format());
        println!("  Clip rect: {:?}", clip);

        if let Some(uniforms) = &effect.uniforms {
            uniforms.debug_print_values();
        }
        RenderPasses::apply_effect(effect, encoder, bind_group, output_view, clip, clear);
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
            self.resize_intermediate_textures(device, size);
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

        for (effect, state) in &mut self.effect_manager.effects {
            state.prepare(effect, queue);

            if let Some(uniforms) = &effect.uniforms {
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
                || self.effect_manager.bind_groups.len() != self.effect_manager.len()
            {
                println!("Invalid texture/bind group configuration, drawing basic video");
                self.video_pipeline.draw(target, encoder, clip, video);
                return;
            }

            // Debug pipeline state
            println!("Effect chain state:");
            println!("  Number of effects: {}", self.effect_manager.len());
            println!(
                "  Number of bind groups: {}",
                self.effect_manager.bind_groups.len()
            );
            println!(
                "  Number of intermediate textures: {}",
                self.texture_manager.len()
            );

            // First pass: YUV to RGB conversion
            if let Some(input_texture) = self.texture_manager.get_texture(0) {
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
                    for i in 0..=self.effect_manager.len() {
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

                    // Process effects chain
                    for i in 0..self.effect_manager.len() {
                        // Get input texture format for debugging
                        if let Some(input_tex) = self.texture_manager.get_texture(i) {
                            println!("Effect {} input format: {:?}", i, input_tex.format());

                            let output_view = if i == self.effect_manager.len() - 1 {
                                target
                            } else {
                                &views[i + 1]
                            };

                            // Apply effect and debug uniforms
                            if let Some(uniforms) = &self.effect_manager.effects[i].0.uniforms {
                                println!("Effect {} uniforms:", i);
                                uniforms.debug_print_values();
                            }

                            self.apply_effect(
                                encoder,
                                &self.effect_manager.effects[i].0,
                                &self.effect_manager.bind_groups[i],
                                output_view,
                                input_tex,
                                clip,
                                i < self.effect_manager.len() - 1,
                            );
                        }
                        println!("Effect {} complete", i);
                    }

                    // Handle comparison line
                    let mut draw_comparison_line = false;
                    if let Some((last_effect, _)) = self.effect_manager.effects.last() {
                        if let Some(uniforms) = &last_effect.uniforms {
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
        shader_effect: ShaderEffect,
        state: Box<dyn Effect>,
    ) -> anyhow::Result<()> {
        println!("Effect Addition Diagnostics:");
        println!("  Current video entries: {}", self.videos.len());
        let texture_size = if let Some(video) = self.videos.values().next() {
            println!("  Y Texture Details:");
            println!("    Format: {:?}", video.texture_y.format());
            println!("    Size: {:?}", video.texture_y.size());

            println!("  UV Texture Details:");
            println!("    Format: {:?}", video.texture_uv.format());
            println!("    Size: {:?}", video.texture_uv.size());
            let extent = video.texture_y.size();
            wgpu::Extent3d {
                width: extent.width,
                height: extent.height,
                depth_or_array_layers: 1,
            }
        } else {
            wgpu::Extent3d {
                width: 1920,
                height: 1080,
                depth_or_array_layers: 1,
            }
        };

        println!("Creating effect with texture size: {:?}", texture_size);
        let previous_format = if self.effect_manager.is_empty() {
            // If there are no previous effects, assume the format is the same as the video format
            if let Some(video) = self.videos.values().next() {
                video.texture_y.format()
            } else {
                // Default to a fallback format if no videos are available
                wgpu::TextureFormat::Bgra8UnormSrgb
            }
        } else {
            // Get the format of the last intermediate texture
            self.texture_manager
                .get_texture(self.effect_manager.len() - 1)
                .unwrap()
                .format()
        };
        let required_format = shader_effect.get_format();

        println!("required_format: {:?}", &required_format);
        println!("previous_format: {:?}", &previous_format);
        if &previous_format != required_format {
            println!("1111111");
            // Add the appropriate conversion effect based on the previous and required formats
            match (previous_format, required_format) {
                (wgpu::TextureFormat::R8Unorm, wgpu::TextureFormat::Bgra8UnormSrgb) => {
                    // Add YUV to RGB conversion effect
                    println!("Adding YUV to RGB conversion effect");
                    let mut yuv_to_rgb_effect = YuvToRgbEffect::new(
                        0, // BT.709 color space
                        wgpu::TextureFormat::Bgra8UnormSrgb,
                    );
                    let yuv_to_rgb_shader_effect = yuv_to_rgb_effect.add(device, queue);
                    self.effect_manager
                        .add_effect(yuv_to_rgb_shader_effect, Box::new(yuv_to_rgb_effect));
                }
                _ => {
                    println!("33333");
                    // Unsupported format conversion
                    return Err(anyhow::anyhow!(
                        "Unsupported format conversion: {:?} to {:?}",
                        previous_format,
                        required_format
                    ));
                }
            }
        }
        // Resize textures first
        self.texture_manager.resize_intermediate_textures(
            device,
            texture_size,
            self.effect_manager.len() + 1,
        );

        // Get input texture view with explicit descriptor

        if let Some(texture) = self.texture_manager.get_texture(self.effect_manager.len()) {
            if let Some(input_view) = self
                .texture_manager
                .create_texture_view(self.effect_manager.len())
            {
                // Create bind group with new texture view
                let bind_group = state.create_bind_group(
                    device,
                    &shader_effect,
                    vec![&input_view],
                    vec![texture],
                )?;

                // Add effect and bind group
                self.effect_manager.add_effect(shader_effect, state);
                self.effect_manager.add_bind_group(bind_group);
            }
        }

        println!("Added effect. Current state:");
        println!("  Effects count: {}", self.effect_manager.len());
        println!(
            "  Bind groups count: {}",
            self.effect_manager.bind_groups.len()
        );
        println!("  Intermediate textures: {}", self.texture_manager.len());
        Ok(())
    }
}
