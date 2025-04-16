pub mod blend;
pub mod buffer;
pub mod dev;
pub mod pipeline;
pub mod tex;

pub mod atlas;
pub mod canvas;

use std::num::NonZeroU32;

use self::tex::GpuTexture;
use atlas::AtlasData;
use blend::BlendingMode;
use buffer::{BufferDimensions, DataBuffer};
use canvas::{CanvasTiling, TileInstance, VertexInput};
use dev::GpuDispatch;
use pipeline::Pipeline;
use wgpu::CommandEncoder;

#[derive(Debug)]
pub struct ChunkTile {
    pub col: u32,
    pub row: u32,
    /// Texture index into an atlas.
    pub atlas_index: NonZeroU32,
    /// Clipping texture index into an atlas`.
    pub mask_atlas_index: Option<NonZeroU32>,
}

/// Compositing layer information.
#[derive(Debug)]
pub struct CompositeLayer {
    pub chunks: Vec<ChunkTile>,
    /// Opacity (0.0..=1.0) of the layer.
    pub opacity: f32,
    /// Blending mode of the layer.
    pub blend: BlendingMode,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable, Default)]
struct ChunkData {
    col: u32,
    row: u32,
    atlas_index: u32,
    mask_index: u32,
    blend: u32,
    opacity: f32,
}

struct CompositorBuffers {
    dispatch: GpuDispatch,
    vertices: DataBuffer<[VertexInput; 4]>,
    indices: DataBuffer<[u16; 4]>,
    atlas: DataBuffer<AtlasData>,
    canvas: DataBuffer<CanvasTiling>,
    chunks: DataBuffer<Vec<ChunkData>>,
    tiles: DataBuffer<Vec<TileInstance>>,
}

impl CompositorBuffers {
    /// Initial vertices
    const SQUARE_VERTICES: [VertexInput; 4] = [
        VertexInput {
            // top left
            position: [0.0, 1.0],
            coords: [0.0, 1.0],
        },
        VertexInput {
            // bottom left
            position: [0.0, 0.0],
            coords: [0.0, 0.0],
        },
        VertexInput {
            // top right
            position: [1.0, 1.0],
            coords: [1.0, 1.0],
        },
        VertexInput {
            // bottom right
            position: [1.0, 0.0],
            coords: [1.0, 0.0],
        },
    ];

    /// Initial indices of the 2 triangle strips
    const INDICES: [u16; 4] = [0, 2, 1, 3];

    fn new(dispatch: GpuDispatch, canvas: CanvasTiling) -> Self {
        let device = dispatch.device();

        // Create the vertex buffer.
        let vertices = DataBuffer::init(
            device,
            "vertex_buffer",
            Self::SQUARE_VERTICES,
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );

        // Index draw buffer
        let indices = DataBuffer::init(
            device,
            "index_buffer",
            Self::INDICES,
            wgpu::BufferUsages::INDEX,
        );

        let chunks = DataBuffer::init_vec(
            device,
            "chunk_buffer",
            Vec::new(),
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        );

        let atlas = DataBuffer::init(
            device,
            "atlas_buffer",
            AtlasData::new(0, 0),
            wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        );

        let tiles = DataBuffer::init_vec(
            device,
            "tile_buffer",
            (0..canvas.rows())
                .flat_map(|row| (0..canvas.cols()).map(move |col| TileInstance::new(col, row)))
                .collect::<Vec<_>>(),
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );

        let canvas = DataBuffer::init(
            device,
            "canvas_buffer",
            canvas,
            wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        );

        Self {
            dispatch,
            vertices,
            indices,
            atlas,
            chunks,
            canvas,
            tiles,
        }
    }

    // /// Flip the vertex data's foreground UV of the compositor target.
    // pub fn flip_vertices(&mut self, horizontal: bool, vertical: bool) {
    //     for v in self.vertices.data_mut() {
    //         v.fg_coords = [
    //             if horizontal {
    //                 1.0 - v.fg_coords[0]
    //             } else {
    //                 v.fg_coords[0]
    //             },
    //             if vertical {
    //                 1.0 - v.fg_coords[1]
    //             } else {
    //                 v.fg_coords[1]
    //             },
    //         ];
    //     }
    //     self.vertices.load_buffer(self.dispatch.queue());
    // }

    // /// Rotate the vertex data's foreground UV of the compositor target.
    // pub fn rotate_vertices(&mut self, ccw: bool) {
    //     let temp = self.vertices[0].fg_coords;
    //     if ccw {
    //         self.vertices[0].fg_coords = self.vertices[1].fg_coords;
    //         self.vertices[1].fg_coords = self.vertices[3].fg_coords;
    //         self.vertices[3].fg_coords = self.vertices[2].fg_coords;
    //         self.vertices[2].fg_coords = temp;
    //     } else {
    //         self.vertices[0].fg_coords = self.vertices[2].fg_coords;
    //         self.vertices[2].fg_coords = self.vertices[3].fg_coords;
    //         self.vertices[3].fg_coords = self.vertices[1].fg_coords;
    //         self.vertices[1].fg_coords = temp;
    //     }
    //     self.load_vertex_buffer();
    // }

    fn load_chunk_buffer(&mut self, composite_layers: &[CompositeLayer]) {
        let chunks = self.chunks.data_mut();
        chunks.clear();

        const MASK_NONE: u32 = 0;
        for layer in composite_layers.iter() {
            for chunk in layer.chunks.iter() {
                chunks.push(ChunkData {
                    col: chunk.col,
                    row: chunk.row,
                    atlas_index: chunk.atlas_index.get(),
                    mask_index: chunk.mask_atlas_index.map(|v| v.get()).unwrap_or(MASK_NONE),
                    blend: layer.blend.to_u32(),
                    opacity: layer.opacity,
                });
            }
        }

        self.chunks.load_vec_buffer(&self.dispatch, "chunk_buffer");
    }
}

/// Output target of a compositor pipeline.
pub struct Target {
    dispatch: GpuDispatch,
    buffers: CompositorBuffers,
    /// Output texture dimensions.
    dim: BufferDimensions,
    /// Compositor output buffers and texture.
    output: GpuTexture,
}

impl Target {
    /// Create a new compositor target.
    pub fn new(dispatch: GpuDispatch, canvas: CanvasTiling) -> Self {
        let dim = BufferDimensions::new(canvas.width, canvas.height);
        Self {
            output: GpuTexture::empty_with_extent(
                &dispatch,
                dim.extent(),
                GpuTexture::OUTPUT_USAGE,
            ),
            dispatch: dispatch.clone(),
            buffers: CompositorBuffers::new(dispatch, canvas),
            dim,
        }
    }

    pub fn dim(&self) -> BufferDimensions {
        self.dim
    }

    pub fn output(&self) -> &GpuTexture {
        &self.output
    }

    /// Render composite layers using the compositor pipeline.
    pub fn render(
        &mut self,
        pipeline: &Pipeline,
        bg: Option<[f32; 4]>,
        layers: &[CompositeLayer],
        atlas: &AtlasData,
        atlas_texture: &GpuTexture,
    ) {
        assert!(!self.dim.is_empty(), "set_dimensions required");

        let command_buffers = {
            let mut encoder = self
                .dispatch
                .device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

            self.render_command(pipeline, &mut encoder, bg, layers, &atlas, atlas_texture);

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
        atlas: &AtlasData,
        atlas_texture: &GpuTexture,
    ) {
        *self.buffers.atlas.data_mut() = *atlas;
        self.buffers.atlas.load_buffer(self.dispatch.queue());
        self.buffers.load_chunk_buffer(composite_layers);

        let canvas_bind_group =
            self.dispatch
                .device()
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    layout: &pipeline.canvas_bind_group_layout,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.buffers.canvas.buffer().as_entire_binding(),
                    }],
                    label: Some("canvas_bind_group"),
                });

        let blending_bind_group =
            self.dispatch
                .device()
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    layout: &pipeline.blending_bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: self.buffers.atlas.buffer().as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(
                                &atlas_texture.create_view(),
                            ),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: {
                                // TODO: upgrade when egui_wgpu hits wgpu 25
                                // wgpu::BindingResource::Buffer(wgpu::BufferBinding::from(self.data.layers.buffer_slice()))

                                wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                                    buffer: self.buffers.chunks.buffer(),
                                    offset: 0,
                                    size: std::num::NonZeroU64::new(self.buffers.chunks.data_len()),
                                })
                            },
                        },
                    ],
                    label: Some("mixing_bind_group"),
                });

        let output_view = self.output.create_view();
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
        pass.set_bind_group(0, &canvas_bind_group, &[]);
        pass.set_bind_group(1, &pipeline.constant_bind_group, &[]);
        pass.set_bind_group(2, &blending_bind_group, &[]);
        pass.set_vertex_buffer(0, self.buffers.vertices.buffer().slice(..));
        pass.set_vertex_buffer(1, self.buffers.tiles.buffer_slice());
        pass.set_index_buffer(
            self.buffers.indices.buffer().slice(..),
            wgpu::IndexFormat::Uint16,
        );
        pass.draw_indexed(
            0..CompositorBuffers::INDICES.len() as u32,
            0,
            0..self.buffers.tiles.data().len() as u32,
        );
    }
}
