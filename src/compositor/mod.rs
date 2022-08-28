pub mod dev;
pub mod tex;

use self::{dev::LogicalDevice, tex::GpuTexture};
use crate::silica::BlendingMode;
use image::{Pixel, Rgba};
use std::num::NonZeroU32;
use wgpu::{util::DeviceExt, CommandEncoder};

const INCLUDE_SHADERS: bool = false;

#[derive(Debug, Clone, Copy)]
pub struct BufferDimensions {
    pub width: u32,
    pub height: u32,
    pub unpadded_bytes_per_row: u32,
    pub padded_bytes_per_row: u32,
    pub extent: wgpu::Extent3d,
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
            extent: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
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
}

impl std::fmt::Debug for CompositeLayer<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompositeLayer")
            // .field("texture", &self.texture)
            .field("clipped", &self.clipped)
            .field("opacity", &self.opacity)
            .field("blend", &self.blend)
            .finish()
    }
}

pub struct Compositor<'device> {
    pub dev: &'device LogicalDevice,
    pub dim: BufferDimensions,
    vertices: [Vertex; 4],
    background: Option<[f32; 4]>,
    constant_bind_group: wgpu::BindGroup,
    blending_group_layout: wgpu::BindGroupLayout,
    render_pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    filled_clipping_mask: Option<GpuTexture>,
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

    pub fn set_dimensions(&mut self, width: u32, height: u32) {
        let buffer_dimensions = BufferDimensions::new(width, height);
        self.dim = buffer_dimensions;
        self.filled_clipping_mask = Some({
            let tex = GpuTexture::empty_with_extent(
                &self.dev,
                self.dim.extent,
                None,
                GpuTexture::OUTPUT_USAGE,
            );
            tex.clear(self.dev, wgpu::Color::WHITE);
            tex
        });
    }

    pub fn reload_vertices_buffer(&mut self) {
        self.vertex_buffer =
            self.dev
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("vertex_buffer"),
                    contents: bytemuck::cast_slice(&self.vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                });
    }

    pub fn new(background: Option<[f32; 4]>, dev: &'device LogicalDevice) -> Self {
        let LogicalDevice { ref device, .. } = dev;

        // Create the vertex buffer.
        // This is a base
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

        let blending_group_layout = blending_group_layout(device, dev.chunks);

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("render_pipeline_layout"),
                bind_group_layouts: &[&constant_bind_group_layout, &blending_group_layout],
                push_constant_ranges: &[],
            });

        let shader = device.create_shader_module(shader_load());

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
                    format: tex::TEX_FORMAT,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        let buffer_dimensions = BufferDimensions::new(0, 0);

        Self {
            dev,
            dim: buffer_dimensions,
            constant_bind_group,
            blending_group_layout,
            background,
            render_pipeline,
            vertices,
            vertex_buffer,
            index_buffer,
            filled_clipping_mask: None,
        }
    }

    pub fn base_composite_texture(&self) -> GpuTexture {
        let tex = GpuTexture::empty_with_extent(
            &self.dev,
            self.dim.extent,
            None,
            GpuTexture::OUTPUT_USAGE,
        );

        if let Some([r, g, b, a]) = self.background {
            tex.clear(
                self.dev,
                wgpu::Color {
                    r: f64::from(r),
                    g: f64::from(g),
                    b: f64::from(b),
                    a: f64::from(a),
                },
            );
        }

        tex
    }

    pub fn render<'a, 'b>(&'a self, layers: &'b [CompositeLayer]) -> GpuTexture {
        assert!(!self.dim.is_empty(), "set_dimensions required");

        let mut composite_texture = self.base_composite_texture();

        self.dev.queue.submit(Some({
            let mut encoder = self
                .dev
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

            for chunked_layers in layers.chunks(self.dev.chunks as usize) {
                composite_texture =
                    self.render_chunk(&mut encoder, composite_texture, layers, chunked_layers);
            }
            encoder.finish()
        }));

        composite_texture
    }

    fn render_chunk(
        &self,
        encoder: &mut CommandEncoder,
        composite_texture: GpuTexture,
        layers: &[CompositeLayer],
        chunked_layers: &[CompositeLayer],
    ) -> GpuTexture {
        let prev_texture_view = composite_texture.make_view();

        let mut mask_views: Vec<wgpu::TextureView> = Vec::with_capacity(self.dev.chunks as usize);
        let mut layer_views = Vec::with_capacity(self.dev.chunks as usize);
        let mut blends = Vec::with_capacity(self.dev.chunks as usize);
        let mut opacities = Vec::with_capacity(self.dev.chunks as usize);

        let filled_clipping_mask = self
            .filled_clipping_mask
            .as_ref()
            .expect("Compositor dimensions not configured?");

        for layer in chunked_layers.iter() {
            mask_views.push(
                (if let Some(mask_layer) = layer.clipped {
                    &layers[mask_layer].texture
                } else {
                    &filled_clipping_mask
                })
                .make_view(),
            );
            layer_views.push(layer.texture.make_view());
            blends.push(layer.blend.to_u32());
            opacities.push(layer.opacity);
        }

        // Fill with dummy
        for _ in 0..self.dev.chunks - chunked_layers.len() as u32 {
            mask_views.push(filled_clipping_mask.make_view());
            layer_views.push(filled_clipping_mask.make_view());
            blends.push(0);
            opacities.push(0.0);
        }

        let ctx_buffer = self
            .dev
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Context"),
                contents: bytemuck::cast_slice(&blends),
                usage: wgpu::BufferUsages::STORAGE,
            });
        let opacity_buffer =
            self.dev
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Context"),
                    contents: bytemuck::cast_slice(&opacities),
                    usage: wgpu::BufferUsages::STORAGE,
                });

        let count_buffer = self
            .dev
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Context"),
                contents: bytemuck::cast_slice(&[chunked_layers.len()]),
                usage: wgpu::BufferUsages::UNIFORM,
            });

        let tex = GpuTexture::empty_with_extent(
            &self.dev,
            self.dim.extent,
            None,
            GpuTexture::OUTPUT_USAGE,
        );
        let tex_view = tex.make_view();

        let blending_bind_group = self
            .dev
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
                        resource: opacity_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: count_buffer.as_entire_binding(),
                    },
                ],
                label: Some("mixing_bind_group"),
            });

        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &tex_view,
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

        tex
    }
}

fn shader_load() -> wgpu::ShaderModuleDescriptor<'static> {
    if INCLUDE_SHADERS {
        // wgpu::include_wgsl!("../shader.wgsl")
        todo!()
    } else {
        wgpu::ShaderModuleDescriptor {
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
        }
    }
}

fn blending_group_layout(device: &wgpu::Device, chunks: u32) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("blending_group_layout"),
        entries: &[
            fragment_bgl_tex_entry(0, None),
            fragment_bgl_tex_entry(1, NonZeroU32::new(chunks)),
            fragment_bgl_tex_entry(2, NonZeroU32::new(chunks)),
            fragment_bgl_buffer_ro_entry(3, None),
            fragment_bgl_buffer_ro_entry(4, None),
            fragment_bgl_uniform_entry(5),
        ],
    })
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
