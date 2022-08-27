use std::num::NonZeroU32;

use super::{TEX_DIM, TEX_FORMAT};

#[derive(Debug)]
pub struct GpuTexture {
    pub size: wgpu::Extent3d,
    pub texture: wgpu::Texture,
}

impl GpuTexture {
    pub fn layer_usage() -> wgpu::TextureUsages {
        wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING
    }

    pub fn output_usage() -> wgpu::TextureUsages {
        wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::RENDER_ATTACHMENT
    }

    pub fn empty(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        label: Option<&str>,
        usage: wgpu::TextureUsages,
    ) -> Self {
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };

        Self::empty_with_extent(device, size, label, usage)
    }

    pub fn empty_with_extent(
        device: &wgpu::Device,
        size: wgpu::Extent3d,
        label: Option<&str>,
        usage: wgpu::TextureUsages,
    ) -> Self {
        // Canvas texture
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: TEX_DIM,
            format: TEX_FORMAT,
            usage,
            label,
        });

        Self { texture, size }
    }

    pub fn replace(
        &self,
        queue: &wgpu::Queue,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        data: &[u8],
    ) {
        queue.write_texture(
            // Tells wgpu where to copy the pixel data
            wgpu::ImageCopyTexture {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            // The actual pixel data
            &data,
            // The layout of the texture
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: NonZeroU32::new(4 * width),
                rows_per_image: NonZeroU32::new(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
    }
}
