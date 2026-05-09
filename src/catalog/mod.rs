use std::path::Path;

use crate::error::Result;
use crate::storage::FileHeader;

pub const DATA_FILE: &str = "skipper.dat";

pub fn init(dir: &Path) -> Result<()> {
    tracing::info!(path = %dir.display(), "initializing database directory");
    std::fs::create_dir(dir)?;
    FileHeader::write(&dir.join(DATA_FILE))
}

pub fn open(dir: &Path) -> Result<FileHeader> {
    tracing::info!(path = %dir.display(), "opening database directory");
    FileHeader::read(&dir.join(DATA_FILE))
}
