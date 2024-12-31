use nebula_common::VideoError;
use std::sync::Arc;
use winit::window::Window;

use super::{decoder::VideoDecoder, renderer::VideoRenderer};

pub struct VideoState {
    window: Arc<Window>,
    decoder: VideoDecoder,
    renderer: VideoRenderer,
    is_playing: bool,
}
impl VideoState {
    pub async fn new(
        window: Arc<Window>,
        video_path: &str,
        start_frame: u32,
        end_frame: u32,
    ) -> Result<Self, VideoError> {
        let video_decoder = VideoDecoder::new(video_path, start_frame, end_frame).await?;
        let renderer = VideoRenderer::new(
            window.clone(),
            video_decoder.decoder.width(),
            video_decoder.decoder.height(),
        )
        .await?;

        Ok(Self {
            window,
            decoder: video_decoder,
            renderer,
            is_playing: false,
        })
    }

    pub fn render(&mut self) -> Result<(), VideoError> {
        if let Some(frame_data) = self.decoder.next_frame()? {
            self.renderer.update_texture(
                &frame_data,
                self.decoder.width(),
                self.decoder.height(),
            )?;
        }
        self.renderer.render()
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) -> Result<(), VideoError> {
        self.renderer.resize(new_size)
    }

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

    pub fn total_frames(&self) -> u32 {
        self.decoder.total_frames()
    }

    pub fn is_playing(&self) -> bool {
        self.is_playing
    }
}
