use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "setbreak", version, about = "Jam-band music library analyzer")]
struct Cli {
    /// Path to the SQLite database
    #[arg(long, global = true)]
    db_path: Option<PathBuf>,

    /// Verbosity (-v, -vv, -vvv)
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Scan directories for audio files and add them to the library
    Scan {
        /// Directories to scan
        #[arg(required = true)]
        paths: Vec<String>,

        /// Force re-scan even if files haven't changed
        #[arg(long)]
        force: bool,
    },

    /// Analyze audio files (extract features and compute scores)
    Analyze {
        /// Number of parallel workers
        #[arg(short = 'j', long, default_value = "2")]
        jobs: usize,

        /// Force re-analysis of already-analyzed tracks
        #[arg(long)]
        force: bool,

        /// Only analyze tracks matching this pattern
        #[arg(long)]
        filter: Option<String>,
    },

    /// Look up song titles from archive.org metadata
    Setlist {
        /// Dry run — show what would be updated without writing to DB
        #[arg(long)]
        dry_run: bool,
    },

    /// Show library statistics
    Stats,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Set up logging based on verbosity
    let log_level = match cli.verbose {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(log_level))
        .format_timestamp(None)
        .init();

    let db_path = cli.db_path.unwrap_or_else(setbreak::config::default_db_path);
    log::info!("Database: {}", db_path.display());

    let db = setbreak::db::Database::open(&db_path)
        .context("Failed to open database")?;

    match cli.command {
        Commands::Scan { paths, force } => {
            let result = setbreak::scanner::scan(&db, &paths, force)
                .context("Scan failed")?;
            println!(
                "Scan complete: {} scanned, {} new, {} updated, {} skipped, {} errors",
                result.scanned, result.new, result.updated, result.skipped, result.errors
            );
        }

        Commands::Analyze { jobs, force, filter } => {
            let result = setbreak::analyzer::analyze_tracks(
                &db,
                force,
                jobs,
                filter.as_deref(),
            )
            .context("Analysis failed")?;
            println!(
                "Analysis complete: {} analyzed, {} failed",
                result.analyzed, result.failed
            );
        }

        Commands::Setlist { dry_run } => {
            if dry_run {
                println!("DRY RUN — no changes will be written to the database");
            }
            let result = setbreak::setlist::lookup_setlists(&db, dry_run)
                .context("Setlist lookup failed")?;
            println!();
            println!(
                "Setlist lookup complete: {} dirs fetched, {} titles updated, {} errors",
                result.directories_fetched, result.titles_updated, result.fetch_errors
            );
            if dry_run && result.titles_updated > 0 {
                println!("(dry run — re-run without --dry-run to write changes)");
            }
        }

        Commands::Stats => {
            let stats = db.stats().context("Failed to get stats")?;
            println!("Library Statistics");
            println!("==================");
            println!("Total tracks:     {}", stats.total_tracks);
            println!("Analyzed tracks:  {}", stats.analyzed_tracks);
            println!(
                "Total duration:   {:.1} hours",
                stats.total_duration_hours
            );
            println!();

            if !stats.formats.is_empty() {
                println!("Formats:");
                for (fmt, count) in &stats.formats {
                    println!("  {:<8} {}", fmt, count);
                }
                println!();
            }

            if !stats.bands.is_empty() {
                println!("Bands:");
                for (band, count) in &stats.bands {
                    println!("  {:<30} {}", band, count);
                }
            }
        }
    }

    Ok(())
}
