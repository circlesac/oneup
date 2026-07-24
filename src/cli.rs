use clap::{Parser, Subcommand, ValueEnum};
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

    /// Version source. `auto` (default) picks crates.io for Cargo.toml,
    /// git tags for gradle/Go, npm otherwise. `git` forces git tags for any target.
    #[arg(long, value_enum, default_value_t = Source::Auto)]
    pub source: Source,

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

/// Where oneup looks for already-published versions.
#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum Source {
    /// Pick based on the primary target file (crates.io / git tags / npm).
    Auto,
    /// Git tags in the target's repository.
    Git,
    /// npm registry.
    Npm,
    /// crates.io.
    Crates,
}
