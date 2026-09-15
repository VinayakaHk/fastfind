use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use fastfind::telemetry::now_ms;
use fastfind::{scan, EventSink, Index, OutputMode, TelemetryEvent};
use std::env;
use std::path::PathBuf;
use std::time::Instant;

#[derive(Parser, Debug)]
#[command(name = "fastfind", version, about = "Fast filename search for Linux")]
struct Cli {
    /// Index database path (defaults to the XDG data directory)
    #[arg(long, global = true)]
    database: Option<PathBuf>,

    /// Print command results as JSON and telemetry as JSON Lines on stderr
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Create or open the index database
    Init,
    /// Scan and reconcile an ext4 directory tree
    Index { path: PathBuf },
    /// Search indexed filenames by case-insensitive substring
    Search {
        query: String,
        #[arg(short, long, default_value_t = 100)]
        limit: usize,
        /// Match ordinary terms against full paths instead of names only
        #[arg(long)]
        match_path: bool,
    },
    /// Show indexed roots, freshness, counts, and health
    Status,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let database = cli.database.unwrap_or_else(default_database_path);
    let events = EventSink::new(if cli.json {
        OutputMode::Json
    } else {
        OutputMode::Human
    });
    let mut index = Index::open(&database)?;

    match cli.command {
        Command::Init => {
            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({"database": database, "initialized": true})
                );
            } else {
                println!("initialized {}", database.display());
            }
        }
        Command::Index { path } => {
            let outcome = scan(&mut index, &path, &[database], &events)?;
            if cli.json {
                println!("{}", serde_json::to_string(&outcome)?);
            }
        }
        Command::Search {
            query,
            limit,
            match_path,
        } => {
            let started = Instant::now();
            let results = index.search_with_options(
                &query,
                limit,
                fastfind::query::SearchOptions { match_path },
            )?;
            events.emit(&TelemetryEvent::QueryCompleted {
                schema_version: 1,
                timestamp_ms: now_ms(),
                query_length: query.chars().count(),
                result_count: results.len(),
                elapsed_ms: started.elapsed().as_millis() as u64,
            });
            if cli.json {
                println!("{}", serde_json::to_string(&results)?);
            } else {
                for result in results {
                    println!("{:?}\t{}", result.kind, result.path);
                }
            }
        }
        Command::Status => {
            let statuses = index.statuses()?;
            if cli.json {
                println!("{}", serde_json::to_string(&statuses)?);
            } else if statuses.is_empty() {
                println!("no indexed roots");
            } else {
                for status in statuses {
                    println!(
                        "{}\t{} entries\t{}\t{} errors\t{}",
                        status.state,
                        status.entry_count,
                        status.path,
                        status.error_count,
                        status
                            .last_scan_ms
                            .map(|value| value.to_string())
                            .unwrap_or_else(|| "never".to_string())
                    );
                }
            }
        }
    }
    Ok(())
}

fn default_database_path() -> PathBuf {
    if let Some(path) = env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(path).join("fastfind/index.db");
    }
    if let Some(home) = env::var_os("HOME") {
        return PathBuf::from(home).join(".local/share/fastfind/index.db");
    }
    env::current_dir()
        .context("cannot determine current directory")
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("fastfind.db")
}
