use std::sync::atomic::{AtomicU32, Ordering};

use crate::{ZipArchiveMmap, tiling::CanvasTiling};

pub(crate) struct LoadParams<'a> {
    pub(crate) queue: &'a wgpu::Queue,
    pub(crate) archive: &'a ZipArchiveMmap<'a>,
    pub(crate) file_names: Vec<&'a str>,
    pub(crate) tiling: CanvasTiling,
    pub(crate) atlas_texture: &'a wgpu::Texture,
    pub(crate) chunk_id_counter: AtomicU32,
    pub(crate) layer_id_counter: AtomicU32,
}

impl LoadParams<'_> {
    pub fn allocate_layer_id(&self) -> u32 {
        self.layer_id_counter.fetch_add(1, Ordering::Relaxed)
    }

    pub fn allocate_chunk_id(&self) -> u32 {
        self.chunk_id_counter.fetch_add(1, Ordering::Relaxed)
    }
}
