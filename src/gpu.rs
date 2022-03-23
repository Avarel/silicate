use std::marker::PhantomData;

use crate::canvas::{
    pixel::{Pixel, Rgba8},
    Rgba8Canvas,
};
use futures::executor::block_on;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Instance {
    position: [f32; 2],
    layer: u32,
}

pub struct Texture {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
}

impl Texture {
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
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::TEXTURE_BINDING,
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
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        let canvas_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        Self {
            texture,
            view: canvas_texture_view,
            sampler: canvas_sampler,
        }
    }
}

struct BufferDimensions<P: Pixel> {
    width: usize,
    height: usize,
    unpadded_bytes_per_row: usize,
    padded_bytes_per_row: usize,
    _phantom: PhantomData<P>,
}

impl<P: Pixel> BufferDimensions<P> {
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
            _phantom: PhantomData::default(),
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 3],
    tex_coords: [f32; 2],
}

const VERTICES: &[Vertex] = &[
    Vertex {
        position: [0.5, 0.5, 0.0],
        tex_coords: [1.0, 0.0],
    },
    Vertex {
        position: [-0.5, -0.5, 0.0],
        tex_coords: [0.0, 1.0],
    },
    Vertex {
        position: [0.5, -0.5, 0.0],
        tex_coords: [1.0, 1.0],
    },
    Vertex {
        position: [-0.5, 0.5, 0.0],
        tex_coords: [0.0, 0.0],
    },
];

const INDICES: &[u16] = &[0, 1, 2, 3, 1, 0];

const INSTANCES: &[Instance] = &[
    Instance {
        position: [0.5, 0.5],
        layer: 1,
    },
    Instance {
        position: [-0.5, -0.5],
        layer: 1,
    },
    Instance {
        position: [0.0, 0.3],
        layer: 1,
    },
];

impl Instance {
    fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        use std::mem;
        wgpu::VertexBufferLayout {
            array_stride: mem::size_of::<Instance>() as wgpu::BufferAddress,
            // We need to switch from using a step mode of Vertex to Instance
            // This means that our shaders will only change to use the next
            // instance when the shader starts processing a new instance
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    // While our vertex shader only uses locations 0, and 1 now, in later tutorials we'll
                    // be using 2, 3, and 4, for Vertex. We'll start at slot 5 not conflict with them later
                    shader_location: 5,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // A mat4 takes up 4 vertex slots as it is technically 4 vec4s. We need to define a slot
                // for each vec4. We'll have to reassemble the mat4 in
                // the shader.
                wgpu::VertexAttribute {
                    offset: mem::size_of::<[f32; 4]>() as wgpu::BufferAddress,
                    shader_location: 6,
                    format: wgpu::VertexFormat::Uint32,
                },
            ],
        }
    }
}

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
            ],
        }
    }
}

pub fn gpu_render(canvas: &Rgba8Canvas) {
    // The instance is a handle to our GPU
    // Backends::all => Vulkan + Metal + DX12 + Browser WebGPU
    let instance = wgpu::Instance::new(wgpu::Backends::all());

    let adapter =
        block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).unwrap();

    let (device, queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: None,
        features: wgpu::Features::TEXTURE_BINDING_ARRAY,
        limits: wgpu::Limits::default(),
    }, None)).unwrap();

    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Vertex Buffer"),
        contents: bytemuck::cast_slice(VERTICES),
        usage: wgpu::BufferUsages::VERTEX,
    });

    let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Index Buffer"),
        contents: bytemuck::cast_slice(INDICES),
        usage: wgpu::BufferUsages::INDEX,
    });

    let instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Instance Buffer"),
        contents: bytemuck::cast_slice(&INSTANCES),
        usage: wgpu::BufferUsages::VERTEX,
    });

    // It is a WebGPU requirement that ImageCopyBuffer.layout.bytes_per_row % wgpu::COPY_BYTES_PER_ROW_ALIGNMENT == 0
    // So we calculate padded_bytes_per_row by rounding unpadded_bytes_per_row
    // up to the next multiple of wgpu::COPY_BYTES_PER_ROW_ALIGNMENT.
    // https://en.wikipedia.org/wiki/Data_structure_alignment#Computing_padding
    let buffer_dimensions = BufferDimensions::<Rgba8>::new(canvas.width, canvas.height);
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

    let Texture {
        texture: _,
        view: canvas_texture_view,
        sampler: canvas_sampler,
    } = Texture::from_image(&device, &queue, canvas, Some("canvas"));

    let texture_bind_group_layout =
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: Some(2.try_into().unwrap()),
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
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
            ],
            label: Some("texture_bind_group_layout"),
        });

    let canvas_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        layout: &texture_bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureViewArray(&[&canvas_texture_view, &canvas_texture_view]),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&canvas_sampler),
            },
        ],
        label: Some("diffuse_bind_group"),
    });

    // The render pipeline renders data into this texture
    let output_texture = device.create_texture(&wgpu::TextureDescriptor {
        size: texture_extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST,
        label: Some("Output texture"),
    });

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
            buffers: &[Vertex::desc(), Instance::desc()],
        },
        fragment: Some(wgpu::FragmentState {
            // 3.
            module: &shader,
            entry_point: "fs_main",
            targets: &[wgpu::ColorTargetState {
                // 4.
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
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

    let command_buffer = {
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        let output_texture_view =
            output_texture.create_view(&wgpu::TextureViewDescriptor::default());

        {
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
            render_pass.set_vertex_buffer(1, instance_buffer.slice(..));
            render_pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint16); // 1.
            render_pass.draw_indexed(0..INDICES.len() as u32, 0, 0..INSTANCES.len() as u32);
            // 3.
        }

        // Copy the data from the texture to the buffer
        encoder.copy_texture_to_buffer(
            output_texture.as_image_copy(),
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
