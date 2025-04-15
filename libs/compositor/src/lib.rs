pub mod blend;
pub mod buffer;
pub mod dev;
pub mod pipeline;
pub mod tex;

use self::tex::GpuTexture;
use blend::BlendingMode;
use buffer::BufferDimensions;
use dev::GpuDispatch;
use pipeline::Pipeline;
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
                    offset: std::mem::offset_of!(VertexInput, position) as wgpu::BufferAddress,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::offset_of!(VertexInput, fg_coords) as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
            ],
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable, Default)]
struct LayerData {
    texture_index: u32,
    mask_index: u32,
    blend: u32,
    opacity: f32,
}

pub struct CompositorData {
    dispatch: GpuDispatch,
    vertices: [VertexInput; 4],
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    layer_buffer: wgpu::Buffer,
    layer_count: u32,
}

impl CompositorData {
    /// Initial vertices
    const SQUARE_VERTICES: [VertexInput; 4] = [
        VertexInput {
            // top left
            position: [-1.0, 1.0, 0.0],
            fg_coords: [0.0, 1.0],
        },
        VertexInput {
            // bottom left
            position: [-1.0, -1.0, 0.0],
            fg_coords: [0.0, 0.0],
        },
        VertexInput {
            // top right
            position: [1.0, 1.0, 0.0],
            fg_coords: [1.0, 1.0],
        },
        VertexInput {
            // bottom right
            position: [1.0, -1.0, 0.0],
            fg_coords: [1.0, 0.0],
        },
    ];

    /// Initial indices of the 2 triangle strips
    const INDICES: [u16; 4] = [0, 2, 1, 3];

    fn new(dispatch: GpuDispatch) -> Self {
        let device = &dispatch.device();

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

        let layer_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("layer_buffer"),
            size: 0,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });

        Self {
            dispatch,
            vertices,
            vertex_buffer,
            index_buffer,
            layer_buffer,
            layer_count: 0,
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
        self.dispatch.queue().write_buffer(
            &self.vertex_buffer,
            0,
            bytemuck::cast_slice(&self.vertices),
        );
    }

    fn load_layer_buffer(&mut self, composite_layers: &[CompositeLayer]) {
        let mut layers = Vec::new();
        const MASK_NONE: u32 = u32::MAX;
        for layer in composite_layers.iter() {
            layers.push(LayerData {
                texture_index: layer.texture,
                mask_index: layer.clipped.unwrap_or(MASK_NONE),
                blend: layer.blend.to_u32(),
                opacity: layer.opacity,
            });
        }

        let data = bytemuck::cast_slice(&layers);

        if self.layer_buffer.size() < data.len() as u64 {
            self.layer_buffer =
                self.dispatch
                    .device()
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("layer_buffer"),
                        contents: data,
                        usage: wgpu::BufferUsages::STORAGE.union(wgpu::BufferUsages::COPY_DST),
                    });
        } else {
            self.dispatch
                .queue()
                .write_buffer(&self.layer_buffer, 0, data);
        }
        self.layer_count = composite_layers.len() as u32;
    }
}

/// Output target of a compositor pipeline.
pub struct Target {
    dispatch: GpuDispatch,
    pub data: CompositorData,
    /// Output texture dimensions.
    dim: BufferDimensions,
    /// Compositor output buffers and texture.
    pub output: Option<GpuTexture>,
}

impl Target {
    /// Create a new compositor target.
    pub fn new(dispatch: GpuDispatch, width: u32, height: u32) -> Self {
        Self {
            dispatch: dispatch.clone(),
            data: CompositorData::new(dispatch),
            dim: BufferDimensions::new(width, height),
            output: None,
        }
    }

    pub fn dim(&self) -> BufferDimensions {
        self.dim
    }

    /// Create an empty texture for this compositor target.
    fn create_texture(&self) -> GpuTexture {
        GpuTexture::empty_with_extent(&self.dispatch, self.dim.extent(), GpuTexture::OUTPUT_USAGE)
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
                .dispatch
                .device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

            self.render_command(pipeline, &mut encoder, bg, layers, textures);

            encoder.finish()
        };
        self.dispatch.queue().submit(Some(command_buffers));
    }

    fn render_command(
        &mut self,
        pipeline: &Pipeline,
        encoder: &mut CommandEncoder,
        bg: Option<[f32; 4]>,
        composite_layers: &[CompositeLayer],
        textures: &GpuTexture,
    ) {
        let output_texture = if let Some(tex) = self.output.as_mut() {
            tex
        } else {
            self.output.insert(self.create_texture())
        };

        self.data.load_layer_buffer(composite_layers);

        let blending_bind_group =
            self.dispatch
                .device()
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    layout: &pipeline.blending_bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(&textures.create_view()),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: self.data.layer_buffer.as_entire_binding(),
                        },
                    ],
                    label: Some("mixing_bind_group"),
                });

        let output_view = output_texture.create_view();
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
        pass.set_bind_group(0, &pipeline.constant_bind_group, &[]);
        pass.set_bind_group(1, &blending_bind_group, &[]);
        pass.set_vertex_buffer(0, self.data.vertex_buffer.slice(..));
        pass.set_index_buffer(self.data.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        pass.draw_indexed(0..CompositorData::INDICES.len() as u32, 0, 0..1);

        drop(pass);
    }
}
