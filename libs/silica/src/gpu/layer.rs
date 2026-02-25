use std::num::NonZeroU32;

use crate::info::layer::SilicaLayer;

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
pub struct SilicaLayerGpu {
    pub info: SilicaLayer,
    pub image: SilicaImageData,
    pub addendum: Addendum,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Addendum {
    pub id: u32,
}
