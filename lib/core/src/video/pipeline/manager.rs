use iced_wgpu::wgpu;
use std::{
    collections::{BTreeMap, HashMap},
    sync::atomic::AtomicUsize,
};

use crate::video::{
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
        self.texture_manager.resize_intermediate_textures(
            device,
            size,
            self.effect_manager.len() + 1,
        );
    }

    fn apply_effect(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        effect: &ShaderEffect,
        bind_group: &wgpu::BindGroup,
        output: &wgpu::TextureView,
        clip: &iced::Rectangle<u32>,
        clear: bool,
    ) {
        if let Some(uniforms) = &effect.uniforms {
            uniforms.debug_print_values();
        }
        RenderPasses::apply_effect(effect, encoder, bind_group, output, clip, clear);
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

            // Single video rendering logic
            if self.effect_manager.is_empty() {
                println!("Effect manager is empty, drawing basic video");
                self.video_pipeline.draw(target, encoder, clip, video);
                return;
            }

            println!("Effect manager state:");
            println!("  Number of effects: {}", self.effect_manager.len());
            println!(
                "  Number of bind groups: {}",
                self.effect_manager.bind_groups.len()
            );
            println!("  Texture manager length: {}", self.texture_manager.len());

            if self.texture_manager.len() <= self.effect_manager.len()
                || self.effect_manager.bind_groups.len() != self.effect_manager.len()
            {
                println!("Invalid texture/bind group configuration, drawing basic video");
                self.video_pipeline.draw(target, encoder, clip, video);
                return;
            }

            // Get first intermediate texture view
            let first_view = self
                .texture_manager
                .get_texture(0)
                .unwrap()
                .create_view(&Default::default());

            // Draw initial video to first intermediate texture
            println!("Drawing initial video to intermediate texture");
            self.video_pipeline
                .draw_clear(&first_view, encoder, clip, video);

            // Handle intermediate effects
            println!("Processing intermediate effects");
            for (i, effect) in self
                .effect_manager
                .effects
                .iter()
                .enumerate()
                .take(self.effect_manager.len() - 1)
            {
                println!("Processing effect {}", i);
                let output = &self
                    .texture_manager
                    .get_texture(i + 1)
                    .unwrap()
                    .create_view(&Default::default());

                self.apply_effect(
                    encoder,
                    &effect.0,
                    &self.effect_manager.bind_groups[i],
                    output,
                    clip,
                    true,
                );
            }

            // Handle final effect
            println!("Processing final effect");
            if let Some((last_effect, last_bind_group)) = self
                .effect_manager
                .effects
                .last()
                .zip(self.effect_manager.bind_groups.last())
            {
                println!("Found last effect and bind group");
                self.apply_effect(
                    encoder,
                    &last_effect.0,
                    last_bind_group,
                    target,
                    clip,
                    false,
                );

                println!("Checking uniforms existence");
                if let Some(uniforms) = &last_effect.0.uniforms {
                    println!("Found uniforms, checking comparison_enabled");
                    let comparison_enabled = uniforms
                        .get_uint("comparison_enabled")
                        .map(|v| v != 0)
                        .unwrap_or(false);

                    println!("Comparison enabled check: {}", comparison_enabled);
                    if comparison_enabled {
                        println!("Drawing comparison line");
                        self.line_pipeline.draw(encoder, target, clip);
                    }
                } else {
                    println!("No uniforms found in last effect");
                }
            } else {
                println!("No last effect or bind group found");
            }
        } else {
            println!("No video found with ID: {}", video_id);
        }
    }

    pub fn add_effect(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        shader_effect: ShaderEffect,
        state: Box<dyn Effect>,
    ) {
        let texture_size = if let Some(video) = self.videos.values().next() {
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

        // Debug print the current state before resizing
        println!("Before resize - Effects len: {}", self.effect_manager.len());
        println!("Texture size: {:?}", texture_size);

        self.texture_manager.resize_intermediate_textures(
            device,
            texture_size,
            self.effect_manager.len() + 1,
        );

        self.effect_manager.add_effect(shader_effect, state);
        self.effect_manager.clear_bind_groups();
    }
}
