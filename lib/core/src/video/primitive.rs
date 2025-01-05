use ffmpeg_next::color::{self, Space};
use iced_wgpu::primitive::Primitive;
use iced_wgpu::wgpu;
use std::{
    collections::{btree_map::Entry, BTreeMap},
    num::NonZero,
    sync::atomic::{AtomicUsize, Ordering},
};

use super::{color_space::BT709_CONFIG, pipeline::VideoPipeline};

#[derive(Debug, Clone)]
pub struct VideoPrimitive {
    video_id: u64,
    alive: bool,
    frame: Vec<u8>,
    size: (u32, u32),
    upload_frame: bool,
    color_space: Space,
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
        }
    }
}

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
        if !storage.has::<VideoPipeline>() {
            let mut video_pipeline = VideoPipeline::new(device, format);

            // Add effects
            // video_pipeline.add_effect(
            //     device,
            //     include_str!("../../../../assets/shaders/grayscale.wgsl").into(),
            //     None,
            // );
            storage.store(video_pipeline);
        }

        let pipeline = storage.get_mut::<VideoPipeline>().unwrap();

        if self.upload_frame {
            pipeline.upload(
                device,
                queue,
                self.video_id,
                self.alive,
                self.size,
                self.frame.as_slice(),
            );
        }

        pipeline.prepare(
            device,
            queue,
            self.video_id,
            &(*bounds
                * iced::Transformation::orthographic(
                    viewport.logical_size().width as _,
                    viewport.logical_size().height as _,
                )),
            self.color_space,
        );
    }

    fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        storage: &iced_wgpu::primitive::Storage,
        target: &wgpu::TextureView,
        clip_bounds: &iced::Rectangle<u32>,
    ) {
        let pipeline = storage.get::<VideoPipeline>().unwrap();
        pipeline.draw(target, encoder, clip_bounds, self.video_id);
    }
}
