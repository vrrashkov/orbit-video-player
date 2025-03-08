use iced_wgpu::wgpu;
use std::{
    collections::{btree_map::Entry, BTreeMap},
    num::NonZero,
    sync::atomic::Ordering,
};

use crate::video::{color_space::BT709_CONFIG, render_passes::RenderPasses};

use super::{manager::VideoEntry, state::PipelineState, PipelineConfig};
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Uniforms {
    pub rect: [f32; 4],
    pub color_space: [u32; 1],
    pub y_range: [f32; 2],     // min, max for Y
    pub uv_range: [f32; 2],    // min, max for UV
    pub matrix: [[f32; 3]; 3], // Color conversion matrix
    pub _pad: [u8; 188],       // Adjusted padding to maintain size
}
pub struct VideoPipeline {
    pipeline: wgpu::RenderPipeline,
    bg0_layout: wgpu::BindGroupLayout,
    config: PipelineConfig,
    sampler: wgpu::Sampler,
}

impl VideoPipeline {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("video_shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../../../assets/shaders/video.wgsl").into(),
            ),
        });

        let bg0_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("video_bind_group_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("video_pipeline_layout"),
            bind_group_layouts: &[&bg0_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("video_pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("video_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            lod_min_clamp: 0.0,
            lod_max_clamp: 1.0,
            compare: None,
            anisotropy_clamp: 1,
            border_color: None,
        });

        Self {
            pipeline,
            bg0_layout,
            config: PipelineConfig::default(),
            sampler,
        }
    }

    pub fn draw(
        &self,
        target: &wgpu::TextureView,
        encoder: &mut wgpu::CommandEncoder,
        clip: &iced::Rectangle<u32>,
        video: &VideoEntry,
    ) {
        RenderPasses::draw_video_pass(
            &self.pipeline,
            target,
            encoder,
            clip,
            video,
            wgpu::LoadOp::Load,
        );
    }

    pub fn draw_clear(
        &self,
        target: &wgpu::TextureView,
        encoder: &mut wgpu::CommandEncoder,
        clip: &iced::Rectangle<u32>,
        video: &VideoEntry,
    ) {
        println!("Video pipeline draw_clear:");
        println!("  Target texture format: {:?}", target);
        println!("  Video Y texture format: {:?}", video.texture_y.format());
        println!("  Video UV texture format: {:?}", video.texture_uv.format());
        println!(
            "  First video frame: {}",
            std::any::type_name_of_val(&video)
        );
        println!("  Clip rect: {:?}", clip);
        RenderPasses::draw_video_pass(
            &self.pipeline,
            target,
            encoder,
            clip,
            video,
            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
        );
    }

    pub fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        video_id: u64,
        alive: bool,
        (width, height): (u32, u32),
        frame: &[u8],
        videos: &mut BTreeMap<u64, VideoEntry>,
    ) {
        let uniform_alignment = device.limits().min_uniform_buffer_offset_alignment as usize;
        let uniform_size = std::mem::size_of::<Uniforms>();
        let aligned_uniform_size =
            (uniform_size + uniform_alignment - 1) & !(uniform_alignment - 1);

        if let Entry::Vacant(entry) = videos.entry(video_id) {
            println!("Uploading new frame data:");
            println!("  Video ID: {}", video_id);
            println!("  Frame size: {}x{}", width, height);
            let texture_y = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("video_texture_y"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R8Unorm,
                usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });

            let texture_uv = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("video_texture_uv"),
                size: wgpu::Extent3d {
                    width: width / 2,
                    height: height / 2,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rg8Unorm,
                usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });

            let view_y = texture_y.create_view(&Default::default());
            let view_uv = texture_uv.create_view(&Default::default());

            let instances = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("video_uniform_buffer"),
                size: (256 * aligned_uniform_size) as u64,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::UNIFORM,
                mapped_at_creation: false,
            });

            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("video_bind_group"),
                layout: &self.bg0_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view_y),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&view_uv),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &instances,
                            offset: 0,
                            size: Some(NonZero::new(std::mem::size_of::<Uniforms>() as _).unwrap()),
                        }),
                    },
                ],
            });

            entry.insert(VideoEntry {
                texture_y,
                texture_uv,
                instances,
                bg0: bind_group,
                alive,
                prepare_index: std::sync::atomic::AtomicUsize::new(0),
                render_index: std::sync::atomic::AtomicUsize::new(0),
                aligned_uniform_size,
            });
        }

        if let Some(video) = videos.get(&video_id) {
            queue.write_texture(
                wgpu::ImageCopyTexture {
                    texture: &video.texture_y,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &frame[..(width * height) as usize],
                wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(width),
                    rows_per_image: Some(height),
                },
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );

            let uv_data = &frame[(width * height) as usize..];
            queue.write_texture(
                wgpu::ImageCopyTexture {
                    texture: &video.texture_uv,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                uv_data,
                wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(width / 2 * 2),
                    rows_per_image: Some(height / 2),
                },
                wgpu::Extent3d {
                    width: width / 2,
                    height: height / 2,
                    depth_or_array_layers: 1,
                },
            );
        }
    }

    pub fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        video_id: u64,
        bounds: &iced::Rectangle,
        color_space: ffmpeg_next::color::Space,
        videos: &mut BTreeMap<u64, VideoEntry>,
        state: &PipelineState,
    ) {
        if let Some(video) = videos.get_mut(&video_id) {
            let prepare_index = video.prepare_index.load(Ordering::Relaxed);
            println!("Preparing frame:");
            println!("  Video ID: {}", video_id);
            println!("  Prepare index: {}", prepare_index);

            let config = match color_space {
                ffmpeg_next::color::Space::BT709 => BT709_CONFIG,
                _ => BT709_CONFIG,
            };

            println!("Color Space Details:");
            println!("  Color Space: {:?}", color_space);
            let uniforms = Uniforms {
                rect: [bounds.x, bounds.y, bounds.width, bounds.height],
                color_space: [color_space as u32],
                y_range: config.y_range,
                uv_range: config.uv_range,
                matrix: config.matrix,
                _pad: [0; 188],
            };

            let offset = video.prepare_index.load(Ordering::Relaxed) * video.aligned_uniform_size;
            queue.write_buffer(
                &video.instances,
                offset as u64,
                bytemuck::cast_slice(&[uniforms]),
            );

            // video.prepare_index.fetch_add(1, Ordering::Relaxed);
            // video.render_index.store(0, Ordering::Relaxed);

            let next_index = (prepare_index + 1) % 256;
            video.prepare_index.store(next_index, Ordering::Relaxed);
        }
    }
}
