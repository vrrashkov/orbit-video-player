use ffmpeg_next::{
    self as ffmpeg,
    color::Space,
    error::EAGAIN,
    ffi::{av_seek_frame, AVMediaType, AVSEEK_FLAG_ANY, AVSEEK_FLAG_FRAME, AV_TIME_BASE},
};

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
    frame_buffer: Vec<u8>,           // Add this for processing
    yuv_frame: ffmpeg::frame::Video, // Reuse these objects
    scaler: ffmpeg::software::scaling::Context,
}

pub struct VideoStreamOptions<'a> {
    pub video_path: &'a str,
    pub start_frame: u64,
    pub end_frame: Option<u64>,
}
const DEFAULT_FPS: i32 = 30;
const DEFAULT_QUEUE_SIZE: usize = 10;
const MAX_PACKETS_PER_FRAME: usize = 100;
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
        let yuv_frame = ffmpeg::frame::Video::empty();
        let scaler = ffmpeg::software::scaling::Context::get(
            decoder.format(),
            decoder.width(),
            decoder.height(),
            ffmpeg::format::Pixel::YUV420P,
            decoder.width(),
            decoder.height(),
            ffmpeg::software::scaling::Flags::BILINEAR,
        )?;
        let frame_buffer = Vec::with_capacity(Self::calculate_buffer_size(&decoder));
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
            max_queue_size: DEFAULT_QUEUE_SIZE,
            is_playing: false,
            color_space,
            yuv_frame,
            scaler,
            frame_buffer,
        };

        decoder.pre_buffer_with_seek(None)?;

        Ok(decoder)
    }
    fn calculate_buffer_size(decoder: &ffmpeg::decoder::Video) -> usize {
        let width = decoder.width() as usize;
        let height = decoder.height() as usize;

        // For YUV420P:
        // Y plane: width * height
        // U and V planes: (width/2) * (height/2) each
        // Then U and V are interleaved, so we need:
        let y_size = width * height;
        let uv_size = width * height / 2; // This accounts for both U and V interleaved

        y_size + uv_size // Total size needed
    }

    fn get_video_stream(&self) -> Result<ffmpeg::Stream, VideoError> {
        self.format_context
            .streams()
            .best(ffmpeg::media::Type::Video)
            .ok_or(VideoError::StreamNotFound("Video stream not found"))
    }
    fn process_video_frame(&mut self, frame: &ffmpeg::frame::Video) -> Result<Vec<u8>, VideoError> {
        // Reuse existing objects instead of creating new ones
        self.scaler.run(frame, &mut self.yuv_frame)?;

        // Clear and reuse buffer
        self.frame_buffer.clear();
        self.frame_buffer.extend_from_slice(self.yuv_frame.data(0));

        // Process UV planes
        for (u, v) in self
            .yuv_frame
            .data(1)
            .iter()
            .zip(self.yuv_frame.data(2).iter())
        {
            self.frame_buffer.push(*u);
            self.frame_buffer.push(*v);
        }

        Ok(self.frame_buffer.clone()) // Only clone at the end
    }

    fn add_frame_to_queue(&mut self, frame: ffmpeg::frame::Video) -> Result<(), VideoError> {
        let combined = self.process_video_frame(&frame)?;

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
        Ok(())
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
    fn decode_next_frame(&mut self) -> Result<(), VideoError> {
        if self.presentation_queue.len() >= self.max_queue_size {
            return Ok(());
        }

        let mut packets_sent = 0;
        let mut frame = ffmpeg::frame::Video::empty();

        loop {
            if packets_sent >= MAX_PACKETS_PER_FRAME {
                return Err(VideoError::Decode(
                    "Too many packets sent without decoding frame".into(),
                ));
            }

            match self.decoder.receive_frame(&mut frame) {
                Ok(_) => {
                    tracing::info!(
                        "Decoded frame - PTS: {}, Best effort timestamp: {}",
                        frame.pts().unwrap_or(-1),
                        frame.timestamp().unwrap_or(-1),
                    );
                    self.add_frame_to_queue(frame)?;
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
        }
    }
    fn pre_buffer_with_seek(&mut self, target_ts: Option<i64>) -> Result<(), VideoError> {
        tracing::info!("Pre-buffering frames...");

        // If we have a target timestamp, we need to find that frame first
        if let Some(target_ts) = target_ts {
            let mut found_target = false;
            while !found_target {
                let mut frame = ffmpeg::frame::Video::empty();
                match self.decoder.receive_frame(&mut frame) {
                    Ok(_) => {
                        let pts = frame.pts().unwrap_or(-1);
                        if pts >= target_ts {
                            // Process frame and add to queue
                            let combined = self.process_video_frame(&frame)?;
                            self.presentation_queue.push_back(QueuedFrame {
                                data: combined,
                                frame_number: self.current_frame,
                            });
                            found_target = true;
                        }
                    }
                    Err(ffmpeg::Error::Other { errno: EAGAIN }) => {
                        if let Some((stream, packet)) = self.format_context.packets().next() {
                            if stream.index() == self.video_stream_index {
                                self.decoder.send_packet(&packet)?;
                            }
                        }
                    }
                    Err(e) => return Err(VideoError::Decode(e.to_string())),
                }
            }
        }

        // Now fill the rest of the buffer
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
    pub fn total_time(&self) -> Result<Duration, VideoError> {
        let video_stream = self.get_video_stream()?;

        let duration = video_stream.duration() as f64 * f64::from(video_stream.time_base());
        Ok(Duration::from_secs_f64(duration))
    }

    pub fn seek_to_time(&mut self, time_s: f64) -> Result<(), VideoError> {
        if time_s < 0.0 || time_s > self.total_time()?.as_secs_f64() {
            return Err(VideoError::InvalidTimestamp);
        }

        let stream = self.get_video_stream()?;

        let time_base = stream.time_base();
        let target_ts = (time_s * time_base.denominator() as f64) as i64;
        let fps = stream.avg_frame_rate();
        let stream_index = stream.index() as i32;

        self.presentation_queue.clear();
        self.decoder.flush();

        unsafe {
            ffmpeg::sys::avformat_seek_file(
                self.format_context.as_mut_ptr(),
                stream_index,
                i64::MIN,
                target_ts,
                target_ts,
                ffmpeg::sys::AVSEEK_FLAG_BACKWARD,
            )
        };

        self.current_frame = (time_s * fps.numerator() as f64 / fps.denominator() as f64) as u64;

        self.pre_buffer_with_seek(Some(target_ts))?;
        Ok(())
    }
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
    pub fn end_frame(&self) -> Result<u64, VideoError> {
        if let Some(value) = self.end_frame {
            Ok(value)
        } else {
            self.total_frames()
        }
    }

    pub fn looping(&self) -> bool {
        self.looping
    }
    pub fn total_frames(&self) -> Result<u64, VideoError> {
        let video_stream = self.get_video_stream()?;

        let duration = video_stream.duration() as f64 * f64::from(video_stream.time_base());
        Ok((duration * self.get_fps()).ceil() as u64)
    }

    pub fn get_fps(&self) -> f64 {
        let frame_rate = self
            .get_video_stream()
            .map(|stream| stream.rate())
            .unwrap_or((DEFAULT_FPS, 1).into()); // fallback to 30fps if we can't get the rate

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
impl std::fmt::Debug for VideoStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VideoStream")
            .field("current_frame", &self.current_frame)
            .field("is_playing", &self.is_playing)
            .field("queue_size", &self.presentation_queue.len())
            .finish()
    }
}

impl Drop for VideoStream {
    fn drop(&mut self) {
        let _ = self.decoder.send_packet(&ffmpeg::Packet::empty());
    }
}
