use std::sync::atomic::AtomicU32;

use crate::{ZipArchiveMmap, tiling::CanvasTiling};

pub(crate) struct LoadParams<'a> {
    pub(crate) queue: &'a wgpu::Queue,
    pub(crate) archive: &'a ZipArchiveMmap<'a>,
    pub(crate) file_names: Vec<&'a str>,
    pub(crate) tiling: CanvasTiling,
    pub(crate) atlas_texture: &'a wgpu::Texture,
    pub(crate) chunk_id_counter: AtomicU32,
    pub(crate) addendum_id_counter: AtomicU32,
}
