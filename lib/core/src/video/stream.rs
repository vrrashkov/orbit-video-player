use nebula_common::VideoError;
use std::{
    collections::VecDeque,
    path::PathBuf,
    time::{Duration, Instant},
};
use video_rs::{decode::Decoder, Time};
pub struct QueuedFrame {
    pub data: Vec<u8>,
    pub time: Time,
}

pub struct VideoStream {
    pub decoder: Decoder,
    // current_frame: u64,
    start_frame: u64,
    end_frame: Option<u64>,
    looping: bool,
    presentation_queue: VecDeque<QueuedFrame>,
    max_queue_size: usize,
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
        let decoder = Decoder::new(options.video_path.parse::<PathBuf>().unwrap()).unwrap();
        let now = Instant::now();

        let mut stream = Self {
            decoder,
            // current_frame: options.start_frame,
            start_frame: options.start_frame,
            end_frame: options.end_frame,
            looping: false,
            presentation_queue: VecDeque::new(),
            max_queue_size: 10,
            frame_timer: now,
            is_playing: false,
        };

        stream.pre_buffer()?;
        Ok(stream)
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

        if let Ok((time, frame)) = self.decoder.decode() {
            self.presentation_queue.push_back(QueuedFrame {
                data: frame.as_slice().map(|v| v.to_vec()).unwrap_or(vec![]),
                time,
            });
        }

        Ok(())
    }

    pub fn seek_to_frame(&mut self, seconds: f64, frame: u64) -> Result<(), VideoError> {
        self.presentation_queue.clear();
        tracing::warn!("seek to frame: {}", frame);
        self.decoder.seek_to_frame(frame as i64).unwrap();
        // self.current_frame = frame;
        self.pre_buffer()?;
        Ok(())
    }
    pub fn pre_buffer(&mut self) -> Result<(), VideoError> {
        tracing::info!("Pre-buffering frames...");
        self.decode_next_frame()?;
        tracing::info!("Pre-buffered {} frames", self.presentation_queue.len());
        Ok(())
    }
    // Add these helper methods for the renderer to know frame dimensions
    pub fn width(&self) -> u32 {
        self.decoder.size().0
    }

    pub fn height(&self) -> u32 {
        self.decoder.size().1
    }

    pub fn total_duration(&self) -> Duration {
        let total_seconds = (self.total_frames() - self.start_frame) as f64 / self.get_fps();
        Duration::from_secs_f64(total_seconds)
    }
    pub fn seek_to_time(&mut self, seconds: f64) -> Result<(), VideoError> {
        tracing::info!("seek_to_time: {}", seconds);
        let frame = (seconds * self.get_fps()) as u64;
        self.seek_to_frame(seconds, frame)
    }
    pub fn current_time(&self) -> Time {
        self.presentation_queue
            .front()
            .map(|f| f.time.clone())
            .unwrap_or(Time::zero())
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
        self.decoder.frames().unwrap_or(0)
    }
    pub fn get_fps(&self) -> f64 {
        self.decoder.frame_rate() as f64
    }
    pub fn total_time(&self) -> f64 {
        self.decoder
            .duration()
            .unwrap_or(Time::zero())
            .as_secs_f64()
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
