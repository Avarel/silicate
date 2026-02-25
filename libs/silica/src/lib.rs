pub mod data;
pub mod error;
pub mod info;
mod ns_archive;

pub mod gpu;

pub(crate) type ZipArchiveMmap<'a> = zip::ZipArchive<std::io::Cursor<&'a [u8]>>;
