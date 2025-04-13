use thiserror::Error;

#[derive(Error, Debug)]
pub enum NsArchiveError {
    #[error("I/O error")]
    Io(#[from] std::io::Error),
    #[error("Plist decoding error")]
    PlistError(#[from] plist::Error),
    #[error("Zip decoding error")]
    ZipError(#[from] zip::result::ZipError),
    #[error("Type mismatch: key {0}")]
    TypeMismatch(String),
    #[error("Missing key {0}")]
    MissingKey(String),
    #[error("Bad index")]
    BadIndex,
}
