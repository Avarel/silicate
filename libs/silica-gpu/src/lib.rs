pub mod error;

mod params;
mod tiling;
mod types;

type ZipArchiveMmap<'a> = zip::ZipArchive<std::io::Cursor<&'a [u8]>>;

pub use types::{
    file::ProcreateFile, file::ProcreateFileAtlas, group::SilicaGroup, hierarchy::SilicaHierarchy,
    layer::SilicaLayer,
};

pub use silica::{BlendingMode, Flipped, Orientation};
