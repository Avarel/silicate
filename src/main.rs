mod canvas;
mod composite;
mod error;
mod ns_archive;
mod silica;

use canvas::Rgba8Canvas;
use silica::{ProcreateFile, SilicaGroup, SilicaHierarchy};
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let mut procreate = ProcreateFile::open("./Gilvana.procreate")?;

    let mut composite = Rgba8Canvas::new(
        procreate.size.width as usize,
        procreate.size.height as usize,
    );
    //RgbaImage::new(procreate.size.width, procreate.size.height);
    render(&mut composite, &mut procreate.layers);
    canvas::adapter::adapt(composite).save("./out/final.png")?;

    // canvas::adapter::adapt(procreate.composite.image.unwrap()).save("./out/reference.png")?;
    gpu_render(&procreate.composite.image.unwrap());
    Ok(())
}

fn render(composite: &mut Rgba8Canvas, layers: &SilicaGroup) {
    let mut mask: Option<Rgba8Canvas> = None;

    for layer in layers.children.iter().rev() {
        match layer {
            SilicaHierarchy::Group(group) => {
                if group.hidden {
                    eprintln!("Hidden group {:?}", group.name);
                    continue;
                }
                eprintln!("Into group {}", group.name);
                render(composite, group);
                eprintln!("Finished group {}", group.name);
            }
            SilicaHierarchy::Layer(layer) => {
                if layer.hidden {
                    eprintln!("Hidden layer {:?}", layer.name);
                    continue;
                }

                let mut layer_image = layer.image.clone().unwrap();

                if layer.clipped {
                    if let Some(mask) = &mask {
                        layer_image.layer_clip(&mask, layer.opacity);
                    }
                }

                composite.layer_blend(
                    &layer_image,
                    layer.opacity,
                    match layer.blend {
                        1 => composite::multiply,
                        2 => composite::screen,
                        11 => composite::overlay,
                        0 | _ => composite::normal,
                    },
                );

                if !layer.clipped {
                    mask = Some(layer_image);
                }

                eprintln!("Finished layer {:?}: {}", layer.name, layer.blend);
            }
        }
    }
}

struct BufferDimensions<P: canvas::pixel::Pixel> {
    width: usize,
    height: usize,
    unpadded_bytes_per_row: usize,
    padded_bytes_per_row: usize,
    _phantom: std::marker::PhantomData<P>,
}

impl<P: canvas::pixel::Pixel> BufferDimensions<P> {
    fn new(width: usize, height: usize) -> Self {
        let bytes_per_pixel = P::CHANNELS * std::mem::size_of::<P::DATA>();
        let unpadded_bytes_per_row = width * bytes_per_pixel;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as usize;
        let padded_bytes_per_row_padding = (align - unpadded_bytes_per_row % align) % align;
        let padded_bytes_per_row = unpadded_bytes_per_row + padded_bytes_per_row_padding;
        Self {
            width,
            height,
            unpadded_bytes_per_row,
            padded_bytes_per_row,
            _phantom: std::marker::PhantomData::default(),
        }
    }
}

fn gpu_render(canvas: &Rgba8Canvas) {
    use futures::executor::block_on;
    // The instance is a handle to our GPU
    // Backends::all => Vulkan + Metal + DX12 + Browser WebGPU
    let instance = wgpu::Instance::new(wgpu::Backends::all());

    let adapter =
        block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).unwrap();

    let (device, queue) = block_on(adapter.request_device(&Default::default(), None)).unwrap();

    // It is a WebGPU requirement that ImageCopyBuffer.layout.bytes_per_row % wgpu::COPY_BYTES_PER_ROW_ALIGNMENT == 0
    // So we calculate padded_bytes_per_row by rounding unpadded_bytes_per_row
    // up to the next multiple of wgpu::COPY_BYTES_PER_ROW_ALIGNMENT.
    // https://en.wikipedia.org/wiki/Data_structure_alignment#Computing_padding
    let buffer_dimensions =
        BufferDimensions::<canvas::pixel::Rgba8>::new(canvas.width, canvas.height);
    // The output buffer lets us retrieve the data as an array
    let output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: (buffer_dimensions.padded_bytes_per_row * buffer_dimensions.height) as u64,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let texture_extent = wgpu::Extent3d {
        width: buffer_dimensions.width as u32,
        height: buffer_dimensions.height as u32,
        depth_or_array_layers: 1,
    };

    let bg_texture = device.create_texture(&wgpu::TextureDescriptor {
        size: texture_extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST,
        label: None,
    });

    // The render pipeline renders data into this texture
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        size: texture_extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST,
        label: None,
    });

    queue.write_texture(
        // Tells wgpu where to copy the pixel data
        wgpu::ImageCopyTexture {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        // The actual pixel data
        &canvas.data,
        // The layout of the texture
        wgpu::ImageDataLayout {
            offset: 0,
            bytes_per_row: std::num::NonZeroU32::new(4 * canvas.width as u32),
            rows_per_image: std::num::NonZeroU32::new(canvas.height as u32),
        },
        texture_extent,
    );

    let render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Render Pipeline Layout"),
        bind_group_layouts: &[],
        push_constant_ranges: &[],
    });

    let shader = device.create_shader_module(&wgpu::include_wgsl!("shader.wgsl"));

    let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Render Pipeline"),
        layout: Some(&render_pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: "vs_main", // 1.
            buffers: &[],           // 2.
        },
        fragment: Some(wgpu::FragmentState {
            // 3.
            module: &shader,
            entry_point: "fs_main",
            targets: &[
                // wgpu::ColorTargetState {
                //     // 4.
                //     format: wgpu::TextureFormat::Rgba8UnormSrgb,
                //     blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                //     write_mask: wgpu::ColorWrites::ALL,
                // },
                wgpu::ColorTargetState {
                    // 4.
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                },
            ],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList, // 1.
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw, // 2.
            cull_mode: Some(wgpu::Face::Back),
            // Setting this to anything other than Fill requires Features::NON_FILL_POLYGON_MODE
            polygon_mode: wgpu::PolygonMode::Fill,
            // Requires Features::DEPTH_CLIP_CONTROL
            unclipped_depth: false,
            // Requires Features::CONSERVATIVE_RASTERIZATION
            conservative: false,
        },
        depth_stencil: None, // 1.
        multisample: wgpu::MultisampleState {
            count: 1,                         // 2.
            mask: !0,                         // 3.
            alpha_to_coverage_enabled: false, // 4.
        },
        multiview: None, // 5.
    });

    let command_buffer = {
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        let bg_view = bg_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[
                    // wgpu::RenderPassColorAttachment {
                    //     view: &bg_view,
                    //     resolve_target: None,
                    //     ops: wgpu::Operations {
                    //         load: wgpu::LoadOp::Clear(wgpu::Color::RED),
                    //         store: true,
                    //     },
                    // },
                    wgpu::RenderPassColorAttachment {
                        view: &texture_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: true,
                        },
                    },
                ],
                depth_stencil_attachment: None,
            });

            render_pass.set_pipeline(&render_pipeline); // 2.
            render_pass.draw(0..3, 0..1); // 3.
        }

        // Copy the data from the texture to the buffer
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::ImageCopyBuffer {
                buffer: &output_buffer,
                layout: wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(
                        std::num::NonZeroU32::new(buffer_dimensions.padded_bytes_per_row as u32)
                            .unwrap(),
                    ),
                    rows_per_image: None,
                },
            },
            texture_extent,
        );

        encoder.finish()
    };

    queue.submit(Some(command_buffer));

    {
        let buffer_slice = output_buffer.slice(..);

        // NOTE: We have to create the mapping THEN device.poll() before await
        // the future. Otherwise the application will freeze.
        let mapping = buffer_slice.map_async(wgpu::MapMode::Read);
        device.poll(wgpu::Maintain::Wait);
        futures::executor::block_on(mapping).unwrap();

        let data = buffer_slice.get_mapped_range();

        use image::{ImageBuffer, Rgba};
        let buffer = ImageBuffer::<Rgba<u8>, _>::from_raw(
            buffer_dimensions.padded_bytes_per_row as u32 / 4,
            buffer_dimensions.height as u32,
            data,
        )
        .unwrap();
        buffer.save("out/image.png").unwrap();
    }
    output_buffer.unmap();
}
