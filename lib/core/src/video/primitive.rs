use ffmpeg_next::color::{self, Space};
use iced_wgpu::primitive::Primitive;
use iced_wgpu::wgpu;
use std::{
    collections::{btree_map::Entry, BTreeMap},
    num::NonZero,
    sync::atomic::{AtomicUsize, Ordering},
};

#[repr(C)]
struct Uniforms {
    rect: [f32; 4],
    color_space: [u32; 1],
    _pad: [u8; 188], // Adjusted padding to maintain size
}

struct VideoEntry {
    texture_y: wgpu::Texture,
    texture_uv: wgpu::Texture,
    instances: wgpu::Buffer,
    bg0: wgpu::BindGroup,
    alive: bool,

    prepare_index: AtomicUsize,
    render_index: AtomicUsize,
}

pub struct ShaderEffect {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    uniform_buffer: wgpu::Buffer,
    enabled: bool,
}

pub struct VideoPipeline {
    // Base YUV pipeline
    yuv_pipeline: wgpu::RenderPipeline,
    yuv_bind_group_layout: wgpu::BindGroupLayout,

    // Effect chain
    effects: Vec<ShaderEffect>,

    // Ping-pong buffers for effects
    intermediate_textures: [wgpu::Texture; 2],
    intermediate_views: [wgpu::TextureView; 2],

    // Final pass components
    final_pipeline: wgpu::RenderPipeline,
    final_bind_groups: [wgpu::BindGroup; 2],
    final_bind_group_layout: wgpu::BindGroupLayout,
    final_uniforms_buffer: wgpu::Buffer,

    videos: BTreeMap<u64, VideoEntry>,
    sampler: wgpu::Sampler,
}

impl VideoPipeline {
    fn create_intermediate_texture(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat, // Add format parameter
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
            label: Some("internmediate texture shader"),
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }
    pub fn add_effect(&mut self, device: &wgpu::Device, effect: ShaderEffect) {
        self.effects.push(effect);
    }
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("video shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../../assets/shaders/video.wgsl").into(),
            ),
        });

        let bg0_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("video bind group 0 layout"),
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
            label: Some("video pipeline layout"),
            bind_group_layouts: &[&bg0_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("video pipeline"),
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

        let final_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Final pass bind group layout"),
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
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });

        let (texture1, view1) = Self::create_intermediate_texture(device, 1920, 1080, format);
        let (texture2, view2) = Self::create_intermediate_texture(device, 1920, 1080, format);

        let intermediate_textures = [texture1, texture2];
        let intermediate_views = [view1, view2];

        // Create final pipeline
        let final_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Final pass shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../../assets/shaders/final_pass.wgsl").into(),
            ),
        });

        let final_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Final pass pipeline layout"),
                bind_group_layouts: &[&final_bind_group_layout],
                push_constant_ranges: &[],
            });

        let final_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Final pass pipeline"),
            layout: Some(&final_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &final_shader,
                entry_point: "vs_main",
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &final_shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format,
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
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });
        let final_uniforms_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Final pass uniform buffer"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // Create final bind groups for both intermediate textures
        let final_bind_groups = [
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Final pass bind group 0"),
                layout: &final_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&intermediate_views[0]),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &final_uniforms_buffer,
                            offset: 0,
                            size: Some(NonZero::new(std::mem::size_of::<Uniforms>() as _).unwrap()),
                        }),
                    },
                ],
            }),
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Final pass bind group 1"),
                layout: &final_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&intermediate_views[1]),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &final_uniforms_buffer,
                            offset: 0,
                            size: Some(NonZero::new(std::mem::size_of::<Uniforms>() as _).unwrap()),
                        }),
                    },
                ],
            }),
        ];
        VideoPipeline {
            yuv_pipeline: pipeline, // renamed from pipeline
            yuv_bind_group_layout: bg0_layout,
            effects: Vec::new(), // Initialize effects vector
            intermediate_textures,
            intermediate_views,
            final_pipeline,
            final_bind_groups,
            final_bind_group_layout,
            final_uniforms_buffer,
            sampler,
            videos: BTreeMap::new(),
        }
    }

    fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        video_id: u64,
        alive: bool,
        (width, height): (u32, u32),
        frame: &[u8],
    ) {
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
                layout: &self.yuv_bind_group_layout,
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

    fn prepare(
        &mut self,
        queue: &wgpu::Queue,
        video_id: u64,
        bounds: &iced::Rectangle,
        color_space: Space,
    ) {
        if let Some(video) = self.videos.get_mut(&video_id) {
            let uniforms = Uniforms {
                rect: [
                    bounds.x,
                    bounds.y,
                    bounds.x + bounds.width,
                    bounds.y + bounds.height,
                ],
                color_space: [color_space as u32],
                _pad: [0; 188],
            };
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

        self.cleanup();
    }

    // fn draw(
    //     &self,
    //     target: &wgpu::TextureView,
    //     encoder: &mut wgpu::CommandEncoder,
    //     clip: &iced::Rectangle<u32>,
    //     video_id: u64,
    // ) {
    //     if let Some(video) = self.videos.get(&video_id) {
    //         let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
    //             label: Some("iced_video_player render pass"),
    //             color_attachments: &[Some(wgpu::RenderPassColorAttachment {
    //                 view: target,
    //                 resolve_target: None,
    //                 ops: wgpu::Operations {
    //                     load: wgpu::LoadOp::Load,
    //                     store: wgpu::StoreOp::Store,
    //                 },
    //             })],
    //             depth_stencil_attachment: None,
    //             timestamp_writes: None,
    //             occlusion_query_set: None,
    //         });

    //         pass.set_pipeline(&self.yuv_pipeline);
    //         pass.set_bind_group(
    //             0,
    //             &video.bg0,
    //             &[
    //                 (video.render_index.load(Ordering::Relaxed) * std::mem::size_of::<Uniforms>())
    //                     as u32,
    //             ],
    //         );
    //         pass.set_scissor_rect(clip.x as _, clip.y as _, clip.width as _, clip.height as _);
    //         pass.draw(0..6, 0..1);

    //         video.prepare_index.store(0, Ordering::Relaxed);
    //         video.render_index.fetch_add(1, Ordering::Relaxed);
    //     }
    // }

    fn draw(
        &self,
        target: &wgpu::TextureView,
        encoder: &mut wgpu::CommandEncoder,
        clip: &iced::Rectangle<u32>,
        video_id: u64,
    ) {
        let Some(video) = self.videos.get(&video_id) else {
            return;
        };

        // Index for ping-pong buffers
        let mut source_idx = 0;
        let mut target_idx = 1;

        // First pass: YUV to RGB
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("YUV to RGB pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            pass.set_pipeline(&self.yuv_pipeline);
            pass.set_bind_group(
                0,
                &video.bg0,
                &[
                    (video.render_index.load(Ordering::Relaxed) * std::mem::size_of::<Uniforms>())
                        as u32,
                ],
            );
            // pass.set_scissor_rect(clip.x as _, clip.y as _, clip.width as _, clip.height as _);
            pass.set_pipeline(&self.yuv_pipeline);
            pass.set_bind_group(
                0,
                &video.bg0,
                &[
                    (video.render_index.load(Ordering::Relaxed) * std::mem::size_of::<Uniforms>())
                        as u32,
                ],
            );
            pass.draw(0..6, 0..1);
            video.prepare_index.store(0, Ordering::Relaxed);
            video.render_index.fetch_add(1, Ordering::Relaxed);
        }

        // Chain effect passes
        for effect in self.effects.iter().filter(|e| e.enabled) {
            std::mem::swap(&mut source_idx, &mut target_idx);

            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Effect pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.intermediate_views[target_idx], // Render to other intermediate texture
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            pass.set_pipeline(&effect.pipeline);
            // Set effect-specific bind groups and draw
        }
        // Final pass to target
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Final pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load, // Load existing content
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            pass.set_pipeline(&self.final_pipeline);
            pass.set_bind_group(0, &self.final_bind_groups[target_idx], &[]); // Use target_idx here
            pass.set_scissor_rect(clip.x, clip.y, clip.width, clip.height);
            pass.draw(0..6, 0..1);
        }

        video.prepare_index.store(0, Ordering::Relaxed);
        video.render_index.fetch_add(1, Ordering::Relaxed);
    }
}

#[derive(Debug, Clone)]
pub struct VideoPrimitive {
    video_id: u64,
    alive: bool,
    frame: Vec<u8>,
    size: (u32, u32),
    upload_frame: bool,
    color_space: Space,
}

impl VideoPrimitive {
    pub fn new(
        video_id: u64,
        alive: bool,
        frame: Vec<u8>,
        size: (u32, u32),
        upload_frame: bool,
        color_space: Space,
    ) -> Self {
        VideoPrimitive {
            video_id,
            alive,
            frame,
            size,
            upload_frame,
            color_space,
        }
    }
}

impl Primitive for VideoPrimitive {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        storage: &mut iced_wgpu::primitive::Storage,
        bounds: &iced::Rectangle,
        viewport: &iced_wgpu::graphics::Viewport,
    ) {
        if !storage.has::<VideoPipeline>() {
            storage.store(VideoPipeline::new(device, format));
        }

        let pipeline = storage.get_mut::<VideoPipeline>().unwrap();

        if self.upload_frame {
            pipeline.upload(
                device,
                queue,
                self.video_id,
                self.alive,
                self.size,
                self.frame.as_slice(),
            );
        }

        pipeline.prepare(
            queue,
            self.video_id,
            &(*bounds
                * iced::Transformation::orthographic(
                    viewport.logical_size().width as _,
                    viewport.logical_size().height as _,
                )),
            self.color_space,
        );
    }

    fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        storage: &iced_wgpu::primitive::Storage,
        target: &wgpu::TextureView,
        clip_bounds: &iced::Rectangle<u32>,
    ) {
        let pipeline = storage.get::<VideoPipeline>().unwrap();
        pipeline.draw(target, encoder, clip_bounds, self.video_id);
    }
}
