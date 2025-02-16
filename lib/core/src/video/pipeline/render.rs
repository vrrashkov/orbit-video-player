pub enum RenderCommand {
    DrawVideo {
        video_id: u64,
        clip: iced::Rectangle<u32>,
    },
    DrawEffect {
        effect_id: String,
        input: wgpu::TextureView,
        output: wgpu::TextureView,
    },
    DrawLine {
        position: f32,
        bounds: iced::Rectangle<u32>,
    },
}
