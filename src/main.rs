mod error;
mod gpu;
mod ns_archive;
mod silica;

use crate::{gpu::RenderState, silica::SilicaHierarchy};
use futures::executor::block_on;
use gpu::{CompositeLayer, LogicalDevice};
use image::{ImageBuffer, Rgba};
use silica::ProcreateFile;
use std::{error::Error, num::NonZeroU32};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() < 2 {
        return Ok(());
    }
    let mut procreate = ProcreateFile::open(&args[1])?;

    gpu_render(
        procreate.size.width,
        procreate.size.height,
        if procreate.background_hidden {
            None
        } else {
            Some(procreate.background_color)
        },
        &mut procreate.layers,
        procreate.render,
    );

    // canvas::adapter::adapt(procreate.composite.image.unwrap()).save("./out/reference.png")?;
    // gpu::gpu_render(&procreate.composite.image.unwrap());
    Ok(())
}

pub fn gpu_render(
    width: u32,
    height: u32,
    background: Option<[f32; 4]>,
    layers: &mut crate::silica::SilicaGroup,
    state: LogicalDevice,
) {
    let mut state = RenderState::new(width, height, background, state);

    let output_buffer = state.handle.device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: (state.buffer_dimensions.padded_bytes_per_row * state.buffer_dimensions.height)
            as u64,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    state.render(&resolve(&state, layers));

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
    let mapping = buffer_slice.map_async(wgpu::MapMode::Read);
    state.handle.device.poll(wgpu::Maintain::Wait);
    block_on(mapping).unwrap();

    let data = buffer_slice.get_mapped_range();

    eprintln!("Loading data to CPU");
    let buffer = ImageBuffer::<Rgba<u8>, _>::from_raw(
        state.buffer_dimensions.padded_bytes_per_row as u32 / 4,
        state.buffer_dimensions.height as u32,
        data,
    )
    .unwrap();
    eprintln!("Writing image");
    buffer.save("out/image.png").unwrap();
    eprintln!("Finished");
    drop(buffer);
    drop(buffer_slice);

    output_buffer.unmap();
}

fn resolve(state: &RenderState, layers: &mut crate::silica::SilicaGroup) -> Vec<CompositeLayer> {
    fn inner(
        state: &RenderState,
        layers: &mut crate::silica::SilicaGroup,
        composite_layers: &mut Vec<CompositeLayer>,
    ) {
        let mut mask_layer: Option<(usize, &crate::silica::SilicaLayer)> = None;

        for (index, layer) in layers.children.iter_mut().rev().enumerate() {
            match layer {
                SilicaHierarchy::Group(group) => {
                    if group.hidden {
                        eprintln!("Hidden group {:?}", group.name);
                        continue;
                    }
                    eprintln!("Into group {}", group.name);
                    inner(state, group, composite_layers);
                    eprintln!("Finished group {}", group.name);
                }
                SilicaHierarchy::Layer(layer) => {
                    if layer.hidden {
                        eprintln!("Hidden layer {:?}", layer.name);
                        continue;
                    }
                    if let Some((_, mask_layer)) = mask_layer {
                        if layer.clipped && mask_layer.hidden {
                            eprintln!("Hidden layer {:?} due to clip to hidden", layer.name);
                            continue;
                        }
                    }

                    let gpu_texture = layer.image.take().unwrap();

                    composite_layers.push(CompositeLayer {
                        texture: gpu_texture,
                        clipped: layer.clipped.then(|| mask_layer.unwrap().0),
                        opacity: layer.opacity,
                        blend: layer.blend,
                        name: layer.name.clone(),
                    });

                    if !layer.clipped {
                        mask_layer = Some((index, layer));
                    }

                    eprintln!("Resolved layer {:?}: {}", layer.name, layer.blend);
                }
            }
        }
    }

    let mut composite_layers = Vec::new();
    inner(&state, layers, &mut composite_layers);
    composite_layers
}
