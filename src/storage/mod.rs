use std::fs::File;
use std::fs::OpenOptions;
use std::io::Read;
use std::io::Write;
use std::path::Path;

use crate::error::{Error, Result};

pub const MAGIC: [u8; 8] = *b"SKPRDB\0\0";
pub const FORMAT_VERSION: u32 = 1;
pub const DEFAULT_PAGE_SIZE: u32 = 4096;
pub struct FileHeader {
    pub magic: [u8; 8],
    pub version: u32,
    pub page_size: u32,
}

impl FileHeader {
    pub fn write(path: &Path) -> Result<()> {
        tracing::debug!(path = %path.display(), "writing file header");
        let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
        let version_bytes = FORMAT_VERSION.to_le_bytes();
        let page_size_bytes = DEFAULT_PAGE_SIZE.to_le_bytes();
        file.write_all(&MAGIC)?;
        file.write_all(&version_bytes)?;
        file.write_all(&page_size_bytes)?;
        let header_used = MAGIC.len() + 4 + 4;
        let padding = vec![0u8; DEFAULT_PAGE_SIZE as usize - header_used];
        file.write_all(&padding)?;

        file.sync_all()?;
        Ok(())
    }

    pub fn read(path: &Path) -> Result<FileHeader> {
        let mut file = File::open(path)?;
        let mut magic = [0u8; 8];
        let mut version = [0u8; 4];
        let mut page_size = [0u8; 4];

        file.read_exact(&mut magic)?;
        file.read_exact(&mut version)?;
        file.read_exact(&mut page_size)?;

        if magic != MAGIC {
            return Err(Error::NotASkipperDb(path.to_path_buf()));
        }

        let version = u32::from_le_bytes(version);
        let page_size = u32::from_le_bytes(page_size);

        if version != FORMAT_VERSION {
            return Err(Error::UnsupportedVersion(version));
        }

        tracing::debug!(path = %path.display(), version, page_size, "read file header");

        Ok(FileHeader {
            magic,
            version,
            page_size,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn write_then_read_roundtrip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("skipper.dat");
        FileHeader::write(&path).unwrap();
        let h = FileHeader::read(&path).unwrap();
        assert_eq!(h.magic, MAGIC);
        assert_eq!(h.version, FORMAT_VERSION);
        assert_eq!(h.page_size, DEFAULT_PAGE_SIZE);
    }

    #[test]
    fn read_rejects_non_skipper_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("garbage.dat");
        std::fs::write(&path, vec![0u8; 4096]).unwrap();
        assert!(matches!(
            FileHeader::read(&path),
            Err(Error::NotASkipperDb(_))
        ));
    }
}
