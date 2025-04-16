#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct AtlasData {
    cols: u32,
    rows: u32,
}

impl AtlasData {
    pub fn new(cols: u32, rows: u32) -> Self {
        Self { cols, rows }
    }
}
