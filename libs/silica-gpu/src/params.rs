use std::sync::atomic::AtomicU32;

use crate::{ZipArchiveMmap, tiling::CanvasTiling};

pub(crate) struct LoadParams<'a> {
    pub(crate) archive: &'a ZipArchiveMmap<'a>,
    pub(crate) file_names: Vec<&'a str>,
    pub(crate) tiling: CanvasTiling,
    pub(crate) chunk_id_counter: AtomicU32,
    pub(crate) addendum_id_counter: AtomicU32,
}
