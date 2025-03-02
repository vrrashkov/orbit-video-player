use iced_wgpu::wgpu;
use std::collections::HashMap;

pub mod effects;
pub mod manager;
pub mod render;
pub mod state;
pub mod video;

pub struct PipelineConfig {
    pub format: wgpu::TextureFormat,
    pub sample_count: u32,
    pub blend_state: Option<wgpu::BlendState>,
    pub primitive_state: wgpu::PrimitiveState,
}
impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            format: wgpu::TextureFormat::Bgra8UnormSrgb,
            sample_count: 1,
            blend_state: None,
            primitive_state: wgpu::PrimitiveState::default(),
        }
    }
}

pub trait Pipeline {
    fn create_pipeline(
        device: &wgpu::Device,
        config: &PipelineConfig,
    ) -> (wgpu::RenderPipeline, wgpu::BindGroupLayout);

    fn create_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        resources: &PipelineResources,
    ) -> wgpu::BindGroup;
}

pub struct PipelineResources {
    pub textures: HashMap<String, wgpu::Texture>,
    pub buffers: HashMap<String, wgpu::Buffer>,
    pub samplers: HashMap<String, wgpu::Sampler>,
}
