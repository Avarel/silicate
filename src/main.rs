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

struct BufferDimensions {
    width: usize,
    height: usize,
    unpadded_bytes_per_row: usize,
    padded_bytes_per_row: usize,
}

impl BufferDimensions {
    fn new(width: usize, height: usize) -> Self {
        let bytes_per_pixel = std::mem::size_of::<u32>();
        let unpadded_bytes_per_row = width * bytes_per_pixel;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as usize;
        let padded_bytes_per_row_padding = (align - unpadded_bytes_per_row % align) % align;
        let padded_bytes_per_row = unpadded_bytes_per_row + padded_bytes_per_row_padding;
        Self {
            width,
            height,
            unpadded_bytes_per_row,
            padded_bytes_per_row,
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

    // let texture_size = 256u32;

    let texture_desc = wgpu::TextureDescriptor {
        size: wgpu::Extent3d {
            width: canvas.width as u32,
            height: canvas.height as u32,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING,
        label: None,
    };
    let texture = device.create_texture(&texture_desc);
    // let texture_view = texture.create_view(&Default::default());

    let texture_view = texture.create_view(&Default::default());
    // let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
    //     address_mode_u: wgpu::AddressMode::ClampToEdge,
    //     address_mode_v: wgpu::AddressMode::ClampToEdge,
    //     address_mode_w: wgpu::AddressMode::ClampToEdge,
    //     mag_filter: wgpu::FilterMode::Linear,
    //     min_filter: wgpu::FilterMode::Nearest,
    //     mipmap_filter: wgpu::FilterMode::Nearest,
    //     ..Default::default()
    // });

    // let texture_bind_group_layout = device.create_bind_group_layout(
    //     &wgpu::BindGroupLayoutDescriptor {
    //         entries: &[
    //             wgpu::BindGroupLayoutEntry {
    //                 binding: 0,
    //                 visibility: wgpu::ShaderStages::FRAGMENT,
    //                 ty: wgpu::BindingType::Texture {
    //                     multisampled: false,
    //                     view_dimension: wgpu::TextureViewDimension::D2,
    //                     sample_type: wgpu::TextureSampleType::Float { filterable: true },
    //                 },
    //                 count: None,
    //             },
    //             wgpu::BindGroupLayoutEntry {
    //                 binding: 1,
    //                 visibility: wgpu::ShaderStages::FRAGMENT,
    //                 ty: wgpu::BindingType::Sampler(
    //                     // SamplerBindingType::Comparison is only for TextureSampleType::Depth
    //                     // SamplerBindingType::Filtering if the sample_type of the texture is:
    //                     //     TextureSampleType::Float { filterable: true }
    //                     // Otherwise you'll get an error.
    //                     wgpu::SamplerBindingType::Filtering,
    //                 ),
    //                 count: None,
    //             },
    //         ],
    //         label: Some("texture_bind_group_layout"),
    //     }
    // );

    // let diffuse_bind_group = device.create_bind_group(
    //     &wgpu::BindGroupDescriptor {
    //         layout: &texture_bind_group_layout,
    //         entries: &[
    //             wgpu::BindGroupEntry {
    //                 binding: 0,
    //                 resource: wgpu::BindingResource::TextureView(&texture_view),
    //             },
    //             wgpu::BindGroupEntry {
    //                 binding: 1,
    //                 resource: wgpu::BindingResource::Sampler(&sampler),
    //             }
    //         ],
    //         label: Some("diffuse_bind_group"),
    //     }
    // );

    // queue.write_texture(
    //     // Tells wgpu where to copy the pixel data
    //     wgpu::ImageCopyTexture {
    //         texture: &texture,
    //         mip_level: 0,
    //         origin: wgpu::Origin3d::ZERO,
    //         aspect: wgpu::TextureAspect::All,
    //     },
    //     // The actual pixel data
    //     &canvas.data,
    //     // The layout of the texture
    //     wgpu::ImageDataLayout {
    //         offset: 0,
    //         bytes_per_row: std::num::NonZeroU32::new(4 * canvas.width as u32),
    //         rows_per_image: std::num::NonZeroU32::new(canvas.height as u32),
    //     },
    //     wgpu::Extent3d {
    //         width: canvas.width as u32,
    //         height: canvas.height as u32,
    //         depth_or_array_layers: 1,
    //     },
    // );

    // let u32_size = std::mem::size_of::<u32>() as u32;
    // let output_buffer_size = (4 * canvas.width as u32 * canvas.height as u32) as wgpu::BufferAddress;
    let buffer_dimensions = BufferDimensions::new(canvas.width, canvas.height);
    let output_buffer_desc = wgpu::BufferDescriptor {
        size: (buffer_dimensions.padded_bytes_per_row * buffer_dimensions.height) as u64,
        usage: wgpu::BufferUsages::COPY_DST
            // this tells wpgu that we want to read this buffer from the cpu
            | wgpu::BufferUsages::MAP_READ,
        label: None,
        mapped_at_creation: false,
    };
    let output_buffer = device.create_buffer(&output_buffer_desc);

    let shader = device.create_shader_module(&wgpu::include_wgsl!("shader.wgsl"));

    let render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Render Pipeline Layout"),
        bind_group_layouts: &[], //&[&texture_bind_group_layout],
        push_constant_ranges: &[],
    });

    let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        multiview: None,
        label: Some("Render Pipeline"),
        layout: Some(&render_pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: "vs_main",
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            // 3.
            module: &shader,
            entry_point: "fs_main",
            targets: &[wgpu::ColorTargetState {
                // 4.
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            }],
        }),
        primitive: wgpu::PrimitiveState {
            conservative: false,
            unclipped_depth: false,
            topology: wgpu::PrimitiveTopology::TriangleStrip,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: Some(wgpu::Face::Back),
            // Setting this to anything other than Fill requires Features::NON_FILL_POLYGON_MODE
            polygon_mode: wgpu::PolygonMode::Fill,
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
    });

    dbg!("PAIN");

    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

    dbg!("HUH");

    {
        let render_pass_desc = wgpu::RenderPassDescriptor {
            label: Some("Render Pass"),
            color_attachments: &[wgpu::RenderPassColorAttachment {
                view: &texture_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::GREEN),
                    store: true,
                },
            }],
            depth_stencil_attachment: None,
        };
        let mut render_pass = encoder.begin_render_pass(&render_pass_desc);

        render_pass.set_pipeline(&render_pipeline);
        // render_pass.set_bind_group(0, &diffuse_bind_group, &[]);
        render_pass.draw(0..3, 0..1);
    }

    dbg!("yur");
    encoder.copy_texture_to_buffer(
        wgpu::ImageCopyTexture {
            aspect: wgpu::TextureAspect::All,
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
        },
        wgpu::ImageCopyBuffer {
            buffer: &output_buffer,
            layout: wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: std::num::NonZeroU32::new(
                    (buffer_dimensions.padded_bytes_per_row) as u32,
                ),
                rows_per_image: std::num::NonZeroU32::new(buffer_dimensions.height as u32),
            },
        },
        texture_desc.size,
    );

    dbg!("yee");

    queue.submit(Some(encoder.finish()));

    dbg!("wow");

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
