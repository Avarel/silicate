use crate::dev::GpuDispatch;

use super::BufferDimensions;

const TEX_DIM: wgpu::TextureDimension = wgpu::TextureDimension::D2;
pub(super) const TEX_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

pub trait TextureExt {
    fn empty(dispatch: &GpuDispatch, width: u32, height: u32, usage: wgpu::TextureUsages) -> Self;
    fn empty_layers(
        dispatch: &GpuDispatch,
        width: u32,
        height: u32,
        layers: u32,
        usage: wgpu::TextureUsages,
    ) -> Self;
    fn empty_with_extent(
        dispatch: &GpuDispatch,
        size: wgpu::Extent3d,
        usage: wgpu::TextureUsages,
    ) -> Self;
    fn create_default_view(&self) -> wgpu::TextureView;
    fn create_array_view(&self) -> wgpu::TextureView;
    fn create_view_layer(&self, layer: u32) -> wgpu::TextureView;
    fn copy_from_texture(
        &self,
        dispatch: &GpuDispatch,
        source_data: &wgpu::Texture,
        source_origin: wgpu::Origin3d,
        destination_origin: wgpu::Origin3d,
        size: wgpu::Extent3d,
    );
    // fn clone_buffer(&self, dispatch: &GpuDispatch) -> Self;
    fn export_buffer(&self, dispatch: &GpuDispatch, dim: BufferDimensions) -> wgpu::Buffer;

    const LAYER_USAGE: wgpu::TextureUsages =
        wgpu::TextureUsages::COPY_DST.union(wgpu::TextureUsages::TEXTURE_BINDING);
    const OUTPUT_USAGE: wgpu::TextureUsages = wgpu::TextureUsages::COPY_SRC
        .union(wgpu::TextureUsages::TEXTURE_BINDING)
        .union(wgpu::TextureUsages::RENDER_ATTACHMENT);
}

impl TextureExt for wgpu::Texture {
    fn empty(dispatch: &GpuDispatch, width: u32, height: u32, usage: wgpu::TextureUsages) -> Self {
        Self::empty_layers(dispatch, width, height, 1, usage)
    }

    /// Create an empty texture.
    fn empty_layers(
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
    fn empty_with_extent(
        dispatch: &GpuDispatch,
        size: wgpu::Extent3d,
        usage: wgpu::TextureUsages,
    ) -> Self {
        // Canvas texture
        dispatch.device().create_texture(&wgpu::TextureDescriptor {
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
        })
    }

    /// Make a texture view of this GPU texture.
    fn create_default_view(&self) -> wgpu::TextureView {
        self.create_view(&wgpu::TextureViewDescriptor::default())
    }

    fn create_array_view(&self) -> wgpu::TextureView {
        self.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..wgpu::TextureViewDescriptor::default()
        })
    }

    // fn create_srgb_view(&self) -> wgpu::TextureView {
    //     self.texture.create_view(&wgpu::TextureViewDescriptor {
    //         format: Some(wgpu::TextureFormat::Rgba8UnormSrgb),
    //         ..Default::default()
    //     })
    // }

    fn create_view_layer(&self, layer: u32) -> wgpu::TextureView {
        self.create_view(&wgpu::TextureViewDescriptor {
            format: None,
            base_array_layer: layer,
            array_layer_count: Some(1),
            dimension: Some(wgpu::TextureViewDimension::D2),
            ..Default::default()
        })
    }

    // fn create_srgb_view_layer(&self, layer: u32) -> wgpu::TextureView {
    //     self.texture.create_view(&wgpu::TextureViewDescriptor {
    //         format: Some(wgpu::TextureFormat::Rgba8UnormSrgb),
    //         base_array_layer: layer,
    //         array_layer_count: Some(1),
    //         dimension: Some(wgpu::TextureViewDimension::D2),
    //         ..Default::default()
    //     })
    // }

    fn copy_from_texture(
        &self,
        dispatch: &GpuDispatch,
        source_data: &wgpu::Texture,
        source_origin: wgpu::Origin3d,
        destination_origin: wgpu::Origin3d,
        size: wgpu::Extent3d,
    ) {
        dispatch.submit_queue(|encoder| {
            // Copy the data from the texture to the buffer
            encoder.copy_texture_to_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &source_data,
                    mip_level: 0,
                    origin: source_origin,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyTextureInfo {
                    texture: &self,
                    mip_level: 0,
                    origin: destination_origin,
                    aspect: wgpu::TextureAspect::All,
                },
                size,
            );
        });
    }

    // /// Clone the texture into an independent clone.
    // ///
    // /// ### Note
    // /// `dev` should be the same device that created this texture
    // /// in the first place.
    // fn clone_buffer(&self, dispatch: &GpuDispatch) -> Self {
    //     let clone = Self::empty_with_extent(
    //         dispatch,
    //         self.size(),
    //         Self::OUTPUT_USAGE | wgpu::TextureUsages::COPY_DST,
    //     );
    //     dispatch.submit_queue(|encoder| {
    //         // Copy the data from the texture to the buffer
    //         encoder.copy_texture_to_texture(
    //             self.as_image_copy(),
    //             clone.as_image_copy(),
    //             self.size(),
    //         );
    //     });
    //     clone
    // }

    fn export_buffer(&self, dispatch: &GpuDispatch, dim: BufferDimensions) -> wgpu::Buffer {
        let output_buffer = dispatch.device().create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: (dim.padded_bytes_per_row() * dim.height()) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            // Copying texture to buffer requires that the buffer is not mapped
            mapped_at_creation: false,
        });

        dispatch.submit_queue(|encoder| {
            // Copy the data from the texture to the buffer
            encoder.copy_texture_to_buffer(
                self.as_image_copy(),
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
        });

        output_buffer
    }
}
