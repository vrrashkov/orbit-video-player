use ffmpeg_next::color::{self, Space};
use iced_wgpu::primitive::Primitive;
use iced_wgpu::wgpu;
use std::{
    collections::{btree_map::Entry, BTreeMap},
    num::NonZero,
    sync::atomic::{AtomicUsize, Ordering},
};

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

pub struct VideoEntry {
    pub texture_y: wgpu::Texture,
    pub texture_uv: wgpu::Texture,
    pub instances: wgpu::Buffer,
    pub bg0: wgpu::BindGroup,
    pub alive: bool,

    pub prepare_index: AtomicUsize,
    pub render_index: AtomicUsize,
}

pub struct VideoPipeline {
    pipeline: wgpu::RenderPipeline,
    bg0_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    videos: BTreeMap<u64, VideoEntry>,
    effects: EffectChain,
    texture_manager: TextureManager,
    format: wgpu::TextureFormat,
}

impl VideoPipeline {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("iced_video_player shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../../assets/shaders/video.wgsl").into(),
            ),
        });

        let bg0_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("iced_video_player bind group 0 layout"),
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
            label: Some("iced_video_player pipeline layout"),
            bind_group_layouts: &[&bg0_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("iced_video_player pipeline"),
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
            label: Some("iced_video_player sampler"),
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

        VideoPipeline {
            pipeline,
            bg0_layout,
            sampler,
            videos: BTreeMap::new(),
            effects: EffectChain::new(),
            texture_manager: TextureManager::new(format),
            format,
        }
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

    pub fn add_effect(
        &mut self,
        device: &wgpu::Device,
        shader_source: &str,
        uniforms: Option<&[u8]>,
    ) {
        let uniforms_buffer = uniforms.map(|data| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("effect_uniforms"),
                size: data.len() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        });

        let effect_builder = ShaderEffect::builder()
            .device(device)
            .shader_source(shader_source)
            .format(self.format)
            .maybe_uniforms(uniforms_buffer)
            .build();

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

        self.texture_manager.resize_intermediate_textures(
            device,
            texture_size,
            self.effects.len() + 1,
        );

        self.effects.add_effect(effect_builder);
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
                label: Some("iced_video_player texture"),
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
                label: Some("iced_video_player texture"),
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
                label: Some("iced_video_player texture view"),
                format: None,
                dimension: None,
                aspect: wgpu::TextureAspect::All,
                base_mip_level: 0,
                mip_level_count: None,
                base_array_layer: 0,
                array_layer_count: None,
            });

            let view_uv = texture_uv.create_view(&wgpu::TextureViewDescriptor {
                label: Some("iced_video_player texture view"),
                format: None,
                dimension: None,
                aspect: wgpu::TextureAspect::All,
                base_mip_level: 0,
                mip_level_count: None,
                base_array_layer: 0,
                array_layer_count: None,
            });

            let instances = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("iced_video_player uniform buffer"),
                size: 256 * std::mem::size_of::<Uniforms>() as u64, // max 256 video players per frame
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::UNIFORM,
                mapped_at_creation: false,
            });

            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("iced_video_player bind group"),
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
            video.prepare_index.fetch_add(1, Ordering::Relaxed);
            video.render_index.store(0, Ordering::Relaxed);
        }

        self.effects.clear_bind_groups();
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

                let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("effect_bind_group"),
                    layout: &self.effects.effects()[i].bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&input),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&self.sampler),
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

            // First render video to first intermediate texture
            let first_view =
                self.texture_manager.intermediate_textures[0].create_view(&Default::default());
            self.draw_video_pass_clear(&first_view, encoder, clip, &video);

            // Apply effects to intermediate textures
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

            // Final draw to target - preserve existing content
            if let Some((last_effect, last_bind_group)) = self
                .effects
                .effects
                .last()
                .zip(self.effects.bind_groups.last())
            {
                self.apply_effect(encoder, last_effect, last_bind_group, target, clip, false);
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
        RenderPasses::draw_video_pass(
            &self.pipeline,
            target,
            encoder,
            clip,
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
        RenderPasses::apply_effect(effect, encoder, bind_group, output, clip, clear);
    }
}
