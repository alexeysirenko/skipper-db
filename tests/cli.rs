use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::tempdir;

fn db_cli() -> Command {
    Command::cargo_bin("db-cli").unwrap()
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
