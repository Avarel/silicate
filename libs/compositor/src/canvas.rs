#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CanvasUniform {
    height: u32,
    width: u32,
    columns: u32,
    rows: u32,
    tile_size: u32,
}

impl CanvasUniform {
    pub fn new((width, height): (u32, u32), (columns, rows): (u32, u32), tile_size: u32) -> Self {
        Self {
            height,
            width,
            columns,
            rows,
            tile_size,
        }
    }
}
