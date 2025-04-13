use thiserror::Error;

#[derive(Error, Debug)]
pub enum SilicaError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Plist error: {0}")]
    PlistError(#[from] plist::Error),
    #[error("Zip error: {0}")]
    ZipError(#[from] zip::result::ZipError),
    #[error("LZO error: {0}")]
    LzoError(#[from] minilzo_rs::Error),
    #[error("LZ4 error: {0}")]
    Lz4Error(#[from] lz4_flex::block::DecompressError),
    #[error("Ns archive error: {0}")]
    NsArchiveError(#[from] crate::ns_archive::error::NsArchiveError),
    #[error("Invalid values in file")]
    InvalidValue,
    #[error("Unknown decoding error")]
    #[allow(dead_code)]
    Unknown,
}
