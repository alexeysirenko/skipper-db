use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::catalog;

#[derive(Parser)]
#[command(name = "db-cli", version, about)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
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
