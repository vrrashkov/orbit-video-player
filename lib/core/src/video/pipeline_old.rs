use ffmpeg_next::color::{self, Space};
use iced_wgpu::primitive::Primitive;
use iced_wgpu::wgpu;
use std::{
    collections::{btree_map::Entry, BTreeMap},
    num::NonZero,
    sync::atomic::{AtomicUsize, Ordering},
};

use crate::video::shader::{ShaderEffectBuilder, ShaderUniforms, UniformValue};

use super::{
    color_space::BT709_CONFIG, effect_chain::EffectChain, render_passes::RenderPasses,
    texture_manager::TextureManager, ShaderEffect,
};

#[repr(C)]
pub struct Uniforms {
    pub rect: [f32; 4],
    pub color_space: [u32; 1],
    pub y_range: [f32; 2],     // min, max for Y
    pub uv_range: [f32; 2],    // min, max for UV
    pub matrix: [[f32; 3]; 3], // Color conversion matrix
    pub _pad: [u8; 188],       // Adjusted padding to maintain size
}
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct LineUniforms {
    position: f32,
    bounds: [f32; 4], // x, y, width, height
    line_width: f32,
    _pad: [f32; 9], // Pad to make total size 64 bytes: (1 + 4 + 1 + 10) * 4 = 64
}

pub struct VideoEntry {
    pub texture_y: wgpu::Texture,
    pub texture_uv: wgpu::Texture,
    pub instances: wgpu::Buffer,
    pub bg0: wgpu::BindGroup,
    pub alive: bool,

    pub prepare_index: AtomicUsize,
    pub render_index: AtomicUsize,
    pub aligned_uniform_size: usize, // Add this field
}
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
pub trait PipelineBuilder {
    fn build_pipeline(
        device: &wgpu::Device,
        config: &PipelineConfig,
        bind_group_layouts: &[&wgpu::BindGroupLayout],
    ) -> wgpu::RenderPipeline;

    fn create_bind_group_layouts(device: &wgpu::Device) -> Vec<wgpu::BindGroupLayout>;
}
pub struct VideoPipeline {
    pipeline: wgpu::RenderPipeline,
    bg0_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    videos: BTreeMap<u64, VideoEntry>,
    effects: EffectChain,
    texture_manager: TextureManager,
    format: wgpu::TextureFormat,
    comparison_enabled: bool,
    comparison_position: f32, // 0.0 to 1.0
    color_threshold: f32,
    color_blend_mode: f32,
    line_pipeline: Option<wgpu::RenderPipeline>,
    line_bind_group_layout: Option<wgpu::BindGroupLayout>,
    line_uniform_buffer: Option<wgpu::Buffer>,
    line_bind_group: Option<wgpu::BindGroup>,
}

impl VideoPipeline {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("nebula_player shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../../assets/shaders/video.wgsl").into(),
            ),
        });

        let bg0_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("nebula_player bind group 0 layout"),
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
            label: Some("nebula_player pipeline layout"),
            bind_group_layouts: &[&bg0_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("nebula_player pipeline"),
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
            label: Some("video sampler"),
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

        let uniform_alignment = device.limits().min_uniform_buffer_offset_alignment as usize;
        let uniform_size = std::mem::size_of::<Uniforms>();
        let aligned_uniform_size =
            (uniform_size + uniform_alignment - 1) & !(uniform_alignment - 1);

        let instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("video uniform buffer"),
            size: (256 * aligned_uniform_size) as u64, // Use aligned size
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::UNIFORM,
            mapped_at_creation: false,
        });

        let mut video_pipeline = VideoPipeline {
            pipeline,
            bg0_layout,
            sampler,
            videos: BTreeMap::new(),
            effects: EffectChain::new(),
            texture_manager: TextureManager::new(format),
            format,
            comparison_enabled: false,
            comparison_position: 0.5,
            line_pipeline: None,
            line_bind_group_layout: None,
            line_uniform_buffer: None,
            line_bind_group: None,
            color_threshold: 0.05,
            color_blend_mode: 2.,
        };
        let (line_pipeline, line_bind_group_layout) = video_pipeline.create_line_pipeline(device);
        // Create uniform buffer for line
        let line_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("line_uniform_buffer"),
            size: 64, // Set explicit size to 64 bytes
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create bind group for line
        let line_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("line_bind_group"),
            layout: &line_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: line_uniform_buffer.as_entire_binding(),
            }],
        });
        video_pipeline.line_pipeline = Some(line_pipeline);
        video_pipeline.line_bind_group_layout = Some(line_bind_group_layout);
        video_pipeline.line_uniform_buffer = Some(line_uniform_buffer);
        video_pipeline.line_bind_group = Some(line_bind_group);

        video_pipeline
    }
    fn create_intermediate_texture(
        &self,
        device: &wgpu::Device,
        size: wgpu::Extent3d,
    ) -> wgpu::Texture {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some("effect_intermediate_texture"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
    }
    pub fn set_comparison_enabled(&mut self, enabled: bool) {
        self.comparison_enabled = enabled;
    }

    pub fn set_comparison_position(&mut self, position: f32) {
        self.comparison_position = position.clamp(0.0, 1.0);
    }
    fn create_line_pipeline(
        &self,
        device: &wgpu::Device,
    ) -> (wgpu::RenderPipeline, wgpu::BindGroupLayout) {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("line_shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../../assets/shaders/split_line.wgsl").into(),
            ),
        });

        // Create bind group layout for line uniforms
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("line_bind_group_layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("line_pipeline_layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("line_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: self.format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::SrcAlpha,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        (pipeline, bind_group_layout)
    }

    fn draw_comparison_line(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        x: f32,
        clip: &iced::Rectangle<u32>,
    ) {
        if let (Some(line_pipeline), Some(line_bind_group)) =
            (&self.line_pipeline, &self.line_bind_group)
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("comparison_line"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // Set viewport to match clip bounds
            pass.set_viewport(
                clip.x as f32,
                clip.y as f32,
                clip.width as f32,
                clip.height as f32,
                0.0,
                1.0,
            );

            pass.set_scissor_rect(clip.x, clip.y, clip.width, clip.height);
            pass.set_pipeline(line_pipeline);
            pass.set_bind_group(0, line_bind_group, &[]);
            pass.draw(0..6, 0..1);
        }
    }
    pub fn add_effect(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, shader_source: &str) {
        // Create uniform buffer with initial values
        let mut shader_uniforms = ShaderUniforms::new(device, 2);

        // Add all the uniforms we currently use
        shader_uniforms.set_uniform(
            "comparison_enabled",
            UniformValue::Uint(self.comparison_enabled as u32),
        );
        shader_uniforms.set_uniform(
            "comparison_position",
            UniformValue::Float(self.comparison_position),
        );
        shader_uniforms.set_uniform("color_threshold", UniformValue::Float(self.color_threshold));
        shader_uniforms.set_uniform(
            "color_blend_mode",
            UniformValue::Float(self.color_blend_mode),
        );

        // Update the buffer with initial values
        shader_uniforms.update_buffer(queue);

        let effect_builder = ShaderEffectBuilder::new("effect")
            .with_shader_source(shader_source)
            .build(device, queue, self.format);

        let texture_size = if let Some(video) = self.videos.values().next() {
            let extent = video.texture_y.size();
            wgpu::Extent3d {
                width: extent.width,
                height: extent.height,
                depth_or_array_layers: 1,
            }
        } else {
            wgpu::Extent3d {
                width: 1920,
                height: 1080,
                depth_or_array_layers: 1,
            }
        };

        // Debug print the current state before resizing
        println!("Before resize - Effects len: {}", self.effects.len());
        println!("Texture size: {:?}", texture_size);

        self.texture_manager.resize_intermediate_textures(
            device,
            texture_size,
            self.effects.len() + 1,
        );

        self.effects.add_effect(effect_builder);
        self.effects.clear_bind_groups();
    }

    pub fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        video_id: u64,
        alive: bool,
        (width, height): (u32, u32),
        frame: &[u8],
    ) {
        let uniform_alignment = device.limits().min_uniform_buffer_offset_alignment as usize;
        let uniform_size = std::mem::size_of::<Uniforms>();
        let aligned_uniform_size =
            (uniform_size + uniform_alignment - 1) & !(uniform_alignment - 1);

        let is_new_video = !self.videos.contains_key(&video_id);
        if is_new_video {
            // If this is a new video, ensure we have the right number of intermediate textures
            let size = wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            };
            self.resize_intermediate_textures(device, size);
        }
        if let Entry::Vacant(entry) = self.videos.entry(video_id) {
            let texture_y = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("video texture"),
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
                label: Some("video texture"),
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

            let view_y = texture_y.create_view(&wgpu::TextureViewDescriptor {
                label: Some("video texture view"),
                format: None,
                dimension: None,
                aspect: wgpu::TextureAspect::All,
                base_mip_level: 0,
                mip_level_count: None,
                base_array_layer: 0,
                array_layer_count: None,
            });

            let view_uv = texture_uv.create_view(&wgpu::TextureViewDescriptor {
                label: Some("video texture view"),
                format: None,
                dimension: None,
                aspect: wgpu::TextureAspect::All,
                base_mip_level: 0,
                mip_level_count: None,
                base_array_layer: 0,
                array_layer_count: None,
            });

            let instances = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("video uniform buffer"),
                size: (256 * aligned_uniform_size) as u64, // Use aligned size
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::UNIFORM,
                mapped_at_creation: false,
            });

            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("video bind group"),
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

                prepare_index: AtomicUsize::new(0),
                render_index: AtomicUsize::new(0),
                aligned_uniform_size, // Add this
            });
        }

        let VideoEntry {
            texture_y,
            texture_uv,
            ..
        } = self.videos.get(&video_id).unwrap();

        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: texture_y,
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
                texture: texture_uv,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            uv_data,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(width / 2 * 2), // Each row needs 2 bytes per pixel for UV
                rows_per_image: Some(height / 2),
            },
            wgpu::Extent3d {
                width: width / 2,
                height: height / 2,
                depth_or_array_layers: 1,
            },
        );
    }

    fn cleanup(&mut self) {
        let ids: Vec<_> = self
            .videos
            .iter()
            .filter_map(|(id, entry)| (!entry.alive).then_some(*id))
            .collect();
        for id in ids {
            if let Some(video) = self.videos.remove(&id) {
                video.texture_y.destroy();
                video.texture_uv.destroy();
                video.instances.destroy();
            }
        }
    }

    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        video_id: u64,
        bounds: &iced::Rectangle,
        color_space: Space,
    ) {
        if let Some(video) = self.videos.get_mut(&video_id) {
            let config = match color_space {
                Space::BT709 => BT709_CONFIG,
                _ => BT709_CONFIG,
            };

            let rect = [
                bounds.x,
                bounds.y,
                bounds.x + bounds.width,
                bounds.y + bounds.height,
            ];

            let uniforms = Uniforms {
                rect: rect,
                color_space: [color_space as u32],
                y_range: config.y_range,
                uv_range: config.uv_range,
                matrix: config.matrix,
                _pad: [0; 188],
            };
            dbg!("rect", &rect);
            queue.write_buffer(
                &video.instances,
                (video.prepare_index.load(Ordering::Relaxed) * std::mem::size_of::<Uniforms>())
                    as u64,
                unsafe {
                    std::slice::from_raw_parts(
                        &uniforms as *const _ as *const u8,
                        std::mem::size_of::<Uniforms>(),
                    )
                },
            );
            // Calculate aligned offset
            let uniform_alignment = device.limits().min_uniform_buffer_offset_alignment as usize;
            let uniform_size = std::mem::size_of::<Uniforms>();
            let aligned_uniform_size =
                (uniform_size + uniform_alignment - 1) & !(uniform_alignment - 1);
            let offset = video.prepare_index.load(Ordering::Relaxed) * aligned_uniform_size;

            queue.write_buffer(&video.instances, offset as u64, unsafe {
                std::slice::from_raw_parts(
                    &uniforms as *const _ as *const u8,
                    std::mem::size_of::<Uniforms>(),
                )
            });

            video.prepare_index.fetch_add(1, Ordering::Relaxed);
            video.render_index.store(0, Ordering::Relaxed);
        }

        self.effects.clear_bind_groups();

        if !self.effects.is_empty() {
            for effect in self.effects.effects_mut() {
                // Note: Order is important should be same as shader
                if let Some(uniforms) = &mut effect.uniforms {
                    uniforms.set_uniform(
                        "comparison_enabled",
                        UniformValue::Uint(self.comparison_enabled as u32),
                    );
                    uniforms.set_uniform(
                        "comparison_position",
                        UniformValue::Float(self.comparison_position),
                    );
                    uniforms
                        .set_uniform("color_threshold", UniformValue::Float(self.color_threshold));
                    uniforms.set_uniform(
                        "color_blend_mode",
                        UniformValue::Uint(self.color_blend_mode as u32),
                    );

                    uniforms.validate_layout();
                    uniforms.update_buffer(queue);
                }
            }
        }
        println!("Setting uniforms:");
        println!("  comparison_enabled: {}", self.comparison_enabled);
        println!("  comparison_position: {}", self.comparison_position);
        println!("  color_threshold: {}", self.color_threshold);
        println!("  color_blend_mode: {}", self.color_blend_mode);
        if let Some(line_uniform_buffer) = &self.line_uniform_buffer {
            let uniforms = LineUniforms {
                position: self.comparison_position * 2.0 - 1.0,
                bounds: [bounds.x, bounds.y, bounds.width, bounds.height],
                line_width: 2.0,
                _pad: [0.0; 9],
            };
            queue.write_buffer(line_uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));
        }
        if !self.effects.is_empty() && self.texture_manager.len() > 0 {
            for i in 0..self.effects.len() {
                let input = if i == 0 {
                    self.texture_manager
                        .get_texture(0)
                        .unwrap()
                        .create_view(&Default::default())
                } else {
                    self.texture_manager
                        .get_texture(i)
                        .unwrap()
                        .create_view(&Default::default())
                };

                let effect = &self.effects.effects()[i];
                let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("effect_bind_group"),
                    layout: &effect.bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&input),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&self.sampler),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                                buffer: effect.uniforms.as_ref().unwrap().buffer(),
                                offset: 0,
                                size: NonZero::new(256), // Match buffer size
                            }),
                        },
                    ],
                });
                self.effects.add_bind_group(bind_group);
            }
        }
        self.cleanup();
    }
    pub fn draw(
        &self,
        target: &wgpu::TextureView,
        encoder: &mut wgpu::CommandEncoder,
        clip: &iced::Rectangle<u32>,
        video_id: u64,
    ) {
        if let Some(video) = self.videos.get(&video_id) {
            // Debug prints to understand the state
            println!("Comparison Enabled: {}", self.comparison_enabled);
            println!("Effects Empty: {}", self.effects.is_empty());
            println!(
                "Intermediate Textures: {}",
                self.texture_manager.intermediate_textures.len()
            );
            println!("Bind Groups: {}", self.effects.bind_groups.len());
            println!("Draw state:");
            println!("  Comparison enabled: {}", self.comparison_enabled);
            println!("  Comparison position: {}", self.comparison_position);
            println!("  Current tex coords bound: {:?}", clip);
            // Single video rendering logic
            if self.effects.is_empty() {
                self.draw_video_pass(target, encoder, clip, &video);
                return;
            }

            if self.texture_manager.intermediate_textures.len() <= self.effects.len()
                || self.effects.bind_groups.len() != self.effects.len()
            {
                self.draw_video_pass(target, encoder, clip, &video);
                return;
            }

            // Existing single video with effects logic
            let first_view =
                self.texture_manager.intermediate_textures[0].create_view(&Default::default());
            self.draw_video_pass_clear(&first_view, encoder, clip, &video);

            for (i, effect) in self
                .effects
                .effects
                .iter()
                .enumerate()
                .take(self.effects.len() - 1)
            {
                let output = &self.texture_manager.intermediate_textures[i + 1]
                    .create_view(&Default::default());
                self.apply_effect(
                    encoder,
                    effect,
                    &self.effects.bind_groups[i],
                    output,
                    clip,
                    true,
                );
            }

            if let Some((last_effect, last_bind_group)) = self
                .effects
                .effects
                .last()
                .zip(self.effects.bind_groups.last())
            {
                self.apply_effect(encoder, last_effect, last_bind_group, target, clip, false);
            }

            if self.comparison_enabled {
                let x = clip.x as f32 + (clip.width as f32 * self.comparison_position);
                self.draw_comparison_line(encoder, target, x, clip);
            }
        }
    }
    fn resize_intermediate_textures(&mut self, device: &wgpu::Device, size: wgpu::Extent3d) {
        for texture in self.texture_manager.intermediate_textures.drain(..) {
            texture.destroy();
        }

        // Create new textures with updated size
        for _ in 0..=self.effects.len() {
            self.texture_manager
                .intermediate_textures
                .push(self.create_intermediate_texture(device, size));
        }
    }
    fn draw_video_pass(
        &self,
        target: &wgpu::TextureView,
        encoder: &mut wgpu::CommandEncoder,
        clip: &iced::Rectangle<u32>,
        video: &VideoEntry,
    ) {
        // let bounded_clip = iced::Rectangle {
        //     x: clip.x,
        //     y: clip.y,
        //     width: clip.width.min(video.texture_y.size().width),
        //     height: clip.height.min(video.texture_y.size().height),
        // };

        RenderPasses::draw_video_pass(
            &self.pipeline,
            target,
            encoder,
            &clip,
            video,
            wgpu::LoadOp::Load,
        );
    }

    fn draw_video_pass_clear(
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
            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
        );
    }

    // Replace apply_effect method
    fn apply_effect(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        effect: &ShaderEffect,
        bind_group: &wgpu::BindGroup,
        output: &wgpu::TextureView,
        clip: &iced::Rectangle<u32>,
        clear: bool,
    ) {
        if let Some(uniforms) = &effect.uniforms {
            println!("Debug before applying effect:");
            uniforms.debug_print_values();
        }
        RenderPasses::apply_effect(effect, encoder, bind_group, output, clip, clear);
    }
}
