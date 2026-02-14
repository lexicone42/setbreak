pub mod decode;
pub mod features;
pub mod jam_metrics;

use crate::db::models::Track;
use crate::db::Database;
use features::ExtractionResult;
use ferrous_waves::analysis::engine::{AnalysisConfig, AnalysisResult};
use indicatif::{ProgressBar, ProgressStyle};
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AnalyzeError {
    #[error("Decode error: {0}")]
    Decode(#[from] decode::DecodeError),
    #[error("Analysis engine error: {0}")]
    Engine(String),
    #[error("Database error: {0}")]
    Db(#[from] crate::db::DbError),
}

pub struct AnalyzeResult {
    pub analyzed: u64,
    pub failed: u64,
}

/// Full result from analyzing a single track (before DB write).
struct TrackAnalysis {
    track_id: i64,
    extraction: ExtractionResult,
}

/// Analyze tracks in parallel using rayon + tokio for the async engine.
///
/// Processes tracks in chunks: analyze a chunk in parallel with rayon,
/// write results to DB, then move to next chunk. This gives:
/// - Incremental DB progress (resumable on crash)
/// - Bounded memory (only one chunk of results in memory)
/// - Visible progress in check_progress.sh
pub fn analyze_tracks(
    db: &Database,
    force: bool,
    jobs: usize,
    filter: Option<&str>,
) -> std::result::Result<AnalyzeResult, AnalyzeError> {
    let tracks = if force {
        db.get_all_tracks()?
    } else {
        db.get_unanalyzed_tracks()?
    };

    // Apply filter if provided
    let tracks: Vec<Track> = if let Some(pattern) = filter {
        let pattern_lower = pattern.to_lowercase();
        tracks
            .into_iter()
            .filter(|t| t.file_path.to_lowercase().contains(&pattern_lower))
            .collect()
    } else {
        tracks
    };

    if tracks.is_empty() {
        log::info!("No tracks to analyze");
        return Ok(AnalyzeResult {
            analyzed: 0,
            failed: 0,
        });
    }

    log::info!("Analyzing {} tracks with {} workers", tracks.len(), jobs);

    let pb = ProgressBar::new(tracks.len() as u64);
    pb.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} ({eta}) {msg}",
        )
        .unwrap()
        .progress_chars("#>-"),
    );

    // Configure rayon thread pool
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(jobs)
        .build()
        .unwrap();

    let mut analyzed: u64 = 0;
    let mut failed: u64 = 0;

    // Process in chunks: analyze chunk in parallel, write to DB, repeat.
    // Chunk size = jobs * 2 gives good parallelism while keeping memory bounded.
    let chunk_size = jobs * 2;

    for chunk in tracks.chunks(chunk_size) {
        // Analyze this chunk in parallel
        let results: Vec<_> = pool.install(|| {
            use rayon::prelude::*;
            chunk
                .par_iter()
                .map(|track| {
                    let result = analyze_single_track(track);
                    pb.inc(1);
                    result
                })
                .collect()
        });

        // Write this chunk's results to DB immediately
        for result in results {
            match result {
                Ok(ta) => {
                    match db.store_full_analysis(
                        &ta.extraction.analysis,
                        &ta.extraction.chords,
                        &ta.extraction.segments,
                        &ta.extraction.tension_points,
                        &ta.extraction.transitions,
                    ) {
                        Ok(()) => analyzed += 1,
                        Err(e) => {
                            log::error!(
                                "DB error storing analysis for track {}: {}",
                                ta.track_id,
                                e
                            );
                            failed += 1;
                        }
                    }
                }
                Err(e) => {
                    log::warn!("Analysis failed: {}", e);
                    failed += 1;
                }
            }
        }

        pb.set_message(format!("{} stored, {} failed", analyzed, failed));
    }

    pb.finish_with_message(format!("Done: {} analyzed, {} failed", analyzed, failed));

    Ok(AnalyzeResult { analyzed, failed })
}

// Thread-local tokio runtime — reused across tracks on the same rayon thread
// to avoid the overhead of creating a runtime per-track.
thread_local! {
    static THREAD_RT: tokio::runtime::Runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
}

/// Analysis config optimized for setbreak's batch processing:
/// - Skip PNG visualization (we never display it)
/// - Skip audio fingerprinting (not used yet, future Phase 4)
/// - Skip per-segment content classification (we use overall classification only)
/// - Reduce PYIN thresholds from 100 to 25 (4x faster pitch detection)
/// - Double PYIN hop size (analyze every 2nd frame)
fn fast_analysis_config() -> AnalysisConfig {
    AnalysisConfig {
        skip_visualization: true,
        skip_fingerprinting: true,
        skip_classification_segments: true,
        pyin_threshold_count: 25,
        pyin_hop_multiplier: 2,
    }
}

/// Analyze a single track: decode -> ferrous-waves analyze -> extract features -> compute scores.
fn analyze_single_track(track: &Track) -> std::result::Result<TrackAnalysis, AnalyzeError> {
    let path = Path::new(&track.file_path);

    log::debug!(
        "Analyzing: {}",
        path.file_name().and_then(|f| f.to_str()).unwrap_or("?")
    );

    // Decode audio
    let audio = decode::load_audio(path)?;

    // Run ferrous-waves analysis with optimized config
    let engine = ferrous_waves::AnalysisEngine::new()
        .without_cache()
        .with_analysis_config(fast_analysis_config());
    let analysis_result: AnalysisResult = THREAD_RT
        .with(|rt| rt.block_on(engine.analyze(&audio)))
        .map_err(|e| AnalyzeError::Engine(e.to_string()))?;

    // Extract all features into DB schema + detail records
    let mut extraction = features::extract(track.id, &analysis_result);

    // Compute jam-specific derived scores using the full analysis result
    jam_metrics::compute_jam_scores(&mut extraction.analysis, &analysis_result);

    Ok(TrackAnalysis {
        track_id: track.id,
        extraction,
    })
}
