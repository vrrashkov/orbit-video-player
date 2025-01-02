use ffmpeg_next::{self as ffmpeg, error::EAGAIN};
use nebula_common::VideoError;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};
pub struct VideoDecoder {
    pub decoder: ffmpeg::decoder::Video,
    format_context: ffmpeg::format::context::Input,
    front_buffer: Option<Vec<u8>>,
    back_buffer: Option<Vec<u8>>,
    video_stream_index: usize,
    last_frame_time: Instant,
    frame_duration: Duration,
    accumulated_time: Duration,
    current_frame: u32,
    start_frame: u32,
    end_frame: u32,
    looping: bool,
    frame_buffer: Vec<Vec<u8>>,
    max_buffer_size: usize,
    presentation_queue: VecDeque<Vec<u8>>,
    max_queue_size: usize, // e.g., 5-10 frames
    frame_timer: Instant,
    pub is_playing: bool,
}

impl VideoDecoder {
    pub fn new(video_path: &str, start_frame: u32, end_frame: u32) -> Result<Self, VideoError> {
        // 3. Initialize FFmpeg
        ffmpeg::init()
            .map_err(|e| VideoError::FFmpeg(format!("Failed to initialize FFmpeg: {}", e)))?;

        tracing::info!("Loading video from: {}", video_path);
        let mut format_context = ffmpeg::format::input(&video_path)
            .map_err(|e| VideoError::FFmpeg(format!("Failed to open video file: {}", e)))?;

        let video_stream = format_context
            .streams()
            .best(ffmpeg::media::Type::Video)
            .ok_or_else(|| VideoError::FFmpeg("No video stream found".into()))?;

        let frame_rate = video_stream.rate();
        tracing::info!("Frame rate: {}/{} fps", frame_rate.0, frame_rate.1);
        let fps = frame_rate.0 as f64 / frame_rate.1 as f64;
        let frame_duration = std::time::Duration::from_secs_f64(1.0 / fps);
        tracing::info!("Frame duration: {:?}", frame_duration);

        let time_base = video_stream.time_base();
        let video_stream_index = video_stream.index();
        let parameters = video_stream.parameters();

        let timestamp = (start_frame as i64 * time_base.denominator() as i64)
            / (time_base.numerator() as i64 * frame_rate.0 as i64);

        format_context
            .seek(timestamp, timestamp..timestamp + 1)
            .map_err(|e| {
                VideoError::SeekError(format!("Failed to seek to frame {}: {}", start_frame, e))
            })?;

        let context = ffmpeg::codec::Context::from_parameters(parameters)
            .map_err(|e| VideoError::FFmpeg(format!("Failed to create codec context: {}", e)))?;

        let decoder = context
            .decoder()
            .video()
            .map_err(|e| VideoError::FFmpeg(format!("Failed to create video decoder: {}", e)))?;
        // let (decode_tx, decode_rx) = channel();
        // let (frame_tx, frame_rx) = channel();

        // // Start decode thread
        // std::thread::spawn(move || {
        //     while let Ok(()) = decode_rx.recv() {
        //         if let Ok(Some(frame)) = self.decode_next_frame() {
        //             let _ = frame_tx.send(frame);
        //         }
        //     }
        // });

        let now = Instant::now();
        let mut decoder = Self {
            decoder,
            format_context,
            video_stream_index,
            frame_duration,
            last_frame_time: now,
            current_frame: start_frame,
            frame_timer: now,
            start_frame,
            end_frame,
            front_buffer: None,
            back_buffer: None,
            accumulated_time: Duration::ZERO,
            looping: false,
            frame_buffer: Vec::new(),
            presentation_queue: VecDeque::new(),
            max_queue_size: 10,
            max_buffer_size: 5, // Adjust this value as needed
            is_playing: false,
        };

        // Pre-buffer frames
        decoder.pre_buffer()?;

        Ok(decoder)
    }
    pub fn next_frame(&mut self) -> Result<Option<Vec<u8>>, VideoError> {
        tracing::info!("Entering next_frame");

        // Fill the queue first (should happen once at start)
        if self.presentation_queue.is_empty() {
            tracing::info!("Initial buffer fill");
            while self.presentation_queue.len() < self.max_queue_size {
                match self.decode_next_frame()? {
                    Some(_) => continue,
                    None => break,
                }
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
            return Ok(self.presentation_queue.pop_front());
        }

        // If not time yet, return current frame
        Ok(self.presentation_queue.front().cloned())
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
        self.frame_buffer.last().cloned()
    }
    fn decode_next_frame(&mut self) -> Result<Option<Vec<u8>>, VideoError> {
        tracing::info!("Starting frame decode");
        if self.presentation_queue.len() >= self.max_queue_size {
            return Ok(None);
        }

        // if self.current_frame < self.start_frame {
        //     self.skip_frame()?;
        //     self.current_frame += 1;
        //     return Ok(None);
        // }

        if self.current_frame > self.end_frame {
            return Ok(None);
        }

        let mut frame_received = false;
        let mut frame = ffmpeg::frame::Video::empty();

        // Try to receive a frame first (in case there are buffered frames)
        match self.decoder.receive_frame(&mut frame) {
            Ok(_) => {
                frame_received = true;
            }
            Err(ffmpeg::Error::Other { errno: EAGAIN }) => {
                // Need to send more packets
            }
            Err(e) => {
                return Err(VideoError::Decode(format!(
                    "Failed to receive frame: {}",
                    e
                )))
            }
        }

        // If no frame received, try to send more packets and receive again
        if !frame_received {
            while let Some((stream, packet)) = self.format_context.packets().next() {
                if stream.index() != self.video_stream_index {
                    continue;
                }

                self.decoder
                    .send_packet(&packet)
                    .map_err(|e| VideoError::Decode(format!("Failed to send packet: {}", e)))?;

                match self.decoder.receive_frame(&mut frame) {
                    Ok(_) => {
                        frame_received = true;
                        break;
                    }
                    Err(ffmpeg::Error::Other { errno: EAGAIN }) => continue,
                    Err(e) => {
                        return Err(VideoError::Decode(format!(
                            "Failed to receive frame: {}",
                            e
                        )))
                    }
                }
            }
        }

        if frame_received {
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
            .map_err(|e| VideoError::FrameProcessing(format!("Failed to create scaler: {}", e)))?;

            scaler.run(&frame, &mut yuv_frame).map_err(|e| {
                VideoError::FrameProcessing(format!("Failed to scale frame: {}", e))
            })?;

            // Get and combine planes
            let y_plane = yuv_frame.data(0).to_vec();
            let u_plane = yuv_frame.data(1).to_vec();
            let v_plane = yuv_frame.data(2).to_vec();

            let mut combined = Vec::new();
            combined.extend_from_slice(&y_plane);
            combined.extend_from_slice(&u_plane);
            combined.extend_from_slice(&v_plane);

            self.presentation_queue.push_back(combined);
            self.current_frame += 1;
            tracing::info!(
                "Successfully decoded frame {} into queue",
                self.current_frame
            );
            Ok(Some(vec![])) // Just indicate success
        } else {
            tracing::info!("No more frames available");
            Ok(None)
        }
    }
    pub fn pre_buffer(&mut self) -> Result<(), VideoError> {
        tracing::info!("Pre-buffering frames...");
        while self.presentation_queue.len() < self.max_queue_size {
            match self.decode_next_frame()? {
                Some(_) => continue,
                None => break,
            }
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
    pub fn seek_to_frame(&mut self, frame: u32) -> Result<(), VideoError> {
        let video_stream = self
            .format_context
            .streams()
            .best(ffmpeg::media::Type::Video)
            .ok_or_else(|| VideoError::FFmpeg("No video stream found".into()))?;

        let time_base = video_stream.time_base();
        let frame_rate = video_stream.rate();

        let timestamp = (frame as i64 * time_base.denominator() as i64)
            / (time_base.numerator() as i64 * frame_rate.0 as i64);

        self.format_context
            .seek(timestamp, timestamp..timestamp + 1)
            .map_err(|e| {
                VideoError::SeekError(format!("Failed to seek to frame {}: {}", frame, e))
            })?;

        self.current_frame = frame;
        Ok(())
    }
    pub fn current_frame(&self) -> u32 {
        self.current_frame
    }

    pub fn start_frame(&self) -> u32 {
        self.start_frame
    }
    pub fn end_frame(&self) -> u32 {
        self.end_frame
    }

    pub fn looping(&self) -> bool {
        self.looping
    }

    pub fn total_frames(&self) -> u32 {
        self.end_frame - self.start_frame
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
        // Return the frame duration based on your video's FPS
        let fps = self.get_fps();
        Duration::from_secs_f64(1.0 / fps)
    }
}
impl Drop for VideoDecoder {
    fn drop(&mut self) {
        let _ = self.decoder.send_packet(&ffmpeg::Packet::empty());
    }
}
