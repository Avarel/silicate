use crate::dev::GpuDispatch;

use super::{BufferDimensions, dev::GpuHandle};

const TEX_DIM: wgpu::TextureDimension = wgpu::TextureDimension::D2;
pub(super) const TEX_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// GPU texture abstraction.
#[derive(Debug)]
pub struct GpuTexture {
    pub size: wgpu::Extent3d,
    pub texture: wgpu::Texture,
}

impl GpuTexture {
    pub const ATLAS_USAGE: wgpu::TextureUsages = wgpu::TextureUsages::COPY_DST
        .union(wgpu::TextureUsages::COPY_SRC)
        .union(wgpu::TextureUsages::TEXTURE_BINDING);
    pub const LAYER_USAGE: wgpu::TextureUsages =
        wgpu::TextureUsages::COPY_DST.union(wgpu::TextureUsages::TEXTURE_BINDING);
    pub const OUTPUT_USAGE: wgpu::TextureUsages = wgpu::TextureUsages::COPY_SRC
        .union(wgpu::TextureUsages::TEXTURE_BINDING)
        .union(wgpu::TextureUsages::RENDER_ATTACHMENT);

    /// Create an empty texture.
    pub fn empty_layers(
        dispatch: &GpuDispatch,
        width: u32,
        height: u32,
        layers: u32,
        usage: wgpu::TextureUsages,
    ) -> Self {
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: layers,
        };

        Self::empty_with_extent(dispatch, size, usage)
    }

    /// Create an empty texture from an extent.
    pub fn empty_with_extent(
        dispatch: &GpuDispatch,
        size: wgpu::Extent3d,
        usage: wgpu::TextureUsages,
    ) -> Self {
        // Canvas texture
        let texture = dispatch.device().create_texture(&wgpu::TextureDescriptor {
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: TEX_DIM,
            format: TEX_FORMAT,
            view_formats: &[
                wgpu::TextureFormat::Rgba8Unorm,
                wgpu::TextureFormat::Rgba8UnormSrgb,
            ],
            usage,
            label: None,
        });

        Self { texture, size }
    }

    pub fn layers(&self) -> u32 {
        self.size.depth_or_array_layers
    }

    /// Make a texture view of this GPU texture.
    pub fn create_view(&self) -> wgpu::TextureView {
        self.texture
            .create_view(&wgpu::TextureViewDescriptor::default())
    }

    pub fn create_srgb_view(&self) -> wgpu::TextureView {
        self.texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(wgpu::TextureFormat::Rgba8UnormSrgb),
            ..Default::default()
        })
    }

    #[allow(dead_code)]
    pub fn create_view_layer(&self, layer: u32) -> wgpu::TextureView {
        self.texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(wgpu::TextureFormat::Rgba8UnormSrgb),
            base_array_layer: layer,
            array_layer_count: Some(1),
            ..Default::default()
        })
    }

    /// Clear the texture with a certain color.
    #[allow(dead_code)]
    pub fn clear(&self, dispatch: &GpuDispatch, color: wgpu::Color) {
        dispatch.queue().submit(Some({
            let mut encoder = dispatch
                .device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

            encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.create_view(),
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            encoder.finish()
        }));
    }

    /// Replace a section of the texture with raw RGBA data.
    ///
    /// ### Note
    /// The position `x` and `y` and size `width` and `height` data
    /// should strictly fit within the texture boundaries.
    pub fn replace_from_bytes(
        &self,
        dispatch: &GpuDispatch,
        (x, y): (u32, u32),
        (width, height): (u32, u32),
        layer: u32,
        data: &[u8],
    ) {
        assert!(
            layer < self.layers(),
            "index {layer} must be less than {}",
            self.layers()
        );
        dispatch.queue().write_texture(
            // Tells wgpu where to copy the pixel data
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: layer },
                aspect: wgpu::TextureAspect::All,
            },
            // The actual pixel data
            data,
            // The layout of the texture
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * width),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
    }

    pub fn replace_from_tex_chunk(
        &self,
        dispatch: &GpuDispatch,
        (x, y): (u32, u32),
        (width, height): (u32, u32),
        layer: u32,
        (data, data_x, data_y, data_z): (&GpuTexture, u32, u32, u32),
    ) {
        assert!(
            layer < self.layers(),
            "index {layer} must be less than {}",
            self.layers()
        );
        // Copy the texture to the output buffer
        dispatch.queue().submit(Some({
            let mut encoder = dispatch
                .device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
            // Copy the data from the texture to the buffer
            encoder.copy_texture_to_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &data.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: data_x,
                        y: data_y,
                        z: data_z,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyTextureInfo {
                    texture: &self.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x, y, z: layer },
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );

            encoder.finish()
        }));
    }

    /// Clone the texture.
    ///
    /// ### Note
    /// `dev` should be the same device that created this texture
    /// in the first place.
    pub fn clone(&self, dispatch: &GpuDispatch) -> Self {
        let clone = Self::empty_with_extent(
            dispatch,
            self.size,
            Self::OUTPUT_USAGE | wgpu::TextureUsages::COPY_DST,
        );
        dispatch.queue().submit(Some({
            let mut encoder = dispatch
                .device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
            // Copy the data from the texture to the buffer
            encoder.copy_texture_to_texture(
                self.texture.as_image_copy(),
                clone.texture.as_image_copy(),
                self.size,
            );
            encoder.finish()
        }));
        clone
    }

    pub fn export_buffer(&self, dispatch: &GpuDispatch, dim: BufferDimensions) -> wgpu::Buffer {
        let output_buffer = dispatch.device().create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: (dim.padded_bytes_per_row() * dim.height()) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            // Copying texture to buffer requires that the buffer is not mapped
            mapped_at_creation: false,
        });

        // Copy the texture to the output buffer
        dispatch.queue().submit(Some({
            let mut encoder = dispatch
                .device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
            // Copy the data from the texture to the buffer
            encoder.copy_texture_to_buffer(
                self.texture.as_image_copy(),
                wgpu::TexelCopyBufferInfo {
                    buffer: &output_buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(dim.padded_bytes_per_row()),
                        rows_per_image: None,
                    },
                },
                dim.extent(),
            );

            encoder.finish()
        }));

        output_buffer
    }
}
