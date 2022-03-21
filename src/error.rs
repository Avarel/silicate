use thiserror::Error;

#[derive(Error, Debug)]
pub enum NsArchiveError {
    #[error("i/o error")]
    Io(#[from] std::io::Error),
    #[error("type mismatch")]
    TypeMismatch,
    #[error("missing key")]
    MissingKey,
    #[error("bad index")]
    BadIndex
}