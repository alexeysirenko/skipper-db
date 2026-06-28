use std::path::PathBuf;

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
        db: PathBuf,
    },
    Open {
        #[arg(long)]
        db: PathBuf,
    },
    Parse {
        #[arg(long)]
        query: String,
    },
    Plan {
        #[arg(long)]
        db: PathBuf,
        #[arg(long)]
        query: String,
    },
    Query {
        #[arg(long)]
        db: PathBuf,
        #[arg(long)]
        sql: String,
    },
    Explain {
        #[arg(long)]
        db: PathBuf,
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
                catalog::init(&db)?;
                println!("initialized {}", db.display());
            }
            Commands::Open { db } => {
                let header = catalog::open(&db)?;
                println!("opened {} (version {})", db.display(), header.version)
            }
            Commands::Parse { query } => {
                let statement = parser::parse(&query)?;
                println!("{statement:#?}");
            }
            Commands::Plan { db, query } => {
                let statement = parser::parse(&query)?;
                let catalog = query::DirCatalog::new(db);
                let plan = query::plan(statement, &catalog)?;
                print!("{plan}");
            }
            Commands::Query { db, sql } => {
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
