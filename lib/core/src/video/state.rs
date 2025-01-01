use nebula_common::VideoError;
use std::sync::Arc;
use winit::window::Window;

use super::{decoder::VideoDecoder, primitive::VideoPrimitive, renderer::VideoRenderer};

pub struct VideoState {
    pub window: Arc<Window>,
    pub decoder: VideoDecoder,
    pub primitive: VideoPrimitive,
    // pub renderer: VideoRenderer,
    pub is_playing: bool,
}
impl VideoState {
    pub async fn new(
        window: Arc<Window>,
        video_path: &str,
        start_frame: u32,
        end_frame: u32,
    ) -> Result<Self, VideoError> {
        let video_decoder = VideoDecoder::new(video_path, start_frame, end_frame).await?;
        let primitive = VideoPrimitive::new(
            window.id().into(),
            // Arc::clone(&inner.alive),
            // Arc::clone(&inner.frame),
            (
                video_decoder.decoder.width() as _,
                video_decoder.decoder.height() as _,
            ),
            // upload_frame,
        );
        // let renderer = VideoRenderer::new(
        //     window.clone(),
        //     video_decoder.decoder.width(),
        //     video_decoder.decoder.height(),
        // )
        // .await?;

        Ok(Self {
            window,
            decoder: video_decoder,
            primitive,
            // renderer,
            is_playing: false,
        })
    }

    // pub fn render(&mut self) -> Result<(), VideoError> {
    //     if let Some(frame_data) = self.decoder.next_frame()? {
    //         self.renderer.update_texture(
    //             &frame_data,
    //             self.decoder.width(),
    //             self.decoder.height(),
    //         )?;
    //     }
    //     self.renderer.render()
    // }

    // pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) -> Result<(), VideoError> {
    //     self.renderer.resize(new_size)
    // }

    // Control methods
    pub fn play(&mut self) {
        self.is_playing = true;
    }

    pub fn pause(&mut self) {
        self.is_playing = false;
    }

    pub fn seek(&mut self, frame: u32) -> Result<(), VideoError> {
        self.decoder.seek_to_frame(frame)
    }

    // Getters for UI
    pub fn current_frame(&self) -> u32 {
        self.decoder.current_frame()
    }
    pub fn start_frame(&self) -> u32 {
        self.decoder.start_frame()
    }
    pub fn end_frame(&self) -> u32 {
        self.decoder.end_frame()
    }
    pub fn looping(&self) -> bool {
        self.decoder.looping()
    }
    pub fn total_frames(&self) -> u32 {
        self.decoder.total_frames()
    }

    pub fn is_playing(&self) -> bool {
        self.is_playing
    }
}
