use crate::canvas::{
    Rgba8Canvas, Rgba8,
};
use futures::executor::block_on;
use wgpu::{util::DeviceExt, BindGroupLayout};

pub struct CanvasTexture {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
}

impl CanvasTexture {
    pub fn from_image(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        canvas: &Rgba8Canvas,
        label: Option<&str>,
    ) -> Self {
        let canvas_extent = wgpu::Extent3d {
            width: canvas.width as u32,
            height: canvas.height as u32,
            depth_or_array_layers: 1,
        };

        // Canvas texture
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            size: canvas_extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            label,
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
            canvas_extent,
        );

        let canvas_texture_view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2),
            ..Default::default()
        });

        Self {
            texture,
            view: canvas_texture_view,
        }
    }
}

#[allow(dead_code)]
struct BufferDimensions {
    width: u32,
    height: u32,
    unpadded_bytes_per_row: u32,
    padded_bytes_per_row: u32
}

impl BufferDimensions {
    fn new(width: u32, height: u32) -> Self {
        let bytes_per_pixel = (Rgba8::CHANNELS * std::mem::size_of::<u8>()) as u32;
        let unpadded_bytes_per_row = width * bytes_per_pixel;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
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

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 3],
    tex_coords: [f32; 2],
    opacity: f32,
    blend: u32,
    clipped: u32,
}

fn vertices(opacity: f32, blend: u32, clipped: u32) -> [Vertex; 4] {
    [
        Vertex {
            position: [1.0, 1.0, 0.0],
            tex_coords: [1.0, 0.0],
            opacity,
            blend,
            clipped,
        },
        Vertex {
            position: [-1.0, -1.0, 0.0],
            tex_coords: [0.0, 1.0],
            opacity,
            blend,
            clipped,
        },
        Vertex {
            position: [1.0, -1.0, 0.0],
            tex_coords: [1.0, 1.0],
            opacity,
            blend,
            clipped,
        },
        Vertex {
            position: [-1.0, 1.0, 0.0],
            tex_coords: [0.0, 0.0],
            opacity,
            blend,
            clipped,
        },
    ]
}

const INDICES: &[u16] = &[0, 1, 2, 3, 1, 0];

impl Vertex {
    fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2, // NEW!
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 5]>() as wgpu::BufferAddress,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32, // NEW!
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 6]>() as wgpu::BufferAddress,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Uint32, // NEW!
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 7]>() as wgpu::BufferAddress,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Uint32, // NEW!
                },
            ],
        }
    }
}

pub fn gpu_render(width: usize, height: usize, layers: &crate::silica::SilicaGroup) {
    // The instance is a handle to our GPU
    // Backends::all => Vulkan + Metal + DX12 + Browser WebGPU
    let instance = wgpu::Instance::new(wgpu::Backends::all());

    let adapter =
        block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).unwrap();

    let (device, queue) = block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: None,
            features: wgpu::Features::empty(),
            limits: wgpu::Limits::default(),
        },
        None,
    ))
    .unwrap();
    dbg!(&device);

    let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Index Buffer"),
        contents: bytemuck::cast_slice(INDICES),
        usage: wgpu::BufferUsages::INDEX,
    });

    // It is a WebGPU requirement that ImageCopyBuffer.layout.bytes_per_row % wgpu::COPY_BYTES_PER_ROW_ALIGNMENT == 0
    // So we calculate padded_bytes_per_row by rounding unpadded_bytes_per_row
    // up to the next multiple of wgpu::COPY_BYTES_PER_ROW_ALIGNMENT.
    // https://en.wikipedia.org/wiki/Data_structure_alignment#Computing_padding
    let buffer_dimensions = BufferDimensions::new(width as u32, height as u32);
    // The output buffer lets us retrieve the data as an array
    let output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: (buffer_dimensions.padded_bytes_per_row * buffer_dimensions.height) as u64,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let sampler = device.create_sampler(&wgpu::SamplerDescriptor::default());

    let texture_extent = wgpu::Extent3d {
        width: buffer_dimensions.width,
        height: buffer_dimensions.height,
        depth_or_array_layers: 1,
    };

    let texture_bind_group_layout =
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(
                        // SamplerBindingType::Comparison is only for TextureSampleType::Depth
                        // SamplerBindingType::Filtering if the sample_type of the texture is:
                        //     TextureSampleType::Float { filterable: true }
                        // Otherwise you'll get an error.
                        wgpu::SamplerBindingType::Filtering,
                    ),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
            ],
            label: Some("texture_bind_group_layout"),
        });

    let mut prev_texture = device.create_texture(&wgpu::TextureDescriptor {
        size: texture_extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::TEXTURE_BINDING,
        label: Some("Output texture"),
    });

    // for i in 0..INSTANCES.len() as u32 {
    //     let CanvasTexture {
    //         texture: _,
    //         view: canvas_texture_view,
    //     } = CanvasTexture::from_image(&device, &queue, canvas, Some("canvas"));

    //     prev_texture = render_layer(
    //         &device,
    //         &queue,
    //         &mut prev_texture,
    //         &texture_extent,
    //         &texture_bind_group_layout,
    //         &sampler,
    //         &vertex_buffer,
    //         &index_buffer,
    //         &instance_buffer,
    //         &canvas_texture_view
    //     );
    //     dbg!(i);
    // }

    let render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Render Pipeline Layout"),
        bind_group_layouts: &[&texture_bind_group_layout],
        push_constant_ranges: &[],
    });

    let shader = device.create_shader_module(&wgpu::include_wgsl!("shader.wgsl"));

    let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Render Pipeline"),
        layout: Some(&render_pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: "vs_main", // 1.
            buffers: &[Vertex::desc()],
        },
        fragment: Some(wgpu::FragmentState {
            // 3.
            module: &shader,
            entry_point: "fs_main",
            targets: &[wgpu::ColorTargetState {
                // 4.
                format: wgpu::TextureFormat::Rgba8Unorm,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            }],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList, // 1.
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw, // 2.
            cull_mode: None,
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

    render(
        layers,
        &device,
        &queue,
        &mut prev_texture,
        &texture_extent,
        &texture_bind_group_layout,
        &render_pipeline,
        &sampler,
        // &vertex_buffer,
        &index_buffer,
    );

    {
        queue.submit(Some({
            let mut encoder =
                device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
            // Copy the data from the texture to the buffer
            encoder.copy_texture_to_buffer(
                prev_texture.as_image_copy(),
                wgpu::ImageCopyBuffer {
                    buffer: &output_buffer,
                    layout: wgpu::ImageDataLayout {
                        offset: 0,
                        bytes_per_row: Some(
                            std::num::NonZeroU32::new(
                                buffer_dimensions.padded_bytes_per_row,
                            )
                            .unwrap(),
                        ),
                        rows_per_image: None,
                    },
                },
                texture_extent,
            );

            encoder.finish()
        }));
    }

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

fn render(
    layers: &crate::silica::SilicaGroup,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    prev_texture: &mut wgpu::Texture,
    texture_extent: &wgpu::Extent3d,
    texture_bind_group_layout: &BindGroupLayout,
    render_pipeline: &wgpu::RenderPipeline,
    sampler: &wgpu::Sampler,
    // vertex_buffer: &wgpu::Buffer,
    index_buffer: &wgpu::Buffer,
) {
    let mut mask_texture_view = device
        .create_texture(&wgpu::TextureDescriptor {
            size: *texture_extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::TEXTURE_BINDING,
            label: Some("Output texture"),
        })
        .create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2),
            ..Default::default()
        });

    for layer in layers.children.iter().rev() {
        match layer {
            crate::silica::SilicaHierarchy::Group(group) => {
                if group.hidden {
                    eprintln!("Hidden group {:?}", group.name);
                    continue;
                }
                eprintln!("Into group {}", group.name);
                render(
                    group,
                    device,
                    queue,
                    prev_texture,
                    texture_extent,
                    texture_bind_group_layout,
                    render_pipeline,
                    sampler,
                    // vertex_buffer,
                    index_buffer,
                );
                eprintln!("Finished group {}", group.name);
            }
            crate::silica::SilicaHierarchy::Layer(layer) => {
                if layer.hidden {
                    eprintln!("Hidden layer {:?}", layer.name);
                    continue;
                }

                let layer_image = layer.image.as_ref().unwrap();

                let CanvasTexture {
                    texture: _,
                    view: canvas_texture_view,
                } = CanvasTexture::from_image(&device, &queue, layer_image, Some("canvas"));

                // composite.layer_blend(
                //     &layer_image,
                //     layer.opacity,
                //     match layer.blend {
                //         1 => composite::multiply,
                //         2 => composite::screen,
                //         11 => composite::overlay,
                //         0 | _ => composite::normal,
                //     },
                // );

                *prev_texture = render_layer(
                    device,
                    queue,
                    prev_texture,
                    texture_extent,
                    texture_bind_group_layout,
                    render_pipeline,
                    sampler,
                    // vertex_buffer,
                    index_buffer,
                    &canvas_texture_view,
                    layer.opacity,
                    layer.blend,
                    layer.clipped,
                    &mask_texture_view,
                );

                if !layer.clipped {
                    mask_texture_view = canvas_texture_view;
                }

                eprintln!("Finished layer {:?}: {}", layer.name, layer.blend);
            }
        }
    }
}

fn render_layer(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    prev_texture: &wgpu::Texture,
    texture_extent: &wgpu::Extent3d,
    texture_bind_group_layout: &BindGroupLayout,
    render_pipeline: &wgpu::RenderPipeline,
    sampler: &wgpu::Sampler,
    // vertex_buffer: &wgpu::Buffer,
    index_buffer: &wgpu::Buffer,
    canvas_texture_view: &wgpu::TextureView,
    layer_opacity: f32,
    layer_blend: u32,
    clipped: bool,
    mask_texture_view: &wgpu::TextureView,
) -> wgpu::Texture {
    let prev_texture_view = prev_texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2),
        ..Default::default()
    });

    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Vertex Buffer"),
        contents: bytemuck::cast_slice(&vertices(layer_opacity, layer_blend, if clipped { 1 } else { 0 })),
        usage: wgpu::BufferUsages::VERTEX,
    });

    // The render pipeline renders data into this texture
    let output_texture = device.create_texture(&wgpu::TextureDescriptor {
        size: *texture_extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::TEXTURE_BINDING,
        label: Some("Output texture"),
    });

    let output_texture_view = output_texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2),
        ..Default::default()
    });

    let canvas_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        layout: &texture_bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&canvas_texture_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&prev_texture_view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(&mask_texture_view),
            },
        ],
        label: Some("diffuse_bind_group"),
    });

    queue.submit(Some({
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[wgpu::RenderPassColorAttachment {
                view: &output_texture_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: true,
                },
            }],
            depth_stencil_attachment: None,
        });

        render_pass.set_pipeline(&render_pipeline); // 2.
        render_pass.set_bind_group(0, &canvas_bind_group, &[]); // NEW!
        render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        render_pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint16); // 1.
        render_pass.draw_indexed(0..INDICES.len() as u32, 0, 0..1);
        // 3.
        drop(render_pass);

        encoder.finish()
    }));

    output_texture
}
