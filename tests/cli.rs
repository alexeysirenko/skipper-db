use assert_cmd::Command;
use predicates::prelude::*;
use skipper_db::catalog::schema::{Column, Schema};
use skipper_db::storage::table::Table;
use skipper_db::types::ColumnType;
use std::path::Path;
use tempfile::tempdir;

fn db_cli() -> Command {
    Command::cargo_bin("db-cli").unwrap()
}

fn create_users_table(dir: &Path) {
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
    Table::create(&dir.join("users.tbl"), schema).unwrap();
}

#[test]
fn help_succeeds() {
    db_cli()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("db-cli"));
}

#[test]
fn version_succeeds() {
    db_cli()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn unknown_subcommand_fails() {
    db_cli().arg("frobnicate").assert().failure();
}

#[test]
fn init_then_open_roundtrip() {
    let tmp = tempdir().unwrap();
    let db = tmp.path().join("demo-db");

    db_cli().args(["init", "--db"]).arg(&db).assert().success();

    db_cli()
        .args(["open", "--db"])
        .arg(&db)
        .assert()
        .success()
        .stdout(predicate::str::contains("version 1"));
}

#[test]
fn second_init_fails() {
    let tmp = tempdir().unwrap();
    let db = tmp.path().join("demo-db");

    db_cli().args(["init", "--db"]).arg(&db).assert().success();

    db_cli().args(["init", "--db"]).arg(&db).assert().failure();
}

#[test]
fn open_nonexistent_fails() {
    let tmp = tempdir().unwrap();
    let db = tmp.path().join("never-existed");
    db_cli().args(["open", "--db"]).arg(&db).assert().failure();
}

#[test]
fn parse_select_prints_ast() {
    db_cli()
        .args([
            "parse",
            "--query",
            "SELECT id, name FROM users WHERE age > 18",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Select"));
}

#[test]
fn parse_create_table_prints_ast() {
    db_cli()
        .args(["parse", "--query", "CREATE TABLE users (id INT, name TEXT)"])
        .assert()
        .success()
        .stdout(predicate::str::contains("CreateTable"));
}

#[test]
fn parse_invalid_query_fails() {
    db_cli()
        .args(["parse", "--query", "SELECT FROM users"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("parse error"));
}

#[test]
fn plan_select_prints_tree() {
    let tmp = tempdir().unwrap();
    create_users_table(tmp.path());

    db_cli()
        .args(["plan", "--db"])
        .arg(tmp.path())
        .args(["--query", "SELECT id FROM users WHERE age > 18 LIMIT 5"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Scan users"));
}

#[test]
fn plan_unknown_table_fails() {
    let tmp = tempdir().unwrap();

    db_cli()
        .args(["plan", "--db"])
        .arg(tmp.path())
        .args(["--query", "SELECT * FROM ghost"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown table"));
}

fn query(db: &Path, sql: &str) -> Command {
    let mut cmd = db_cli();
    cmd.args(["query", "--db"]).arg(db).args(["--sql", sql]);
    cmd
}

#[test]
fn query_create_insert_select_roundtrip() {
    let tmp = tempdir().unwrap();
    let db = tmp.path();

    query(db, "CREATE TABLE users (id INT, name TEXT, age INT)")
        .assert()
        .success();
    query(db, "INSERT INTO users VALUES (1, 'Alice', 20)")
        .assert()
        .success();
    query(db, "INSERT INTO users VALUES (2, 'Bob', 17)")
        .assert()
        .success();

    // separate processes prove the data persisted to disk
    query(db, "SELECT name FROM users WHERE age > 18")
        .assert()
        .success()
        .stdout(predicate::str::contains("Alice").and(predicate::str::contains("Bob").not()));
}

#[test]
fn explain_shows_logical_and_physical_plans() {
    let tmp = tempdir().unwrap();
    let db = tmp.path();
    query(db, "CREATE TABLE users (id INT, name TEXT, age INT)")
        .assert()
        .success();

    db_cli()
        .args(["explain", "--db"])
        .arg(db)
        .args(["--sql", "SELECT id FROM users WHERE age > 18"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Logical Plan:")
                .and(predicate::str::contains("Physical Plan:"))
                .and(predicate::str::contains("TableScanExec users")),
        );
}
