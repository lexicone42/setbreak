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

        /// Include studio and non-live recordings (default: live only)
        #[arg(long)]
        all_types: bool,
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

        /// Include studio and non-live recordings (default: live only)
        #[arg(long)]
        all_types: bool,
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

        /// Filter by band (gd, phish, bts, etc.)
        #[arg(short, long)]
        band: Option<String>,

        /// Number of results
        #[arg(short = 'n', long, default_value = "20")]
        limit: usize,

        /// Show individual track scores within each chain
        #[arg(long)]
        detail: bool,
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

    /// Download a show from archive.org (picks best non-SBD source for restricted bands)
    Download {
        /// Band code (gd, phish, bts)
        #[arg(long)]
        band: String,

        /// Show date (YYYY-MM-DD)
        date: String,

        /// Destination directory (default: first music_dir from config)
        #[arg(long)]
        dest: Option<String>,

        /// Dry run — show what would be downloaded without actually downloading
        #[arg(long)]
        dry_run: bool,
    },

    /// Import canonical setlists from gdshowsdb YAML files (Grateful Dead)
    ImportSetlists {
        /// Path to gdshowsdb data directory (contains year YAML files)
        path: String,

        /// Source identifier (default: gdshowsdb)
        #[arg(long, default_value = "gdshowsdb")]
        source: String,

        /// Dry run — parse and validate without writing to DB
        #[arg(long)]
        dry_run: bool,
    },

    /// Fetch Phish setlists from phish.in API (incremental, caches in DB)
    FetchPhishin {
        /// Dry run — fetch and count but don't write to DB
        #[arg(long)]
        dry_run: bool,
    },

    /// Classify tracks as live, studio, or live_album (backfill existing tracks)
    Classify,

    /// Flag tracks with bad audio quality (DTS bitstreams, corrupt files)
    QualityCheck,

    /// Extract boundary features from audio (lightweight decode for segue detection)
    ExtractBoundaries {
        /// Number of parallel workers (0 = auto-detect from config)
        #[arg(short = 'j', long, default_value = "0")]
        jobs: usize,
    },

    /// Detect segues between consecutive tracks from audio boundary analysis
    Segues {
        /// Minimum segue confidence (0.0-1.0)
        #[arg(long, default_value = "0.5")]
        min_confidence: f64,

        /// Filter by band (gd, phish, etc.)
        #[arg(short, long)]
        band: Option<String>,

        /// Filter to a specific show date (YYYY-MM-DD)
        #[arg(short, long)]
        date: Option<String>,

        /// Number of results
        #[arg(short = 'n', long, default_value = "50")]
        limit: usize,

        /// Show all tracks in each show (not just detected segues)
        #[arg(long)]
        detail: bool,
    },

    /// Run a raw SQL query against the database and display results
    Sql {
        /// SQL query to execute
        query: String,
    },

    /// Show percentile ranks for a specific track across all scores
    Rank {
        /// Song title (substring match)
        song: String,

        /// Show date to narrow the search (YYYY-MM-DD)
        #[arg(short, long)]
        date: Option<String>,
    },

    /// Show score distribution as a histogram
    Dist {
        /// Which score to show distribution for
        #[arg(value_enum)]
        score: ScoreName,

        /// Number of histogram bins
        #[arg(long, default_value = "20")]
        bins: usize,

        /// Highlight a specific song in the distribution
        #[arg(long)]
        song: Option<String>,

        /// Filter to only live recordings
        #[arg(long)]
        live_only: bool,

        /// Minimum duration in minutes
        #[arg(long)]
        min_duration: Option<f64>,
    },

    /// Find features most correlated with a score (or between two scores)
    Correlate {
        /// Target score to find predictors for
        #[arg(value_enum)]
        score: ScoreName,

        /// Only analyze live recordings
        #[arg(long)]
        live_only: bool,

        /// Minimum duration in minutes (useful for jam-focused analysis)
        #[arg(long)]
        min_duration: Option<f64>,

        /// Number of top features to show
        #[arg(short = 'n', long, default_value = "25")]
        limit: usize,
    },

    /// Show correlation matrix between all jam scores
    ScoreMatrix {
        /// Only live recordings
        #[arg(long)]
        live_only: bool,

        /// Minimum duration in minutes
        #[arg(long)]
        min_duration: Option<f64>,
    },

    /// Show library statistics
    Stats,

    /// Show analysis_results column inventory (names, types, categories)
    Schema {
        /// Filter columns by substring match (case-insensitive)
        #[arg(long)]
        grep: Option<String>,

        /// Show only a specific category
        #[arg(long)]
        category: Option<String>,

        /// Show only jam score columns
        #[arg(long)]
        scores: bool,
    },
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

        Commands::Top { score, limit, song, min_duration, all_types } => {
            let min_dur_secs = min_duration.map(|m| m * 60.0);
            let results = db.query_top(
                score.column(), limit, song.as_deref(), min_dur_secs, !all_types,
            ).context("Query failed")?;

            if results.is_empty() {
                println!("No results found.");
                return Ok(());
            }

            println!("Top {} tracks by {}:", results.len(), score.label());
            println!();
            print_score_table(&results, Some(&score));
        }

        Commands::Compare { song, sort, limit, all_types } => {
            let results = db.query_compare(&song, sort.column(), limit, !all_types)
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

        Commands::Chains { sort, date, min_length, min_duration, song, band, limit, detail } => {
            let dates = if let Some(ref d) = date {
                if db.date_has_analysis(d).context("Query failed")? {
                    vec![d.clone()]
                } else {
                    println!("No analyzed tracks for date {}.", d);
                    return Ok(());
                }
            } else {
                db.get_dates_with_chains_or_setlists().context("Query failed")?
            };

            if dates.is_empty() {
                println!("No segue chains found in analyzed tracks.");
                return Ok(());
            }

            // Resolve band filter: match band code against file paths
            let band_path_substr: Option<String> = band.as_ref().map(|b| {
                match b.to_lowercase().as_str() {
                    "gd" | "grateful dead" | "dead" => "grateful_dead",
                    "phish" | "ph" => "phish",
                    "bts" | "built to spill" => "built_to_spill",
                    other => other,
                }.to_string()
            });

            // Collect chains from all dates (prefer setlist segue data when available)
            let mut all_chains = Vec::new();
            for d in &dates {
                let tracks = db.query_show(d).context("Query failed")?;
                // If band filter active, skip shows that don't match
                if let Some(ref substr) = band_path_substr {
                    if !tracks.is_empty() && !tracks[0].file_path.to_lowercase().contains(substr.as_str()) {
                        continue;
                    }
                }
                // Use setlist segue data if available, else fall back to filename markers
                let chains = match db.get_setlist_for_date(d) {
                    Ok(setlist) if !setlist.is_empty() => {
                        setbreak::chains::detect_chains_with_setlist(&tracks, &setlist, min_length)
                    }
                    _ => setbreak::chains::detect_chains(&tracks, min_length),
                };
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

            if detail {
                println!();
                for c in &chains {
                    println!("=== {} ({}) — {} songs, {:.1} min ===", c.chain_title(), c.date, c.chain_length, c.duration_min);
                    print_score_table(&c.tracks, None);
                    println!();
                }
            }
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
                println!("Download with: setbreak download --band {} <DATE>", band);
                let registry = setbreak::bands::registry();
                if registry.is_sbd_stream_only(&band) {
                    println!("  (SBD/matrix sources are stream-only; download will auto-select audience tapes)");
                }
            }
        }

        Commands::Download { band, date, dest, dry_run } => {
            let registry = setbreak::bands::registry();

            // Resolve band → archive strategy
            let strategy = registry.resolve_archive_query(&band)
                .context(format!("Unknown band '{}' or no archive.org strategy configured", band))?;
            let collection = match strategy {
                setbreak::bands::ArchiveStrategy::Collection(c) => c.as_str(),
                setbreak::bands::ArchiveStrategy::Creator(c) => c.as_str(),
            };
            let sbd_restricted = registry.is_sbd_stream_only(&band);

            // Pick best source
            let result = setbreak::discovery::pick_best_source(&db, collection, &date, sbd_restricted)
                .context("Failed to query archive shows")?;

            match result {
                None => {
                    if sbd_restricted {
                        println!("No downloadable recordings found for {} on {}.", band, date);
                        println!("(SBD/matrix sources are stream-only for this band; no audience tapes available)");
                    } else {
                        println!("No recordings found for {} on {}.", band, date);
                        println!("Try running `setbreak discover --band {}` first to populate the cache.", band);
                    }
                }
                Some((identifier, source_q, format_q, skipped_sbd)) => {
                    let src = setbreak::discovery::source_label(source_q);
                    let fmt = setbreak::discovery::format_label(format_q);

                    if skipped_sbd {
                        println!("Note: Skipping SBD/matrix sources (stream-only for this band)");
                    }
                    println!("Best source: {} ({}/{})", identifier, src, fmt);

                    if dry_run {
                        let glob = setbreak::discovery::download_glob(format_q);
                        println!("\nWould run:");
                        println!("  ia download {} --glob='{}' --no-directories", identifier, glob);
                    } else {
                        // Resolve destination directory
                        let dest_dir = dest.unwrap_or_else(|| {
                            if !config.music_dirs.is_empty() {
                                config.music_dirs[0].to_string_lossy().to_string()
                            } else {
                                ".".to_string()
                            }
                        });

                        let glob = setbreak::discovery::download_glob(format_q);
                        let dest_path = format!("{}/{}", dest_dir, identifier);

                        println!("Downloading to: {}", dest_path);
                        println!("Running: ia download {} --destdir={} --glob='{}'",
                            identifier, dest_dir, glob);

                        let status = std::process::Command::new("ia")
                            .args(["download", &identifier,
                                &format!("--destdir={}", dest_dir),
                                &format!("--glob={}", glob)])
                            .status()
                            .context("Failed to run 'ia' command. Install with: pip install internetarchive")?;

                        if status.success() {
                            println!("Download complete!");
                            println!("Next: setbreak scan {} && setbreak analyze", dest_path);
                        } else {
                            println!("Download failed (exit code: {:?})", status.code());
                        }
                    }
                }
            }
        }

        Commands::ImportSetlists { path, source, dry_run } => {
            if dry_run {
                println!("DRY RUN — no changes will be written to the database");
            }

            let data_path = std::path::Path::new(&path);
            if !data_path.is_dir() {
                anyhow::bail!("Not a directory: {}", path);
            }

            println!("Parsing setlist data from {}...", path);
            let entries = setbreak::setlist::import::parse_gdshowsdb(data_path)
                .context("Failed to parse setlist data")?;

            let show_count = entries.iter()
                .map(|e| &e.date)
                .collect::<std::collections::HashSet<_>>()
                .len();

            println!("Parsed {} songs across {} shows", entries.len(), show_count);

            if dry_run {
                // Show a sample
                let mut by_date: std::collections::BTreeMap<&str, Vec<&setbreak::setlist::import::SetlistEntry>> =
                    std::collections::BTreeMap::new();
                for e in &entries {
                    by_date.entry(&e.date).or_default().push(e);
                }

                // Show last 3 dates as a sample
                let sample: Vec<_> = by_date.iter().rev().take(3).collect();
                for (date, songs) in sample.iter().rev() {
                    println!("\n{date}:");
                    let mut current_set = 0;
                    for s in *songs {
                        if s.set_num != current_set {
                            current_set = s.set_num;
                            println!("  Set {current_set}:");
                        }
                        let segue = if s.segued { " ->" } else { "" };
                        println!("    {: >2}. {}{}", s.position, s.song, segue);
                    }
                }
            } else {
                let result = setbreak::setlist::import::import_setlists(&db, &entries, &source)
                    .context("Failed to import setlists")?;
                println!(
                    "Import complete: {} shows, {} songs (source: {})",
                    result.shows_imported, result.songs_imported, source
                );
            }
        }

        Commands::FetchPhishin { dry_run } => {
            if dry_run {
                println!("DRY RUN — no changes will be written to the database");
            }

            let result = setbreak::setlist::phishin::fetch_phish_setlists(
                &db,
                config.archive.rate_limit_ms,
                dry_run,
            ).context("Failed to fetch Phish setlists from phish.in")?;

            println!(
                "Phish setlists: {} shows, {} songs imported",
                result.shows_imported, result.songs_imported
            );
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

        Commands::ExtractBoundaries { jobs } => {
            let workers = if jobs > 0 { jobs } else { config.resolve_workers() };
            let result = setbreak::analyzer::extract_boundaries(&db, workers)
                .context("Boundary extraction failed")?;
            println!(
                "Boundary extraction complete: {} extracted, {} failed",
                result.analyzed, result.failed
            );
        }

        Commands::Segues { min_confidence, band, date, limit, detail } => {
            setbreak::segues::run_segue_detection(
                &db, min_confidence, band.as_deref(), date.as_deref(), limit, detail,
            ).context("Segue detection failed")?;
        }

        Commands::Sql { query } => {
            let mut stmt = db.conn.prepare(&query)
                .context("SQL prepare failed")?;
            let col_count = stmt.column_count();
            let col_names: Vec<String> = (0..col_count)
                .map(|i| stmt.column_name(i).unwrap_or("?").to_string())
                .collect();

            // Compute column widths from data
            let mut rows_data: Vec<Vec<String>> = Vec::new();
            let mut widths: Vec<usize> = col_names.iter().map(|n| n.len()).collect();

            let mut rows = stmt.query(rusqlite::params![])
                .context("SQL query failed")?;
            while let Some(row) = rows.next()? {
                let mut row_strs = Vec::with_capacity(col_count);
                for i in 0..col_count {
                    let val: String = match row.get_ref(i)? {
                        rusqlite::types::ValueRef::Null => "NULL".to_string(),
                        rusqlite::types::ValueRef::Integer(n) => n.to_string(),
                        rusqlite::types::ValueRef::Real(f) => {
                            if f.fract() == 0.0 && f.abs() < 1e15 {
                                format!("{:.1}", f)
                            } else {
                                format!("{:.4}", f)
                            }
                        }
                        rusqlite::types::ValueRef::Text(s) => {
                            String::from_utf8_lossy(s).to_string()
                        }
                        rusqlite::types::ValueRef::Blob(b) => format!("<blob {}B>", b.len()),
                    };
                    if val.len() > widths[i] {
                        widths[i] = val.len();
                    }
                    row_strs.push(val);
                }
                rows_data.push(row_strs);
            }

            // Cap column widths at 40 for readability
            for w in widths.iter_mut() {
                if *w > 40 { *w = 40; }
            }

            // Print header
            for (i, name) in col_names.iter().enumerate() {
                if i > 0 { print!("  "); }
                print!("{:>width$}", name, width = widths[i]);
            }
            println!();
            let total_width: usize = widths.iter().sum::<usize>() + (col_count - 1) * 2;
            println!("{}", "-".repeat(total_width));

            // Print rows
            for row_strs in &rows_data {
                for (i, val) in row_strs.iter().enumerate() {
                    if i > 0 { print!("  "); }
                    let display = if val.len() > widths[i] {
                        format!("{}...", &val[..widths[i] - 3])
                    } else {
                        val.clone()
                    };
                    print!("{:>width$}", display, width = widths[i]);
                }
                println!();
            }
            println!();
            println!("{} rows", rows_data.len());
        }

        Commands::Rank { song, date } => {
            use setbreak::db::columns::SCORE_COLUMNS;

            let found = db.find_track_id(&song, date.as_deref())
                .context("Search failed")?;
            let (track_id, title, track_date) = match found {
                Some(t) => t,
                None => {
                    println!("No analyzed track matching \"{}\".", song);
                    return Ok(());
                }
            };

            println!("Percentile ranks for \"{}\" ({})", title, track_date);
            println!();

            // Get this track's scores and compute percentiles
            let score_names = [
                ("energy",        "Energy"),
                ("intensity",     "Intensity"),
                ("groove",        "Groove"),
                ("improvisation", "Improvisation"),
                ("tightness",     "Tightness"),
                ("build_quality", "Build Quality"),
                ("exploratory",   "Exploratory"),
                ("transcendence", "Transcendence"),
                ("valence",       "Valence"),
                ("arousal",       "Arousal"),
            ];

            let sql = format!(
                "SELECT {} FROM analysis_results WHERE track_id = ?1",
                SCORE_COLUMNS.join(", ")
            );
            let scores: Vec<f64> = self::rusqlite_row_to_f64_vec(&db, &sql, track_id)?;

            println!("{:<16} {:>6} {:>8} {:>8}", "Score", "Value", "Pctl", "Rank");
            println!("{}", "-".repeat(42));

            for (i, (_, label)) in score_names.iter().enumerate() {
                let col = SCORE_COLUMNS[i];
                let val = scores[i];

                // Count tracks below this score, and total with non-null scores
                let pctl_sql = format!(
                    "SELECT
                        COUNT(CASE WHEN a.{col} < ?1 THEN 1 END),
                        COUNT(a.{col})
                     FROM analysis_results a
                     JOIN tracks t ON t.id = a.track_id
                     WHERE {NOT_GARBAGE}",
                    col = col,
                    NOT_GARBAGE = setbreak::db::columns::NOT_GARBAGE,
                );
                let (below, total): (i64, i64) = db.conn.query_row(
                    &pctl_sql,
                    rusqlite::params![val],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )?;

                let pctl = if total > 0 {
                    100.0 * below as f64 / total as f64
                } else {
                    0.0
                };
                let rank_from_top = total - below;

                println!(
                    "{:<16} {:>6.0} {:>7.1}% {:>5}/{:<5}",
                    label, val, pctl, rank_from_top, total
                );
            }

            // Also show duration
            let dur: f64 = db.conn.query_row(
                "SELECT COALESCE(duration, 0) / 60.0 FROM analysis_results WHERE track_id = ?1",
                rusqlite::params![track_id],
                |row| row.get(0),
            )?;
            println!();
            println!("Duration: {:.1} min", dur);
        }

        Commands::Dist { score, bins, song, live_only, min_duration } => {
            let col = score.column();
            let min_dur_secs = min_duration.map(|m| m * 60.0);

            // Build WHERE clause
            let mut where_parts = vec![
                format!("a.{col} IS NOT NULL"),
                format!("{}", setbreak::db::columns::NOT_GARBAGE),
            ];
            if live_only {
                where_parts.push(format!("{}", setbreak::db::columns::LIVE_ONLY));
            }
            if let Some(dur) = min_dur_secs {
                where_parts.push(format!("a.duration >= {dur}"));
            }
            let where_clause = where_parts.join(" AND ");

            // Get min, max, and all values
            let stats_sql = format!(
                "SELECT MIN(a.{col}), MAX(a.{col}), COUNT(*), AVG(a.{col}),
                        ROUND(AVG(a.{col} * a.{col}) - AVG(a.{col}) * AVG(a.{col}), 2)
                 FROM analysis_results a
                 JOIN tracks t ON t.id = a.track_id
                 WHERE {where_clause}"
            );
            let (min_val, max_val, total, mean, variance): (f64, f64, i64, f64, f64) =
                db.conn.query_row(&stats_sql, [], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?))
                })?;

            let std_dev = variance.max(0.0).sqrt();

            println!("Distribution: {} ({} tracks)", score.label(), total);
            println!("  min={:.1}  mean={:.1}  std={:.1}  max={:.1}", min_val, mean, std_dev, max_val);
            println!();

            // Build histogram buckets
            let range = max_val - min_val;
            if range <= 0.0 || total == 0 {
                println!("No variation in data.");
                return Ok(());
            }
            let bin_width = range / bins as f64;

            let hist_sql = format!(
                "SELECT CAST((a.{col} - ?1) / ?2 AS INTEGER) as bucket, COUNT(*)
                 FROM analysis_results a
                 JOIN tracks t ON t.id = a.track_id
                 WHERE {where_clause}
                 GROUP BY bucket
                 ORDER BY bucket"
            );

            let mut stmt = db.conn.prepare(&hist_sql)?;
            let mut bucket_counts = vec![0i64; bins];
            let rows = stmt.query_map(
                rusqlite::params![min_val, bin_width],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
            )?;
            for r in rows {
                let (bucket, count) = r?;
                let idx = (bucket as usize).min(bins - 1);
                bucket_counts[idx] += count;
            }

            let max_count = *bucket_counts.iter().max().unwrap_or(&1);
            let bar_max = 50;

            // If song filter, find where it falls
            let song_bucket: Option<usize> = if let Some(ref s) = song {
                let song_sql = format!(
                    "SELECT a.{col}
                     FROM analysis_results a
                     JOIN tracks t ON t.id = a.track_id
                     WHERE (t.parsed_title LIKE ?1 OR t.title LIKE ?1)
                       AND a.{col} IS NOT NULL
                     ORDER BY a.{col} DESC LIMIT 1"
                );
                let pattern = format!("%{s}%");
                let val: Option<f64> = db.conn.query_row(
                    &song_sql, rusqlite::params![pattern], |row| row.get(0),
                ).ok();
                val.map(|v| ((v - min_val) / bin_width).floor() as usize).map(|b| b.min(bins - 1))
            } else {
                None
            };

            for i in 0..bins {
                let lo = min_val + i as f64 * bin_width;
                let hi = lo + bin_width;
                let count = bucket_counts[i];
                let bar_len = if max_count > 0 {
                    (count as f64 / max_count as f64 * bar_max as f64) as usize
                } else {
                    0
                };
                let marker = if song_bucket == Some(i) { "*" } else { " " };
                let bar: String = "#".repeat(bar_len);
                println!(
                    "{:>5.0}-{:<5.0} {:>5} {}{} {}",
                    lo, hi, count, marker, bar,
                    if count > 0 {
                        format!("{:.1}%", 100.0 * count as f64 / total as f64)
                    } else {
                        String::new()
                    }
                );
            }

            if let Some(ref s) = song {
                println!();
                println!("* = bucket containing \"{}\"", s);
            }
        }

        Commands::Correlate { score, live_only, min_duration, limit } => {
            use setbreak::db::columns::ANALYSIS_SCHEMA;

            let target_col = score.column();
            let min_dur_secs = min_duration.map(|m| m * 60.0);

            // Build WHERE clause
            let mut where_parts = vec![
                format!("a.{target_col} IS NOT NULL"),
                format!("{}", setbreak::db::columns::NOT_GARBAGE),
            ];
            if live_only {
                where_parts.push(format!("{}", setbreak::db::columns::LIVE_ONLY));
            }
            if let Some(dur) = min_dur_secs {
                where_parts.push(format!("a.duration >= {dur}"));
            }
            let where_clause = where_parts.join(" AND ");

            // Collect numeric feature columns (skip TEXT/JSON columns and the target itself)
            let feature_cols: Vec<&str> = ANALYSIS_SCHEMA.iter()
                .filter(|c| c.sql_type == "REAL" || c.sql_type == "INT")
                .filter(|c| c.name != target_col)
                .filter(|c| !c.name.ends_with("_score")) // skip other scores
                .map(|c| c.name)
                .collect();

            println!("Correlating {} features with {}...", feature_cols.len(), score.label());
            println!();

            // Compute Pearson correlation for each feature
            let mut correlations: Vec<(&str, f64, &str, &str)> = Vec::new();

            for &feat in &feature_cols {
                // SQLite Pearson r via covariance / (std_x * std_y)
                let sql = format!(
                    "SELECT
                        COUNT(*),
                        AVG(a.{feat}),
                        AVG(a.{target_col}),
                        AVG(a.{feat} * a.{target_col}),
                        AVG(a.{feat} * a.{feat}),
                        AVG(a.{target_col} * a.{target_col})
                     FROM analysis_results a
                     JOIN tracks t ON t.id = a.track_id
                     WHERE {where_clause} AND a.{feat} IS NOT NULL"
                );

                let result = db.conn.query_row(&sql, [], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, f64>(1)?,
                        row.get::<_, f64>(2)?,
                        row.get::<_, f64>(3)?,
                        row.get::<_, f64>(4)?,
                        row.get::<_, f64>(5)?,
                    ))
                });

                if let Ok((n, mean_x, mean_y, mean_xy, mean_x2, mean_y2)) = result {
                    if n < 100 { continue; } // skip sparse features
                    let cov = mean_xy - mean_x * mean_y;
                    let var_x = mean_x2 - mean_x * mean_x;
                    let var_y = mean_y2 - mean_y * mean_y;
                    if var_x > 1e-10 && var_y > 1e-10 {
                        let r = cov / (var_x.sqrt() * var_y.sqrt());
                        let category = ANALYSIS_SCHEMA.iter()
                            .find(|c| c.name == feat)
                            .map(|c| c.category)
                            .unwrap_or("?");
                        let desc = ANALYSIS_SCHEMA.iter()
                            .find(|c| c.name == feat)
                            .map(|c| c.description)
                            .unwrap_or("");
                        correlations.push((feat, r, category, desc));
                    }
                }
            }

            // Sort by absolute correlation
            correlations.sort_by(|a, b| b.1.abs().partial_cmp(&a.1.abs()).unwrap());

            println!("{:<35} {:>6} {:<15} {}", "Feature", "r", "Category", "Description");
            println!("{}", "-".repeat(100));

            for (feat, r, cat, desc) in correlations.iter().take(limit) {
                let sign = if *r > 0.0 { "+" } else { "-" };
                println!(
                    "{:<35} {}{:.3} {:<15} {}",
                    feat, sign, r.abs(), cat,
                    if desc.len() > 35 { &desc[..35] } else { desc }
                );
            }
            println!();
            println!("{} features analyzed", correlations.len());
        }

        Commands::Schema { grep, category, scores } => {
            use setbreak::db::columns::ANALYSIS_SCHEMA;

            let columns: Vec<_> = ANALYSIS_SCHEMA.iter().filter(|c| {
                if scores {
                    return c.category == "Score";
                }
                if let Some(ref pat) = grep {
                    let pat_lower = pat.to_lowercase();
                    return c.name.to_lowercase().contains(&pat_lower)
                        || c.category.to_lowercase().contains(&pat_lower)
                        || c.description.to_lowercase().contains(&pat_lower);
                }
                if let Some(ref cat) = category {
                    let cat_lower = cat.to_lowercase();
                    return c.category.to_lowercase().contains(&cat_lower);
                }
                true
            }).collect();

            if columns.is_empty() {
                println!("No matching columns found.");
            } else {
                let mut current_category = "";
                for c in &columns {
                    if c.category != current_category {
                        if !current_category.is_empty() {
                            println!();
                        }
                        println!("  {}", c.category);
                        println!("  {}", "-".repeat(c.category.len()));
                        current_category = c.category;
                    }
                    println!(
                        "  {:<40} {:<4}  {}",
                        c.name, c.sql_type, c.description
                    );
                }
                println!();
                println!("{} columns", columns.len());
            }
        }

        Commands::ScoreMatrix { live_only, min_duration } => {
            use setbreak::db::columns::SCORE_COLUMNS;
            let min_dur_secs = min_duration.map(|m| m * 60.0);

            let mut where_parts = vec![
                format!("{}", setbreak::db::columns::NOT_GARBAGE),
                "a.energy_score IS NOT NULL".to_string(),
            ];
            if live_only {
                where_parts.push(format!("{}", setbreak::db::columns::LIVE_ONLY));
            }
            if let Some(dur) = min_dur_secs {
                where_parts.push(format!("a.duration >= {dur}"));
            }
            let where_clause = where_parts.join(" AND ");

            // Load all score vectors
            let sql = format!(
                "SELECT {} FROM analysis_results a JOIN tracks t ON t.id = a.track_id WHERE {}",
                SCORE_COLUMNS.join(", "),
                where_clause,
            );
            let mut stmt = db.conn.prepare(&sql)?;
            let mut data: Vec<Vec<f64>> = vec![Vec::new(); SCORE_COLUMNS.len()];
            let mut n = 0usize;
            let mut rows = stmt.query([])?;
            while let Some(row) = rows.next()? {
                for (i, col_data) in data.iter_mut().enumerate() {
                    col_data.push(row.get::<_, f64>(i).unwrap_or(0.0));
                }
                n += 1;
            }

            // Compute Pearson r for each pair
            let labels = ["Eng", "Int", "Grv", "Imp", "Tgt", "Bld", "Exp", "Trn", "Val", "Aro"];

            println!("Score correlation matrix ({} tracks)", n);
            println!();

            // Header
            print!("{:>5}", "");
            for l in &labels {
                print!("{:>6}", l);
            }
            println!();
            println!("{}", "-".repeat(5 + labels.len() * 6));

            for i in 0..SCORE_COLUMNS.len() {
                print!("{:>5}", labels[i]);
                for j in 0..SCORE_COLUMNS.len() {
                    if j > i {
                        print!("{:>6}", "");
                        continue;
                    }
                    let r = pearson_r(&data[i], &data[j]);
                    print!("{:>6.2}", r);
                }
                println!();
            }

            println!();
            println!("Read: row correlates with column at r value");
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

/// Pearson correlation coefficient between two equal-length f64 slices.
fn pearson_r(x: &[f64], y: &[f64]) -> f64 {
    let n = x.len() as f64;
    if n < 2.0 { return 0.0; }
    let sum_x: f64 = x.iter().sum();
    let sum_y: f64 = y.iter().sum();
    let sum_xy: f64 = x.iter().zip(y.iter()).map(|(a, b)| a * b).sum();
    let sum_x2: f64 = x.iter().map(|a| a * a).sum();
    let sum_y2: f64 = y.iter().map(|a| a * a).sum();
    let num = n * sum_xy - sum_x * sum_y;
    let den = ((n * sum_x2 - sum_x * sum_x) * (n * sum_y2 - sum_y * sum_y)).sqrt();
    if den < 1e-10 { 0.0 } else { num / den }
}

/// Extract a row of f64 values from a single-row query.
fn rusqlite_row_to_f64_vec(db: &setbreak::db::Database, sql: &str, track_id: i64) -> Result<Vec<f64>> {
    let mut stmt = db.conn.prepare(sql)?;
    let col_count = stmt.column_count();
    let row = stmt.query_row(rusqlite::params![track_id], |row| {
        let mut vals = Vec::with_capacity(col_count);
        for i in 0..col_count {
            vals.push(row.get::<_, f64>(i).unwrap_or(0.0));
        }
        Ok(vals)
    })?;
    Ok(row)
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
