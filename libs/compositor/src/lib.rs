pub mod bind;
pub mod blend;
pub mod buffer;
pub mod dev;
pub mod pipeline;
pub mod tex;

use self::{
    bind::{CpuBuffers, GpuBuffers},
    dev::GpuHandle,
    tex::GpuTexture,
};
use blend::BlendingMode;
use buffer::BufferDimensions;
use pipeline::Pipeline;
use std::sync::Arc;
use wgpu::{CommandEncoder, util::DeviceExt};

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
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::offset_of!(VertexInput, bg_coords) as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::offset_of!(VertexInput, fg_coords) as wgpu::BufferAddress,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x2,
                },
            ],
        }
    }
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
pub struct Target {
    pub dev: Arc<GpuHandle>,
    pub data: CompositorData,
    /// Output texture dimensions.
    pub dim: BufferDimensions,
    /// Compositor output buffers and texture.
    pub output: Option<Output>,
}

/// Compositor stage buffers. This is so that the rendering process
/// can reuse buffers and textures whenever possible.
pub struct Output {
    dev: Arc<GpuHandle>,
    size: usize,
    bindings: CpuBuffers,
    buffers: GpuBuffers,
    pub texture: GpuTexture,
}

impl Output {
    /// Create a new compositor stage.
    pub fn new(target: &Target, size: usize) -> Self {
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

impl Target {
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
        pipeline: &Pipeline,
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
        pipeline: &Pipeline,
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
                .insert(Output::new(self, composite_layers.len()))
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
