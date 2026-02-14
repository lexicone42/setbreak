pub mod decode;
pub mod features;
pub mod jam_metrics;

use crate::db::models::Track;
use crate::db::Database;
use features::ExtractionResult;
use ferrous_waves::analysis::engine::AnalysisResult;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::Path;
use std::sync::Mutex;
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

    let analyzed = Mutex::new(0u64);
    let failed = Mutex::new(0u64);

    // Collect results first (rayon parallel), then write to DB sequentially
    let results: Vec<_> = pool.install(|| {
        use rayon::prelude::*;
        tracks
            .par_iter()
            .map(|track| {
                let result = analyze_single_track(track);
                pb.inc(1);
                result
            })
            .collect()
    });

    // Write results to DB sequentially (SQLite single-writer)
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
                    Ok(()) => *analyzed.lock().unwrap() += 1,
                    Err(e) => {
                        log::error!("DB error storing analysis for track {}: {}", ta.track_id, e);
                        *failed.lock().unwrap() += 1;
                    }
                }
            }
            Err(e) => {
                log::warn!("Analysis failed: {}", e);
                *failed.lock().unwrap() += 1;
            }
        }
    }

    let analyzed_count = *analyzed.lock().unwrap();
    let failed_count = *failed.lock().unwrap();

    pb.finish_with_message(format!("Done: {} analyzed, {} failed", analyzed_count, failed_count));

    Ok(AnalyzeResult {
        analyzed: analyzed_count,
        failed: failed_count,
    })
}

// Thread-local tokio runtime and analysis engine — reused across tracks on the same thread
// to avoid the overhead of creating them per-track.
thread_local! {
    static THREAD_RT: tokio::runtime::Runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
}

/// Analyze a single track: decode -> ferrous-waves analyze -> extract features -> compute scores.
fn analyze_single_track(
    track: &Track,
) -> std::result::Result<TrackAnalysis, AnalyzeError> {
    let path = Path::new(&track.file_path);

    log::debug!("Analyzing: {}", path.file_name().and_then(|f| f.to_str()).unwrap_or("?"));

    // Decode audio
    let audio = decode::load_audio(path)?;

    // Run ferrous-waves analysis (reuse thread-local tokio runtime)
    let engine = ferrous_waves::AnalysisEngine::new().without_cache();
    let analysis_result: AnalysisResult = THREAD_RT.with(|rt| {
        rt.block_on(engine.analyze(&audio))
    }).map_err(|e| AnalyzeError::Engine(e.to_string()))?;

    // Extract all features into DB schema + detail records
    let mut extraction = features::extract(track.id, &analysis_result);

    // Compute jam-specific derived scores using the full analysis result
    jam_metrics::compute_jam_scores(&mut extraction.analysis, &analysis_result);

    Ok(TrackAnalysis {
        track_id: track.id,
        extraction,
    })
}
