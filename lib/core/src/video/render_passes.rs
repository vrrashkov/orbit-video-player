use ffmpeg_next::color::{self, Space};
use iced::Rectangle;
use iced_wgpu::primitive::Primitive;
use iced_wgpu::wgpu;
use std::{
    collections::{btree_map::Entry, BTreeMap},
    num::NonZero,
    sync::atomic::{AtomicUsize, Ordering},
};
use tracing::{debug, info, trace, warn};
use wgpu::{
    Color, CommandEncoder, LoadOp, RenderPassColorAttachment, RenderPassDescriptor, StoreOp,
    TextureView,
};

use super::{color_space::BT709_CONFIG, pipeline::manager::VideoEntry, ShaderEffect};

/// Utility struct for creating various render passes in the video pipeline
pub(crate) struct RenderPasses;

impl RenderPasses {
    /// Draw a video frame to a target texture
    ///
    /// Creates a render pass that draws the YUV video frame with proper color conversion
    pub fn draw_video_pass(
        pipeline: &wgpu::RenderPipeline,
        target: &TextureView,
        encoder: &mut CommandEncoder,
        clip: &Rectangle<u32>,
        video: &VideoEntry,
        load_op: LoadOp<Color>,
    ) {
        trace!(
            "Creating video render pass: clip={:?}, load_op={:?}",
            clip,
            if let LoadOp::Clear(_) = &load_op {
                "Clear"
            } else {
                "Load"
            }
        );

        // Create render pass with the specified load operation
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("video render pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: load_op,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        // Set the pipeline for this render pass
        pass.set_pipeline(&pipeline);

        // Calculate uniform buffer offset for this frame
        let offset = video.render_index.load(Ordering::Relaxed) * video.aligned_uniform_size;
        pass.set_bind_group(0, &video.bg0, &[offset as u32]);

        // Set appropriate scissor rectangle based on render mode
        if load_op == wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT) {
            // When clearing, use the video texture dimensions
            let video_size = video.texture_y.size();
            pass.set_scissor_rect(0, 0, video_size.width, video_size.height);
            trace!(
                "Using video dimensions for scissor: {}x{}",
                video_size.width,
                video_size.height
            );
        } else {
            // For final render to target, use UI coordinates from clip rectangle
            pass.set_scissor_rect(clip.x, clip.y, clip.width, clip.height);
            trace!("Using clip bounds for scissor: {:?}", clip);
        }

        // Draw a full-screen quad (2 triangles, 6 vertices)
        pass.draw(0..6, 0..1);

        // Increment render index for next frame
        video.render_index.fetch_add(1, Ordering::Relaxed);
    }

    /// Apply a shader effect to a texture
    ///
    /// Creates a render pass that applies the given shader effect,
    /// reading from input textures and writing to the output texture view.
    pub fn apply_effect(
        effect: &ShaderEffect,
        encoder: &mut wgpu::CommandEncoder,
        bind_group: &wgpu::BindGroup,
        output: &wgpu::TextureView,
        clip: &iced::Rectangle<u32>,
        clear: bool,
        render_target_width: f32,
        render_target_height: f32,
        texture_width: f32,
        texture_height: f32,
    ) {
        debug!(
            "Applying effect '{}': clear={}, texture={}x{}, target={}x{}",
            effect.name,
            clear,
            texture_width,
            texture_height,
            render_target_width,
            render_target_height
        );

        // Log uniform buffer details for debugging
        if let Some(uniforms) = &effect.uniforms {
            trace!(
                "Effect uniform buffer size: {} bytes",
                uniforms.buffer().size()
            );
        }

        // Create render pass with appropriate load operation
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(&format!("{}_effect_pass", effect.name)),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: output,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: if clear {
                        trace!("Using clear load operation");
                        wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
                    } else {
                        trace!("Using load load operation");
                        wgpu::LoadOp::Load
                    },
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        // Set viewport and scissor rect based on render mode
        if clear {
            // When rendering to intermediate texture, use full texture dimensions
            pass.set_viewport(0.0, 0.0, texture_width, texture_height, 0.0, 1.0);
            pass.set_scissor_rect(0, 0, texture_width as u32, texture_height as u32);
            trace!(
                "Set full texture viewport: {}x{}",
                texture_width,
                texture_height
            );
        } else {
            // When rendering to screen, use UI coordinates from clip rectangle
            pass.set_viewport(
                clip.x as f32,
                clip.y as f32,
                clip.width as f32,
                clip.height as f32,
                0.0,
                1.0,
            );
            pass.set_scissor_rect(clip.x, clip.y, clip.width, clip.height);
            trace!("Set clip-based viewport: {:?}", clip);
        }

        // Set the shader pipeline and bind resources
        trace!("Setting up effect pipeline and resources");
        pass.set_pipeline(&effect.pipeline);
        pass.set_bind_group(0, bind_group, &[]);

        // Draw a full-screen quad (2 triangles, 6 vertices)
        pass.draw(0..6, 0..1);
        trace!("Effect render pass completed");
    }
}
