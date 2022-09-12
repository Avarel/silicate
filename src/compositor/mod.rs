pub mod dev;
pub mod tex;

use self::{dev::LogicalDevice, tex::GpuTexture};
use crate::silica::BlendingMode;
use image::{Pixel, Rgba};
use std::{collections::HashMap, num::NonZeroU32};
use wgpu::{util::DeviceExt, CommandEncoder};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug)]
pub struct CompositeLayer {
    pub texture: usize,
    pub clipped: Option<usize>,
    pub opacity: f32,
    pub blend: BlendingMode,
}

// impl std::fmt::Debug for CompositeLayer<'_> {
//     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//         f.debug_struct("CompositeLayer")
//             // .field("texture", &self.texture)
//             .field("clipped", &self.clipped)
//             .field("opacity", &self.opacity)
//             .field("blend", &self.blend)
//             .finish()
//     }
// }

pub struct Compositor<'device> {
    pub dev: &'device LogicalDevice,
    pub dim: BufferDimensions,
    vertices: [Vertex; 4],
    constant_bind_group: wgpu::BindGroup,
    blending_group_layout: wgpu::BindGroupLayout,
    render_pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    pub output_texture: Option<GpuTexture>,
    stages: Vec<CompositorStage<'device>>
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
        if self.dim == buffer_dimensions {
            return;
        }
        self.dim = buffer_dimensions;
        self.output_texture = Some({
            let tex = GpuTexture::empty_with_extent(
                &self.dev,
                self.dim.extent,
                None,
                wgpu::TextureUsages::COPY_DST
                    | wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
            );
            tex.clear(self.dev, wgpu::Color::WHITE);
            tex
        });
        self.stages.clear()
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

    pub fn new(dev: &'device LogicalDevice) -> Self {
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
                targets: &[
                    Some(wgpu::ColorTargetState {
                        format: tex::TEX_FORMAT,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    }),
                    Some(wgpu::ColorTargetState {
                        format: tex::TEX_FORMAT,
                        blend: Some(wgpu::BlendState::REPLACE),
                        write_mask: wgpu::ColorWrites::ALL,
                    }),
                ],
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
            render_pipeline,
            vertices,
            vertex_buffer,
            index_buffer,
            output_texture: None,
            stages: Vec::new(),
        }
    }

    pub fn base_composite_texture(&self) -> GpuTexture {
        GpuTexture::empty_with_extent(&self.dev, self.dim.extent, None, GpuTexture::OUTPUT_USAGE)
    }

    pub fn render(
        &mut self,
        background: Option<[f32; 4]>,
        layers: &[CompositeLayer],
        textures: &[GpuTexture],
    ) {
        assert!(!self.dim.is_empty(), "set_dimensions required");

        self.dev.queue.submit(Some({
            let mut encoder = self
                .dev
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

            let mut stage_idx = 0;
            let mut count = 0;
            while count < layers.len() {
                if self.stages.len() <= stage_idx {
                    self.stages.push(CompositorStage::new(self));
                }

                let delta = self.render_chunk(
                    &mut encoder,
                    background,
                    stage_idx,
                    &layers[count..],
                    textures,
                );
                count += delta as usize;
                stage_idx += 1;
            }

            self.stages.truncate(stage_idx);

            encoder.copy_texture_to_texture(
                self.stages[stage_idx - 1].output.texture.as_image_copy(),
                self.output_texture
                    .as_ref()
                    .unwrap()
                    .texture
                    .as_image_copy(),
                    self.stages[stage_idx - 1].output.size,
            );

            encoder.finish()
        }));
    }

    fn render_chunk(
        &mut self,
        encoder: &mut CommandEncoder,
        background: Option<[f32; 4]>,
        stage_idx: usize,
        composite_layers: &[CompositeLayer],
        textures: &[GpuTexture],
    ) -> u32 {
        let prev_texture_view = if stage_idx > 0 {
            self.stages[stage_idx - 1].output.make_view()
        } else {
            self.base_composite_texture().make_view()
        };

        let mut texture_views = Vec::new();
        texture_views.resize_with(self.dev.chunks as usize, || textures[0].make_view());
        let stage = &mut self.stages[stage_idx];
        let bind = &mut stage.bindings;
        bind.reset();

        let mut mapped_texture_views = HashMap::new();

        for (index, layer) in composite_layers.into_iter().enumerate() {
            assert_eq!(index, bind.count as usize);

            if let Some(clip_layer) = layer.clipped {
                if let Some(&clip_index) = mapped_texture_views.get(&clip_layer) {
                    if (mapped_texture_views.len() as u32) + 1 > self.dev.chunks {
                        break;
                    }

                    if let Some(&mapped_texture) = mapped_texture_views.get(&layer.texture) {
                        bind.layers[index] = mapped_texture;
                    } else {
                        bind.layers[index] = mapped_texture_views.len() as u32;
                        texture_views[index] = textures[layer.texture].make_view();
                        mapped_texture_views
                            .insert(layer.texture, mapped_texture_views.len() as u32);
                    }

                    bind.masks[index] = clip_index as i32;
                } else {
                    if (mapped_texture_views.len() as u32) + 2 > self.dev.chunks {
                        break;
                    }

                    bind.masks[index] = mapped_texture_views.len() as i32;
                    texture_views[index] = textures[clip_layer].make_view();
                    mapped_texture_views.insert(clip_layer, mapped_texture_views.len() as u32);

                    if let Some(&mapped_texture) = mapped_texture_views.get(&layer.texture) {
                        bind.layers[index] = mapped_texture;
                    } else {
                        bind.layers[index] = mapped_texture_views.len() as u32;
                        texture_views[index] = textures[layer.texture].make_view();
                        mapped_texture_views
                            .insert(layer.texture, mapped_texture_views.len() as u32);
                    }
                }
            } else {
                if (mapped_texture_views.len() as u32) + 1 > self.dev.chunks {
                    break;
                }

                if let Some(&mapped_texture) = mapped_texture_views.get(&layer.texture) {
                    bind.layers[index] = mapped_texture;
                } else {
                    bind.layers[index] = mapped_texture_views.len() as u32;
                    texture_views[index] = textures[layer.texture].make_view();
                    mapped_texture_views.insert(layer.texture, mapped_texture_views.len() as u32);
                }

                bind.masks[index] = -1;
            }

            bind.blends[index] = layer.blend.to_u32();
            bind.opacities[index] = layer.opacity;
            bind.count += 1;
        }
        let count = bind.count;

        assert_eq!(texture_views.len(), self.dev.chunks as usize);

        let tex_view = stage.output.make_view();

        let buffers = &stage.buffers;
        bind.update(&buffers);

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
                            texture_views
                                .iter()
                                .collect::<Vec<_>>()
                                .as_slice(),
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: buffers.layers.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: buffers.masks.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: buffers.blends.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: buffers.opacities.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 6,
                        resource: buffers.count.as_entire_binding(),
                    },
                ],
                label: Some("mixing_bind_group"),
            });

        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[
                Some(wgpu::RenderPassColorAttachment {
                    view: &tex_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(
                            background
                                .map(|[r, g, b, _]| wgpu::Color {
                                    r: f64::from(r),
                                    g: f64::from(g),
                                    b: f64::from(b),
                                    a: 1.0,
                                })
                                .unwrap_or(wgpu::Color::TRANSPARENT),
                        ),
                        store: true,
                    },
                }),
                Some(wgpu::RenderPassColorAttachment {
                    view: &tex_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: true,
                    },
                }),
            ],
            depth_stencil_attachment: None,
        });

        render_pass.set_pipeline(&self.render_pipeline);
        render_pass.set_bind_group(0, &self.constant_bind_group, &[]);
        render_pass.set_bind_group(1, &blending_bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        render_pass.draw_indexed(0..INDICES.len() as u32, 0, 0..1);
        drop(render_pass);

        count
    }
}

struct CompositorStage<'dev> {
    bindings: ShaderBindings,
    buffers: ShaderBuffers<'dev>,
    output: GpuTexture
}

impl<'dev> CompositorStage<'dev> {
    pub fn new(compositor: &Compositor<'dev> ) -> Self {
        Self {
            bindings: ShaderBindings::new(compositor.dev.chunks),
            buffers: ShaderBuffers::new(&compositor.dev),
            output: compositor.base_composite_texture()
        }
    }
}

#[derive(Debug)]
struct ShaderBindings {
    blends: Box<[u32]>,
    opacities: Box<[f32]>,
    masks: Box<[i32]>,
    layers: Box<[u32]>,
    count: u32,
}

struct ShaderBuffers<'dev> {
    dev: &'dev LogicalDevice,
    blends: wgpu::Buffer,
    opacities: wgpu::Buffer,
    masks: wgpu::Buffer,
    layers: wgpu::Buffer,
    count: wgpu::Buffer,
}

impl ShaderBindings {
    fn new(chunks: u32) -> Self {
        let chunks = chunks as usize;

        Self {
            // texture_views: Vec::with_capacity(chunks),
            blends: vec![0; chunks].into_boxed_slice(),
            opacities: vec![0.0f32; chunks].into_boxed_slice(),
            masks: vec![-1; chunks].into_boxed_slice(),
            layers: vec![0; chunks].into_boxed_slice(),
            count: 0u32,
        }
    }

    fn reset(&mut self) {
        self.blends.fill(0);
        self.opacities.fill(0.0);
        self.masks.fill(-1);
        self.layers.fill(0);
        self.count = 0;
    }

    fn update(&self, buffers: &ShaderBuffers) {
        buffers.update_buffer(&buffers.blends, bytemuck::cast_slice(&self.blends));
        buffers.update_buffer(&buffers.opacities, bytemuck::cast_slice(&self.opacities));
        buffers.update_buffer(&buffers.masks, bytemuck::cast_slice(&self.masks));
        buffers.update_buffer(&buffers.layers, bytemuck::cast_slice(&self.layers));
        buffers.update_buffer(&buffers.count, bytemuck::cast_slice(&[self.count]));
    }
}

impl<'dev> ShaderBuffers<'dev> {
    fn new(dev: &'dev LogicalDevice) -> Self {
        let chunks = u64::from(dev.chunks);
        let storage_desc: wgpu::BufferDescriptor = wgpu::BufferDescriptor {
            label: None,
            size: 4 * chunks,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        };
        ShaderBuffers {
            dev,
            blends: dev.device.create_buffer(&storage_desc),
            opacities: dev.device.create_buffer(&storage_desc),
            masks: dev.device.create_buffer(&storage_desc),
            layers: dev.device.create_buffer(&storage_desc),
            count: dev.device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                size: 4,
                mapped_at_creation: false,
            }),
        }
    }

    fn update_buffer(&self, buffer: &wgpu::Buffer, content: &[u8]) {
        self.dev.queue.write_buffer(buffer, 0, content);
    }
}

fn shader_load() -> wgpu::ShaderModuleDescriptor<'static> {
    #[cfg(not(debug_assertions))]
    {
        wgpu::include_wgsl!("../shader.wgsl")
    }
    #[cfg(debug_assertions)]
    {
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
            fragment_bgl_buffer_ro_entry(2, None),
            fragment_bgl_buffer_ro_entry(3, None),
            fragment_bgl_buffer_ro_entry(4, None),
            fragment_bgl_buffer_ro_entry(5, None),
            fragment_bgl_uniform_entry(6),
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
