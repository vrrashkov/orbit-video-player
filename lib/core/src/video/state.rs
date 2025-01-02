use nebula_common::VideoError;
use std::{sync::Arc, time::Duration};
use winit::window::Window;

use super::{decoder::VideoDecoder, primitive::VideoPrimitive};

pub struct VideoState {
    pub decoder: VideoDecoder,
    pub is_playing: bool,
}
impl VideoState {
    pub fn new(
        id: u64,
        video_path: &str,
        start_frame: u32,
        end_frame: u32,
    ) -> Result<Self, VideoError> {
        let video_decoder = VideoDecoder::new(video_path, start_frame, end_frame)?;

        Ok(Self {
            decoder: video_decoder,
            is_playing: false,
        })
    }
    pub fn update(&mut self) -> Result<Option<Vec<u8>>, VideoError> {
        if self.is_playing {
            self.decoder.next_frame()
        } else {
            Ok(self.decoder.get_last_frame())
        }
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
    pub fn get_frame_duration(&self) -> Duration {
        // Return the frame duration based on your video's FPS
        let fps = self.decoder.get_fps();
        Duration::from_secs_f64(1.0 / fps)
    }
    pub fn is_playing(&self) -> bool {
        self.is_playing
    }
}
