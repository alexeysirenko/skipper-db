use crate::error::Result;
use crate::types::ColumnType;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Column {
    pub name: String,
    pub ty: ColumnType,
    pub nullable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Schema {
    pub columns: Vec<Column>,
}

impl Schema {
    pub fn new(columns: Vec<Column>) -> Self {
        Self { columns }
    }

    pub fn column_index(&self, name: &str) -> Option<usize> {
        todo!()
    }

    // Binary form of the schema, written into catalog metadata by create_table.
    pub fn serialize(&self) -> Vec<u8> {
        todo!()
    }

    // Reconstruct the schema produced by `serialize`, used by open_table.
    pub fn deserialize(bytes: &[u8]) -> Result<Schema> {
        todo!()
    }
}
