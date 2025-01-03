use ffmpeg_next::{self as ffmpeg, error::EAGAIN, ffi::AV_TIME_BASE};
use nebula_common::VideoError;
use std::{
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
        };

        decoder.pre_buffer()?;

        Ok(decoder)
    }
    pub fn next_frame(&mut self) -> Result<Option<Vec<u8>>, VideoError> {
        tracing::info!("Entering next_frame");

        // Fill the queue first (should happen once at start)
        if self.presentation_queue.is_empty() {
            tracing::info!("Initial buffer fill");
            while self.presentation_queue.len() < self.max_queue_size {
                self.decode_next_frame()?;
            }
        }

        // Only get next frame if it's time
        if self.should_process_frame() {
            tracing::info!("YES PROCESS NOW");
            // Try to decode one more frame to keep buffer full
            if self.presentation_queue.len() < self.max_queue_size {
                let _ = self.decode_next_frame()?;
            }

            // Return the next frame from queue
            return Ok(self.presentation_queue.pop_front().map(|f| f.data.clone()));
        }

        // If not time yet, return current frame
        Ok(self.presentation_queue.front().map(|f| f.data.clone()))
    }
    pub fn should_process_frame(&mut self) -> bool {
        let now = Instant::now();
        let elapsed = now.duration_since(self.frame_timer);

        // Calculate target duration based on FPS
        let target_duration = Duration::from_secs_f64(1.0 / self.get_fps());

        if elapsed >= target_duration {
            self.frame_timer = now;
            tracing::info!(
                "Time for new frame (elapsed: {:?}), target_duration: {:?}, self.get_fps: {:?}",
                elapsed,
                target_duration,
                self.get_fps()
            );
            true
        } else {
            tracing::debug!(
                "Not time yet (elapsed: {:?}), target_duration: {:?}, self.get_fps: {:?}",
                elapsed,
                target_duration,
                self.get_fps()
            );
            false
        }
    }
    pub fn get_last_frame(&self) -> Option<Vec<u8>> {
        self.presentation_queue.front().map(|f| f.data.clone())
    }
    fn decode_next_frame(&mut self) -> Result<(), VideoError> {
        if self.presentation_queue.len() >= self.max_queue_size {
            return Ok(());
        }

        let mut frame = ffmpeg::frame::Video::empty();
        let mut packets = self.format_context.packets();

        while let Some((stream, packet)) = packets.next() {
            if stream.index() != self.video_stream_index {
                continue;
            }

            self.decoder
                .send_packet(&packet)
                .map_err(|e| VideoError::Decode(format!("Failed to send packet: {}", e)))?;

            // tracing::info!("Processing packet, current frame: {}", self.current_frame);
            match self.decoder.receive_frame(&mut frame) {
                Ok(_) => {
                    // tracing::info!("Received frame {}", self.current_frame);
                    tracing::info!(
                        "Frame PTS: {:?}, current_frame: {}",
                        frame.pts(),
                        self.current_frame
                    );
                    let mut yuv_frame = ffmpeg::frame::Video::empty();
                    let mut scaler = ffmpeg::software::scaling::Context::get(
                        self.decoder.format(),
                        self.decoder.width(),
                        self.decoder.height(),
                        ffmpeg::format::Pixel::YUV420P,
                        self.decoder.width(),
                        self.decoder.height(),
                        ffmpeg::software::scaling::Flags::BILINEAR,
                    )
                    .map_err(|e| {
                        VideoError::FrameProcessing(format!("Failed to create scaler: {}", e))
                    })?;
                    scaler.run(&frame, &mut yuv_frame).map_err(|e| {
                        VideoError::FrameProcessing(format!("Failed to scale frame: {}", e))
                    })?;

                    if let Some(pts) = frame.pts() {
                        // Add logging/validation for YUV data
                        tracing::info!("Y plane size: {}", yuv_frame.data(0).len());
                    }
                    let combined = [
                        yuv_frame.data(0).to_vec(),
                        yuv_frame.data(1).to_vec(),
                        yuv_frame.data(2).to_vec(),
                    ]
                    .concat();
                    self.presentation_queue.push_back(QueuedFrame {
                        data: combined,
                        frame_number: self.current_frame,
                    });
                    tracing::info!(
                        "Added frame {} to queue (queue size: {})",
                        self.current_frame,
                        self.presentation_queue.len()
                    );
                    self.current_frame += 1;
                    return Ok(());
                }
                Err(ffmpeg::Error::Other { errno: EAGAIN }) => continue,
                Err(e) => return Err(VideoError::Decode(e.to_string())),
            }
        }
        Ok(())
    }

    pub fn seek_to_frame(&mut self, time_s: f64, c_frame: u64) -> Result<(), VideoError> {
        // Clear state first
        self.presentation_queue.clear();

        let timestamp = (time_s * AV_TIME_BASE as f64) as i64;
        tracing::warn!("seek for: {}", timestamp);
        self.format_context.seek(timestamp, timestamp..)?;

        self.current_frame = c_frame;
        self.pre_buffer()?;

        Ok(())
    }
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
        let current_second = (self.current_frame() - self.start_frame) as f64 / self.get_fps();
        Duration::from_secs_f64(current_second)
    }

    pub fn total_time(&self) -> Duration {
        let total_seconds = (self.total_frames() - self.start_frame) as f64 / self.get_fps();
        Duration::from_secs_f64(total_seconds)
    }
    pub fn seek_to_time(&mut self, seconds: f64) -> Result<(), VideoError> {
        tracing::info!("seek_to_time: {}", seconds);
        let frame = (seconds * self.get_fps()) as u64;
        self.seek_to_frame(seconds, frame)
    }
    pub fn current_frame(&self) -> u64 {
        self.presentation_queue
            .front()
            .map(|f| f.frame_number.clone())
            .unwrap_or(1)
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

        video_stream.frames() as u64
    }
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
        if self.is_playing {
            self.next_frame()
        } else {
            Ok(self.get_last_frame())
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
