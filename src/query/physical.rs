use std::fmt;

use crate::parser::ast::{ColumnDef, Expr, Literal};
use crate::query::plan::{LogicalPlan, format_column, format_expr, format_literal};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PhysicalPlan {
    CreateTableExec {
        table: String,
        columns: Vec<ColumnDef>,
    },
    InsertExec {
        table: String,
        values: Vec<Literal>,
    },
    TableScanExec {
        table: String,
        columns: Vec<String>,
    },
    FilterExec {
        predicate: Expr,
        input: Box<PhysicalPlan>,
    },
    ProjectionExec {
        columns: Vec<String>,
        input: Box<PhysicalPlan>,
    },
    SortExec {
        column: String,
        descending: bool,
        input: Box<PhysicalPlan>,
    },
    LimitExec {
        count: i64,
        input: Box<PhysicalPlan>,
    },
}

pub fn from_logical(plan: LogicalPlan) -> PhysicalPlan {
    match plan {
        LogicalPlan::CreateTable { table, columns } => {
            PhysicalPlan::CreateTableExec { table, columns }
        }
        LogicalPlan::Insert { table, values } => PhysicalPlan::InsertExec { table, values },
        LogicalPlan::Scan { table, columns } => PhysicalPlan::TableScanExec { table, columns },
        LogicalPlan::Filter { predicate, input } => PhysicalPlan::FilterExec {
            predicate,
            input: Box::new(from_logical(*input)),
        },
        LogicalPlan::Projection { columns, input } => PhysicalPlan::ProjectionExec {
            columns,
            input: Box::new(from_logical(*input)),
        },
        LogicalPlan::Sort {
            column,
            descending,
            input,
        } => PhysicalPlan::SortExec {
            column,
            descending,
            input: Box::new(from_logical(*input)),
        },
        LogicalPlan::Limit { count, input } => PhysicalPlan::LimitExec {
            count,
            input: Box::new(from_logical(*input)),
        },
    }
}

impl fmt::Display for PhysicalPlan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.write_at(f, 0)
    }
}

impl PhysicalPlan {
    fn write_at(&self, f: &mut fmt::Formatter<'_>, depth: usize) -> fmt::Result {
        let pad = "  ".repeat(depth);
        match self {
            PhysicalPlan::CreateTableExec { table, columns } => {
                let cols: Vec<String> = columns.iter().map(format_column).collect();
                writeln!(f, "{pad}CreateTableExec {table} [{}]", cols.join(", "))
            }
            PhysicalPlan::InsertExec { table, values } => {
                let vals: Vec<String> = values.iter().map(format_literal).collect();
                writeln!(f, "{pad}InsertExec {table} [{}]", vals.join(", "))
            }
            PhysicalPlan::TableScanExec { table, .. } => writeln!(f, "{pad}TableScanExec {table}"),
            PhysicalPlan::FilterExec { predicate, input } => {
                writeln!(f, "{pad}FilterExec [{}]", format_expr(predicate))?;
                input.write_at(f, depth + 1)
            }
            PhysicalPlan::ProjectionExec { columns, input } => {
                writeln!(f, "{pad}ProjectionExec [{}]", columns.join(", "))?;
                input.write_at(f, depth + 1)
            }
            PhysicalPlan::SortExec {
                column,
                descending,
                input,
            } => {
                let dir = if *descending { " DESC" } else { "" };
                writeln!(f, "{pad}SortExec [{column}{dir}]")?;
                input.write_at(f, depth + 1)
            }
            PhysicalPlan::LimitExec { count, input } => {
                writeln!(f, "{pad}LimitExec {count}")?;
                input.write_at(f, depth + 1)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ast::{CompareOp, DataType};

    fn scan() -> LogicalPlan {
        LogicalPlan::Scan {
            table: "users".to_string(),
            columns: vec!["id".to_string(), "name".to_string(), "age".to_string()],
        }
    }

    #[test]
    fn lowers_and_formats_select() {
        let logical = LogicalPlan::Limit {
            count: 10,
            input: Box::new(LogicalPlan::Projection {
                columns: vec!["id".to_string(), "name".to_string()],
                input: Box::new(LogicalPlan::Filter {
                    predicate: Expr::Compare {
                        left: Box::new(Expr::Column("age".to_string())),
                        op: CompareOp::Gt,
                        right: Box::new(Expr::Literal(Literal::Int(18))),
                    },
                    input: Box::new(scan()),
                }),
            }),
        };

        assert_eq!(
            from_logical(logical).to_string(),
            "LimitExec 10\n  ProjectionExec [id, name]\n    FilterExec [age > 18]\n      TableScanExec users\n"
        );
    }

    #[test]
    fn lowers_create_table() {
        let logical = LogicalPlan::CreateTable {
            table: "users".to_string(),
            columns: vec![ColumnDef {
                name: "id".to_string(),
                ty: DataType::Int,
                nullable: false,
            }],
        };
        assert_eq!(
            from_logical(logical).to_string(),
            "CreateTableExec users [id INT]\n"
        );
    }
}
