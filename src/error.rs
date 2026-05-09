use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("path is not a Skipper database: {0}")]
    NotASkipperDb(std::path::PathBuf),
    #[error("unsupported format version {0}")]
    UnsupportedVersion(u32),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
