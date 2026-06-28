use std::fmt;

use crate::parser::ast::{ColumnDef, CompareOp, DataType, Expr, Literal};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjItem {
    pub expr: Expr,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogicalPlan {
    CreateTable {
        table: String,
        columns: Vec<ColumnDef>,
    },
    Insert {
        table: String,
        values: Vec<Literal>,
    },
    Scan {
        table: String,
        columns: Vec<String>,
    },
    Filter {
        predicate: Expr,
        input: Box<LogicalPlan>,
    },
    Projection {
        items: Vec<ProjItem>,
        input: Box<LogicalPlan>,
    },
    Sort {
        column: String,
        descending: bool,
        input: Box<LogicalPlan>,
    },
    Limit {
        count: i64,
        input: Box<LogicalPlan>,
    },
}

impl fmt::Display for LogicalPlan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.write_at(f, 0)
    }
}

impl LogicalPlan {
    fn write_at(&self, f: &mut fmt::Formatter<'_>, depth: usize) -> fmt::Result {
        let pad = "  ".repeat(depth);
        match self {
            LogicalPlan::CreateTable { table, columns } => {
                let cols: Vec<String> = columns.iter().map(format_column).collect();
                writeln!(f, "{pad}CreateTable {table} [{}]", cols.join(", "))
            }
            LogicalPlan::Insert { table, values } => {
                let vals: Vec<String> = values.iter().map(format_literal).collect();
                writeln!(f, "{pad}Insert {table} [{}]", vals.join(", "))
            }
            LogicalPlan::Scan { table, .. } => writeln!(f, "{pad}Scan {table}"),
            LogicalPlan::Filter { predicate, input } => {
                writeln!(f, "{pad}Filter [{}]", format_expr(predicate))?;
                input.write_at(f, depth + 1)
            }
            LogicalPlan::Projection { items, input } => {
                let names: Vec<&str> = items.iter().map(|i| i.name.as_str()).collect();
                writeln!(f, "{pad}Projection [{}]", names.join(", "))?;
                input.write_at(f, depth + 1)
            }
            LogicalPlan::Sort {
                column,
                descending,
                input,
            } => {
                let dir = if *descending { " DESC" } else { "" };
                writeln!(f, "{pad}Sort [{column}{dir}]")?;
                input.write_at(f, depth + 1)
            }
            LogicalPlan::Limit { count, input } => {
                writeln!(f, "{pad}Limit {count}")?;
                input.write_at(f, depth + 1)
            }
        }
    }
}

pub(crate) fn format_column(c: &ColumnDef) -> String {
    let ty = match c.ty {
        DataType::Int => "INT",
        DataType::Text => "TEXT",
    };
    format!("{} {ty}", c.name)
}

pub(crate) fn format_literal(l: &Literal) -> String {
    match l {
        Literal::Int(n) => n.to_string(),
        Literal::Str(s) => format!("'{s}'"),
        Literal::Null => "NULL".to_string(),
    }
}

pub(crate) fn format_expr(e: &Expr) -> String {
    match e {
        Expr::Column(c) => c.clone(),
        Expr::Literal(l) => format_literal(l),
        Expr::Compare { left, op, right } => {
            format!(
                "{} {} {}",
                format_expr(left),
                format_op(*op),
                format_expr(right)
            )
        }
        Expr::And(l, r) => format!("{} AND {}", format_expr(l), format_expr(r)),
        Expr::Or(l, r) => format!("({} OR {})", format_expr(l), format_expr(r)),
    }
}

fn format_op(op: CompareOp) -> &'static str {
    match op {
        CompareOp::Eq => "=",
        CompareOp::NotEq => "!=",
        CompareOp::Lt => "<",
        CompareOp::LtEq => "<=",
        CompareOp::Gt => ">",
        CompareOp::GtEq => ">=",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan() -> LogicalPlan {
        LogicalPlan::Scan {
            table: "users".to_string(),
            columns: vec!["id".to_string(), "name".to_string(), "age".to_string()],
        }
    }

    #[test]
    fn formats_select_tree() {
        let plan = LogicalPlan::Limit {
            count: 10,
            input: Box::new(LogicalPlan::Projection {
                items: vec![
                    ProjItem {
                        expr: Expr::Column("id".to_string()),
                        name: "id".to_string(),
                    },
                    ProjItem {
                        expr: Expr::Column("name".to_string()),
                        name: "name".to_string(),
                    },
                ],
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
            plan.to_string(),
            "Limit 10\n  Projection [id, name]\n    Filter [age > 18]\n      Scan users\n"
        );
    }

    #[test]
    fn formats_create_table() {
        let plan = LogicalPlan::CreateTable {
            table: "users".to_string(),
            columns: vec![
                ColumnDef {
                    name: "id".to_string(),
                    ty: DataType::Int,
                    nullable: false,
                },
                ColumnDef {
                    name: "name".to_string(),
                    ty: DataType::Text,
                    nullable: true,
                },
            ],
        };
        assert_eq!(plan.to_string(), "CreateTable users [id INT, name TEXT]\n");
    }

    #[test]
    fn formats_insert() {
        let plan = LogicalPlan::Insert {
            table: "users".to_string(),
            values: vec![
                Literal::Int(1),
                Literal::Str("Alice".to_string()),
                Literal::Null,
            ],
        };
        assert_eq!(plan.to_string(), "Insert users [1, 'Alice', NULL]\n");
    }
}
