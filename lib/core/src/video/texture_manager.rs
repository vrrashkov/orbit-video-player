use iced_wgpu::primitive::Primitive;
use iced_wgpu::wgpu;
use std::{
    collections::{btree_map::Entry, BTreeMap, HashMap},
    num::NonZero,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};
use tracing::{debug, info, trace, warn};

/// Manages intermediate textures for effect processing pipelines
///
/// Handles creation, storage, and access to textures used between shader effects
/// in a render pipeline.
pub struct TextureManager {
    pub intermediate_textures: Vec<std::sync::Arc<wgpu::Texture>>,
    format: wgpu::TextureFormat,
}

impl TextureManager {
    /// Get a view for the texture at the specified index
    pub fn get_texture_view(&self, index: usize) -> Option<wgpu::TextureView> {
        let result = self
            .intermediate_textures
            .get(index)
            .map(|texture| texture.create_view(&wgpu::TextureViewDescriptor::default()));

        if result.is_none() {
            trace!("No texture found at index {}", index);
        }

        result
    }

    /// Get a clone of the Arc<Texture> at the specified index
    pub fn get_texture(&self, index: usize) -> Option<std::sync::Arc<wgpu::Texture>> {
        let result = self.intermediate_textures.get(index).cloned();

        if let Some(texture) = &result {
            trace!(
                "Retrieved texture {}: format={:?}, size={}x{}",
                index,
                texture.format(),
                texture.size().width,
                texture.size().height
            );
        } else {
            trace!("No texture found at index {}", index);
        }

        result
    }

    /// Create a new texture manager with the specified format
    pub fn new(format: wgpu::TextureFormat) -> Self {
        debug!("Creating TextureManager with format: {:?}", format);
        Self {
            intermediate_textures: Vec::new(),
            format,
        }
    }

    /// Create a new intermediate texture with the specified size
    fn create_intermediate_texture(
        &self,
        device: &wgpu::Device,
        size: wgpu::Extent3d,
    ) -> Arc<wgpu::Texture> {
        debug!(
            "Creating intermediate texture: size={}x{}, format={:?}",
            size.width, size.height, self.format
        );

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

        Arc::new(texture)
    }

    /// Create a texture view with explicit parameters for the texture at the specified index
    pub fn create_texture_view(&self, index: usize) -> Option<wgpu::TextureView> {
        self.get_texture(index).map(|texture| {
            trace!(
                "Creating view for texture {}: format={:?}",
                index,
                texture.format()
            );

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

    /// Get the texture format used by this manager
    pub fn format(&self) -> wgpu::TextureFormat {
        self.format
    }

    /// Print detailed information about all textures for debugging
    pub fn debug_print_state(&self) {
        debug!(
            "TextureManager state: format={:?}, count={}",
            self.format,
            self.len()
        );

        for (i, texture) in self.intermediate_textures.iter().enumerate() {
            debug!(
                "Texture {}: format={:?}, size={}x{}, usage={:?}",
                i,
                texture.format(),
                texture.size().width,
                texture.size().height,
                texture.usage()
            );
        }
    }

    /// Resize or recreate intermediate textures to match the specified size and count
    ///
    /// This is typically called when the video dimensions change or when the
    /// number of effects in the pipeline changes.
    pub fn resize_intermediate_textures(
        &mut self,
        device: &wgpu::Device,
        size: wgpu::Extent3d,
        num_effects: usize,
    ) {
        // Check if we already have enough textures of the right size
        if !self.intermediate_textures.is_empty() {
            let existing_size = self.intermediate_textures[0].size();
            if existing_size.width == size.width
                && existing_size.height == size.height
                && self.intermediate_textures.len() >= num_effects + 1
            {
                trace!(
                    "No need to resize textures - already have {} textures of size {}x{}",
                    self.intermediate_textures.len(),
                    size.width,
                    size.height
                );
                return;
            }
        }

        debug!(
            "Resizing intermediate textures: size={}x{}, count={}",
            size.width,
            size.height,
            num_effects + 1
        );

        // Clear existing textures and create new ones
        self.intermediate_textures.clear();

        for i in 0..=num_effects {
            let texture = self.create_intermediate_texture(device, size);
            self.intermediate_textures.push(texture);
            trace!("Created intermediate texture {}", i);
        }

        debug!(
            "Created {} intermediate textures",
            self.intermediate_textures.len()
        );
    }

    /// Validate that all textures have the expected format
    ///
    /// Returns true if all textures match the manager's format, false otherwise.
    pub fn validate_formats(&self) -> bool {
        let mut valid = true;
        for (i, texture) in self.intermediate_textures.iter().enumerate() {
            if texture.format() != self.format {
                warn!(
                    "Format mismatch in texture {}: Expected {:?}, got {:?}",
                    i,
                    self.format,
                    texture.format()
                );
                valid = false;
            }
        }

        if valid {
            trace!(
                "All {} textures have correct format: {:?}",
                self.len(),
                self.format
            );
        }

        valid
    }

    /// Get the number of intermediate textures
    pub fn len(&self) -> usize {
        self.intermediate_textures.len()
    }
}
