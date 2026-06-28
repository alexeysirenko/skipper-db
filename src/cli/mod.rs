use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

use crate::catalog;
use crate::parser;
use crate::query;
use crate::query::exec::{QueryResult, ResultSet};
use crate::types::Value;

#[derive(Parser)]
#[command(name = "db-cli", version, about)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
    #[arg(long, global = true)]
    debug: bool,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Init {
        #[arg(long)]
        db: Option<PathBuf>,
    },
    Open {
        #[arg(long)]
        db: Option<PathBuf>,
    },
    Parse {
        #[arg(long)]
        query: String,
    },
    Plan {
        #[arg(long)]
        db: Option<PathBuf>,
        #[arg(long)]
        query: String,
    },
    Query {
        #[arg(long)]
        db: Option<PathBuf>,
        #[arg(long)]
        sql: String,
    },
    Explain {
        #[arg(long)]
        db: Option<PathBuf>,
        #[arg(long)]
        sql: String,
    },
}

impl Cli {
    pub fn run() -> anyhow::Result<()> {
        let cli = Cli::parse();

        let filter = if cli.debug {
            EnvFilter::new("debug")
        } else {
            EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into())
        };

        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_target(false)
            .with_writer(std::io::stderr)
            .init();

        tracing::info!(version = env!("CARGO_PKG_VERSION"), "skipper-db starting");
        tracing::debug!(?cli.command, "dispatching command");

        match cli.command {
            Commands::Init { db } => {
                let db = resolve_db(db)?;
                catalog::init(&db)?;
                println!("initialized {}", db.display());
            }
            Commands::Open { db } => {
                let db = resolve_db(db)?;
                let header = catalog::open(&db)?;
                println!("opened {} (version {})", db.display(), header.version)
            }
            Commands::Parse { query } => {
                let statement = parser::parse(&query)?;
                println!("{statement:#?}");
            }
            Commands::Plan { db, query } => {
                let db = resolve_db(db)?;
                let statement = parser::parse(&query)?;
                let catalog = query::DirCatalog::new(db);
                let plan = query::plan(statement, &catalog)?;
                print!("{plan}");
            }
            Commands::Query { db, sql } => {
                let db = resolve_db(db)?;
                let statement = parser::parse(&sql)?;
                let catalog = query::DirCatalog::new(db.clone());
                let logical = query::plan(statement, &catalog)?;
                let physical = query::physical::from_logical(logical);
                match query::exec::execute(physical, &db)? {
                    QueryResult::Select(rs) => print_result(&rs),
                    QueryResult::Inserted => println!("1 row inserted"),
                    QueryResult::Created(name) => println!("table {name} created"),
                }
            }
            Commands::Explain { db, sql } => {
                let db = resolve_db(db)?;
                let statement = parser::parse(&sql)?;
                let catalog = query::DirCatalog::new(db);
                let logical = query::plan(statement, &catalog)?;
                let physical = query::physical::from_logical(logical.clone());
                println!("Logical Plan:");
                print!("{logical}");
                println!("Physical Plan:");
                print!("{physical}");
            }
        }
        Ok(())
    }
}

const DEFAULT_DB: &str = "db";

// Resolve the database directory. With no `--db`, default to a `db` directory
// next to the binary; a bare name (no path separators) is a subdir of that same
// base; anything with a separator or absolute is used as given.
fn resolve_db(db: Option<PathBuf>) -> anyhow::Result<PathBuf> {
    let base = std::env::current_exe()?
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    Ok(match db {
        None => base.join(DEFAULT_DB),
        Some(p) if is_bare_name(&p) => base.join(p),
        Some(p) => p,
    })
}

fn is_bare_name(p: &Path) -> bool {
    !p.is_absolute() && p.parent() == Some(Path::new(""))
}

fn print_result(rs: &ResultSet) {
    println!("{}", rs.columns.join(" | "));
    for row in &rs.rows {
        let cells: Vec<String> = row.iter().map(format_value).collect();
        println!("{}", cells.join(" | "));
    }
}

fn format_value(v: &Value) -> String {
    match v {
        Value::Int(n) => n.to_string(),
        Value::Text(s) => s.clone(),
        Value::Null => "NULL".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exe_dir() -> PathBuf {
        std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf()
    }

    #[test]
    fn classifies_bare_names() {
        assert!(is_bare_name(Path::new("demo")));
        assert!(!is_bare_name(Path::new("./demo")));
        assert!(!is_bare_name(Path::new("a/b")));
        assert!(!is_bare_name(Path::new("/abs/demo")));
    }

    #[test]
    fn default_db_sits_next_to_binary() {
        assert_eq!(resolve_db(None).unwrap(), exe_dir().join("db"));
    }

    #[test]
    fn bare_name_is_a_subdir_of_base() {
        assert_eq!(
            resolve_db(Some(PathBuf::from("shop"))).unwrap(),
            exe_dir().join("shop")
        );
    }

    #[test]
    fn explicit_path_is_used_as_is() {
        let path = PathBuf::from("/tmp/skipper-demo");
        assert_eq!(resolve_db(Some(path.clone())).unwrap(), path);
    }
}
