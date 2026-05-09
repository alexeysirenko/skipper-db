use std::path::Path;

use crate::error::Result;
use crate::storage::FileHeader;

pub const DATA_FILE: &str = "skipper.dat";

pub fn init(dir: &Path) -> Result<()> {
    std::fs::create_dir(dir)?;
    FileHeader::write(&dir.join(DATA_FILE))
}

pub fn open(dir: &Path) -> Result<FileHeader> {
    FileHeader::read(&dir.join(DATA_FILE))
}
