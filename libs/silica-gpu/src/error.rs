use thiserror::Error;

#[derive(Error, Debug)]
pub enum SilicaError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("silica error: {0}")]
    Silica(#[from] silica::error::SilicaError),
    #[error("Ns archive error: {0}")]
    NsArchiveError(#[from] silica::ns_archive::error::NsArchiveError),
    #[error("Zip error: {0}")]
    ZipError(#[from] zip::result::ZipError),
    #[error("LZO error: {0}")]
    LzoError(#[from] lzokay::Error),
    #[error("LZ4 error: {0}")]
    Lz4Error(#[from] lz4_flex::block::DecompressError),
    #[error("Corrupted format")]
    CorruptedFormat,
}
