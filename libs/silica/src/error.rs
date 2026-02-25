use thiserror::Error;

#[derive(Error, Debug)]
pub enum SilicaError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Plist error: {0}")]
    PlistError(#[from] plist::Error),
    #[error("Ns archive error: {0}")]
    NsArchiveError(#[from] crate::ns_archive::error::NsArchiveError),
    #[error("Invalid values in file")]
    InvalidValue,
    #[error("Unknown decoding error")]
    #[allow(dead_code)]
    Unknown,
}
