mod cli;
mod crates_io;
mod format;
mod npmrc;
mod registry;
mod target;
mod version;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Commands};

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Version(args) => version::run(args),
    }
}
