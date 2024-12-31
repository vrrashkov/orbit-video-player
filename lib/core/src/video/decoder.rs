use ffmpeg_next as ffmpeg;
use nebula_common::VideoError;
use std::time::{Duration, Instant};
pub struct VideoDecoder {
    pub decoder: ffmpeg::decoder::Video,
    format_context: ffmpeg::format::context::Input,
    video_stream_index: usize,
    last_frame_time: Instant,
    frame_duration: Duration,
    current_frame: u32,
    start_frame: u32,
    end_frame: u32,
}

impl VideoDecoder {
    pub async fn new(
        video_path: &str,
        start_frame: u32,
        end_frame: u32,
    ) -> Result<Self, VideoError> {
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
        let frame_duration = std::time::Duration::from_micros(
            (1_000_000 * frame_rate.1 as u64) / frame_rate.0 as u64,
        );

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

        Ok(Self {
            decoder,
            format_context,
            video_stream_index,
            frame_duration,
            last_frame_time: Instant::now(),
            current_frame: start_frame,
            start_frame,
            end_frame,
        })
    }

    pub fn should_process_frame(&mut self) -> bool {
        let now = Instant::now();
        if now.duration_since(self.last_frame_time) >= self.frame_duration {
            self.last_frame_time = now;
            true
        } else {
            false
        }
    }

    pub fn next_frame(&mut self) -> Result<Option<Vec<u8>>, VideoError> {
        if !self.should_process_frame() {
            return Ok(None);
        }

        if self.current_frame < self.start_frame {
            self.skip_frame()?;
            self.current_frame += 1;
            return Ok(None);
        }

        if self.current_frame > self.end_frame {
            return Ok(None);
        }

        match self.format_context.packets().next() {
            Some((stream, packet)) => {
                if stream.index() == self.video_stream_index {
                    self.decoder
                        .send_packet(&packet)
                        .map_err(|e| VideoError::Decode(format!("Failed to send packet: {}", e)))?;

                    let mut frame = ffmpeg::frame::Video::empty();
                    match self.decoder.receive_frame(&mut frame) {
                        Ok(_) => {
                            let mut rgb_frame = ffmpeg::frame::Video::empty();
                            let mut scaler = ffmpeg::software::scaling::Context::get(
                                self.decoder.format(),
                                self.decoder.width(),
                                self.decoder.height(),
                                ffmpeg::format::Pixel::RGBA,
                                self.decoder.width(),
                                self.decoder.height(),
                                ffmpeg::software::scaling::Flags::BILINEAR,
                            )
                            .map_err(|e| {
                                VideoError::FrameProcessing(format!(
                                    "Failed to create scaler: {}",
                                    e
                                ))
                            })?;

                            scaler.run(&frame, &mut rgb_frame).map_err(|e| {
                                VideoError::FrameProcessing(format!("Failed to scale frame: {}", e))
                            })?;

                            self.current_frame += 1;
                            Ok(Some(rgb_frame.data(0).to_vec()))
                        }
                        Err(e) => Err(VideoError::Decode(format!(
                            "Failed to receive frame: {}",
                            e
                        ))),
                    }
                } else {
                    Ok(None)
                }
            }
            None => Ok(None),
        }
    }

    // Add these helper methods for the renderer to know frame dimensions
    pub fn width(&self) -> u32 {
        self.decoder.width() as u32
    }

    pub fn height(&self) -> u32 {
        self.decoder.height() as u32
    }
    fn skip_frame(&mut self) -> Result<(), VideoError> {
        if let Some((stream, packet)) = self.format_context.packets().next() {
            if stream.index() == self.video_stream_index {
                self.decoder.send_packet(&packet).map_err(|e| {
                    VideoError::Decode(format!("Failed to send packet while skipping: {}", e))
                })?;
                let mut frame = ffmpeg::frame::Video::empty();
                self.decoder.receive_frame(&mut frame).map_err(|e| {
                    VideoError::Decode(format!("Failed to receive frame while skipping: {}", e))
                })?;
            }
        }
        Ok(())
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

    pub fn total_frames(&self) -> u32 {
        self.end_frame - self.start_frame
    }
}
