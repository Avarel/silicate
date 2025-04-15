use wgpu::util::DeviceExt;

use crate::dev::GpuDispatch;

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
    pub fn init_vec(device: &wgpu::Device, data: Vec<T>, usage: wgpu::BufferUsages) -> Self {
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("data_buffer"),
            contents: bytemuck::cast_slice(data.as_slice()),
            usage,
        });
        Self { data, buffer }
    }

    pub(super) fn data_len(&self) -> u64 {
        (self.data.len() * std::mem::size_of::<T>()) as u64
    }

    /// Load the GPU vertex buffer with updated data. Expanding the GPU buffer if needed.
    pub fn load_vec_buffer(&mut self, dispatch: &GpuDispatch) {
        if self.buffer.size() < self.data_len() {
            self.buffer = dispatch
                .device()
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("data_buffer"),
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
    pub fn init(device: &wgpu::Device, data: T, usage: wgpu::BufferUsages) -> Self {
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("data_buffer"),
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
