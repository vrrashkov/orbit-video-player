use ffmpeg_next::color::{self, Space};
use iced::Rectangle;
use iced_wgpu::primitive::Primitive;
use iced_wgpu::wgpu;
use std::{
    collections::{btree_map::Entry, BTreeMap},
    num::NonZero,
    sync::atomic::{AtomicUsize, Ordering},
};
use wgpu::{
    Color, CommandEncoder, LoadOp, RenderPassColorAttachment, RenderPassDescriptor, StoreOp,
    TextureView,
};

use super::{color_space::BT709_CONFIG, pipeline::manager::VideoEntry, ShaderEffect};

pub(crate) struct RenderPasses;

impl RenderPasses {
    pub fn draw_video_pass(
        pipeline: &wgpu::RenderPipeline,
        target: &TextureView,
        encoder: &mut CommandEncoder,
        clip: &Rectangle<u32>,
        video: &VideoEntry,
        load_op: LoadOp<Color>,
    ) {
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

        pass.set_pipeline(&pipeline);

        let offset = video.render_index.load(Ordering::Relaxed) * video.aligned_uniform_size;

        pass.set_bind_group(0, &video.bg0, &[offset as u32]);

        // // TODO check if this is necessary
        if load_op == wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT) {
            let video_size = video.texture_y.size();
            pass.set_scissor_rect(0, 0, video_size.width, video_size.height);
        } else {
            // For final render to target, use UI coordinates
            pass.set_scissor_rect(clip.x, clip.y, clip.width, clip.height);
        }
        pass.draw(0..6, 0..1);

        video.prepare_index.store(0, Ordering::Relaxed);
        video.render_index.fetch_add(1, Ordering::Relaxed);
    }

    pub fn apply_effect(
        effect: &ShaderEffect,
        encoder: &mut CommandEncoder,
        bind_group: &wgpu::BindGroup,
        output: &TextureView,
        clip: &Rectangle<u32>,
        clear: bool,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("effect_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: output,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: if clear {
                        wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
                    } else {
                        wgpu::LoadOp::Load
                    },
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        pass.set_pipeline(&effect.pipeline);
        pass.set_bind_group(0, bind_group, &[]);
        if clear {
            pass.set_scissor_rect(0, 0, clip.width, clip.height);
        } else {
            pass.set_scissor_rect(clip.x, clip.y, clip.width, clip.height);
        }
        pass.draw(0..6, 0..1);
    }
}
