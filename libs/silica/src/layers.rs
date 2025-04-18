use std::num::NonZeroU32;

use crate::ns_archive::Size;
use silicate_compositor::blend::BlendingMode;

#[derive(Debug, Clone, Copy)]
pub struct AtlasTextureTiling {
    pub cols: u32,
    pub rows: u32,
    pub layers: u32,
}

impl AtlasTextureTiling {
    pub fn compute_atlas_size(chunk_count: u32, tile_size: u32) -> Self {
        const TEX_MAX_DIM: u32 = 8192;
        if chunk_count * tile_size <= TEX_MAX_DIM {
            AtlasTextureTiling {
                cols: chunk_count,
                rows: 1,
                layers: 1,
            }
        } else {
            let columns = TEX_MAX_DIM / tile_size;
            let rows = chunk_count.div_ceil(columns);

            if rows * tile_size <= TEX_MAX_DIM {
                AtlasTextureTiling {
                    cols: columns,
                    rows,
                    layers: 1,
                }
            } else {
                let rows = TEX_MAX_DIM / tile_size;
                let layers = chunk_count.div_ceil(columns * rows);
                AtlasTextureTiling {
                    cols: columns,
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
    pub fn tile_extent(&self, col: u32, row: u32) -> silicate_compositor::tex::Extent3d {
        silicate_compositor::tex::Extent3d {
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

    pub fn atlas_origin(&self, index: u32) -> silicate_compositor::tex::Origin3d {
        let (x, y, z) = self.atlas.index(index);
        silicate_compositor::tex::Origin3d {
            x: x * self.size,
            y: y * self.size,
            z,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SilicaHierarchy {
    Layer(SilicaLayer),
    Group(SilicaGroup),
}

#[derive(Debug, Clone, PartialEq)]
pub struct SilicaGroup {
    pub hidden: bool,
    pub children: Vec<SilicaHierarchy>,
    pub name: Option<String>,

    // This is unofficial
    pub id: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SilicaChunk {
    pub col: u32,
    pub row: u32,
    pub atlas_index: NonZeroU32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SilicaImageData {
    pub chunks: Vec<SilicaChunk>,
}



#[derive(Debug, Clone, PartialEq)]
pub struct SilicaLayer {
    // animationHeldLength:Int?
    pub blend: BlendingMode,
    // bundledImagePath:String?
    // bundledMaskPath:String?
    // bundledVideoPath:String?
    pub clipped: bool,
    // contentsRect:Data?
    // contentsRectValid:Bool?
    // document:SilicaDocument?
    // extendedBlend:Int?
    pub hidden: bool,
    // locked:Bool?
    pub mask: Option<usize>,
    pub name: Option<String>,
    pub opacity: f32,
    // perspectiveAssisted:Bool?
    // preserve:Bool?
    // private:Bool?
    // text:ValkyrieText?
    // textPDF:Data?
    // transform:Data?
    // type:Int?
    pub size: Size<u32>,
    pub uuid: String,
    pub version: u64,

    pub image: SilicaImageData,

    // This is unofficial
    pub id: u32,
}
