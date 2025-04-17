use wgpu::util::DeviceExt;

use crate::{
    ChunkTile, CompositeLayer,
    atlas::AtlasData,
    canvas::{CanvasTiling, ChunkData, ChunkSegment, LayerData, TileInstance, VertexInput},
    dev::GpuDispatch,
};

/// Associates the texture's actual dimensions and its buffer dimensions on the GPU.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BufferDimensions<const ALIGN: u32 = { wgpu::COPY_BYTES_PER_ROW_ALIGNMENT }> {
    width: u32,
    height: u32,
    unpadded_bytes_per_row: u32,
    padded_bytes_per_row: u32,
    extent: wgpu::Extent3d,
}

impl BufferDimensions {
    pub const RGBA_CHANNEL_COUNT: usize = 4;
    const BYTES_PER_PIXEL: u32 = (Self::RGBA_CHANNEL_COUNT * std::mem::size_of::<u8>()) as u32;
}

impl<const ALIGN: u32> BufferDimensions<ALIGN> {
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
        let unpadded_bytes_per_row = extent.width * BufferDimensions::BYTES_PER_PIXEL;
        let padded_bytes_per_row_padding = (ALIGN - unpadded_bytes_per_row % ALIGN) % ALIGN;
        let padded_bytes_per_row = unpadded_bytes_per_row + padded_bytes_per_row_padding;
        Self {
            width: extent.width,
            height: extent.height,
            unpadded_bytes_per_row,
            padded_bytes_per_row,
            extent,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    pub fn to_vec2(&self) -> (f32, f32) {
        (self.width as f32, self.height as f32)
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn unpadded_bytes_per_row(&self) -> u32 {
        self.unpadded_bytes_per_row
    }

    pub fn padded_bytes_per_row(&self) -> u32 {
        self.padded_bytes_per_row
    }

    pub fn extent(&self) -> wgpu::Extent3d {
        self.extent
    }
}

/// Association between CPU buffer and GPU buffer.
pub struct DataBuffer<T> {
    data: T,
    buffer: wgpu::Buffer,
}

impl<T> DataBuffer<T> {
    pub fn data(&self) -> &T {
        &self.data
    }

    /// Get CPU data.
    pub fn data_mut(&mut self) -> &mut T {
        &mut self.data
    }

    /// Get GPU data.
    pub fn buffer(&self) -> &wgpu::Buffer {
        &self.buffer
    }
}

impl<T> DataBuffer<Vec<T>>
where
    T: bytemuck::NoUninit,
{
    pub fn init_vec(
        device: &wgpu::Device,
        name: &str,
        data: Vec<T>,
        usage: wgpu::BufferUsages,
    ) -> Self {
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(name),
            contents: bytemuck::cast_slice(data.as_slice()),
            usage,
        });
        Self { data, buffer }
    }

    pub(super) fn data_len(&self) -> u64 {
        (self.data.len() * std::mem::size_of::<T>()) as u64
    }

    /// Load the GPU vertex buffer with updated data. Expanding the GPU buffer if needed.
    pub fn load_vec_buffer(&mut self, dispatch: &GpuDispatch, name: &str) {
        if self.buffer.size() < self.data_len() {
            self.buffer = dispatch
                .device()
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(name),
                    contents: bytemuck::cast_slice(self.data.as_slice()),
                    usage: self.buffer.usage(),
                });
        } else {
            dispatch.queue().write_buffer(
                &self.buffer,
                0,
                bytemuck::cast_slice(self.data.as_slice()),
            );
        }
    }

    pub fn buffer_slice(&self) -> wgpu::BufferSlice<'_> {
        self.buffer.slice(..self.data_len())
    }
}

impl<T> DataBuffer<T>
where
    T: bytemuck::NoUninit,
{
    pub fn init(device: &wgpu::Device, name: &str, data: T, usage: wgpu::BufferUsages) -> Self {
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(name),
            contents: bytemuck::bytes_of(&data),
            usage,
        });
        Self { data, buffer }
    }

    /// Load the GPU vertex buffer with updated data.
    pub fn load_buffer(&mut self, queue: &wgpu::Queue) {
        queue.write_buffer(&self.buffer, 0, bytemuck::bytes_of(&self.data));
    }
}

pub(crate) struct CompositorBuffers {
    dispatch: GpuDispatch,
    pub(crate) vertices: DataBuffer<[VertexInput; 4]>,
    pub(crate) indices: DataBuffer<[u16; 4]>,
    pub(crate) atlas: DataBuffer<AtlasData>,
    pub(crate) canvas: DataBuffer<CanvasTiling>,
    pub(crate) segments: DataBuffer<Vec<ChunkSegment>>,
    pub(crate) chunks: DataBuffer<Vec<ChunkData>>,
    pub(crate) layers: DataBuffer<Vec<LayerData>>,
    pub(crate) tiles: DataBuffer<Vec<TileInstance>>,
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
    pub(super) const INDICES: [u16; 4] = [0, 2, 1, 3];

    pub(super) fn new(
        dispatch: GpuDispatch,
        canvas_data: CanvasTiling,
        atlas_data: AtlasData,
    ) -> Self {
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

        let layers = DataBuffer::init_vec(
            device,
            "layer_buffer",
            Vec::new(),
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        );

        let chunks = DataBuffer::init_vec(
            device,
            "chunk_buffer",
            Vec::new(),
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        );

        let segments = DataBuffer::init_vec(
            device,
            "segment_buffer",
            Vec::new(),
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        );

        let atlas = DataBuffer::init(
            device,
            "atlas_buffer",
            atlas_data,
            wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        );

        let tiles = DataBuffer::init_vec(
            device,
            "tile_buffer",
            (0..canvas_data.rows())
                .flat_map(|row| (0..canvas_data.cols()).map(move |col| TileInstance::new(col, row)))
                .collect::<Vec<_>>(),
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );

        let canvas = DataBuffer::init(
            device,
            "canvas_buffer",
            canvas_data,
            wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        );

        Self {
            dispatch,
            vertices,
            indices,
            atlas,
            layers,
            segments,
            chunks,
            canvas,
            tiles,
        }
    }

    pub(super) fn load_layer_buffer(&mut self, composite_layers: &[CompositeLayer]) {
        let layers = self.layers.data_mut();
        layers.clear();

        for layer in composite_layers.iter() {
            layers.push(LayerData {
                blend: layer.blend.to_u32(),
                opacity: layer.opacity,
                clipped: if layer.clipped { 1 } else { 0 },
                hidden: if layer.hidden { 1 } else { 0 },
            });
        }

        self.layers.load_vec_buffer(&self.dispatch, "layer_buffer");
    }

    pub(super) fn load_chunk_buffer(&mut self, chunks_data: &[ChunkTile]) {
        debug_assert!(chunks_data.is_sorted_by_key(|v| (v.col, v.row)));

        let num_cols = self.canvas.data().cols();
        let num_rows = self.canvas.data().rows();

        let chunks = self.chunks.data_mut();
        chunks.clear();

        let segments = self.segments.data_mut();
        segments.resize(
            (num_cols * num_rows) as usize,
            ChunkSegment { start: 0, end: 0 },
        );

        // Create a mutable list of segment references by index for fast access
        for chunk in chunks_data {
            let index = (chunk.row * num_cols + chunk.col) as usize;
            let start = chunks.len();
            chunks.push(ChunkData {
                atlas_index: chunk.atlas_index.get(),
                mask_index: chunk.mask_atlas_index.map(|v| v.get()).unwrap_or(0),
                layer_index: chunk.layer_index,
            });

            let segment = &mut segments[index];

            if segment.start == 0 && segment.end == 0 {
                // First time seeing this (col, row)
                segment.start = start as u32;
            }
            segment.end = chunks.len() as u32; // always update end to current
        }

        self.chunks.load_vec_buffer(&self.dispatch, "chunk_buffer");
        self.segments
            .load_vec_buffer(&self.dispatch, "segment_buffer");
    }
}
