pub mod blend;
pub mod buffer;
pub mod dev;
pub mod pipeline;
pub mod tex;

pub mod canvas;

use std::num::NonZeroU32;

use self::tex::GpuTexture;
use blend::BlendingMode;
use buffer::{BufferDimensions, CompositorBuffers};
use canvas::{CompositorAtlasTiling, CompositorCanvasTiling, ChunkInstance, VertexInput};
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
    pub clip_atlas_index: Option<NonZeroU32>,
    pub layer_index: u32,
}

/// Compositing layer information.
#[derive(Debug)]
pub struct CompositeLayer {
    pub clipped: bool,
    pub hidden: bool,
    /// Opacity (0.0..=1.0) of the layer.
    pub opacity: f32,
    /// Blending mode of the layer.
    pub blend: BlendingMode,
}

/// Output target of a compositor pipeline.
pub struct Target {
    dispatch: GpuDispatch,
    buffers: CompositorBuffers,
    /// Output texture dimensions.
    dim: BufferDimensions,
    /// Compositor output buffers and texture.
    output: GpuTexture,
    atlas_texture: GpuTexture,
}

impl Target {
    /// Create a new compositor target.
    pub fn new(
        dispatch: GpuDispatch,
        canvas: CompositorCanvasTiling,
        atlas_data: CompositorAtlasTiling,
        atlas_texture: GpuTexture,
    ) -> Self {
        let dim = BufferDimensions::new(canvas.width, canvas.height);
        Self {
            output: GpuTexture::empty_with_extent(
                &dispatch,
                dim.extent(),
                GpuTexture::OUTPUT_USAGE,
            ),
            dispatch: dispatch.clone(),
            buffers: CompositorBuffers::new(dispatch, canvas, atlas_data),
            dim,
            atlas_texture,
        }
    }

    pub fn dim(&self) -> BufferDimensions {
        self.dim
    }

    pub fn output(&self) -> &GpuTexture {
        &self.output
    }

    pub fn load_layer_buffer(&mut self, layers: &[CompositeLayer]) {
        self.buffers.load_layer_buffer(layers);
    }

    pub fn load_chunk_buffer(&mut self, chunks_data: &[ChunkTile]) {
        self.buffers.load_chunk_buffer(chunks_data);
    }

    pub fn set_flipped(&mut self, horizontally: bool, vertically: bool) {
        self.buffers.canvas.data_mut().set_flipped(horizontally, vertically);
        self.buffers.canvas.load_buffer(self.dispatch.queue());
    }

    /// Render composite layers using the compositor pipeline.
    pub fn render(&self, pipeline: &Pipeline, bg: Option<[f32; 4]>) {
        assert!(!self.dim.is_empty(), "set_dimensions required");

        let command_buffers = {
            let mut encoder = self
                .dispatch
                .device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

            self.render_command(pipeline, &mut encoder, bg);

            encoder.finish()
        };
        self.dispatch.queue().submit(Some(command_buffers));
    }

    fn render_command(
        &self,
        pipeline: &Pipeline,
        encoder: &mut CommandEncoder,
        bg: Option<[f32; 4]>,
    ) {
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
                                &self.atlas_texture.create_array_view(),
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
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: {
                                // TODO: upgrade when egui_wgpu hits wgpu 25
                                // wgpu::BindingResource::Buffer(wgpu::BufferBinding::from(self.data.layers.buffer_slice()))

                                wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                                    buffer: self.buffers.layers.buffer(),
                                    offset: 0,
                                    size: std::num::NonZeroU64::new(self.buffers.layers.data_len()),
                                })
                            },
                        },
                        wgpu::BindGroupEntry {
                            binding: 4,
                            resource: {
                                // TODO: upgrade when egui_wgpu hits wgpu 25
                                // wgpu::BindingResource::Buffer(wgpu::BufferBinding::from(self.data.layers.buffer_slice()))

                                wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                                    buffer: self.buffers.segments.buffer(),
                                    offset: 0,
                                    size: std::num::NonZeroU64::new(
                                        self.buffers.segments.data_len(),
                                    ),
                                })
                            },
                        },
                    ],
                    label: Some("mixing_bind_group"),
                });

        let output_view = self.output.create_default_view();
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
