use iced_wgpu::primitive::Primitive;
use iced_wgpu::wgpu;
use std::{
    collections::{btree_map::Entry, BTreeMap},
    num::NonZero,
    sync::atomic::{AtomicUsize, Ordering},
};

pub struct TextureManager {
    pub intermediate_textures: Vec<wgpu::Texture>,
    format: wgpu::TextureFormat,
}

impl TextureManager {
    pub fn new(format: wgpu::TextureFormat) -> Self {
        Self {
            intermediate_textures: Vec::new(),
            format,
        }
    }

    pub fn create_intermediate_texture(
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

    pub fn resize_intermediate_textures(
        &mut self,
        device: &wgpu::Device,
        size: wgpu::Extent3d,
        num_effects: usize,
    ) {
        // Clear existing textures
        for texture in self.intermediate_textures.drain(..) {
            texture.destroy();
        }

        // Create new textures with updated size
        for _ in 0..=num_effects {
            self.intermediate_textures
                .push(self.create_intermediate_texture(device, size));
        }
    }

    pub fn get_texture(&self, index: usize) -> Option<&wgpu::Texture> {
        self.intermediate_textures.get(index)
    }

    pub fn textures_mut(&mut self) -> &mut Vec<wgpu::Texture> {
        &mut self.intermediate_textures
    }

    pub fn len(&self) -> usize {
        self.intermediate_textures.len()
    }
}
