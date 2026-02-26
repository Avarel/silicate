use silica::ns_archive::Size;

#[derive(Debug, Clone, Copy)]
pub struct AtlasTextureTiling {
    pub cols: u32,
    pub rows: u32,
    pub layers: u32,
}

impl AtlasTextureTiling {
    pub fn compute_atlas_size(chunk_count: u32, tile_size: u32, limits: &wgpu::Limits) -> Self {
        if chunk_count * tile_size <= limits.max_texture_dimension_1d {
            AtlasTextureTiling {
                cols: chunk_count,
                rows: 1,
                layers: 1,
            }
        } else {
            let cols = limits.max_texture_dimension_1d / tile_size;
            let rows = chunk_count.div_ceil(cols);

            if rows * tile_size <= limits.max_texture_dimension_2d {
                AtlasTextureTiling {
                    cols,
                    rows,
                    layers: 1,
                }
            } else {
                let rows = limits.max_texture_dimension_2d / tile_size;
                let layers = chunk_count.div_ceil(cols * rows);
                assert!(layers <= limits.max_texture_array_layers);
                AtlasTextureTiling {
                    cols,
                    rows,
                    layers,
                }
            }
        }
    }

    pub fn index(&self, atlas_index: u32) -> (u32, u32, u32) {
        return (
            atlas_index % self.cols,
            atlas_index / self.cols % self.rows,
            atlas_index / (self.cols * self.rows),
        );
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CanvasTiling {
    pub cols: u32,
    pub rows: u32,
    pub diff: Size<u32>,
    pub size: u32,
    pub atlas: AtlasTextureTiling,
}

impl CanvasTiling {
    pub fn tile_extent(&self, col: u32, row: u32) -> wgpu::Extent3d {
        wgpu::Extent3d {
            width: if col != self.cols - 1 {
                self.size
            } else {
                self.size - self.diff.width
            },
            height: if row != self.rows - 1 {
                self.size
            } else {
                self.size - self.diff.height
            },
            depth_or_array_layers: 1,
        }
    }

    pub fn atlas_origin(&self, index: u32) -> wgpu::Origin3d {
        let (x, y, z) = self.atlas.index(index);
        wgpu::Origin3d {
            x: x * self.size,
            y: y * self.size,
            z,
        }
    }
}
