use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use setbreak::db::models::TrackScore;
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

#[derive(Clone, ValueEnum)]
enum ScoreName {
    Energy,
    Intensity,
    Groove,
    Improvisation,
    Tightness,
    #[value(alias = "build")]
    BuildQuality,
    Exploratory,
    Transcendence,
    Valence,
    Arousal,
}

impl ScoreName {
    fn column(&self) -> &'static str {
        match self {
            Self::Energy => "energy_score",
            Self::Intensity => "intensity_score",
            Self::Groove => "groove_score",
            Self::Improvisation => "improvisation_score",
            Self::Tightness => "tightness_score",
            Self::BuildQuality => "build_quality_score",
            Self::Exploratory => "exploratory_score",
            Self::Transcendence => "transcendence_score",
            Self::Valence => "valence_score",
            Self::Arousal => "arousal_score",
        }
    }

    fn label(&self) -> &'static str {
        match self {
            Self::Energy => "energy",
            Self::Intensity => "intensity",
            Self::Groove => "groove",
            Self::Improvisation => "improvisation",
            Self::Tightness => "tightness",
            Self::BuildQuality => "build quality",
            Self::Exploratory => "exploratory",
            Self::Transcendence => "transcendence",
            Self::Valence => "valence",
            Self::Arousal => "arousal",
        }
    }
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

    /// Recompute jam scores from stored features (no audio re-analysis)
    Rescore,

    /// Show top tracks ranked by a jam score
    Top {
        /// Which score to rank by
        #[arg(value_enum, default_value = "groove")]
        score: ScoreName,

        /// Number of results
        #[arg(short = 'n', long, default_value = "20")]
        limit: usize,

        /// Filter by song title (substring match)
        #[arg(short, long)]
        song: Option<String>,

        /// Minimum duration in minutes
        #[arg(long)]
        min_duration: Option<f64>,
    },

    /// Compare versions of a song across shows
    Compare {
        /// Song title to search for (substring match)
        song: String,

        /// Sort by this score (or "duration")
        #[arg(short, long, default_value = "improvisation")]
        sort: ScoreName,

        /// Number of results
        #[arg(short = 'n', long, default_value = "20")]
        limit: usize,
    },

    /// View a show's setlist with scores
    Show {
        /// Show date (YYYY-MM-DD)
        date: String,
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

        Commands::Rescore => {
            let result = setbreak::analyzer::rescore_tracks(&db)
                .context("Rescore failed")?;
            println!("Rescore complete: {} tracks updated", result.rescored);
        }

        Commands::Top { score, limit, song, min_duration } => {
            let min_dur_secs = min_duration.map(|m| m * 60.0);
            let results = db.query_top(
                score.column(), limit, song.as_deref(), min_dur_secs,
            ).context("Query failed")?;

            if results.is_empty() {
                println!("No results found.");
                return Ok(());
            }

            println!("Top {} tracks by {}:", results.len(), score.label());
            println!();
            print_score_table(&results, Some(&score));
        }

        Commands::Compare { song, sort, limit } => {
            let results = db.query_compare(&song, sort.column(), limit)
                .context("Query failed")?;

            if results.is_empty() {
                println!("No analyzed tracks matching \"{}\".", song);
                return Ok(());
            }

            println!(
                "{} versions of \"{}\" (sorted by {}):",
                results.len(),
                results[0].title,
                sort.label()
            );
            println!();
            print_score_table(&results, Some(&sort));
        }

        Commands::Show { date } => {
            let results = db.query_show(&date).context("Query failed")?;

            if results.is_empty() {
                println!("No analyzed tracks for date {}.", date);
                return Ok(());
            }

            println!("Show: {}", date);
            println!();
            print_score_table(&results, None);
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

/// Print a table of track scores with the sort column highlighted.
fn print_score_table(tracks: &[TrackScore], highlight: Option<&ScoreName>) {
    // Header
    println!(
        "{:<25} {:>10} {:>5}  {:>4} {:>4} {:>4} {:>4} {:>4} {:>4} {:>4} {:>4}",
        "Song", "Date", "Min",
        "Grv", "Imp", "Eng", "Int", "Tgt", "Bld", "Exp", "Trn"
    );
    println!("{}", "-".repeat(97));

    for t in tracks {
        // Truncate long titles
        let title: String = if t.title.len() > 25 {
            format!("{}...", &t.title[..22])
        } else {
            t.title.clone()
        };

        println!(
            "{:<25} {:>10} {:>5.1}  {:>4.0} {:>4.0} {:>4.0} {:>4.0} {:>4.0} {:>4.0} {:>4.0} {:>4.0}",
            title,
            t.date,
            t.duration_min,
            t.groove,
            t.improvisation,
            t.energy,
            t.intensity,
            t.tightness,
            t.build_quality,
            t.exploratory,
            t.transcendence,
        );
    }

    // Legend
    println!();
    println!("Grv=Groove  Imp=Improvisation  Eng=Energy  Int=Intensity");
    println!("Tgt=Tightness  Bld=Build Quality  Exp=Exploratory  Trn=Transcendence");

    if let Some(hl) = highlight {
        println!("Sorted by: {}", hl.label());
    }
}
