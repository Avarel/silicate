#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CompositorAtlasTiling {
    cols: u32,
    rows: u32,
}

impl CompositorAtlasTiling {
    pub fn new(cols: u32, rows: u32) -> Self {
        Self { cols, rows }
    }
}


#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CompositorCanvasTiling {
    pub(super) height: u32,
    pub(super) width: u32,
    cols: u32,
    rows: u32,
    tile_size: u32,
    flipped: u32,
}

impl CompositorCanvasTiling {
    pub fn new((width, height): (u32, u32), (cols, rows): (u32, u32), tile_size: u32) -> Self {
        Self {
            height,
            width,
            cols,
            rows,
            tile_size,
            flipped: 0
        }
    }

    pub fn cols(&self) -> u32 {
        self.cols
    }

    pub fn rows(&self) -> u32 {
        self.rows
    }

    pub fn set_flipped(&mut self, horizontally: bool, vertically: bool) {
        self.flipped = u32::from(horizontally) << 1 | u32::from(vertically);
    }
}

/// Vertex input to the shader.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable, Default)]
pub struct VertexInput {
    /// Position of the vertex.
    pub(super) position: [f32; 2],
    /// Holds the UV information of the foreground.
    /// The layers to be composited on the output texture uses this.
    pub(super) coords: [f32; 2],
}

impl VertexInput {
    pub fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<VertexInput>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: std::mem::offset_of!(VertexInput, position) as wgpu::BufferAddress,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::offset_of!(VertexInput, coords) as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
            ],
        }
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ChunkInstance {
    col: u32,
    row: u32,
}

impl ChunkInstance {
    pub fn new(col: u32, row: u32) -> Self {
        Self { col, row }
    }

    pub fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<ChunkInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: std::mem::offset_of!(ChunkInstance, col) as wgpu::BufferAddress,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Uint32,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::offset_of!(ChunkInstance, row) as wgpu::BufferAddress,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Uint32,
                },
            ],
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable, Default)]
pub(crate) struct LayerData {
    pub opacity: f32,
    pub blend: u32,
    pub clipped: u32,
    pub hidden: u32,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable, Default)]
pub(crate) struct ChunkSegment {
    pub start: u32,
    pub end: u32,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable, Default)]
pub(crate) struct ChunkData {
    pub atlas_index: u32,
    pub clip_atlas_index: u32,
    pub layer_index: u32,
}
