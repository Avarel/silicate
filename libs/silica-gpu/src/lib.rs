pub mod error;
pub mod file;
pub mod group;
pub mod hierarchy;
pub mod layer;
pub mod tiling;

pub(crate) mod ir;

pub type ZipArchiveMmap<'a> = zip::ZipArchive<std::io::Cursor<&'a [u8]>>;

pub use silica as raw;
