//! `sezar-agility scan --target <path> --rules <dir>` — CLI entry point.
//!
//! Runs the static agility scanner over a source repository or
//! installed-package layout and emits the resulting `AgilityBlock` as
//! JSON (one record per target). Designed to compose with shell
//! pipelines: pipe the output into the Sezar collector or into the
//! reproducibility corpus.

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use sezar_agility::rules::load_ruleset;
use sezar_agility::scanner::{scan_target, ScanTarget};
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(name = "sezar-agility", author, version, about)]
struct Args {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Scan a target directory and emit the agility block as JSON.
    Scan {
        /// Path to the target (source repo or installed package root).
        #[arg(long)]
        target: PathBuf,
        /// Path to the ruleset directory (e.g. `rules/v1`).
        #[arg(long)]
        rules: PathBuf,
        /// Maximum file size to scan (bytes).
        #[arg(long, default_value_t = 5 * 1024 * 1024)]
        max_file_bytes: u64,
        /// Maximum lines per file to scan.
        #[arg(long, default_value_t = 50_000)]
        max_lines: usize,
    },
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    let args = Args::parse();
    match args.cmd {
        Cmd::Scan {
            target,
            rules,
            max_file_bytes,
            max_lines,
        } => {
            let compiled = load_ruleset(&rules)?;
            let st = ScanTarget {
                root: target,
                max_file_bytes,
                max_lines,
            };
            let block = scan_target(&st, &compiled);
            println!("{}", serde_json::to_string_pretty(&block)?);
        }
    }
    Ok(())
}
