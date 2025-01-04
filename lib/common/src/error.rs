use thiserror::Error;
#[derive(Error, Debug)]
pub enum VideoError {
    #[error("FFmpeg error: {0}")]
    FFmpeg(#[from] ffmpeg_next::Error),

    #[error("Seek error: {0}")]
    Seek(String),

    #[error("Frame decode error: {0}")]
    Decode(String),

    #[error("Frame processing error: {0}")]
    FrameProcessing(String),

    #[error("GPU error: {0}")]
    Gpu(#[from] wgpu::Error),

    #[error("Invalid window size")]
    InvalidWindowSize,

    #[error("Invalid timestamp")]
    InvalidTimestamp,

    #[error("Stream not found: {0}")]
    StreamNotFound(&'static str),
}
