use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "oneup", about = "CalVer-based version management for npm packages")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Calculate next version, update target file, and optionally create git commit + tag
    Version(VersionArgs),
}

#[derive(Parser)]
pub struct VersionArgs {
    /// Target JSON file
    #[arg(long, default_value = "./package.json")]
    pub target: PathBuf,

    /// npm registry URL override (auto-detected from .npmrc if not set)
    #[arg(long)]
    pub registry: Option<String>,

    /// Skip git commit and tag creation
    #[arg(long)]
    pub no_git_tag_version: bool,

    /// Proceed even if the working tree has uncommitted changes
    #[arg(long)]
    pub force: bool,

    /// Custom git commit/tag message (%s = version placeholder)
    #[arg(short, long, default_value = "%s")]
    pub message: String,

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
