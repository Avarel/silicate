use std::num::NonZeroU32;

use crate::{
    canvas::{Rgba8, Rgba8Canvas},
    silica::SilicaHierarchy,
};
use futures::executor::block_on;
use image::{ImageBuffer, Rgba};
use wgpu::util::DeviceExt;

const TEX_DIM: wgpu::TextureDimension = wgpu::TextureDimension::D2;
const TEX_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

pub struct GpuTexture {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
}

impl GpuTexture {
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
            dimension: TEX_DIM,
            format: TEX_FORMAT,
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
                bytes_per_row: NonZeroU32::new(4 * canvas.width as u32),
                rows_per_image: NonZeroU32::new(canvas.height as u32),
            },
            canvas_extent,
        );

        let canvas_texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

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
    padded_bytes_per_row: u32,
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
}

const SQUARE_VERTICES: &[Vertex] = &[
    Vertex {
        position: [-1.0, 1.0, 0.0],
        tex_coords: [0.0, 0.0],
    },
    Vertex {
        position: [-1.0, -1.0, 0.0],
        tex_coords: [0.0, 1.0],
    },
    Vertex {
        position: [1.0, 1.0, 0.0],
        tex_coords: [1.0, 0.0],
    },
    Vertex {
        position: [1.0, -1.0, 0.0],
        tex_coords: [1.0, 1.0],
    },
];

const INDICES: &[u16] = &[0, 1, 2, 3];

// We need this for Rust to store our data correctly for the shaders
#[repr(C)]
// This is so we can store this in a buffer
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct LayerContext {
    opacity: f32,
    blend: u32,
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

pub fn gpu_render(width: usize, height: usize, layers: &crate::silica::SilicaGroup) {
    // It is a WebGPU requirement that ImageCopyBuffer.layout.bytes_per_row % wgpu::COPY_BYTES_PER_ROW_ALIGNMENT == 0
    // So we calculate padded_bytes_per_row by rounding unpadded_bytes_per_row
    // up to the next multiple of wgpu::COPY_BYTES_PER_ROW_ALIGNMENT.
    // https://en.wikipedia.org/wiki/Data_structure_alignment#Computing_padding
    let buffer_dimensions = BufferDimensions::new(width as u32, height as u32);
    // The output buffer lets us retrieve the data as an array

    let texture_extent = wgpu::Extent3d {
        width: buffer_dimensions.width,
        height: buffer_dimensions.height,
        depth_or_array_layers: 1,
    };

    let mut state = RenderState::new(texture_extent);

    let output_buffer = state.device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: (buffer_dimensions.padded_bytes_per_row * buffer_dimensions.height) as u64,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    render(layers, &mut state);

    state.queue.submit(Some({
        let mut encoder = state
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        // Copy the data from the texture to the buffer
        encoder.copy_texture_to_buffer(
            state.composite_texture.as_image_copy(),
            wgpu::ImageCopyBuffer {
                buffer: &output_buffer,
                layout: wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: NonZeroU32::new(buffer_dimensions.padded_bytes_per_row),
                    rows_per_image: None,
                },
            },
            state.texture_extent,
        );

        encoder.finish()
    }));

    {
        let buffer_slice = output_buffer.slice(..);

        // NOTE: We have to create the mapping THEN device.poll() before await
        // the future. Otherwise the application will freeze.
        let mapping = buffer_slice.map_async(wgpu::MapMode::Read);
        state.device.poll(wgpu::Maintain::Wait);
        block_on(mapping).unwrap();

        let data = buffer_slice.get_mapped_range();

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

struct CompositeLayer {
    texture: GpuTexture,
    clipped: bool,
    opacity: f32,
    blend: u32,
}

fn resolve(layers: &crate::silica::SilicaGroup, render_state: &RenderState, composite_layers: &mut Vec<CompositeLayer>) {
    for layer in layers.children.iter().rev() {
        match layer {
            SilicaHierarchy::Group(group) => {
                if group.hidden {
                    eprintln!("Hidden group {:?}", group.name);
                    continue;
                }
                eprintln!("Into group {}", group.name);
                resolve(group, render_state, composite_layers);
                eprintln!("Finished group {}", group.name);
            }
            SilicaHierarchy::Layer(layer) => {
                if layer.hidden {
                    eprintln!("Hidden layer {:?}", layer.name);
                    continue;
                }

                let layer_image = layer.image.as_ref().unwrap();

                let gpu_texture = GpuTexture::from_image(
                    &render_state.device,
                    &render_state.queue,
                    layer_image,
                    Some("canvas"),
                );

                composite_layers.push(CompositeLayer {
                    texture: gpu_texture,
                    clipped: layer.clipped,
                    opacity: layer.opacity,
                    blend: layer.blend
                });

                eprintln!("Resolved layer {:?}: {}", layer.name, layer.blend);
            }
        }
    }
}

fn render(layers: &crate::silica::SilicaGroup, render_state: &mut RenderState) {
    let mut mask_texture_view = None;

    for layer in layers.children.iter().rev() {
        match layer {
            SilicaHierarchy::Group(group) => {
                if group.hidden {
                    eprintln!("Hidden group {:?}", group.name);
                    continue;
                }
                eprintln!("Into group {}", group.name);
                render(group, render_state);
                eprintln!("Finished group {}", group.name);
            }
            SilicaHierarchy::Layer(layer) => {
                if layer.hidden {
                    eprintln!("Hidden layer {:?}", layer.name);
                    continue;
                }

                let layer_image = layer.image.as_ref().unwrap();

                let GpuTexture {
                    texture: _,
                    view: canvas_texture_view,
                } = GpuTexture::from_image(
                    &render_state.device,
                    &render_state.queue,
                    layer_image,
                    Some("canvas"),
                );

                render_state.composite_texture = render_layer(
                    &render_state,
                    &canvas_texture_view,
                    LayerContext {
                        opacity: layer.opacity,
                        blend: layer.blend,
                    },
                    if layer.clipped {
                        &mask_texture_view
                            .as_ref()
                            .unwrap_or(&render_state.filled_clipping_mask_view)
                    } else {
                        &render_state.filled_clipping_mask_view
                    },
                );

                if !layer.clipped {
                    mask_texture_view = Some(canvas_texture_view);
                }

                eprintln!("Finished layer {:?}: {}", layer.name, layer.blend);
            }
        }
    }
}

struct RenderState {
    device: wgpu::Device,
    queue: wgpu::Queue,
    composite_texture: wgpu::Texture,
    texture_extent: wgpu::Extent3d,
    constant_bind_group: wgpu::BindGroup,
    blending_group_layout: wgpu::BindGroupLayout,
    render_pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    filled_clipping_mask_view: wgpu::TextureView,
}

impl RenderState {
    fn texture_bind_group_layout_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
        wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                multisampled: false,
                view_dimension: wgpu::TextureViewDimension::D2,
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
            },
            count: None,
        }
    }

    pub fn new(texture_extent: wgpu::Extent3d) -> Self {
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

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vertex_buffer"),
            contents: bytemuck::cast_slice(&SQUARE_VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("index_buffer"),
            contents: bytemuck::cast_slice(INDICES),
            usage: wgpu::BufferUsages::INDEX,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor::default());

        let (constant_bind_group_layout, constant_bind_group) = {
            let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("texture_bind_group_layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                    count: None,
                }],
            });

            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("constant_bind_group"),
                layout: &layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                }],
            });

            (layout, bind_group)
        };

        let blending_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("blending_group_layout"),
                entries: &[
                    Self::texture_bind_group_layout_entry(0),
                    Self::texture_bind_group_layout_entry(1),
                    Self::texture_bind_group_layout_entry(2),
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("render_pipeline_layout"),
                bind_group_layouts: &[&constant_bind_group_layout, &blending_group_layout],
                push_constant_ranges: &[],
            });

        let shader = device.create_shader_module(&wgpu::include_wgsl!("shader.wgsl"));

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("render_pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[Vertex::desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[wgpu::ColorTargetState {
                    format: TEX_FORMAT,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                }],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip, // 1.
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw, // 2.
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        let filled_clipping_mask_view = {
            let complete_clipping_mask = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("output_texture"),
                size: texture_extent,
                mip_level_count: 1,
                sample_count: 1,
                dimension: TEX_DIM,
                format: TEX_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
            });

            let view = complete_clipping_mask.create_view(&wgpu::TextureViewDescriptor::default());

            queue.submit(Some({
                let mut encoder =
                    device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

                encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                            store: true,
                        },
                    }],
                    depth_stencil_attachment: None,
                });

                encoder.finish()
            }));

            view
        };

        let composite_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("output_texture"),
            size: texture_extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: TEX_DIM,
            format: TEX_FORMAT,
            usage: wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::TEXTURE_BINDING,
        });

        Self {
            device,
            queue,
            composite_texture,
            texture_extent,
            constant_bind_group,
            blending_group_layout,
            render_pipeline,
            vertex_buffer,
            index_buffer,
            filled_clipping_mask_view,
        }
    }

    pub fn new_output_texture(&self, device: &wgpu::Device) -> GpuTexture {
        // The render pipeline renders data into this texture
        let output_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("output_texture"),
            size: self.texture_extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: TEX_DIM,
            format: TEX_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::TEXTURE_BINDING,
        });

        let output_texture_view =
            output_texture.create_view(&wgpu::TextureViewDescriptor::default());

        GpuTexture {
            texture: output_texture,
            view: output_texture_view,
        }
    }
}

fn render_layer(
    state: &RenderState,
    layer_texture_view: &wgpu::TextureView,
    layer_ctx: LayerContext,
    mask_texture_view: &wgpu::TextureView,
) -> wgpu::Texture {
    let prev_texture_view = state
        .composite_texture
        .create_view(&wgpu::TextureViewDescriptor::default());

    let ctx_buffer = state
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Context"),
            contents: bytemuck::cast_slice(&[layer_ctx]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

    let GpuTexture {
        texture: output_texture,
        view: output_texture_view,
    } = state.new_output_texture(&state.device);

    let mixing_bind_group = state.device.create_bind_group(&wgpu::BindGroupDescriptor {
        layout: &state.blending_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&prev_texture_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&mask_texture_view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&layer_texture_view),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: ctx_buffer.as_entire_binding(),
            },
        ],
        label: Some("mixing_bind_group"),
    });

    state.queue.submit(Some({
        let mut encoder = state
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

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

        render_pass.set_pipeline(&state.render_pipeline);
        render_pass.set_bind_group(0, &state.constant_bind_group, &[]);
        render_pass.set_bind_group(1, &mixing_bind_group, &[]);
        render_pass.set_vertex_buffer(0, state.vertex_buffer.slice(..));
        render_pass.set_index_buffer(state.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        render_pass.draw_indexed(0..INDICES.len() as u32, 0, 0..1);
        drop(render_pass);

        encoder.finish()
    }));

    output_texture
}
