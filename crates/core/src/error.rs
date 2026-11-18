use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("bad data: {0}")]
    Format(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid: {0}")]
    Invalid(String),
    #[error("storage: {0}")]
    Io(#[from] std::io::Error),
    #[error("merge: {0}")]
    Crdt(String),
}

impl From<loro::LoroError> for Error {
    fn from(e: loro::LoroError) -> Self {
        Error::Crdt(e.to_string())
    }
}

impl From<loro::LoroEncodeError> for Error {
    fn from(e: loro::LoroEncodeError) -> Self {
        Error::Crdt(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
