pub mod dev;
pub mod tex;

use self::{dev::LogicalDevice, tex::GpuTexture};
use crate::silica::BlendingMode;
use image::{Pixel, Rgba};
use std::num::NonZeroU32;
use wgpu::{util::DeviceExt, CommandEncoder};

const TEX_DIM: wgpu::TextureDimension = wgpu::TextureDimension::D2;
const TEX_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const CHUNKS: u32 = 32;

#[derive(Debug, Clone, Copy)]
pub struct BufferDimensions {
    pub width: u32,
    pub height: u32,
    pub unpadded_bytes_per_row: u32,
    pub padded_bytes_per_row: u32,
}

impl BufferDimensions {
    fn new(width: u32, height: u32) -> Self {
        // It is a WebGPU requirement that ImageCopyBuffer.layout.bytes_per_row % wgpu::COPY_BYTES_PER_ROW_ALIGNMENT == 0
        // So we calculate padded_bytes_per_row by rounding unpadded_bytes_per_row
        // up to the next multiple of wgpu::COPY_BYTES_PER_ROW_ALIGNMENT.
        // https://en.wikipedia.org/wiki/Data_structure_alignment#Computing_padding
        let bytes_per_pixel =
            (usize::from(Rgba::<u8>::CHANNEL_COUNT) * std::mem::size_of::<u8>()) as u32;
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
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable, Default)]
struct Vertex {
    position: [f32; 3],
    bg_coords: [f32; 2],
    fg_coords: [f32; 2],
}

const SQUARE_VERTICES: [Vertex; 4] = [
    Vertex {
        position: [-1.0, 1.0, 0.0],
        bg_coords: [0.0, 0.0],
        fg_coords: [0.0, 1.0],
    },
    Vertex {
        position: [-1.0, -1.0, 0.0],
        bg_coords: [0.0, 1.0],
        fg_coords: [0.0, 0.0],
    },
    Vertex {
        position: [1.0, 1.0, 0.0],
        bg_coords: [1.0, 0.0],
        fg_coords: [1.0, 1.0],
    },
    Vertex {
        position: [1.0, -1.0, 0.0],
        bg_coords: [1.0, 1.0],
        fg_coords: [1.0, 0.0],
    },
];

const INDICES: &[u16] = &[0, 1, 2, 3];

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct LayerContext {
    opacity: f32,
    blend: u32,
    _padding: [f32; 2],
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
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 2 + 3]>() as wgpu::BufferAddress,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x2,
                },
            ],
        }
    }
}

pub struct CompositeLayer<'a> {
    pub texture: &'a GpuTexture,
    pub clipped: Option<usize>,
    pub opacity: f32,
    pub blend: BlendingMode,
    pub name: Option<&'a str>,
}

pub struct Compositor<'device> {
    pub handle: &'device LogicalDevice,
    pub buffer_dimensions: BufferDimensions,
    pub texture_extent: wgpu::Extent3d,
    vertices: [Vertex; 4],
    background: Option<[f32; 4]>,
    constant_bind_group: wgpu::BindGroup,
    blending_group_layout: wgpu::BindGroupLayout,
    render_pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    filled_clipping_mask: wgpu::Texture,
}

impl<'device> Compositor<'device> {
    pub fn flip_vertices(&mut self, flip_hv: (bool, bool)) {
        for v in &mut self.vertices {
            v.fg_coords = [
                if flip_hv.0 {
                    1.0 - v.fg_coords[0]
                } else {
                    v.fg_coords[0]
                },
                if flip_hv.1 {
                    1.0 - v.fg_coords[1]
                } else {
                    v.fg_coords[1]
                },
            ];
        }
        self.reload_vertices_buffer();
    }

    pub fn rotate_vertices(&mut self, ccw: bool) {
        let temp = self.vertices[0].fg_coords;
        if ccw {
            self.vertices[0].fg_coords = self.vertices[1].fg_coords;
            self.vertices[1].fg_coords = self.vertices[3].fg_coords;
            self.vertices[3].fg_coords = self.vertices[2].fg_coords;
            self.vertices[2].fg_coords = temp;
        } else {
            self.vertices[0].fg_coords = self.vertices[2].fg_coords;
            self.vertices[2].fg_coords = self.vertices[3].fg_coords;
            self.vertices[3].fg_coords = self.vertices[1].fg_coords;
            self.vertices[1].fg_coords = temp;
        }
        self.reload_vertices_buffer();
    }

    pub fn tranpose_dimensions(&mut self) {
        let buffer_dimensions =
            BufferDimensions::new(self.buffer_dimensions.height, self.buffer_dimensions.width);

        let texture_extent = wgpu::Extent3d {
            width: buffer_dimensions.width,
            height: buffer_dimensions.height,
            depth_or_array_layers: 1,
        };

        let filled_clipping_mask = {
            let texture = self.handle.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("filled_clipping_mask"),
                size: texture_extent,
                mip_level_count: 1,
                sample_count: 1,
                dimension: TEX_DIM,
                format: TEX_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
            });

            let view = default_view(&texture);

            self.handle.queue.submit(Some({
                let mut encoder = self
                    .handle
                    .device
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

                encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                            store: true,
                        },
                    })],
                    depth_stencil_attachment: None,
                });

                encoder.finish()
            }));

            texture
        };

        self.buffer_dimensions = buffer_dimensions;
        self.texture_extent = texture_extent;
        self.filled_clipping_mask = filled_clipping_mask;
    }

    pub fn reload_vertices_buffer(&mut self) {
        self.vertex_buffer =
            self.handle
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("vertex_buffer"),
                    contents: bytemuck::cast_slice(&self.vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                });
    }

    pub fn new(
        width: u32,
        height: u32,
        background: Option<[f32; 4]>,
        handle: &'device LogicalDevice,
    ) -> Self {
        let LogicalDevice {
            ref device,
            ref queue,
            ..
        } = handle;

        let vertices = SQUARE_VERTICES;

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vertex_buffer"),
            contents: bytemuck::cast_slice(&vertices),
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
                    fragment_bgl_tex_entry(0, None),
                    fragment_bgl_tex_entry(1, NonZeroU32::new(CHUNKS)),
                    fragment_bgl_tex_entry(2, NonZeroU32::new(CHUNKS)),
                    fragment_bgl_buffer_ro_entry(3, NonZeroU32::new(CHUNKS)),
                    fragment_bgl_uniform_entry(4),
                ],
            });

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("render_pipeline_layout"),
                bind_group_layouts: &[&constant_bind_group_layout, &blending_group_layout],
                push_constant_ranges: &[],
            });

        // wgpu::include_wgsl!("shader.wgsl")
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Dynamically loaded shader module"),
            source: wgpu::ShaderSource::Wgsl({
                use std::fs::OpenOptions;
                use std::io::Read;
                let mut file = OpenOptions::new()
                    .read(true)
                    .open("./src/shader.wgsl")
                    .unwrap();

                let mut buf = String::new();
                file.read_to_string(&mut buf).unwrap();
                buf.into()
            }),
        });

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
                targets: &[Some(wgpu::ColorTargetState {
                    format: TEX_FORMAT,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
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

        let buffer_dimensions = BufferDimensions::new(width, height);
        // The output buffer lets us retrieve the data as an array

        let texture_extent = wgpu::Extent3d {
            width: buffer_dimensions.width,
            height: buffer_dimensions.height,
            depth_or_array_layers: 1,
        };

        let filled_clipping_mask = {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("filled_clipping_mask"),
                size: texture_extent,
                mip_level_count: 1,
                sample_count: 1,
                dimension: TEX_DIM,
                format: TEX_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
            });

            let view = default_view(&texture);

            queue.submit(Some({
                let mut encoder =
                    device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

                encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                            store: true,
                        },
                    })],
                    depth_stencil_attachment: None,
                });

                encoder.finish()
            }));

            texture
        };

        Self {
            handle,
            buffer_dimensions,
            texture_extent,
            constant_bind_group,
            blending_group_layout,
            background,
            render_pipeline,
            vertices,
            vertex_buffer,
            index_buffer,
            filled_clipping_mask,
        }
    }

    pub fn base_composite_texture(&self) -> wgpu::Texture {
        let GpuTexture { texture, .. } = GpuTexture::empty_with_extent(
            &self.handle.device,
            self.texture_extent,
            None,
            GpuTexture::output_usage(),
        );

        if let Some([r, g, b, a]) = self.background {
            let view = default_view(&texture);

            self.handle.queue.submit(Some({
                let mut encoder = self
                    .handle
                    .device
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

                encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: f64::from(r),
                                g: f64::from(g),
                                b: f64::from(b),
                                a: f64::from(a),
                            }),
                            store: true,
                        },
                    })],
                    depth_stencil_attachment: None,
                });

                encoder.finish()
            }));
        }

        texture
    }

    pub fn render(&self, layers: &[CompositeLayer]) -> wgpu::Texture {
        let mut composite_texture = self.base_composite_texture();

        self.handle.queue.submit(Some({
            let mut encoder = self
                .handle
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

            for chunked_layers in layers.chunks(CHUNKS as usize) {
                composite_texture =
                    self.render_pass(&mut encoder, composite_texture, layers, chunked_layers);
            }
            encoder.finish()
        }));

        composite_texture
    }

    fn render_pass(
        &self,
        encoder: &mut CommandEncoder,
        composite_texture: wgpu::Texture,
        layers: &[CompositeLayer],
        chunked_layers: &[CompositeLayer],
    ) -> wgpu::Texture {
        let prev_texture_view = default_view(&composite_texture);

        let mut mask_views: Vec<wgpu::TextureView> = Vec::with_capacity(CHUNKS as usize);
        let mut layer_views = Vec::with_capacity(CHUNKS as usize);
        let mut ctxs = Vec::with_capacity(CHUNKS as usize);

        for layer in chunked_layers.iter() {
            mask_views.push(default_view(if let Some(mask_layer) = layer.clipped {
                &layers[mask_layer].texture.texture
            } else {
                &self.filled_clipping_mask
            }));
            layer_views.push(default_view(&layer.texture.texture));
            ctxs.push(LayerContext {
                opacity: layer.opacity,
                blend: layer.blend.to_u32(),
                _padding: [0.0; 2],
            });
        }

        // Fill with dummy
        for _ in 0..CHUNKS - chunked_layers.len() as u32 {
            mask_views.push(default_view(&self.filled_clipping_mask));
            layer_views.push(default_view(&self.filled_clipping_mask));
            ctxs.push(LayerContext {
                opacity: 0.0,
                blend: 0,
                _padding: [0.0; 2],
            });
        }

        let ctx_buffer = self
            .handle
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Context"),
                contents: bytemuck::cast_slice(&ctxs),
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            });

        let count_buffer =
            self.handle
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Context"),
                    contents: bytemuck::cast_slice(&[chunked_layers.len()]),
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                });

        let GpuTexture {
            texture: output_texture,
            ..
        } = GpuTexture::empty_with_extent(
            &self.handle.device,
            self.texture_extent,
            None,
            GpuTexture::output_usage(),
        );
        let output_texture_view = default_view(&output_texture);

        let blending_bind_group =
            self.handle
                .device
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    layout: &self.blending_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&prev_texture_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureViewArray(
                                &mask_views.iter().collect::<Vec<_>>(),
                            ),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::TextureViewArray(
                                &layer_views.iter().collect::<Vec<_>>(),
                            ),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: ctx_buffer.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 4,
                            resource: count_buffer.as_entire_binding(),
                        },
                    ],
                    label: Some("mixing_bind_group"),
                });

        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &output_texture_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: true,
                },
            })],
            depth_stencil_attachment: None,
        });

        render_pass.set_pipeline(&self.render_pipeline);
        render_pass.set_bind_group(0, &self.constant_bind_group, &[]);
        render_pass.set_bind_group(1, &blending_bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        render_pass.draw_indexed(0..INDICES.len() as u32, 0, 0..1);
        drop(render_pass);

        output_texture
    }
}

fn default_view(tex: &wgpu::Texture) -> wgpu::TextureView {
    tex.create_view(&wgpu::TextureViewDescriptor::default())
}

fn fragment_bgl_tex_entry(binding: u32, count: Option<NonZeroU32>) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            multisampled: false,
            view_dimension: wgpu::TextureViewDimension::D2,
            sample_type: wgpu::TextureSampleType::Float { filterable: false },
        },
        count,
    }
}

fn fragment_bgl_buffer_ro_entry(
    binding: u32,
    count: Option<NonZeroU32>,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count,
    }
}

fn fragment_bgl_uniform_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}
