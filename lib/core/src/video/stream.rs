use ffmpeg_next::{self as ffmpeg, color::Space, error::EAGAIN, ffi::AV_TIME_BASE};
use nebula_common::VideoError;
use std::{
    borrow::Borrow,
    collections::VecDeque,
    time::{Duration, Instant},
};
pub struct QueuedFrame {
    pub data: Vec<u8>,
    pub frame_number: u64,
}

pub struct VideoStream {
    pub decoder: ffmpeg::decoder::Video,
    format_context: ffmpeg::format::context::Input,
    video_stream_index: usize,
    current_frame: u64,
    start_frame: u64,
    end_frame: Option<u64>,
    looping: bool,
    presentation_queue: VecDeque<QueuedFrame>,
    max_queue_size: usize, // e.g., 5-10 frames
    frame_timer: Instant,
    pub is_playing: bool,
    pub color_space: Space,
}

pub struct VideoStreamOptions<'a> {
    pub video_path: &'a str,
    pub start_frame: u64,
    pub end_frame: Option<u64>,
}
impl VideoStream {
    pub fn new(options: VideoStreamOptions) -> Result<Self, VideoError> {
        // 3. Initialize FFmpeg
        ffmpeg::init()?;

        tracing::info!("Loading video from: {}", options.video_path);
        let mut format_context = ffmpeg::format::input(&options.video_path)?;

        let video_stream = format_context
            .streams()
            .best(ffmpeg::media::Type::Video)
            .ok_or(VideoError::StreamNotFound("Video stream not found"))?;

        let frame_rate = video_stream.rate();
        tracing::info!("Frame rate: {}/{} fps", frame_rate.0, frame_rate.1);
        let fps = frame_rate.0 as f64 / frame_rate.1 as f64;
        let frame_duration = std::time::Duration::from_secs_f64(1.0 / fps);
        tracing::info!("Frame duration: {:?}", frame_duration);

        let video_stream_index = video_stream.index();
        let parameters = video_stream.parameters();
        let time_s = ((options.start_frame - 1) as f64 / fps) as f64;
        let timestamp = (time_s * AV_TIME_BASE as f64) as i64;
        tracing::info!("timestamp: {:?}", timestamp);
        format_context.seek(timestamp, timestamp..)?;

        let context = ffmpeg::codec::Context::from_parameters(parameters)?;

        let decoder = context.decoder().video()?;

        let color_space = decoder.color_space();
        tracing::warn!("color_space: {:?}", color_space);

        let now = Instant::now();
        let mut decoder = Self {
            decoder,
            format_context,
            video_stream_index,
            current_frame: options.start_frame,
            frame_timer: now,
            start_frame: options.start_frame,
            end_frame: options.end_frame,
            looping: false,
            presentation_queue: VecDeque::new(),
            max_queue_size: 10,
            is_playing: false,
            color_space,
        };

        decoder.pre_buffer()?;

        Ok(decoder)
    }
    pub fn next_frame(&mut self) -> Result<Option<Vec<u8>>, VideoError> {
        tracing::info!("Entering next_frame");

        // Fill the queue if empty
        if self.presentation_queue.is_empty() {
            tracing::info!("Queue empty, filling buffer");
            while self.presentation_queue.len() < self.max_queue_size {
                self.decode_next_frame()?;
            }
        }

        // If we have frames and it's time to show the next one
        if !self.presentation_queue.is_empty() && self.should_process_frame() {
            tracing::info!(
                "Processing frame {} from queue (size: {})",
                self.current_frame,
                self.presentation_queue.len()
            );

            // Get the next frame
            let frame = self.presentation_queue.pop_front().map(|f| f.data);

            // Try to keep buffer full
            if self.presentation_queue.len() < self.max_queue_size {
                let _ = self.decode_next_frame()?;
            }

            return Ok(frame);
        }

        // Return current frame if it's not time for next one
        Ok(self.presentation_queue.front().map(|f| f.data.clone()))
    }

    pub fn should_process_frame(&mut self) -> bool {
        let now = Instant::now();
        let elapsed = now.duration_since(self.frame_timer);
        let frame_duration = Duration::from_secs_f64(1.0 / self.get_fps());

        if elapsed >= frame_duration {
            // Update timer by exact frame duration to prevent drift
            let frames_to_advance = elapsed.div_duration_f64(frame_duration);
            self.frame_timer += frame_duration.mul_f64(frames_to_advance.floor());

            tracing::info!(
                "Processing frame: elapsed={:?}, frame_duration={:?}, frames_advanced={}",
                elapsed,
                frame_duration,
                frames_to_advance
            );
            true
        } else {
            false
        }
    }
    pub fn get_last_frame(&self) -> Option<Vec<u8>> {
        if let Some(frame) = self.presentation_queue.front() {
            tracing::info!(
                "Returning frame {} from queue (queue size: {})",
                frame.frame_number,
                self.presentation_queue.len()
            );
            Some(frame.data.clone())
        } else {
            tracing::warn!("No frames in queue to return");
            None
        }
    }
    fn decode_next_frame(&mut self) -> Result<(), VideoError> {
        if self.presentation_queue.len() >= self.max_queue_size {
            return Ok(());
        }

        let mut frame = ffmpeg::frame::Video::empty();
        let mut packets_sent = 0;

        loop {
            match self.decoder.receive_frame(&mut frame) {
                Ok(_) => {
                    let mut yuv_frame = ffmpeg::frame::Video::empty();
                    let mut scaler = ffmpeg::software::scaling::Context::get(
                        self.decoder.format(),
                        self.decoder.width(),
                        self.decoder.height(),
                        ffmpeg::format::Pixel::YUV420P,
                        self.decoder.width(),
                        self.decoder.height(),
                        ffmpeg::software::scaling::Flags::BILINEAR,
                    )?;

                    scaler.run(&frame, &mut yuv_frame)?;

                    let mut uv_plane = Vec::with_capacity(yuv_frame.data(1).len() * 2);
                    for (u, v) in yuv_frame.data(1).iter().zip(yuv_frame.data(2).iter()) {
                        uv_plane.push(*u);
                        uv_plane.push(*v);
                    }

                    let combined = [yuv_frame.data(0).to_vec(), uv_plane].concat();

                    tracing::info!(
                        "Adding frame {} to queue (current size: {})",
                        self.current_frame,
                        self.presentation_queue.len()
                    );

                    self.presentation_queue.push_back(QueuedFrame {
                        data: combined,
                        frame_number: self.current_frame,
                    });

                    self.current_frame += 1;
                    return Ok(());
                }
                Err(ffmpeg::Error::Other { errno: EAGAIN }) => {
                    if let Some((stream, packet)) = self.format_context.packets().next() {
                        if stream.index() == self.video_stream_index {
                            self.decoder.send_packet(&packet)?;
                            packets_sent += 1;
                        }
                    } else {
                        self.decoder.send_packet(&ffmpeg::Packet::empty())?;
                        return Ok(());
                    }
                }
                Err(e) => return Err(VideoError::Decode(e.to_string())),
            }

            if packets_sent > 100 {
                tracing::warn!("Too many packets sent without decoding frame");
                return Ok(());
            }
        }
    }
    fn get_current_frame(&self) -> Option<Vec<u8>> {
        static mut LAST_FRAME_NUMBER: u64 = 0;

        if let Some(frame) = self.presentation_queue.front() {
            // Only log if frame number changed
            unsafe {
                if LAST_FRAME_NUMBER != frame.frame_number {
                    tracing::info!(
                        "New frame {} (previous: {})",
                        frame.frame_number,
                        LAST_FRAME_NUMBER
                    );
                    LAST_FRAME_NUMBER = frame.frame_number;
                }
            }
            Some(frame.data.clone())
        } else {
            None
        }
    }
    pub fn seek_to_frame(&mut self, time_s: f64, c_frame: u64) -> Result<(), VideoError> {
        tracing::info!("Starting seek to frame {} at time {}s", c_frame, time_s);

        // Clear state first
        self.presentation_queue.clear();

        // Flush decoder
        self.decoder.flush();

        let timestamp = (time_s * AV_TIME_BASE as f64) as i64;
        self.format_context.seek(timestamp, ..)?;

        // Update current_frame before decoding new frames
        self.current_frame = c_frame;

        // Reset frame timer
        self.frame_timer = Instant::now();

        // Fill buffer with new frames
        self.pre_buffer()?;

        tracing::info!(
            "Seek complete, first frame in queue: {}, queue size: {}",
            self.current_frame(),
            self.presentation_queue.len()
        );

        Ok(())
    }
    // pub fn seek_to_frame(&mut self, time_s: f64, c_frame: u64) -> Result<(), VideoError> {
    //     // Clear state first
    //     self.presentation_queue.clear();

    //     let timestamp = (time_s * AV_TIME_BASE as f64) as i64;
    //     tracing::warn!("seek for: {}", timestamp);
    //     self.format_context.seek(timestamp, timestamp..)?;

    //     self.current_frame = c_frame;
    //     self.pre_buffer()?;

    //     Ok(())
    // }
    pub fn pre_buffer(&mut self) -> Result<(), VideoError> {
        tracing::info!("Pre-buffering frames...");
        while self.presentation_queue.len() < self.max_queue_size {
            self.decode_next_frame()?;
        }
        tracing::info!("Pre-buffered {} frames", self.presentation_queue.len());
        Ok(())
    }
    // Add these helper methods for the renderer to know frame dimensions
    pub fn width(&self) -> u32 {
        self.decoder.width() as u32
    }

    pub fn height(&self) -> u32 {
        self.decoder.height() as u32
    }
    pub fn current_time(&self) -> Duration {
        if self.current_frame() > 0 {
            let current_second = (self.current_frame() - self.start_frame) as f64 / self.get_fps();
            Duration::from_secs_f64(current_second)
        } else {
            Duration::from_secs_f64(0.)
        }
    }
    pub fn total_time(&self) -> Duration {
        let video_stream = self
            .format_context
            .streams()
            .best(ffmpeg::media::Type::Video)
            .expect("Video stream should exist");

        let duration = video_stream.duration() as f64 * f64::from(video_stream.time_base());
        Duration::from_secs_f64(duration)
    }
    // pub fn total_time(&self) -> Duration {
    //     let total_seconds = (self.total_frames() - self.start_frame) as f64 / self.get_fps();
    //     Duration::from_secs_f64(total_seconds)
    // }
    pub fn seek_to_time(&mut self, seconds: f64) -> Result<(), VideoError> {
        if seconds < 0.0 || seconds > self.total_time().as_secs_f64() {
            return Err(VideoError::Decode("Invalid seek position".to_string()));
        }

        tracing::info!("Seeking to time: {} seconds", seconds);
        let frame = (seconds * self.get_fps()).round() as u64;
        self.seek_to_frame(seconds, frame)
    }
    // pub fn seek_to_time(&mut self, seconds: f64) -> Result<(), VideoError> {
    //     tracing::info!("seek_to_time: {}", seconds);
    //     let frame = (seconds * self.get_fps()) as u64;
    //     tracing::info!("seek_to_time frameee: {}", frame);
    //     self.seek_to_frame(seconds, frame)
    // }
    pub fn current_frame(&self) -> u64 {
        // Always use the first frame in queue if available
        self.presentation_queue
            .front()
            .map(|f| f.frame_number)
            .unwrap_or(self.current_frame)
    }

    pub fn start_frame(&self) -> u64 {
        self.start_frame
    }
    pub fn end_frame(&self) -> u64 {
        if let Some(value) = self.end_frame {
            value
        } else {
            self.total_frames()
        }
    }

    pub fn looping(&self) -> bool {
        self.looping
    }
    pub fn total_frames(&self) -> u64 {
        let video_stream = self
            .format_context
            .streams()
            .best(ffmpeg::media::Type::Video)
            .expect("Video stream should exist");

        let duration = video_stream.duration() as f64 * f64::from(video_stream.time_base());
        (duration * self.get_fps()).ceil() as u64
    }
    // pub fn total_frames(&self) -> u64 {
    //     let video_stream = self
    //         .format_context
    //         .streams()
    //         .best(ffmpeg::media::Type::Video)
    //         .expect("Video stream should exist");

    //     video_stream.frames() as u64
    // }
    pub fn get_fps(&self) -> f64 {
        let frame_rate = self
            .format_context
            .streams()
            .best(ffmpeg::media::Type::Video)
            .map(|stream| stream.rate())
            .unwrap_or((30, 1).into()); // fallback to 30fps if we can't get the rate

        frame_rate.0 as f64 / frame_rate.1 as f64
    }

    // Control methods
    pub fn play(&mut self) {
        self.is_playing = true;
    }

    pub fn pause(&mut self) {
        self.is_playing = false;
    }
    pub fn is_playing(&self) -> bool {
        self.is_playing
    }
    pub fn update(&mut self) -> Result<Option<Vec<u8>>, VideoError> {
        // Only get a new frame if we're playing and it's time
        if self.is_playing {
            self.next_frame()
        } else {
            // When paused or not time for next frame, return current frame without cloning
            // unless it's actually needed
            Ok(self.get_current_frame())
        }
    }
    pub fn get_frame_duration(&self) -> Duration {
        let fps = self.get_fps();
        Duration::from_secs_f64(1.0 / fps)
    }
}
impl Drop for VideoStream {
    fn drop(&mut self) {
        let _ = self.decoder.send_packet(&ffmpeg::Packet::empty());
    }
}
