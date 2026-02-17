//! LUFS-based score calibration to remove recording quality bias.
//!
//! Per-show median LUFS regression: `adjusted = raw - β × (show_lufs - corpus_lufs)`
//! where β is the OLS slope of each score against show median LUFS.

use anyhow::Result;
use std::collections::HashMap;

use crate::db::models::{CalibrationRow, NewAnalysis};
use crate::db::Database;

const SCORE_NAMES: [&str; 10] = [
    "energy",
    "intensity",
    "groove",
    "improvisation",
    "tightness",
    "build_quality",
    "exploratory",
    "transcendence",
    "valence",
    "arousal",
];

/// Minimum |β| to bother correcting — below this, the bias is negligible.
const BETA_THRESHOLD: f64 = 0.1;

pub struct CalibrateResult {
    pub total_tracks: usize,
    pub calibrated: usize,
    pub skipped_no_show: usize,
    pub betas: Vec<(String, f64)>,
    pub corpus_median_lufs: f64,
}

/// Calibrate all jam scores by regressing out per-show LUFS bias.
pub fn calibrate_scores(db: &Database, dry_run: bool) -> Result<CalibrateResult> {
    let rows = db.get_calibration_data()?;
    let total_tracks = rows.len();

    if rows.is_empty() {
        println!("No calibration data (need analyzed tracks with LUFS and parsed dates).");
        return Ok(CalibrateResult {
            total_tracks: 0,
            calibrated: 0,
            skipped_no_show: 0,
            betas: Vec::new(),
            corpus_median_lufs: 0.0,
        });
    }

    // Build show key → track indices
    let mut show_tracks: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, row) in rows.iter().enumerate() {
        let key = show_key(row);
        show_tracks.entry(key).or_default().push(i);
    }

    // Compute per-show median LUFS
    let mut show_median_lufs: HashMap<String, f64> = HashMap::new();
    for (key, indices) in &show_tracks {
        let mut lufs_vals: Vec<f64> = indices.iter().map(|&i| rows[i].lufs).collect();
        show_median_lufs.insert(key.clone(), median(&mut lufs_vals));
    }

    // Corpus median = median of show medians (not all tracks)
    let mut all_show_medians: Vec<f64> = show_median_lufs.values().copied().collect();
    let corpus_median = median(&mut all_show_medians);

    println!(
        "Calibration: {} tracks across {} shows, corpus median LUFS = {:.1}",
        total_tracks,
        show_median_lufs.len(),
        corpus_median
    );
    println!();

    // Per-track show median LUFS (for regression + adjustment)
    let track_show_lufs: Vec<Option<f64>> = rows
        .iter()
        .map(|row| show_median_lufs.get(&show_key(row)).copied())
        .collect();

    // Compute β for each score
    let mut betas = Vec::with_capacity(10);
    for score_idx in 0..10 {
        // Collect (show_median_lufs, score) pairs where both exist
        let mut x_vals = Vec::new();
        let mut y_vals = Vec::new();
        for (i, row) in rows.iter().enumerate() {
            if let (Some(show_lufs), Some(score)) = (track_show_lufs[i], row.scores[score_idx]) {
                x_vals.push(show_lufs);
                y_vals.push(score);
            }
        }

        let beta = if x_vals.len() >= 10 {
            ols_slope(&x_vals, &y_vals)
        } else {
            0.0
        };

        let direction = if beta > BETA_THRESHOLD {
            "louder tapes score higher — will reduce loud, boost quiet"
        } else if beta < -BETA_THRESHOLD {
            "quieter tapes score higher — will reduce quiet, boost loud"
        } else {
            "negligible — no correction"
        };

        println!(
            "  {:<15} β = {:+.4}  ({})",
            SCORE_NAMES[score_idx], beta, direction
        );

        betas.push((SCORE_NAMES[score_idx].to_string(), beta));
    }
    println!();

    if dry_run {
        println!("DRY RUN — no changes written.");
        return Ok(CalibrateResult {
            total_tracks,
            calibrated: 0,
            skipped_no_show: rows
                .iter()
                .enumerate()
                .filter(|(i, _)| track_show_lufs[*i].is_none())
                .count(),
            betas,
            corpus_median_lufs: corpus_median,
        });
    }

    // Apply adjustments
    let mut calibrated = 0;
    let mut skipped_no_show = 0;

    for (i, row) in rows.iter().enumerate() {
        let show_lufs = match track_show_lufs[i] {
            Some(l) => l,
            None => {
                skipped_no_show += 1;
                continue;
            }
        };

        let lufs_delta = show_lufs - corpus_median;
        let mut adjusted_scores: [Option<f64>; 10] = row.scores;

        for (score_idx, (_, beta)) in betas.iter().enumerate() {
            if beta.abs() < BETA_THRESHOLD {
                continue;
            }
            if let Some(raw) = adjusted_scores[score_idx] {
                let adj = (raw - beta * lufs_delta).clamp(0.0, 100.0);
                adjusted_scores[score_idx] = Some(adj);
            }
        }

        // Build a minimal NewAnalysis for update_jam_scores
        let update = NewAnalysis {
            track_id: row.track_id,
            energy_score: adjusted_scores[0],
            intensity_score: adjusted_scores[1],
            groove_score: adjusted_scores[2],
            improvisation_score: adjusted_scores[3],
            tightness_score: adjusted_scores[4],
            build_quality_score: adjusted_scores[5],
            exploratory_score: adjusted_scores[6],
            transcendence_score: adjusted_scores[7],
            valence_score: adjusted_scores[8],
            arousal_score: adjusted_scores[9],
            // All other fields unused by update_jam_scores
            ..NewAnalysis::default()
        };

        db.update_jam_scores(&update)?;
        calibrated += 1;
    }

    Ok(CalibrateResult {
        total_tracks,
        calibrated,
        skipped_no_show,
        betas,
        corpus_median_lufs: corpus_median,
    })
}

fn show_key(row: &CalibrationRow) -> String {
    match &row.parsed_band {
        Some(band) => format!("{}|{}", band, row.parsed_date),
        None => row.parsed_date.clone(),
    }
}

fn median(v: &mut [f64]) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = v.len();
    if n == 0 {
        return 0.0;
    }
    if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    }
}

fn ols_slope(x: &[f64], y: &[f64]) -> f64 {
    let n = x.len() as f64;
    let x_mean = x.iter().sum::<f64>() / n;
    let y_mean = y.iter().sum::<f64>() / n;

    let mut cov = 0.0;
    let mut var = 0.0;
    for i in 0..x.len() {
        let dx = x[i] - x_mean;
        cov += dx * (y[i] - y_mean);
        var += dx * dx;
    }

    if var < 1e-12 {
        0.0
    } else {
        cov / var
    }
}
