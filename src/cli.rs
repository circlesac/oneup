use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "oneup", about = "CalVer-based version management")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Calculate next version and update target files
    Version(VersionArgs),
}

#[derive(Parser)]
pub struct VersionArgs {
    /// Target file(s) — repeatable (auto-detected if omitted)
    #[arg(long)]
    pub target: Vec<PathBuf>,

    /// Registry URL override (auto-detected from .npmrc or crates.io)
    #[arg(long)]
    pub registry: Option<String>,

    /// Version format (CalVer tokens: YYYY, YY, MM, DD, MICRO)
    #[arg(long, default_value = "YY.MM.MICRO")]
    pub format: String,

    /// Show what would happen without making changes
    #[arg(long)]
    pub dry_run: bool,

    /// Print detailed debug output
    #[arg(long)]
    pub verbose: bool,
}
