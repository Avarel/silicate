use super::{dev::GpuHandle, BufferDimensions};

const TEX_DIM: wgpu::TextureDimension = wgpu::TextureDimension::D2;
pub(super) const TEX_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// GPU texture abstraction.
#[derive(Debug)]
pub struct GpuTexture {
    pub size: wgpu::Extent3d,
    pub texture: wgpu::Texture,
}

impl GpuTexture {
    pub const LAYER_USAGE: wgpu::TextureUsages =
        wgpu::TextureUsages::COPY_DST.union(wgpu::TextureUsages::TEXTURE_BINDING);
    pub const OUTPUT_USAGE: wgpu::TextureUsages = wgpu::TextureUsages::COPY_SRC
        .union(wgpu::TextureUsages::TEXTURE_BINDING)
        .union(wgpu::TextureUsages::RENDER_ATTACHMENT);

    /// Create an empty texture.
    pub fn empty_layers(
        dev: &GpuHandle,
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

        Self::empty_with_extent(dev, size, usage)
    }

    /// Create an empty texture from an extent.
    pub fn empty_with_extent(
        dev: &GpuHandle,
        size: wgpu::Extent3d,
        usage: wgpu::TextureUsages,
    ) -> Self {
        // Canvas texture
        let texture = dev.device.create_texture(&wgpu::TextureDescriptor {
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
    pub fn clear(&self, dev: &GpuHandle, color: wgpu::Color) {
        dev.queue.submit(Some({
            let mut encoder = dev
                .device
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
    pub fn replace(
        &self,
        dev: &GpuHandle,
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
        dev.queue.write_texture(
            // Tells wgpu where to copy the pixel data
            wgpu::ImageCopyTexture {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: layer },
                aspect: wgpu::TextureAspect::All,
            },
            // The actual pixel data
            data,
            // The layout of the texture
            wgpu::ImageDataLayout {
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

    /// Clone the texture.
    ///
    /// ### Note
    /// `dev` should be the same device that created this texture
    /// in the first place.
    pub fn clone(&self, dev: &GpuHandle) -> Self {
        let clone = Self::empty_with_extent(
            dev,
            self.size,
            Self::OUTPUT_USAGE | wgpu::TextureUsages::COPY_DST,
        );
        dev.queue.submit(Some({
            let mut encoder = dev
                .device
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

    /// Export the texture to the given path.
    pub async fn export(
        &self,
        dev: &GpuHandle,
        dim: BufferDimensions,
        path: std::path::PathBuf,
    ) -> image::ImageResult<()> {
        let output_buffer = dev.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: (dim.padded_bytes_per_row * dim.height) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            // Copying texture to buffer requires that the buffer is not mapped
            mapped_at_creation: false,
        });

        // Copy the texture to the output buffer
        dev.queue.submit(Some({
            let mut encoder = dev
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
            // Copy the data from the texture to the buffer
            encoder.copy_texture_to_buffer(
                self.texture.as_image_copy(),
                wgpu::ImageCopyBuffer {
                    buffer: &output_buffer,
                    layout: wgpu::ImageDataLayout {
                        offset: 0,
                        bytes_per_row: Some(dim.padded_bytes_per_row),
                        rows_per_image: None,
                    },
                },
                dim.extent,
            );

            encoder.finish()
        }));

        let buffer_slice = output_buffer.slice(..);

        // NOTE: We have to create the mapping THEN device.poll() before await
        // the future. Otherwise the application will freeze.
        let (tx, rx) = tokio::sync::oneshot::channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |result| tx.send(result).unwrap());
        dev.device.poll(wgpu::Maintain::Wait);
        rx.await.unwrap().expect("Buffer mapping failed");

        let data = buffer_slice.get_mapped_range().to_vec();
        output_buffer.unmap();

        eprintln!("Loading data to CPU");
        let buffer = image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(
            dim.padded_bytes_per_row / 4,
            dim.height,
            data,
        )
        .unwrap();

        let buffer = image::imageops::crop_imm(&buffer, 0, 0, dim.width, dim.height).to_image();

        eprintln!("Saving the file to {}", path.display());
        tokio::task::spawn_blocking(move || buffer.save(path))
            .await
            .unwrap()
    }
}
