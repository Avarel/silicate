mod bind;
pub mod dev;
pub mod tex;

use self::{
    bind::{CpuBuffers, GpuBuffers},
    dev::GpuHandle,
    tex::GpuTexture,
};
use crate::silica::BlendingMode;
use image::{Pixel, Rgba};
use std::{num::NonZeroU32, sync::Arc};
use wgpu::{util::DeviceExt, CommandEncoder};

/// Associates the texture's actual dimensions and its buffer dimensions on the GPU.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BufferDimensions {
    pub width: u32,
    pub height: u32,
    pub unpadded_bytes_per_row: u32,
    pub padded_bytes_per_row: u32,
    pub extent: wgpu::Extent3d,
}

impl BufferDimensions {
    /// Computes the buffer dimensions between the texture's actual dimensions
    /// and its buffer dimensions on the GPU.
    pub const fn new(width: u32, height: u32) -> Self {
        Self::from_extent(wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        })
    }

    /// Computes the buffer dimensions from the GPU texture extent.
    pub const fn from_extent(extent: wgpu::Extent3d) -> Self {
        // It is a WebGPU requirement that
        // ImageCopyBuffer.layout.bytes_per_row % wgpu::COPY_BYTES_PER_ROW_ALIGNMENT == 0
        // So we calculate padded_bytes_per_row by rounding unpadded_bytes_per_row
        // up to the next multiple of wgpu::COPY_BYTES_PER_ROW_ALIGNMENT.
        // https://en.wikipedia.org/wiki/Data_structure_alignment#Computing_padding
        debug_assert!(extent.depth_or_array_layers == 1);
        let width = extent.width;
        let height = extent.height;
        let bytes_per_pixel =
            (Rgba::<u8>::CHANNEL_COUNT as usize * std::mem::size_of::<u8>()) as u32;
        let unpadded_bytes_per_row = width * bytes_per_pixel;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded_bytes_per_row_padding = (align - unpadded_bytes_per_row % align) % align;
        let padded_bytes_per_row = unpadded_bytes_per_row + padded_bytes_per_row_padding;
        Self {
            width,
            height,
            unpadded_bytes_per_row,
            padded_bytes_per_row,
            extent,
        }
    }

    fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    pub fn to_vec2(&self) -> egui::Vec2 {
        egui::Vec2::new(self.width as f32, self.height as f32)
    }
}

/// Vertex input to the shader.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable, Default)]
struct VertexInput {
    /// Position of the vertex.
    position: [f32; 3],
    /// Holds the UV information of the background.
    /// The base texture uses this, which may be the texture from a
    /// previous pass.
    bg_coords: [f32; 2],
    /// Holds the UV information of the foreground.
    /// The layers to be composited on the output texture uses this.
    fg_coords: [f32; 2],
}

impl VertexInput {
    fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<VertexInput>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                // position
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                // bg_coords
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // fg_coords
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 2 + 3]>() as wgpu::BufferAddress,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x2,
                },
            ],
        }
    }
}

/// Compositing layer information.
#[derive(Debug)]
pub struct CompositeLayer {
    /// Texture index into a `&[GpuBuffer]`.
    pub texture: u32,
    /// Clipping texture index into a `&[GpuBuffer]`.
    pub clipped: Option<u32>,
    /// Opacity (0.0..=1.0) of the layer.
    pub opacity: f32,
    /// Blending mode of the layer.
    pub blend: BlendingMode,
}

pub struct CompositorData {
    dev: Arc<GpuHandle>,
    vertices: [VertexInput; 4],
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
}

impl CompositorData {
    /// Initial vertices
    const SQUARE_VERTICES: [VertexInput; 4] = [
        VertexInput {
            position: [-1.0, 1.0, 0.0],
            bg_coords: [0.0, 0.0],
            fg_coords: [0.0, 1.0],
        },
        VertexInput {
            position: [-1.0, -1.0, 0.0],
            bg_coords: [0.0, 1.0],
            fg_coords: [0.0, 0.0],
        },
        VertexInput {
            position: [1.0, 1.0, 0.0],
            bg_coords: [1.0, 0.0],
            fg_coords: [1.0, 1.0],
        },
        VertexInput {
            position: [1.0, -1.0, 0.0],
            bg_coords: [1.0, 1.0],
            fg_coords: [1.0, 0.0],
        },
    ];

    /// Initial indices of the 2 triangle strips
    const INDICES: [u16; 4] = [0, 1, 2, 3];

    fn new(dev: Arc<GpuHandle>) -> Self {
        let device = &dev.device;

        // Create the vertex buffer.
        let vertices = Self::SQUARE_VERTICES;
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vertex_buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });

        // Index draw buffer
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("index_buffer"),
            contents: bytemuck::cast_slice(&Self::INDICES),
            usage: wgpu::BufferUsages::INDEX,
        });

        Self {
            dev,
            vertices,
            vertex_buffer,
            index_buffer,
        }
    }

    /// Flip the vertex data's foreground UV of the compositor target.
    pub fn flip_vertices(&mut self, horizontal: bool, vertical: bool) {
        for v in &mut self.vertices {
            v.fg_coords = [
                if horizontal {
                    1.0 - v.fg_coords[0]
                } else {
                    v.fg_coords[0]
                },
                if vertical {
                    1.0 - v.fg_coords[1]
                } else {
                    v.fg_coords[1]
                },
            ];
        }
        self.load_vertex_buffer();
    }

    /// Rotate the vertex data's foreground UV of the compositor target.
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
        self.load_vertex_buffer();
    }

    /// Load the GPU vertex buffer with updated data.
    fn load_vertex_buffer(&mut self) {
        self.dev
            .queue
            .write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&self.vertices));
    }
}

/// Output target of a compositor pipeline.
pub struct CompositorTarget {
    pub dev: Arc<GpuHandle>,
    pub data: CompositorData,
    /// Output texture dimensions.
    pub dim: BufferDimensions,
    /// Compositor output buffers and texture.
    pub output: Option<CompositorOutput>,
}

/// Compositor stage buffers. This is so that the rendering process
/// can reuse buffers and textures whenever possible.
pub struct CompositorOutput {
    dev: Arc<GpuHandle>,
    size: usize,
    bindings: CpuBuffers,
    buffers: GpuBuffers,
    pub texture: GpuTexture,
}

impl CompositorOutput {
    /// Create a new compositor stage.
    pub fn new(target: &CompositorTarget, size: usize) -> Self {
        Self {
            dev: target.dev.clone(),
            size,
            bindings: CpuBuffers::new(size),
            buffers: GpuBuffers::new(target.dev.clone(), size),
            texture: target.create_texture(),
        }
    }

    fn reserve_buffers(&mut self, size: usize) {
        if size <= self.size {
            return;
        }

        self.size = size;
        self.bindings = CpuBuffers::new(size);
        self.buffers = GpuBuffers::new(self.dev.clone(), size);
    }
}

impl CompositorTarget {
    /// Create a new compositor target.
    pub fn new(dev: Arc<GpuHandle>) -> Self {
        Self {
            data: CompositorData::new(dev.clone()),
            dev,
            dim: BufferDimensions::new(0, 0),
            output: None,
        }
    }

    /// Create an empty texture for this compositor target.
    fn create_texture(&self) -> GpuTexture {
        GpuTexture::empty_with_extent(&self.dev, self.dim.extent, GpuTexture::OUTPUT_USAGE)
    }

    /// Transpose the dimensions of the compositor target's output.
    pub fn transpose_dimensions(&mut self) -> bool {
        self.set_dimensions(self.dim.height, self.dim.width)
    }

    /// Set the dimensions of the compositor target's output.
    pub fn set_dimensions(&mut self, width: u32, height: u32) -> bool {
        let buffer_dimensions = BufferDimensions::new(width, height);
        if self.dim == buffer_dimensions {
            return false;
        }
        self.dim = buffer_dimensions;
        self.output = None;
        true
    }

    /// Render composite layers using the compositor pipeline.
    pub fn render(
        &mut self,
        pipeline: &CompositorPipeline,
        bg: Option<[f32; 4]>,
        layers: &[CompositeLayer],
        textures: &GpuTexture,
    ) {
        assert!(!self.dim.is_empty(), "set_dimensions required");

        let command_buffers = {
            let mut encoder = self
                .dev
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

            self.render_command(pipeline, &mut encoder, bg, layers, textures);

            encoder.finish()
        };
        self.dev.queue.submit(Some(command_buffers));
    }

    fn render_command(
        &mut self,
        pipeline: &CompositorPipeline,
        encoder: &mut CommandEncoder,
        bg: Option<[f32; 4]>,
        composite_layers: &[CompositeLayer],
        textures: &GpuTexture,
    ) {
        let composite_view = self.create_texture().create_view();

        let stage = if let Some(stage) = self.output.as_mut() {
            stage.reserve_buffers(composite_layers.len());
            stage
        } else {
            self.output
                .insert(CompositorOutput::new(self, composite_layers.len()))
        };

        stage.bindings.map_composite_layers(composite_layers);
        stage.buffers.load(&stage.bindings);

        let blending_bind_group = self
            .dev
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                layout: &pipeline.blending_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&composite_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&textures.create_view()),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: stage.buffers.layers.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: stage.buffers.masks.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: stage.buffers.blends.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: stage.buffers.opacities.as_entire_binding(),
                    },
                ],
                label: Some("mixing_bind_group"),
            });

        let output_view = stage.texture.create_view();
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[
                // background color clear pass
                Some(wgpu::RenderPassColorAttachment {
                    view: &output_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(
                            bg.map(|[r, g, b, _]| wgpu::Color {
                                r: f64::from(r),
                                g: f64::from(g),
                                b: f64::from(b),
                                a: 1.0,
                            })
                            .unwrap_or(wgpu::Color::TRANSPARENT),
                        ),
                        store: wgpu::StoreOp::Store,
                    },
                }),
                // compositing pass
                Some(wgpu::RenderPassColorAttachment {
                    view: &output_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                }),
            ],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        // Finish and set the render pass's binding groups and data
        pass.set_pipeline(&pipeline.render_pipeline);
        // We use push constants for the binding count.
        pass.set_push_constants(
            wgpu::ShaderStages::FRAGMENT,
            0,
            &stage.bindings.count.to_ne_bytes(),
        );
        pass.set_bind_group(0, &pipeline.constant_bind_group, &[]);
        pass.set_bind_group(1, &blending_bind_group, &[]);
        pass.set_vertex_buffer(0, self.data.vertex_buffer.slice(..));
        pass.set_index_buffer(self.data.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        pass.draw_indexed(0..CompositorData::INDICES.len() as u32, 0, 0..1);

        drop(pass);
    }
}

pub struct CompositorPipeline {
    constant_bind_group: wgpu::BindGroup,
    blending_bind_group_layout: wgpu::BindGroupLayout,
    render_pipeline: wgpu::RenderPipeline,
}

impl CompositorPipeline {
    /// Create a new compositor pipeline.
    pub fn new(dev: &GpuHandle) -> Self {
        let device = &dev.device;

        // This bind group only binds the sampler, which is a constant
        // through out all rendering passes.
        let (constant_bind_group_layout, constant_bind_group) = {
            let sampler = device.create_sampler(&wgpu::SamplerDescriptor::default());
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

        // This bind group changes per composition run.
        let blending_bind_group_layout = {
            const fn fragment_bgl_buffer_ro_entry(
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

            const fn fragment_bgl_tex_entry(
                binding: u32,
                count: Option<NonZeroU32>,
            ) -> wgpu::BindGroupLayoutEntry {
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

            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("blending_group_layout"),
                entries: &[
                    // composite
                    fragment_bgl_tex_entry(0, None),
                    // textures
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2Array,
                            sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        },
                        count: None,
                    },
                    // layers
                    fragment_bgl_buffer_ro_entry(2, None),
                    // masks
                    fragment_bgl_buffer_ro_entry(3, None),
                    // blends
                    fragment_bgl_buffer_ro_entry(4, None),
                    // opacities
                    fragment_bgl_buffer_ro_entry(5, None),
                ],
            })
        };

        // Loads the shader and creates the render pipeline.
        let render_pipeline = {
            let shader = device.create_shader_module(shader_load());

            let render_pipeline_layout =
                device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("render_pipeline_layout"),
                    bind_group_layouts: &[&constant_bind_group_layout, &blending_bind_group_layout],
                    push_constant_ranges: &[wgpu::PushConstantRange {
                        stages: wgpu::ShaderStages::FRAGMENT,
                        range: 0..4,
                    }],
                });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("render_pipeline"),
                layout: Some(&render_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: "vs_main",
                    buffers: &[VertexInput::desc()],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: "fs_main",
                    targets: &[
                        // Used to clear a background color
                        Some(wgpu::ColorTargetState {
                            format: tex::TEX_FORMAT,
                            blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                            write_mask: wgpu::ColorWrites::ALL,
                        }),
                        // Used to blend the shader
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
            })
        };

        Self {
            constant_bind_group,
            blending_bind_group_layout,
            render_pipeline,
        }
    }
}

/// Load the shader.
fn shader_load() -> wgpu::ShaderModuleDescriptor<'static> {
    // In release mode, the final binary includes the file directly so that
    // the binary does not rely on the shader file being at a specific location.
    #[cfg(not(debug_assertions))]
    {
        wgpu::include_wgsl!("../shader.wgsl")
    }
    // In debug mode, this reads directly from a file so that recompilation
    // will not be necessary in the event that only the shader file changes.
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
