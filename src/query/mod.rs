pub mod exec;
pub mod physical;
pub mod plan;

use std::collections::HashSet;
use std::fmt;
use std::path::PathBuf;

use crate::catalog::schema::Schema;
use crate::parser::ast::{
    CreateTable, Expr, Insert, Literal, Projection, Select, SelectItem, Statement,
};
use crate::storage::table::Table;
use plan::{LogicalPlan, ProjItem};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanError {
    pub message: String,
}

impl fmt::Display for PlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "plan error: {}", self.message)
    }
}

impl std::error::Error for PlanError {}

pub trait Catalog {
    fn schema(&self, table: &str) -> Option<Schema>;
}

pub struct DirCatalog {
    dir: PathBuf,
}

impl DirCatalog {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }
}

impl Catalog for DirCatalog {
    fn schema(&self, table: &str) -> Option<Schema> {
        let path = self.dir.join(format!("{table}.tbl"));
        Table::open(&path).ok().map(|t| t.schema().clone())
    }
}

pub fn plan(statement: Statement, catalog: &dyn Catalog) -> Result<LogicalPlan, PlanError> {
    match statement {
        Statement::CreateTable(ct) => plan_create_table(ct),
        Statement::Insert(ins) => plan_insert(ins, catalog),
        Statement::Select(sel) => plan_select(sel, catalog),
    }
}

fn plan_create_table(ct: CreateTable) -> Result<LogicalPlan, PlanError> {
    {
        let mut seen = HashSet::new();
        for c in &ct.columns {
            if !seen.insert(c.name.as_str()) {
                return Err(err(format!("duplicate column \"{}\"", c.name)));
            }
        }
    }
    Ok(LogicalPlan::CreateTable {
        table: ct.table,
        columns: ct.columns,
    })
}

fn plan_insert(ins: Insert, catalog: &dyn Catalog) -> Result<LogicalPlan, PlanError> {
    let Insert {
        table,
        columns,
        values,
    } = ins;
    let schema = catalog
        .schema(&table)
        .ok_or_else(|| unknown_table(&table))?;

    // Reorder values into table-column order so execution can insert positionally.
    let ordered: Vec<Literal> = match columns {
        None => {
            if values.len() != schema.columns.len() {
                return Err(err(format!(
                    "table \"{table}\" expects {} values but got {}",
                    schema.columns.len(),
                    values.len()
                )));
            }
            values
        }
        Some(names) => {
            if names.len() != values.len() {
                return Err(err(format!(
                    "got {} columns but {} values",
                    names.len(),
                    values.len()
                )));
            }
            for n in &names {
                if schema.column_index(n).is_none() {
                    return Err(unknown_column(n, &table));
                }
            }
            schema
                .columns
                .iter()
                .map(|col| {
                    names
                        .iter()
                        .position(|n| n == &col.name)
                        .map(|i| values[i].clone())
                        .unwrap_or(Literal::Null)
                })
                .collect()
        }
    };

    for (value, column) in ordered.iter().zip(&schema.columns) {
        if matches!(value, Literal::Null) && !column.nullable {
            return Err(err(format!("column \"{}\" is NOT NULL", column.name)));
        }
    }

    Ok(LogicalPlan::Insert {
        table,
        values: ordered,
    })
}

fn plan_select(sel: Select, catalog: &dyn Catalog) -> Result<LogicalPlan, PlanError> {
    let schema = catalog
        .schema(&sel.from)
        .ok_or_else(|| unknown_table(&sel.from))?;
    let table_columns: Vec<String> = schema.columns.iter().map(|c| c.name.clone()).collect();

    let mut node = LogicalPlan::Scan {
        table: sel.from.clone(),
        columns: table_columns.clone(),
    };

    if let Some(predicate) = sel.filter {
        check_columns(&predicate, &schema, &sel.from)?;
        node = LogicalPlan::Filter {
            predicate,
            input: Box::new(node),
        };
    }

    if let Some(order) = sel.order_by {
        if schema.column_index(&order.column).is_none() {
            return Err(unknown_column(&order.column, &sel.from));
        }
        node = LogicalPlan::Sort {
            column: order.column,
            descending: order.descending,
            input: Box::new(node),
        };
    }

    let items: Vec<ProjItem> = match sel.projection {
        Projection::All => table_columns
            .iter()
            .map(|c| ProjItem {
                expr: Expr::Column(c.clone()),
                name: c.clone(),
            })
            .collect(),
        Projection::Items(select_items) => {
            let mut out = Vec::with_capacity(select_items.len());
            for item in select_items {
                check_columns(&item.expr, &schema, &sel.from)?;
                let name = output_name(&item);
                out.push(ProjItem {
                    expr: item.expr,
                    name,
                });
            }
            out
        }
    };
    node = LogicalPlan::Projection {
        items,
        input: Box::new(node),
    };

    if let Some(count) = sel.limit {
        if count < 0 {
            return Err(err(format!("LIMIT must not be negative, got {count}")));
        }
        node = LogicalPlan::Limit {
            count,
            input: Box::new(node),
        };
    }

    Ok(node)
}

fn check_columns(e: &Expr, schema: &Schema, table: &str) -> Result<(), PlanError> {
    match e {
        Expr::Column(c) => {
            if schema.column_index(c).is_none() {
                return Err(unknown_column(c, table));
            }
            Ok(())
        }
        Expr::Literal(_) => Ok(()),
        Expr::Compare { left, right, .. } => {
            check_columns(left, schema, table)?;
            check_columns(right, schema, table)
        }
        Expr::And(l, r) | Expr::Or(l, r) => {
            check_columns(l, schema, table)?;
            check_columns(r, schema, table)
        }
    }
}

fn output_name(item: &SelectItem) -> String {
    if let Some(alias) = &item.alias {
        return alias.clone();
    }
    match &item.expr {
        Expr::Column(c) => c.clone(),
        other => plan::format_expr(other),
    }
}

fn err(message: String) -> PlanError {
    PlanError { message }
}

fn unknown_table(table: &str) -> PlanError {
    err(format!("unknown table \"{table}\""))
}

fn unknown_column(column: &str, table: &str) -> PlanError {
    err(format!("unknown column \"{column}\" in table \"{table}\""))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::schema::Column;
    use crate::parser::parse;
    use crate::types::ColumnType;
    use std::collections::HashMap;

    struct MockCatalog {
        tables: HashMap<String, Schema>,
    }

    impl Catalog for MockCatalog {
        fn schema(&self, table: &str) -> Option<Schema> {
            self.tables.get(table).cloned()
        }
    }

    fn users_catalog() -> MockCatalog {
        let schema = Schema::new(vec![
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
            Column {
                name: "age".to_string(),
                ty: ColumnType::Int,
                nullable: true,
            },
        ]);
        MockCatalog {
            tables: HashMap::from([("users".to_string(), schema)]),
        }
    }

    fn planned(sql: &str) -> Result<LogicalPlan, PlanError> {
        plan(parse(sql).unwrap(), &users_catalog())
    }

    #[test]
    fn plans_select_filter_limit() {
        let plan = planned("SELECT id, name FROM users WHERE age > 18 LIMIT 10").unwrap();
        assert_eq!(
            plan.to_string(),
            "Limit 10\n  Projection [id, name]\n    Filter [age > 18]\n      Scan users\n"
        );
    }

    #[test]
    fn plans_select_star_with_sort() {
        let plan = planned("SELECT * FROM users ORDER BY name DESC").unwrap();
        assert_eq!(
            plan.to_string(),
            "Projection [id, name, age]\n  Sort [name DESC]\n    Scan users\n"
        );
    }

    #[test]
    fn plans_create_table() {
        let plan = planned("CREATE TABLE t (a INT, b TEXT)").unwrap();
        assert_eq!(plan.to_string(), "CreateTable t [a INT, b TEXT]\n");
    }

    #[test]
    fn plans_insert() {
        let plan = planned("INSERT INTO users VALUES (1, 'Alice', 20)").unwrap();
        assert_eq!(plan.to_string(), "Insert users [1, 'Alice', 20]\n");
    }

    #[test]
    fn rejects_unknown_table() {
        assert!(planned("SELECT * FROM ghost").is_err());
        assert!(planned("INSERT INTO ghost VALUES (1)").is_err());
    }

    #[test]
    fn rejects_unknown_columns() {
        assert!(planned("SELECT zzz FROM users").is_err());
        assert!(planned("SELECT id FROM users WHERE zzz > 1").is_err());
        assert!(planned("SELECT id FROM users ORDER BY zzz").is_err());
    }

    #[test]
    fn rejects_duplicate_create_columns() {
        assert!(
            plan(
                parse("CREATE TABLE t (id INT, id TEXT)").unwrap(),
                &users_catalog()
            )
            .is_err()
        );
    }

    #[test]
    fn rejects_insert_value_count_mismatch() {
        assert!(planned("INSERT INTO users VALUES (1)").is_err());
    }

    #[test]
    fn insert_column_list_is_reordered() {
        let plan = planned("INSERT INTO users (age, id, name) VALUES (20, 1, 'Alice')").unwrap();
        assert_eq!(plan.to_string(), "Insert users [1, 'Alice', 20]\n");
    }

    #[test]
    fn rejects_null_into_not_null_column() {
        assert!(planned("INSERT INTO users VALUES (NULL, 'a', 1)").is_err());
    }

    #[test]
    fn rejects_negative_limit() {
        assert!(planned("SELECT id FROM users LIMIT -1").is_err());
    }
}
