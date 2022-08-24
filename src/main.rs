mod error;
mod gpu;
mod ns_archive;
mod silica;

use crate::{gpu::RenderState, silica::SilicaHierarchy};
use futures::executor::block_on;
use gpu::{CompositeLayer, LogicalDevice};
use image::{ImageBuffer, Rgba};
use silica::{ProcreateFile, SilicaError};
use std::{error::Error, num::NonZeroU32};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() < 2 {
        return Ok(());
    }

    let device =
        futures::executor::block_on(LogicalDevice::new()).ok_or(SilicaError::NoGraphicsDevice)?;

    let procreate = ProcreateFile::open(&args[1], &device)?;

    gpu_render(&procreate, false, &device, "out/image.png");
    gpu_render(&procreate, true, &device, "out/reference.png");

    Ok(())
}

pub fn gpu_render(
    pc: &ProcreateFile,
    composite_reference: bool,
    state: &LogicalDevice,
    out_path: &str,
) {
    let mut state = RenderState::new(
        pc.size.width,
        pc.size.height,
        (pc.flipped_horizontally, pc.flipped_vertically),
        (!pc.background_hidden).then_some(pc.background_color),
        state,
    );

    let output_buffer = state.handle.device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: (state.buffer_dimensions.padded_bytes_per_row * state.buffer_dimensions.height)
            as u64,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    if composite_reference {
        state.render(&[CompositeLayer {
            texture: pc.composite.image.as_ref().unwrap(),
            clipped: None,
            opacity: 1.0,
            blend: 0,
            name: Some("Composite"),
        }]);
    } else {
        state.render(&resolve(&state, &pc.layers));
    }

    state.handle.queue.submit(Some({
        let mut encoder = state
            .handle
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        // Copy the data from the texture to the buffer
        encoder.copy_texture_to_buffer(
            state.composite_texture.as_image_copy(),
            wgpu::ImageCopyBuffer {
                buffer: &output_buffer,
                layout: wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: NonZeroU32::new(state.buffer_dimensions.padded_bytes_per_row),
                    rows_per_image: None,
                },
            },
            state.texture_extent,
        );

        encoder.finish()
    }));

    let buffer_slice = output_buffer.slice(..);

    // NOTE: We have to create the mapping THEN device.poll() before await
    // the future. Otherwise the application will freeze.
    let (tx, rx) = futures::channel::oneshot::channel();
    buffer_slice.map_async(wgpu::MapMode::Read, move |result| tx.send(result).unwrap());
    state.handle.device.poll(wgpu::Maintain::Wait);
    block_on(rx).unwrap().unwrap();

    let data = buffer_slice.get_mapped_range();

    // eprintln!("Loading data to CPU");
    // let buffer = ImageBuffer::<Rgba<u8>, _>::from_raw(
    //     state.buffer_dimensions.padded_bytes_per_row as u32 / 4,
    //     state.buffer_dimensions.height as u32,
    //     data,
    // )
    // .unwrap();
    // eprintln!("Writing image");

    eprintln!("Loading data to CPU");
    let mut buffer = ImageBuffer::<Rgba<u8>, _>::from_raw(
        state.buffer_dimensions.padded_bytes_per_row as u32 / 4,
        state.buffer_dimensions.height as u32,
        data.to_vec(),
    )
    .unwrap();
    eprintln!("Rotating image");

    buffer = image::imageops::crop_imm(&buffer, 0, 0, pc.size.width, pc.size.height).to_image();
    match pc.orientation {
        0 => {}
        1 | 4 => buffer = image::imageops::rotate90(&buffer),
        2 => buffer = image::imageops::rotate180(&buffer),
        3 => buffer = image::imageops::rotate270(&buffer),
        _ => println!("Unknown orientation!"),
    };
    eprintln!("Writing image");

    buffer.save(out_path).unwrap();

    eprintln!("Finished");
    drop(buffer);
    drop(buffer_slice);

    // output_buffer.unmap();
}

fn resolve<'a>(
    state: &RenderState,
    layers: &'a crate::silica::SilicaGroup,
) -> Vec<CompositeLayer<'a>> {
    fn inner<'a>(
        state: &RenderState,
        layers: &'a crate::silica::SilicaGroup,
        composite_layers: &mut Vec<CompositeLayer<'a>>,
    ) {
        let mut mask_layer: Option<(usize, &crate::silica::SilicaLayer)> = None;

        for (index, layer) in layers.children.iter().rev().enumerate() {
            match layer {
                SilicaHierarchy::Group(group) if !group.hidden => {
                    inner(state, group, composite_layers);
                }
                SilicaHierarchy::Layer(layer) if !layer.hidden => {
                    if let Some((_, mask_layer)) = mask_layer {
                        if layer.clipped && mask_layer.hidden {
                            // eprintln!("Hidden layer {:?} due to clip to hidden", layer.name);
                            continue;
                        }
                    }

                    let gpu_texture = layer.image.as_ref().unwrap();

                    composite_layers.push(CompositeLayer {
                        texture: gpu_texture,
                        clipped: layer.clipped.then(|| mask_layer.unwrap().0),
                        opacity: layer.opacity,
                        blend: layer.blend,
                        name: layer.name.as_deref(),
                    });

                    if !layer.clipped {
                        mask_layer = Some((index, layer));
                    }

                    // eprintln!("Resolved layer {:?}: {}", layer.name, layer.blend);
                }
                _ => continue,
            }
        }
    }

    let mut composite_layers = Vec::new();
    inner(&state, layers, &mut composite_layers);
    composite_layers
}
