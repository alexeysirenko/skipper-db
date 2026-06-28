use std::cmp::Ordering;
use std::path::{Path, PathBuf};

use crate::catalog::schema::{Column, Schema};
use crate::error::Result;
use crate::parser::ast::{ColumnDef, CompareOp, DataType, Expr, Literal};
use crate::query::physical::PhysicalPlan;
use crate::storage::table::Table;
use crate::types::{ColumnType, Value};

type Row = Vec<Value>;

pub struct ResultSet {
    pub columns: Vec<String>,
    pub rows: Vec<Row>,
}

pub enum QueryResult {
    Select(ResultSet),
    Inserted,
    Created(String),
}

pub fn execute(plan: PhysicalPlan, dir: &Path) -> Result<QueryResult> {
    match plan {
        PhysicalPlan::CreateTableExec { table, columns } => {
            std::fs::create_dir_all(dir)?;
            let schema = Schema::new(columns.iter().map(to_column).collect());
            Table::create(&table_path(dir, &table), schema)?;
            Ok(QueryResult::Created(table))
        }
        PhysicalPlan::InsertExec { table, values } => {
            let mut t = Table::open(&table_path(dir, &table))?;
            let row: Row = values.iter().map(to_value).collect();
            t.insert(&row)?;
            Ok(QueryResult::Inserted)
        }
        query => {
            let (mut op, columns) = build(&query, dir)?;
            let mut rows = Vec::new();
            while let Some(row) = op.next() {
                rows.push(row);
            }
            Ok(QueryResult::Select(ResultSet { columns, rows }))
        }
    }
}

trait Operator {
    fn next(&mut self) -> Option<Row>;
}

fn build(plan: &PhysicalPlan, dir: &Path) -> Result<(Box<dyn Operator>, Vec<String>)> {
    match plan {
        PhysicalPlan::TableScanExec { table, columns } => {
            let rows = read_table(dir, table)?;
            Ok((
                Box::new(ScanOp {
                    rows: rows.into_iter(),
                }),
                columns.clone(),
            ))
        }
        PhysicalPlan::FilterExec { predicate, input } => {
            let (child, cols) = build(input, dir)?;
            Ok((
                Box::new(FilterOp {
                    input: child,
                    predicate: predicate.clone(),
                    columns: cols.clone(),
                }),
                cols,
            ))
        }
        PhysicalPlan::ProjectionExec { items, input } => {
            let (child, child_cols) = build(input, dir)?;
            let exprs = items.iter().map(|i| i.expr.clone()).collect();
            let out_cols = items.iter().map(|i| i.name.clone()).collect();
            Ok((
                Box::new(ProjectOp {
                    input: child,
                    exprs,
                    columns: child_cols,
                }),
                out_cols,
            ))
        }
        PhysicalPlan::SortExec {
            column,
            descending,
            input,
        } => {
            let (child, cols) = build(input, dir)?;
            let index = cols.iter().position(|x| x == column).unwrap();
            Ok((
                Box::new(SortOp {
                    input: child,
                    index,
                    descending: *descending,
                    sorted: None,
                }),
                cols,
            ))
        }
        PhysicalPlan::LimitExec { count, input } => {
            let (child, cols) = build(input, dir)?;
            Ok((
                Box::new(LimitOp {
                    input: child,
                    remaining: *count as usize,
                }),
                cols,
            ))
        }
        PhysicalPlan::CreateTableExec { .. } | PhysicalPlan::InsertExec { .. } => {
            unreachable!("DDL/DML is handled directly in execute")
        }
    }
}

struct ScanOp {
    rows: std::vec::IntoIter<Row>,
}

impl Operator for ScanOp {
    fn next(&mut self) -> Option<Row> {
        self.rows.next()
    }
}

struct FilterOp {
    input: Box<dyn Operator>,
    predicate: Expr,
    columns: Vec<String>,
}

impl Operator for FilterOp {
    fn next(&mut self) -> Option<Row> {
        while let Some(row) = self.input.next() {
            if eval_predicate(&self.predicate, &row, &self.columns) {
                return Some(row);
            }
        }
        None
    }
}

struct ProjectOp {
    input: Box<dyn Operator>,
    exprs: Vec<Expr>,
    columns: Vec<String>,
}

impl Operator for ProjectOp {
    fn next(&mut self) -> Option<Row> {
        self.input.next().map(|row| {
            self.exprs
                .iter()
                .map(|e| eval_scalar(e, &row, &self.columns))
                .collect()
        })
    }
}

struct SortOp {
    input: Box<dyn Operator>,
    index: usize,
    descending: bool,
    sorted: Option<std::vec::IntoIter<Row>>,
}

impl Operator for SortOp {
    fn next(&mut self) -> Option<Row> {
        if self.sorted.is_none() {
            let mut rows = Vec::new();
            while let Some(row) = self.input.next() {
                rows.push(row);
            }
            rows.sort_by(|a, b| {
                let ord = value_order(&a[self.index], &b[self.index]);
                if self.descending { ord.reverse() } else { ord }
            });
            self.sorted = Some(rows.into_iter());
        }
        self.sorted.as_mut().unwrap().next()
    }
}

struct LimitOp {
    input: Box<dyn Operator>,
    remaining: usize,
}

impl Operator for LimitOp {
    fn next(&mut self) -> Option<Row> {
        if self.remaining == 0 {
            return None;
        }
        let row = self.input.next()?;
        self.remaining -= 1;
        Some(row)
    }
}

fn eval_predicate(expr: &Expr, row: &[Value], columns: &[String]) -> bool {
    match expr {
        Expr::And(l, r) => eval_predicate(l, row, columns) && eval_predicate(r, row, columns),
        Expr::Or(l, r) => eval_predicate(l, row, columns) || eval_predicate(r, row, columns),
        Expr::Compare { left, op, right } => {
            let l = eval_scalar(left, row, columns);
            let r = eval_scalar(right, row, columns);
            compare(&l, *op, &r)
        }
        Expr::Column(_) | Expr::Literal(_) | Expr::Function { .. } => {
            matches!(eval_scalar(expr, row, columns), Value::Int(n) if n != 0)
        }
    }
}

fn eval_scalar(expr: &Expr, row: &[Value], columns: &[String]) -> Value {
    match expr {
        Expr::Column(name) => columns
            .iter()
            .position(|c| c == name)
            .map(|i| row[i].clone())
            .unwrap_or(Value::Null),
        Expr::Literal(l) => to_value(l),
        Expr::Function { name, args } => apply_function(name, eval_scalar(&args[0], row, columns)),
        _ => Value::Int(eval_predicate(expr, row, columns) as i64),
    }
}

fn apply_function(name: &str, arg: Value) -> Value {
    // Only LENGTH is supported; the planner rejects anything else.
    if name.eq_ignore_ascii_case("LENGTH") {
        match arg {
            Value::Text(s) => Value::Int(s.chars().count() as i64),
            _ => Value::Null,
        }
    } else {
        Value::Null
    }
}

// NULL and type-mismatched comparisons are false (no three-valued logic).
fn compare(l: &Value, op: CompareOp, r: &Value) -> bool {
    let ord = match (l, r) {
        (Value::Int(a), Value::Int(b)) => a.cmp(b),
        (Value::Text(a), Value::Text(b)) => a.cmp(b),
        _ => return false,
    };
    match op {
        CompareOp::Eq => ord == Ordering::Equal,
        CompareOp::NotEq => ord != Ordering::Equal,
        CompareOp::Lt => ord == Ordering::Less,
        CompareOp::LtEq => ord != Ordering::Greater,
        CompareOp::Gt => ord == Ordering::Greater,
        CompareOp::GtEq => ord != Ordering::Less,
    }
}

// Total order for ORDER BY; NULL sorts first.
fn value_order(a: &Value, b: &Value) -> Ordering {
    match (a, b) {
        (Value::Null, Value::Null) => Ordering::Equal,
        (Value::Null, _) => Ordering::Less,
        (_, Value::Null) => Ordering::Greater,
        (Value::Int(x), Value::Int(y)) => x.cmp(y),
        (Value::Text(x), Value::Text(y)) => x.cmp(y),
        (Value::Int(_), Value::Text(_)) => Ordering::Less,
        (Value::Text(_), Value::Int(_)) => Ordering::Greater,
    }
}

fn read_table(dir: &Path, table: &str) -> Result<Vec<Row>> {
    let mut t = Table::open(&table_path(dir, table))?;
    t.scan().map(|r| r.map(|(_, row)| row)).collect()
}

fn table_path(dir: &Path, table: &str) -> PathBuf {
    dir.join(format!("{table}.tbl"))
}

fn to_value(l: &Literal) -> Value {
    match l {
        Literal::Int(n) => Value::Int(*n),
        Literal::Str(s) => Value::Text(s.clone()),
        Literal::Null => Value::Null,
    }
}

fn to_column(c: &ColumnDef) -> Column {
    Column {
        name: c.name.clone(),
        ty: match c.ty {
            DataType::Int => ColumnType::Int,
            DataType::Text => ColumnType::Text,
        },
        nullable: c.nullable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;
    use crate::query::physical::from_logical;
    use crate::query::{DirCatalog, plan};
    use tempfile::tempdir;

    fn run(dir: &Path, sql: &str) -> QueryResult {
        let statement = parse(sql).unwrap();
        let logical = plan(statement, &DirCatalog::new(dir.to_path_buf())).unwrap();
        execute(from_logical(logical), dir).unwrap()
    }

    fn rows(result: QueryResult) -> Vec<Row> {
        match result {
            QueryResult::Select(rs) => rs.rows,
            _ => panic!("expected a result set"),
        }
    }

    fn setup(dir: &Path) {
        run(dir, "CREATE TABLE users (id INT, name TEXT, age INT)");
        run(dir, "INSERT INTO users VALUES (1, 'Alice', 20)");
        run(dir, "INSERT INTO users VALUES (2, 'Bob', 17)");
        run(dir, "INSERT INTO users VALUES (3, 'Carol', 30)");
    }

    #[test]
    fn select_filter_project_sort_limit() {
        let tmp = tempdir().unwrap();
        let dir = tmp.path();
        setup(dir);

        let result = run(
            dir,
            "SELECT id, name FROM users WHERE age > 18 ORDER BY name DESC LIMIT 10",
        );
        match result {
            QueryResult::Select(rs) => {
                assert_eq!(rs.columns, vec!["id".to_string(), "name".to_string()]);
                assert_eq!(
                    rs.rows,
                    vec![
                        vec![Value::Int(3), Value::Text("Carol".to_string())],
                        vec![Value::Int(1), Value::Text("Alice".to_string())],
                    ]
                );
            }
            _ => panic!("expected rows"),
        }
    }

    #[test]
    fn select_star_returns_all_columns() {
        let tmp = tempdir().unwrap();
        let dir = tmp.path();
        setup(dir);

        let result = run(dir, "SELECT * FROM users WHERE id = 2");
        match result {
            QueryResult::Select(rs) => {
                assert_eq!(rs.columns, vec!["id", "name", "age"]);
                assert_eq!(
                    rs.rows,
                    vec![vec![
                        Value::Int(2),
                        Value::Text("Bob".to_string()),
                        Value::Int(17)
                    ]]
                );
            }
            _ => panic!(),
        }
    }

    #[test]
    fn limit_zero_is_empty() {
        let tmp = tempdir().unwrap();
        let dir = tmp.path();
        setup(dir);
        assert!(rows(run(dir, "SELECT * FROM users LIMIT 0")).is_empty());
    }

    #[test]
    fn data_survives_reopen() {
        let tmp = tempdir().unwrap();
        let dir = tmp.path();
        setup(dir);

        // a fresh run() opens the table files again, simulating a restart
        assert_eq!(rows(run(dir, "SELECT id FROM users ORDER BY id")).len(), 3);
    }

    #[test]
    fn scalar_function_in_projection() {
        let tmp = tempdir().unwrap();
        let dir = tmp.path();
        setup(dir);

        let result = run(dir, "SELECT name, LENGTH(name) FROM users WHERE id = 1");
        match result {
            QueryResult::Select(rs) => {
                assert_eq!(rs.columns, vec!["name", "LENGTH(name)"]);
                assert_eq!(
                    rs.rows,
                    vec![vec![Value::Text("Alice".to_string()), Value::Int(5)]]
                );
            }
            _ => panic!(),
        }
    }

    #[test]
    fn scalar_function_in_where() {
        let tmp = tempdir().unwrap();
        let dir = tmp.path();
        setup(dir);

        // Alice=5, Bob=3, Carol=5; only > 3 survive
        let result = run(
            dir,
            "SELECT name FROM users WHERE LENGTH(name) > 3 ORDER BY name",
        );
        assert_eq!(
            rows(result),
            vec![
                vec![Value::Text("Alice".to_string())],
                vec![Value::Text("Carol".to_string())],
            ]
        );
    }

    #[test]
    fn insert_with_column_list_reorders() {
        let tmp = tempdir().unwrap();
        let dir = tmp.path();
        run(dir, "CREATE TABLE users (id INT, name TEXT, age INT)");
        run(
            dir,
            "INSERT INTO users (age, id, name) VALUES (40, 9, 'Zoe')",
        );

        let result = run(dir, "SELECT * FROM users");
        assert_eq!(
            rows(result),
            vec![vec![
                Value::Int(9),
                Value::Text("Zoe".to_string()),
                Value::Int(40)
            ]]
        );
    }
}
