use crate::catalog::schema::Schema;
use crate::error::{Error, Result};
use crate::types::{ColumnType, Value};

// Serialize one row into bytes: a null bitmap (1 bit per column) followed by the
// non-null values — INT inline (fixed width), TEXT length-prefixed.
pub fn encode(schema: &Schema, values: &[Value]) -> Result<Vec<u8>> {
    if values.len() != schema.columns.len() {
        return Err(Error::RecordArity {
            expected: schema.columns.len(),
            got: values.len(),
        });
    }

    let bitmap_len = schema.columns.len().div_ceil(8);
    let mut buf = vec![0u8; bitmap_len];

    for (i, (column, value)) in schema.columns.iter().zip(values).enumerate() {
        match value {
            Value::Null => buf[i / 8] |= 1 << (i % 8),
            Value::Int(n) if column.ty == ColumnType::Int => {
                buf.extend_from_slice(&n.to_le_bytes());
            }
            Value::Text(s) if column.ty == ColumnType::Text => {
                buf.extend_from_slice(&(s.len() as u32).to_le_bytes());
                buf.extend_from_slice(s.as_bytes());
            }
            _ => {
                return Err(Error::TypeMismatch {
                    column: column.name.clone(),
                });
            }
        }
    }

    Ok(buf)
}

// Inverse of `encode`: rebuild the values for `schema` from a record's bytes.
pub fn decode(schema: &Schema, bytes: &[u8]) -> Result<Vec<Value>> {
    let bitmap_len = schema.columns.len().div_ceil(8);
    let bitmap = bytes.get(..bitmap_len).ok_or(Error::MalformedRecord)?;
    let mut payload = &bytes[bitmap_len..];

    let mut values = Vec::with_capacity(schema.columns.len());
    for (i, column) in schema.columns.iter().enumerate() {
        if bitmap[i / 8] & (1 << (i % 8)) != 0 {
            values.push(Value::Null);
            continue;
        }
        match column.ty {
            ColumnType::Int => {
                let raw = payload.get(..8).ok_or(Error::MalformedRecord)?;
                values.push(Value::Int(i64::from_le_bytes(raw.try_into().unwrap())));
                payload = &payload[8..];
            }
            ColumnType::Text => {
                let len_raw = payload.get(..4).ok_or(Error::MalformedRecord)?;
                let len = u32::from_le_bytes(len_raw.try_into().unwrap()) as usize;
                payload = &payload[4..];
                let text = payload.get(..len).ok_or(Error::MalformedRecord)?;
                let s = std::str::from_utf8(text)
                    .map_err(|_| Error::MalformedRecord)?
                    .to_string();
                values.push(Value::Text(s));
                payload = &payload[len..];
            }
        }
    }

    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::schema::Column;

    fn schema() -> Schema {
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

    fn roundtrip(schema: &Schema, values: &[Value]) -> Vec<Value> {
        let bytes = encode(schema, values).unwrap();
        decode(schema, &bytes).unwrap()
    }

    #[test]
    fn roundtrip_with_text() {
        let values = vec![Value::Int(7), Value::Text("alice".to_string())];
        assert_eq!(roundtrip(&schema(), &values), values);
    }

    #[test]
    fn roundtrip_with_null() {
        let values = vec![Value::Int(-1), Value::Null];
        assert_eq!(roundtrip(&schema(), &values), values);
    }

    #[test]
    fn rejects_wrong_arity() {
        assert!(matches!(
            encode(&schema(), &[Value::Int(1)]),
            Err(Error::RecordArity {
                expected: 2,
                got: 1
            })
        ));
    }

    #[test]
    fn rejects_truncated_bytes() {
        let bytes = encode(&schema(), &[Value::Int(1), Value::Null]).unwrap();
        assert!(matches!(
            decode(&schema(), &bytes[..bytes.len() - 1]),
            Err(Error::MalformedRecord)
        ));
    }
}
