use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use setbreak::db::models::{ChainScore, TrackScore};
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
        /// Directories to scan (defaults to config file music_dirs)
        paths: Vec<String>,

        /// Force re-scan even if files haven't changed
        #[arg(long)]
        force: bool,
    },

    /// Analyze audio files (extract features and compute scores)
    Analyze {
        /// Number of parallel workers (0 = auto-detect from config)
        #[arg(short = 'j', long, default_value = "0")]
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

    /// Adjust scores to remove recording quality bias (LUFS regression)
    Calibrate {
        /// Show what would change without writing to DB
        #[arg(long)]
        dry_run: bool,
    },

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

    /// Compute track-to-track similarity from audio features
    Similarity {
        /// Number of parallel workers (0 = auto-detect from config)
        #[arg(short = 'j', long, default_value = "0")]
        jobs: usize,
    },

    /// Find tracks that sound similar to a given track
    Similar {
        /// Song title to search for (substring match)
        song: String,

        /// Show date to narrow the search (YYYY-MM-DD)
        #[arg(short, long)]
        date: Option<String>,

        /// Number of results
        #[arg(short = 'n', long, default_value = "15")]
        limit: usize,
    },

    /// Find and rank segue chains (multi-song jam suites connected by ->)
    Chains {
        /// Sort by this score
        #[arg(short, long, value_enum, default_value = "transcendence")]
        sort: ScoreName,

        /// Filter to a specific show date (YYYY-MM-DD)
        #[arg(short, long)]
        date: Option<String>,

        /// Minimum chain length (number of songs)
        #[arg(long, default_value = "2")]
        min_length: usize,

        /// Minimum total chain duration in minutes
        #[arg(long)]
        min_duration: Option<f64>,

        /// Filter chains containing this song (substring match)
        #[arg(long)]
        song: Option<String>,

        /// Number of results
        #[arg(short = 'n', long, default_value = "20")]
        limit: usize,
    },

    /// Discover missing shows from archive.org collections
    Discover {
        /// Band code (gd, phish, bts)
        #[arg(long, default_value = "gd")]
        band: String,

        /// Force refresh of cached archive.org data
        #[arg(long)]
        refresh: bool,

        /// Filter by year or year range (e.g., "1977" or "1977-1980")
        #[arg(long)]
        year: Option<String>,

        /// Number of results
        #[arg(short = 'n', long, default_value = "50")]
        limit: usize,
    },

    /// Classify tracks as live, studio, or live_album (backfill existing tracks)
    Classify,

    /// Flag tracks with bad audio quality (DTS bitstreams, corrupt files)
    QualityCheck,

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

    // Load config file (optional, defaults if missing)
    let config = setbreak::config::AppConfig::load();

    // Initialize global band registry (must happen before any band lookups)
    setbreak::bands::init(&config.custom_bands);

    // Resolve database path: CLI > config > XDG default
    let db_path = cli.db_path
        .or(config.db_path.clone())
        .unwrap_or_else(setbreak::config::default_db_path);
    log::info!("Database: {}", db_path.display());

    let db = setbreak::db::Database::open(&db_path)
        .context("Failed to open database")?;

    match cli.command {
        Commands::Scan { paths, force } => {
            // Resolve scan paths: CLI args > config music_dirs
            let scan_paths = if !paths.is_empty() {
                paths
            } else if !config.music_dirs.is_empty() {
                config.music_dirs.iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect()
            } else {
                anyhow::bail!(
                    "No directories to scan. Pass paths as arguments or set music_dirs in config."
                );
            };

            let result = setbreak::scanner::scan(&db, &scan_paths, force)
                .context("Scan failed")?;
            println!(
                "Scan complete: {} scanned, {} new, {} updated, {} skipped, {} errors",
                result.scanned, result.new, result.updated, result.skipped, result.errors
            );
        }

        Commands::Analyze { jobs, force, filter } => {
            let workers = if jobs > 0 { jobs } else { config.resolve_workers() };
            let result = setbreak::analyzer::analyze_tracks(
                &db,
                force,
                workers,
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
            let result = setbreak::setlist::lookup_setlists(
                &db, dry_run, config.archive.rate_limit_ms,
            ).context("Setlist lookup failed")?;
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

        Commands::Calibrate { dry_run } => {
            if dry_run {
                println!("DRY RUN — no changes will be written to the database");
                println!();
            }
            let result = setbreak::calibrate::calibrate_scores(&db, dry_run)
                .context("Calibration failed")?;
            println!(
                "Calibration complete: {} calibrated, {} skipped (no show date)",
                result.calibrated, result.skipped_no_show
            );
            if dry_run && result.total_tracks > 0 {
                println!("(dry run — re-run without --dry-run to apply)");
            }
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

        Commands::Similarity { jobs } => {
            let workers = if jobs > 0 { jobs } else { config.resolve_workers() };
            let result = setbreak::similarity::compute_similarity(&db, workers)
                .context("Similarity computation failed")?;
            println!(
                "Similarity complete: {} tracks processed, {} pairs stored",
                result.tracks_processed, result.pairs_stored
            );
        }

        Commands::Similar { song, date, limit } => {
            let found = db.find_track_id(&song, date.as_deref())
                .context("Search failed")?;

            let (track_id, title, track_date) = match found {
                Some(t) => t,
                None => {
                    println!("No analyzed track matching \"{}\".", song);
                    return Ok(());
                }
            };

            let results = db.query_similar(track_id, limit)
                .context("Query failed")?;

            if results.is_empty() {
                println!("No similarity data. Run `setbreak similarity` first.");
                return Ok(());
            }

            println!("Tracks similar to \"{}\" ({}):", title, track_date);
            println!();

            // Print with distance column
            println!(
                "{:<25} {:>10} {:>5} {:>6}  {:>4} {:>4} {:>4} {:>4} {:>4} {:>4}",
                "Song", "Date", "Min", "Dist",
                "Grv", "Imp", "Eng", "Int", "Bld", "Exp"
            );
            println!("{}", "-".repeat(95));

            for (t, dist) in &results {
                let title_display: String = if t.title.len() > 25 {
                    format!("{}...", &t.title[..22])
                } else {
                    t.title.clone()
                };

                println!(
                    "{:<25} {:>10} {:>5.1} {:>6.3}  {:>4.0} {:>4.0} {:>4.0} {:>4.0} {:>4.0} {:>4.0}",
                    title_display,
                    t.date,
                    t.duration_min,
                    dist,
                    t.groove,
                    t.improvisation,
                    t.energy,
                    t.intensity,
                    t.build_quality,
                    t.exploratory,
                );
            }

            println!();
            println!("Dist = cosine distance (0 = identical, lower = more similar)");
        }

        Commands::Chains { sort, date, min_length, min_duration, song, limit } => {
            let dates = if let Some(ref d) = date {
                if db.date_has_analysis(d).context("Query failed")? {
                    vec![d.clone()]
                } else {
                    println!("No analyzed tracks for date {}.", d);
                    return Ok(());
                }
            } else {
                db.get_dates_with_chains().context("Query failed")?
            };

            if dates.is_empty() {
                println!("No segue chains found in analyzed tracks.");
                return Ok(());
            }

            // Collect chains from all dates
            let mut all_chains = Vec::new();
            for d in &dates {
                let tracks = db.query_show(d).context("Query failed")?;
                let chains = setbreak::chains::detect_chains(&tracks, min_length);
                all_chains.extend(chains);
            }

            let chains = setbreak::chains::filter_and_sort_chains(
                all_chains,
                min_duration,
                song.as_deref(),
                sort.column(),
                limit,
            );

            if chains.is_empty() {
                println!("No chains match the given criteria.");
                return Ok(());
            }

            println!("Top {} segue chains (sorted by {}):", chains.len(), sort.label());
            println!();
            print_chain_table(&chains, &sort);
        }

        Commands::Discover { band, refresh, year, limit } => {
            let result = setbreak::discovery::discover_missing_shows(
                &db, &band, refresh, year.as_deref(), limit,
                config.archive.cache_ttl_days,
                config.archive.rate_limit_ms,
            ).context("Discovery failed")?;

            println!(
                "Collection: {} ({} total shows in archive)",
                result.collection, result.archive_count
            );
            println!(
                "Local shows: {} dates | Missing: {} dates",
                result.local_count, result.missing.len()
            );
            println!();

            if result.missing.is_empty() {
                println!("You have every show! (or no missing shows match the filter)");
            } else {
                print_missing_shows(&result.missing);
                println!();
                println!("Download with: ia download <identifier>");
                println!("  (install: pip install internetarchive)");
            }
        }

        Commands::Classify => {
            let tracks = db.get_tracks_for_classify()
                .context("Failed to load tracks for classification")?;
            let total = tracks.len();

            let tx = db.conn.unchecked_transaction()?;
            let mut counts = std::collections::HashMap::new();

            for (id, file_path, parsed_date, album) in &tracks {
                let rtype = setbreak::scanner::classify::classify_recording_type(
                    file_path,
                    parsed_date.as_deref(),
                    album.as_deref(),
                );
                *counts.entry(rtype).or_insert(0usize) += 1;
                tx.execute(
                    "UPDATE tracks SET recording_type = ?1 WHERE id = ?2",
                    rusqlite::params![rtype, id],
                )?;
            }
            tx.commit()?;

            let live = counts.get("live").copied().unwrap_or(0);
            let studio = counts.get("studio").copied().unwrap_or(0);
            let live_album = counts.get("live_album").copied().unwrap_or(0);
            let unknown = counts.get("unknown").copied().unwrap_or(0);

            println!(
                "Classify complete: {} tracks — {} live, {} studio, {} live_album, {} unknown",
                total, live, studio, live_album, unknown
            );
        }

        Commands::QualityCheck => {
            let tracks = db.get_tracks_for_quality_check()
                .context("Failed to load tracks for quality check")?;
            let total = tracks.len();

            let tx = db.conn.unchecked_transaction()?;
            let mut ok = 0usize;
            let mut suspect = 0usize;
            let mut garbage = 0usize;

            for (id, file_path, snr_db, clipping_ratio) in &tracks {
                let quality = setbreak::analyzer::classify_data_quality(*snr_db, *clipping_ratio, file_path);
                match quality {
                    "ok" => ok += 1,
                    "suspect" => suspect += 1,
                    "garbage" => garbage += 1,
                    _ => ok += 1,
                }
                tx.execute(
                    "UPDATE tracks SET data_quality = ?1 WHERE id = ?2",
                    rusqlite::params![quality, id],
                )?;
            }
            tx.commit()?;

            println!(
                "Quality check complete: {} tracks — {} ok, {} suspect, {} garbage",
                total, ok, suspect, garbage
            );
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

/// Print a table of segue chains.
fn print_chain_table(chains: &[ChainScore], sort: &ScoreName) {
    println!(
        "{:<40} {:>10} {:>3} {:>5}  {:>4} {:>4} {:>4} {:>4}",
        "Chain", "Date", "Len", "Min",
        "Trn", "Imp", "Eng", "Exp"
    );
    println!("{}", "-".repeat(85));

    for c in chains {
        let chain_title = c.chain_title();
        let title_display: String = if chain_title.len() > 40 {
            format!("{}...", &chain_title[..37])
        } else {
            chain_title
        };

        println!(
            "{:<40} {:>10} {:>3} {:>5.1}  {:>4.0} {:>4.0} {:>4.0} {:>4.0}",
            title_display,
            c.date,
            c.chain_length,
            c.duration_min,
            c.transcendence,
            c.improvisation,
            c.energy,
            c.exploratory,
        );
    }

    println!();
    println!("Trn=Transcendence  Imp=Improvisation  Eng=Energy  Exp=Exploratory");
    println!("Sorted by: {}", sort.label());
}

/// Print a table of missing shows from archive.org.
fn print_missing_shows(shows: &[setbreak::db::models::MissingShow]) {
    println!(
        "{:<12} {:>6} {:>6} {:>5}  {}",
        "Date", "Source", "Format", "Tapes", "Identifier"
    );
    println!("{}", "-".repeat(80));

    for s in shows {
        let source = match s.source_quality {
            3 => "SBD",
            2 => "Matrix",
            1 => "AUD",
            _ => "?",
        };
        let format = match s.format_quality {
            3 => "FLAC",
            2 => "SHN",
            1 => "MP3",
            _ => "?",
        };

        println!(
            "{:<12} {:>6} {:>6} {:>5}  {}",
            s.date, source, format, s.tape_count, s.best_identifier
        );
    }
}
