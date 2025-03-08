use ffmpeg_next::{
    self as ffmpeg,
    color::Space,
    error::EAGAIN,
    ffi::{
        av_seek_frame, AVColorPrimaries, AVColorRange, AVColorSpace, AVColorTransferCharacteristic,
        AVMediaType, AVSEEK_FLAG_ANY, AVSEEK_FLAG_FRAME, AV_TIME_BASE,
    },
};

use nebula_common::VideoError;
use std::{
    borrow::Borrow,
    collections::VecDeque,
    time::{Duration, Instant},
};
use tracing::{debug, error, info, trace, warn};

/// A decoded video frame ready for display
pub struct QueuedFrame {
    pub data: Vec<u8>,     // YUV data in planar format
    pub frame_number: u64, // Sequential frame number
}

/// Video stream decoder that handles reading, buffering, and playback control
pub struct VideoStream {
    pub decoder: ffmpeg::decoder::Video,
    format_context: ffmpeg::format::context::Input,
    video_stream_index: usize,
    current_frame: u64,
    start_frame: u64,
    end_frame: Option<u64>,
    looping: bool,
    presentation_queue: VecDeque<QueuedFrame>,
    max_queue_size: usize,
    frame_timer: Instant,
    pub is_playing: bool,
    pub color_space: Space,
    frame_buffer: Vec<u8>,           // Buffer for processing frames
    yuv_frame: ffmpeg::frame::Video, // Reusable frame object
    scaler: ffmpeg::software::scaling::Context,
}

/// Options for creating a new video stream
pub struct VideoStreamOptions<'a> {
    pub video_path: &'a str,
    pub start_frame: u64,
    pub end_frame: Option<u64>,
}

// Constants
const DEFAULT_FPS: i32 = 30;
const DEFAULT_QUEUE_SIZE: usize = 10;
const MAX_PACKETS_PER_FRAME: usize = 100;

impl VideoStream {
    /// Create a new video stream from the specified path and options
    pub fn new(options: VideoStreamOptions) -> Result<Self, VideoError> {
        // Initialize FFmpeg
        ffmpeg::init()?;

        info!("Loading video from: {}", options.video_path);
        let mut format_context = ffmpeg::format::input(&options.video_path)?;

        // Find the best video stream
        let video_stream = format_context
            .streams()
            .best(ffmpeg::media::Type::Video)
            .ok_or(VideoError::StreamNotFound("Video stream not found"))?;

        // Extract frame rate information
        let frame_rate = video_stream.rate();
        info!("Frame rate: {}/{} fps", frame_rate.0, frame_rate.1);
        let fps = frame_rate.0 as f64 / frame_rate.1 as f64;
        let frame_duration = std::time::Duration::from_secs_f64(1.0 / fps);
        debug!("Frame duration: {:?}", frame_duration);

        // Get stream details
        let video_stream_index = video_stream.index();
        let parameters = video_stream.parameters();

        // Seek to start frame
        let time_s = ((options.start_frame - 1) as f64 / fps) as f64;
        let timestamp = (time_s * AV_TIME_BASE as f64) as i64;
        debug!(
            "Seeking to timestamp: {:?} (start frame: {})",
            timestamp, options.start_frame
        );
        format_context.seek(timestamp, timestamp..)?;

        // Set up decoder
        let context = ffmpeg::codec::Context::from_parameters(parameters)?;
        let mut decoder = context.decoder().video()?;

        // Get color space information
        let color_space = decoder.color_space();
        debug!("Detected color space: {:?}", color_space);

        // Log detailed input format information
        info!(
            "Input format: {:?}, Color space: {:?}, Color range: {:?}, Color primaries: {:?}, Color TRC: {:?}",
            decoder.format(),
            decoder.color_space(),
            decoder.color_range(),
            decoder.color_primaries(),
            decoder.color_transfer_characteristic()
        );

        // Initialize timing and frame objects
        let now = Instant::now();
        let yuv_frame = ffmpeg::frame::Video::empty();

        // Create scaler for pixel format conversion
        let scaler = ffmpeg::software::scaling::Context::get(
            decoder.format(),
            decoder.width(),
            decoder.height(),
            ffmpeg::format::Pixel::YUV420P,
            decoder.width(),
            decoder.height(),
            ffmpeg::software::scaling::Flags::BITEXACT |    // Ensure exact conversion
            ffmpeg::software::scaling::Flags::ACCURATE_RND, // Use accurate rounding
        )?;

        // Set color space properties for accurate color reproduction
        unsafe {
            (*decoder.as_mut_ptr()).colorspace = AVColorSpace::AVCOL_SPC_BT709;
            (*decoder.as_mut_ptr()).color_primaries = AVColorPrimaries::AVCOL_PRI_BT709;
            (*decoder.as_mut_ptr()).color_trc = AVColorTransferCharacteristic::AVCOL_TRC_BT709;
            (*decoder.as_mut_ptr()).color_range = AVColorRange::AVCOL_RANGE_MPEG;
        }

        // Create output buffer with appropriate capacity
        let frame_buffer = Vec::with_capacity(Self::calculate_buffer_size(&decoder));

        // Initialize the video stream object
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
            is_playing: true,
            color_space,
            yuv_frame,
            scaler,
            frame_buffer,
        };

        // Pre-buffer frames to fill the queue
        decoder.pre_buffer_with_seek(None)?;
        info!("Video stream initialized successfully");

        Ok(decoder)
    }

    /// Calculate the required buffer size for a frame in YUV420P format
    fn calculate_buffer_size(decoder: &ffmpeg::decoder::Video) -> usize {
        let width = decoder.width() as usize;
        let height = decoder.height() as usize;

        // For YUV420P:
        // Y plane: width * height
        // U and V planes: (width/2) * (height/2) each, interleaved
        let y_size = width * height;
        let uv_size = width * height / 2; // This accounts for both U and V interleaved

        y_size + uv_size // Total size needed
    }

    /// Get the video stream from the format context
    fn get_video_stream(&self) -> Result<ffmpeg::Stream, VideoError> {
        self.format_context
            .streams()
            .best(ffmpeg::media::Type::Video)
            .ok_or(VideoError::StreamNotFound("Video stream not found"))
    }

    /// Process a decoded frame into planar YUV420 format
    fn process_video_frame(&mut self, frame: &ffmpeg::frame::Video) -> Result<Vec<u8>, VideoError> {
        self.frame_buffer.clear();

        // Preserve color properties in output frame
        unsafe {
            (*self.yuv_frame.as_mut_ptr()).colorspace = AVColorSpace::AVCOL_SPC_BT709;
            (*self.yuv_frame.as_mut_ptr()).color_primaries = AVColorPrimaries::AVCOL_PRI_BT709;
            (*self.yuv_frame.as_mut_ptr()).color_trc =
                AVColorTransferCharacteristic::AVCOL_TRC_BT709;
            (*self.yuv_frame.as_mut_ptr()).color_range = AVColorRange::AVCOL_RANGE_MPEG;
        }

        // Convert frame format if needed
        self.scaler.run(frame, &mut self.yuv_frame)?;

        // Copy Y plane directly (full resolution)
        let y_plane = self.yuv_frame.data(0);
        self.frame_buffer.extend_from_slice(y_plane);

        // Interleave U and V planes (half resolution)
        let width = self.decoder.width() as usize;
        let height = self.decoder.height() as usize;
        let uv_width = width / 2;
        let uv_height = height / 2;

        for y in 0..uv_height {
            let u_line = &self.yuv_frame.data(1)[y * uv_width..(y + 1) * uv_width];
            let v_line = &self.yuv_frame.data(2)[y * uv_width..(y + 1) * uv_width];

            for x in 0..uv_width {
                self.frame_buffer.push(u_line[x]);
                self.frame_buffer.push(v_line[x]);
            }
        }

        trace!(
            "Processed frame with size: {} bytes",
            self.frame_buffer.len()
        );
        Ok(self.frame_buffer.clone())
    }

    /// Add a decoded frame to the presentation queue
    fn add_frame_to_queue(&mut self, frame: ffmpeg::frame::Video) -> Result<(), VideoError> {
        let combined = self.process_video_frame(&frame)?;

        debug!(
            "Adding frame {} to queue (queue size: {}/{})",
            self.current_frame,
            self.presentation_queue.len(),
            self.max_queue_size
        );

        self.presentation_queue.push_back(QueuedFrame {
            data: combined,
            frame_number: self.current_frame,
        });

        self.current_frame += 1;
        Ok(())
    }

    /// Get the next frame from the queue or decode if needed
    pub fn next_frame(&mut self) -> Result<Option<Vec<u8>>, VideoError> {
        trace!("Retrieving next frame");

        // Fill the queue if empty
        if self.presentation_queue.is_empty() {
            debug!("Frame queue empty, filling buffer");
            while self.presentation_queue.len() < self.max_queue_size {
                self.decode_next_frame()?;
            }
        }

        // If we have frames and it's time to show the next one
        if !self.presentation_queue.is_empty() && self.should_process_frame() {
            debug!(
                "Processing frame {} from queue (queue size: {}/{})",
                self.current_frame,
                self.presentation_queue.len(),
                self.max_queue_size
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

    /// Determine if it's time to process the next frame based on timing
    pub fn should_process_frame(&mut self) -> bool {
        let now = Instant::now();
        let elapsed = now.duration_since(self.frame_timer);
        let frame_duration = Duration::from_secs_f64(1.0 / self.get_fps());

        if elapsed >= frame_duration {
            // Update timer by exact frame duration to prevent drift
            let frames_to_advance = elapsed.div_duration_f64(frame_duration);
            self.frame_timer += frame_duration.mul_f64(frames_to_advance.floor());

            trace!(
                "Time to process frame: elapsed={:?}, frame_duration={:?}, frames_advanced={}",
                elapsed,
                frame_duration,
                frames_to_advance
            );
            true
        } else {
            false
        }
    }

    /// Get the oldest frame in the queue without removing it
    pub fn get_last_frame(&self) -> Option<Vec<u8>> {
        if let Some(frame) = self.presentation_queue.front() {
            trace!(
                "Returning frame {} from queue (queue size: {}/{})",
                frame.frame_number,
                self.presentation_queue.len(),
                self.max_queue_size
            );
            Some(frame.data.clone())
        } else {
            warn!("No frames in queue to return");
            None
        }
    }

    /// Get the current frame for display without advancing
    fn get_current_frame(&self) -> Option<Vec<u8>> {
        static mut LAST_FRAME_NUMBER: u64 = 0;

        if let Some(frame) = self.presentation_queue.front() {
            // Only log if frame number changed (to reduce log spam)
            unsafe {
                if LAST_FRAME_NUMBER != frame.frame_number {
                    trace!(
                        "Current frame: {} (previous: {})",
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

    /// Decode the next frame from the video stream
    fn decode_next_frame(&mut self) -> Result<(), VideoError> {
        // Skip if queue is already full
        if self.presentation_queue.len() >= self.max_queue_size {
            return Ok(());
        }

        let mut packets_sent = 0;
        let mut frame = ffmpeg::frame::Video::empty();

        loop {
            // Prevent infinite loop by limiting packet processing
            if packets_sent >= MAX_PACKETS_PER_FRAME {
                return Err(VideoError::Decode(
                    "Too many packets sent without decoding frame".into(),
                ));
            }

            match self.decoder.receive_frame(&mut frame) {
                Ok(_) => {
                    trace!(
                        "Decoded frame - PTS: {}, Timestamp: {}",
                        frame.pts().unwrap_or(-1),
                        frame.timestamp().unwrap_or(-1),
                    );
                    self.add_frame_to_queue(frame)?;
                    return Ok(());
                }
                Err(ffmpeg::Error::Other { errno: EAGAIN }) => {
                    // Need more input data
                    if let Some((stream, packet)) = self.format_context.packets().next() {
                        if stream.index() == self.video_stream_index {
                            self.decoder.send_packet(&packet)?;
                            packets_sent += 1;
                        }
                    } else {
                        // End of stream, flush decoder
                        self.decoder.send_packet(&ffmpeg::Packet::empty())?;
                        return Ok(());
                    }
                }
                Err(e) => return Err(VideoError::Decode(e.to_string())),
            }
        }
    }

    /// Pre-buffer frames starting from current position or a target timestamp
    fn pre_buffer_with_seek(&mut self, target_ts: Option<i64>) -> Result<(), VideoError> {
        debug!("Pre-buffering frames...");

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
                            debug!("Found target frame at PTS: {}", pts);
                        }
                    }
                    Err(ffmpeg::Error::Other { errno: EAGAIN }) => {
                        match self.format_context.packets().next() {
                            Some((stream, packet)) if stream.index() == self.video_stream_index => {
                                self.decoder.send_packet(&packet)?;
                            }
                            None => {
                                // End of file reached during seek
                                if !found_target {
                                    debug!(
                                        "Reached end of file during seek without finding target"
                                    );
                                    // If we haven't found our target frame, it means we're seeking past the end
                                    return Ok(());
                                }
                                break;
                            }
                            _ => continue,
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

        info!("Pre-buffered {} frames", self.presentation_queue.len());
        Ok(())
    }

    /// Get the width of the video in pixels
    pub fn width(&self) -> u32 {
        self.decoder.width() as u32
    }

    /// Get the height of the video in pixels
    pub fn height(&self) -> u32 {
        self.decoder.height() as u32
    }

    /// Get the current playback time in seconds
    pub fn current_time(&self) -> Duration {
        if self.current_frame() > 0 {
            let current_second = (self.current_frame() - self.start_frame) as f64 / self.get_fps();
            Duration::from_secs_f64(current_second)
        } else {
            Duration::from_secs_f64(0.)
        }
    }

    /// Get the total duration of the video
    pub fn total_time(&self) -> Result<Duration, VideoError> {
        let video_stream = self.get_video_stream()?;
        let raw_duration = video_stream.duration();
        let time_base = video_stream.time_base();

        // Precise calculation using time base
        let seconds =
            (raw_duration * time_base.numerator() as i64) as f64 / time_base.denominator() as f64;

        Ok(Duration::from_secs_f64(seconds))
    }

    /// Seek to a specific time in seconds
    pub fn seek_to_time(&mut self, time_s: f64) -> Result<(), VideoError> {
        let total_time = self.total_time()?.as_secs_f64();

        if time_s < 0.0 || time_s > total_time {
            warn!("Invalid seek time: {} (total time: {})", time_s, total_time);
            return Err(VideoError::InvalidTimestamp);
        }

        info!("Seeking to time: {:.2}s", time_s);
        let stream = self.get_video_stream()?;

        let time_base = stream.time_base();
        let target_ts = (time_s * time_base.denominator() as f64) as i64;
        let fps = stream.avg_frame_rate();
        let stream_index = stream.index() as i32;

        // Clear queue and flush decoder
        self.presentation_queue.clear();
        self.decoder.flush();

        // Perform the seek operation
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

        // Update current frame based on time
        self.current_frame = (time_s * fps.numerator() as f64 / fps.denominator() as f64) as u64;
        debug!("New current frame after seek: {}", self.current_frame);

        // Refill buffer
        self.pre_buffer_with_seek(Some(target_ts))?;
        Ok(())
    }

    /// Get the current frame number
    pub fn current_frame(&self) -> u64 {
        // Always use the first frame in queue if available
        self.presentation_queue
            .front()
            .map(|f| f.frame_number)
            .unwrap_or(self.current_frame)
    }

    /// Get the starting frame number
    pub fn start_frame(&self) -> u64 {
        self.start_frame
    }

    /// Get the ending frame number (if specified, otherwise total frames)
    pub fn end_frame(&self) -> Result<u64, VideoError> {
        if let Some(value) = self.end_frame {
            Ok(value)
        } else {
            self.total_frames()
        }
    }

    /// Check if looping playback is enabled
    pub fn looping(&self) -> bool {
        self.looping
    }

    /// Get the total number of frames in the video
    pub fn total_frames(&self) -> Result<u64, VideoError> {
        let video_stream = self.get_video_stream()?;
        let duration = video_stream.duration() as f64 * f64::from(video_stream.time_base());
        Ok((duration * self.get_fps()).ceil() as u64)
    }

    /// Get the frames per second of the video
    pub fn get_fps(&self) -> f64 {
        let frame_rate = self
            .get_video_stream()
            .map(|stream| stream.rate())
            .unwrap_or((DEFAULT_FPS, 1).into()); // fallback to 30fps if we can't get the rate

        frame_rate.0 as f64 / frame_rate.1 as f64
    }

    /// Start playing the video
    pub fn play(&mut self) {
        debug!("Video playback started");
        self.is_playing = true;
    }

    /// Pause the video playback
    pub fn pause(&mut self) {
        debug!("Video playback paused");
        self.is_playing = false;
    }

    /// Check if video is currently playing
    pub fn is_playing(&self) -> bool {
        self.is_playing
    }

    /// Update the video state and get the current frame
    pub fn update(&mut self) -> Result<Option<Vec<u8>>, VideoError> {
        // Only get a new frame if we're playing and it's time
        if self.is_playing {
            self.next_frame()
        } else {
            // When paused, return current frame without advancing
            Ok(self.get_current_frame())
        }
    }

    /// Get the duration of a single frame
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
        debug!("Dropping VideoStream and flushing decoder");
        let _ = self.decoder.send_packet(&ffmpeg::Packet::empty());
    }
}
