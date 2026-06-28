#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnType {
    Int,
    Text,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    Text(String),
    Null,
}

impl Value {
    pub fn type_of(&self) -> Option<ColumnType> {
        match self {
            Value::Int(_) => Some(ColumnType::Int),
            Value::Text(_) => Some(ColumnType::Text),
            Value::Null => None,
        }
    }
}
