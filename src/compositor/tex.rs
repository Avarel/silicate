use std::num::NonZeroU32;

use super::{dev::LogicalDevice, BufferDimensions};

const TEX_DIM: wgpu::TextureDimension = wgpu::TextureDimension::D2;
pub(super) const TEX_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

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

    pub fn empty(
        dev: &LogicalDevice,
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

        Self::empty_with_extent(dev, size, label, usage)
    }

    pub fn empty_with_extent(
        dev: &LogicalDevice,
        size: wgpu::Extent3d,
        label: Option<&str>,
        usage: wgpu::TextureUsages,
    ) -> Self {
        // Canvas texture
        let texture = dev.device.create_texture(&wgpu::TextureDescriptor {
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

    pub fn make_view(&self) -> wgpu::TextureView {
        self.texture
            .create_view(&wgpu::TextureViewDescriptor::default())
    }

    pub fn clear(&self, dev: &LogicalDevice, color: wgpu::Color) {
        dev.queue.submit(Some({
            let mut encoder = dev
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

            encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.make_view(),
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(color),
                        store: true,
                    },
                })],
                depth_stencil_attachment: None,
            });

            encoder.finish()
        }));
    }

    pub fn replace(
        &self,
        dev: &LogicalDevice,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        data: &[u8],
    ) {
        dev.queue.write_texture(
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

    pub fn export(&self, dev: &LogicalDevice, dim: BufferDimensions) {
        let output_buffer = dev.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: (dim.padded_bytes_per_row * dim.height) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            // Copying texture to buffer requires that the buffer is not mapped
            mapped_at_creation: false,
        });

        dev.queue.submit(Some({
            let mut encoder = dev
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
            // Copy the data from the texture to the buffer
            encoder.copy_texture_to_buffer(
                self.texture.as_image_copy(),
                wgpu::ImageCopyBuffer {
                    buffer: &output_buffer,
                    layout: wgpu::ImageDataLayout {
                        offset: 0,
                        bytes_per_row: NonZeroU32::new(dim.padded_bytes_per_row),
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
        let (tx, rx) = futures::channel::oneshot::channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |result| tx.send(result).unwrap());
        dev.device.poll(wgpu::Maintain::Wait);
        futures::executor::block_on(rx).unwrap().unwrap();

        let data = buffer_slice.get_mapped_range().to_vec();
        drop(buffer_slice);
        output_buffer.unmap();

        eprintln!("Loading data to CPU");
        let buffer = image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(
            dim.padded_bytes_per_row as u32 / 4,
            dim.height as u32,
            data,
        )
        .unwrap();

        let buffer = image::imageops::crop_imm(&buffer, 0, 0, dim.width, dim.height).to_image();

        eprintln!("Writing image");

        buffer.save("out/output.png").unwrap();

        eprintln!("Finished");
    }
}
