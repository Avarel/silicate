#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CanvasTiling {
    pub(super) height: u32,
    pub(super) width: u32,
    cols: u32,
    rows: u32,
    tile_size: u32,
}

impl CanvasTiling {
    pub fn new((width, height): (u32, u32), (cols, rows): (u32, u32), tile_size: u32) -> Self {
        Self {
            height,
            width,
            cols,
            rows,
            tile_size,
        }
    }

    pub fn cols(&self) -> u32 {
        self.cols
    }

    pub fn rows(&self) -> u32 {
        self.rows
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
pub struct TileInstance {
    col: u32,
    row: u32,
}

impl TileInstance {
    pub fn new(col: u32, row: u32) -> Self {
        Self { col, row }
    }

    pub fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<TileInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: std::mem::offset_of!(TileInstance, col) as wgpu::BufferAddress,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Uint32,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::offset_of!(TileInstance, row) as wgpu::BufferAddress,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Uint32,
                },
            ],
        }
    }
}
