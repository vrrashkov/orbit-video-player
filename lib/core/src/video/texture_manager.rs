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
        println!("Creating TextureManager with format: {:?}", format);
        Self {
            intermediate_textures: Vec::new(),
            format,
        }
    }

    fn create_intermediate_texture(
        &self,
        device: &wgpu::Device,
        size: wgpu::Extent3d,
    ) -> wgpu::Texture {
        println!("Creating intermediate texture:");
        println!("  Size: {:?}", size);
        println!("  Format: {:?}", self.format);

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("effect_intermediate_texture"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[self.format],
        });

        println!("Created intermediate texture:");
        println!("  Format: {:?}", texture.format());
        println!("  Size: {:?}", texture.size());
        println!("  Usage: {:?}", texture.usage());

        texture
    }
    pub fn create_texture_view(&self, index: usize) -> Option<wgpu::TextureView> {
        self.get_texture(index).map(|texture| {
            println!("Creating view for texture {}", index);
            println!("  Texture format: {:?}", texture.format());
            println!("  Manager format: {:?}", self.format);

            texture.create_view(&wgpu::TextureViewDescriptor {
                label: Some(&format!("intermediate_texture_view_{}", index)),
                format: Some(self.format),
                dimension: Some(wgpu::TextureViewDimension::D2),
                aspect: wgpu::TextureAspect::All,
                base_mip_level: 0,
                mip_level_count: None,
                base_array_layer: 0,
                array_layer_count: None,
            })
        })
    }
    pub fn format(&self) -> wgpu::TextureFormat {
        self.format
    }
    pub fn debug_print_state(&self) {
        println!("\nTextureManager State:");
        println!("Format: {:?}", self.format);
        println!("Number of textures: {}", self.len());

        for (i, texture) in self.intermediate_textures.iter().enumerate() {
            println!("\nTexture {}:", i);
            println!("  Format: {:?}", texture.format());
            println!("  Size: {:?}", texture.size());
            println!("  Usage: {:?}", texture.usage());
        }
        println!();
    }
    pub fn resize_intermediate_textures(
        &mut self,
        device: &wgpu::Device,
        size: wgpu::Extent3d,
        num_effects: usize,
    ) {
        println!("\nResizing intermediate textures:");
        println!("  Requested size: {:?}", size);
        println!("  Number of effects: {}", num_effects);
        println!("  Format: {:?}", self.format);

        // Validate input parameters
        if size.width == 0 || size.height == 0 {
            println!("WARNING: Invalid texture size requested");
            return;
        }

        self.intermediate_textures.clear();

        for i in 0..=num_effects {
            println!("\nCreating intermediate texture {}", i);
            let texture = self.create_intermediate_texture(device, size);

            // Validate created texture
            if texture.format() != self.format {
                println!("WARNING: Format mismatch in created texture");
                println!("  Expected: {:?}", self.format);
                println!("  Got: {:?}", texture.format());
            }

            self.intermediate_textures.push(texture);
        }

        println!(
            "\nFinished creating {} intermediate textures",
            self.intermediate_textures.len()
        );

        // Validate final state
        if !self.validate_formats() {
            println!("WARNING: Format validation failed after resize");
        }
    }
    pub fn validate_formats(&self) -> bool {
        let mut valid = true;
        for (i, texture) in self.intermediate_textures.iter().enumerate() {
            if texture.format() != self.format {
                println!(
                    "Format mismatch in texture {}: Expected {:?}, got {:?}",
                    i,
                    self.format,
                    texture.format()
                );
                valid = false;
            }
        }
        valid
    }
    pub fn get_texture(&self, index: usize) -> Option<&wgpu::Texture> {
        let result = self.intermediate_textures.get(index);
        println!("Accessing texture at index {}: {}", index, result.is_some());
        if let Some(texture) = result {
            println!("  Format: {:?}", texture.format());
            println!("  Size: {:?}", texture.size());
        }
        result
    }

    pub fn textures_mut(&mut self) -> &mut Vec<wgpu::Texture> {
        &mut self.intermediate_textures
    }

    pub fn len(&self) -> usize {
        self.intermediate_textures.len()
    }
}
