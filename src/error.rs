use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("path is not a Skipper database: {0}")]
    NotASkipperDb(std::path::PathBuf),
    #[error("unsupported format version {0}")]
    UnsupportedVersion(u32),
    #[error("record has {got} values but schema has {expected} columns")]
    RecordArity { expected: usize, got: usize },
    #[error("value type does not match column \"{column}\"")]
    TypeMismatch { column: String },
    #[error("malformed record")]
    MalformedRecord,
    #[error("malformed schema")]
    MalformedSchema,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
