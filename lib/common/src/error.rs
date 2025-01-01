use thiserror::Error;

#[derive(Error, Debug)]
pub enum VideoError {
    #[error("Iced error: {0}")]
    Iced(#[from] iced::Error),

    #[error("Anyhow error: {0}")]
    Anyhow(#[from] anyhow::Error),

    #[error("FFmpeg error: {0}")]
    FFmpeg(String),

    #[error("Failed to initialize video: {0}")]
    Initialization(String),

    #[error("Failed to decode frame: {0}")]
    Decode(String),

    #[error("GPU error: {0}")]
    Gpu(#[from] wgpu::Error),

    #[error("Failed to find GPU adapter: {0}")]
    AdapterNotFound(String),

    #[error("Failed to find GPU surface: {0}")]
    SurfaceNotFound(String),

    #[error("Failed to create GPU adapter: {0}")]
    FailedToCreateDevice(String),

    #[error("Failed to create GPU device: {0}")]
    DeviceCreation(#[from] wgpu::RequestDeviceError),

    #[error("Surface error: {0}")]
    Surface(#[from] wgpu::SurfaceError),

    #[error("CreateSurfaceError error: {0}")]
    CreateSurfaceError(#[from] wgpu::CreateSurfaceError),

    #[error("Invalid seek position: {0}")]
    SeekError(String),

    #[error("Frame processing error: {0}")]
    FrameProcessing(String),

    #[error("Invalid window size")]
    InvalidWindowSize,
}
