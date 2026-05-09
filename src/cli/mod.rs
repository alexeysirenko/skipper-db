use std::path::PathBuf;

use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

use crate::catalog;

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
        }
        Ok(())
    }
}
