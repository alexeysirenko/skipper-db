use crate::error::{Error, Result};
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
        self.columns.iter().position(|c| c.name == name)
    }

    // Binary form of the schema, written into catalog metadata by create_table.
    pub fn serialize(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&(self.columns.len() as u16).to_le_bytes());
        for column in &self.columns {
            buf.push(match column.ty {
                ColumnType::Int => 0,
                ColumnType::Text => 1,
            });
            buf.push(column.nullable as u8);
            buf.extend_from_slice(&(column.name.len() as u16).to_le_bytes());
            buf.extend_from_slice(column.name.as_bytes());
        }
        buf
    }

    // Reconstruct the schema produced by `serialize`, used by open_table.
    pub fn deserialize(bytes: &[u8]) -> Result<Schema> {
        let mut cursor = bytes;
        let count = read_u16(&mut cursor)? as usize;
        let mut columns = Vec::with_capacity(count);
        for _ in 0..count {
            let ty = match read_u8(&mut cursor)? {
                0 => ColumnType::Int,
                1 => ColumnType::Text,
                _ => return Err(Error::MalformedSchema),
            };
            let nullable = read_u8(&mut cursor)? != 0;
            let name_len = read_u16(&mut cursor)? as usize;
            let name = std::str::from_utf8(take(&mut cursor, name_len)?)
                .map_err(|_| Error::MalformedSchema)?
                .to_string();
            columns.push(Column { name, ty, nullable });
        }
        Ok(Schema { columns })
    }
}

fn take<'a>(cursor: &mut &'a [u8], n: usize) -> Result<&'a [u8]> {
    if cursor.len() < n {
        return Err(Error::MalformedSchema);
    }
    let (head, tail) = cursor.split_at(n);
    *cursor = tail;
    Ok(head)
}

fn read_u8(cursor: &mut &[u8]) -> Result<u8> {
    Ok(take(cursor, 1)?[0])
}

fn read_u16(cursor: &mut &[u8]) -> Result<u16> {
    Ok(u16::from_le_bytes(take(cursor, 2)?.try_into().unwrap()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Schema {
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
    fn column_index_lookup() {
        let schema = sample();
        assert_eq!(schema.column_index("id"), Some(0));
        assert_eq!(schema.column_index("name"), Some(1));
        assert_eq!(schema.column_index("missing"), None);
    }

    #[test]
    fn serialize_deserialize_roundtrip() {
        let schema = sample();
        let bytes = schema.serialize();
        assert_eq!(Schema::deserialize(&bytes).unwrap(), schema);
    }

    #[test]
    fn deserialize_rejects_truncated() {
        let bytes = sample().serialize();
        assert!(matches!(
            Schema::deserialize(&bytes[..bytes.len() - 1]),
            Err(Error::MalformedSchema)
        ));
    }
}
