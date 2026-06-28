use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use crate::catalog::schema::Schema;
use crate::error::{Error, Result};
use crate::storage::page::{PAGE_SIZE, SlotId, SlottedPage};
use crate::storage::record;
use crate::storage::{DEFAULT_PAGE_SIZE, FORMAT_VERSION, MAGIC};
use crate::types::Value;

// Metadata page (page 0) layout: magic, version, page size, schema length, then
// the serialized schema. Data lives in slotted pages 1..N.
const SCHEMA_OFFSET: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rid {
    pub page: u32,
    pub slot: SlotId,
}

pub struct Table {
    file: File,
    schema: Schema,
}

impl Table {
    pub fn create(path: &Path, schema: Schema) -> Result<Table> {
        let schema_bytes = schema.serialize();
        if SCHEMA_OFFSET + schema_bytes.len() > PAGE_SIZE {
            return Err(Error::SchemaTooLarge);
        }

        let mut meta = [0u8; PAGE_SIZE];
        meta[0..8].copy_from_slice(&MAGIC);
        meta[8..12].copy_from_slice(&FORMAT_VERSION.to_le_bytes());
        meta[12..16].copy_from_slice(&DEFAULT_PAGE_SIZE.to_le_bytes());
        meta[16..20].copy_from_slice(&(schema_bytes.len() as u32).to_le_bytes());
        meta[SCHEMA_OFFSET..SCHEMA_OFFSET + schema_bytes.len()].copy_from_slice(&schema_bytes);

        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(path)?;
        file.write_all(&meta)?;
        file.sync_all()?;

        Ok(Table { file, schema })
    }

    pub fn open(path: &Path) -> Result<Table> {
        let mut file = OpenOptions::new().read(true).write(true).open(path)?;
        let mut meta = [0u8; PAGE_SIZE];
        file.read_exact(&mut meta)?;

        if meta[0..8] != MAGIC {
            return Err(Error::NotASkipperDb(path.to_path_buf()));
        }
        let version = u32::from_le_bytes(meta[8..12].try_into().unwrap());
        if version != FORMAT_VERSION {
            return Err(Error::UnsupportedVersion(version));
        }

        let schema_len = u32::from_le_bytes(meta[16..20].try_into().unwrap()) as usize;
        let schema_bytes = meta
            .get(SCHEMA_OFFSET..SCHEMA_OFFSET + schema_len)
            .ok_or(Error::MalformedSchema)?;
        let schema = Schema::deserialize(schema_bytes)?;

        Ok(Table { file, schema })
    }

    pub fn schema(&self) -> &Schema {
        &self.schema
    }

    pub fn insert(&mut self, values: &[Value]) -> Result<Rid> {
        let record = record::encode(&self.schema, values)?;
        let page_count = self.page_count()?;

        if page_count > 1 {
            let last = page_count - 1;
            let mut page = self.read_page(last)?;
            if let Some(slot) = page.insert(&record) {
                self.write_page(last, &page)?;
                return Ok(Rid { page: last, slot });
            }
        }

        let page_id = page_count.max(1);
        let mut page = SlottedPage::new();
        let slot = page.insert(&record).ok_or(Error::RecordTooLarge)?;
        self.write_page(page_id, &page)?;
        Ok(Rid {
            page: page_id,
            slot,
        })
    }

    pub fn get(&mut self, rid: Rid) -> Result<Option<Vec<Value>>> {
        if rid.page == 0 || rid.page >= self.page_count()? {
            return Ok(None);
        }
        let page = self.read_page(rid.page)?;
        match page.get(rid.slot) {
            Some(bytes) => Ok(Some(record::decode(&self.schema, bytes)?)),
            None => Ok(None),
        }
    }

    fn page_count(&self) -> Result<u32> {
        Ok((self.file.metadata()?.len() / PAGE_SIZE as u64) as u32)
    }

    fn read_page(&mut self, page: u32) -> Result<SlottedPage> {
        let mut buf = [0u8; PAGE_SIZE];
        self.file.seek(SeekFrom::Start(page as u64 * PAGE_SIZE as u64))?;
        self.file.read_exact(&mut buf)?;
        Ok(SlottedPage::from_bytes(buf))
    }

    fn write_page(&mut self, page: u32, slotted: &SlottedPage) -> Result<()> {
        self.file
            .seek(SeekFrom::Start(page as u64 * PAGE_SIZE as u64))?;
        self.file.write_all(slotted.as_bytes())?;
        self.file.sync_all()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::schema::Column;
    use crate::types::ColumnType;
    use tempfile::tempdir;

    fn sample_schema() -> Schema {
        Schema::new(vec![
            Column {
                name: "id".to_string(),
                ty: ColumnType::Int,
                nullable: false,
            },
            Column {
                name: "name".to_string(),
                ty: ColumnType::Text,
                nullable: true,
            },
        ])
    }

    #[test]
    fn create_then_reopen_preserves_schema() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("users.tbl");
        let schema = sample_schema();

        Table::create(&path, schema.clone()).unwrap();
        let table = Table::open(&path).unwrap();

        assert_eq!(table.schema(), &schema);
    }

    #[test]
    fn create_rejects_existing_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("users.tbl");
        Table::create(&path, sample_schema()).unwrap();
        assert!(Table::create(&path, sample_schema()).is_err());
    }

    #[test]
    fn open_rejects_non_table() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("garbage.tbl");
        std::fs::write(&path, vec![0u8; PAGE_SIZE]).unwrap();
        assert!(matches!(Table::open(&path), Err(Error::NotASkipperDb(_))));
    }

    #[test]
    fn insert_then_get() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("users.tbl");
        let mut table = Table::create(&path, sample_schema()).unwrap();

        let row = vec![Value::Int(1), Value::Text("alice".to_string())];
        let rid = table.insert(&row).unwrap();

        assert_eq!(table.get(rid).unwrap(), Some(row));
    }

    #[test]
    fn rows_survive_reopen() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("users.tbl");

        let row1 = vec![Value::Int(1), Value::Text("alice".to_string())];
        let row2 = vec![Value::Int(2), Value::Null];
        let (rid1, rid2);
        {
            let mut table = Table::create(&path, sample_schema()).unwrap();
            rid1 = table.insert(&row1).unwrap();
            rid2 = table.insert(&row2).unwrap();
        }

        let mut table = Table::open(&path).unwrap();
        assert_eq!(table.get(rid1).unwrap(), Some(row1));
        assert_eq!(table.get(rid2).unwrap(), Some(row2));
    }

    #[test]
    fn get_unknown_rid_is_none() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("users.tbl");
        let mut table = Table::create(&path, sample_schema()).unwrap();

        assert_eq!(table.get(Rid { page: 1, slot: 0 }).unwrap(), None);
        assert_eq!(table.get(Rid { page: 0, slot: 0 }).unwrap(), None);
    }

    #[test]
    fn rows_span_multiple_pages() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("big.tbl");
        let mut table = Table::create(&path, sample_schema()).unwrap();

        let mut rids = Vec::new();
        for i in 0..500 {
            let row = vec![Value::Int(i), Value::Text("x".repeat(40))];
            rids.push(table.insert(&row).unwrap());
        }

        assert!(rids.iter().any(|r| r.page > 1));
        assert_eq!(
            table.get(rids[0]).unwrap(),
            Some(vec![Value::Int(0), Value::Text("x".repeat(40))])
        );
        assert_eq!(
            table.get(rids[499]).unwrap(),
            Some(vec![Value::Int(499), Value::Text("x".repeat(40))])
        );
    }
}
