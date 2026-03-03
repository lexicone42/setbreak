//! Audio-based segue detection between consecutive tracks.
//!
//! Uses boundary features (tail RMS, silence percentage) stored during analysis
//! to detect segues programmatically — no reliance on filename markers or setlist data.
//!
//! This is "tier 3" in the segue detection hierarchy:
//! 1. Setlist `segued` boolean (authoritative, from gdshowsdb/phish.in)
//! 2. Filename `->` markers (heuristic, from taper conventions)
//! 3. Audio boundary analysis (this module — works for any recording)

use crate::analyzer::boundary;
use crate::db::models::SegueTrackRow;
use crate::db::Database;

/// A detected segue between two consecutive tracks.
pub struct DetectedSegue {
    pub date: String,
    pub band: Option<String>,
    pub track_a_title: String,
    pub track_b_title: String,
    pub confidence: f64,
    pub track_a_tail_rms: f64,
    pub track_a_tail_silence: f64,
    pub track_b_head_rms: f64,
    pub track_b_head_silence: f64,
}

/// Run segue detection and print results.
pub fn run_segue_detection(
    db: &Database,
    min_confidence: f64,
    band: Option<&str>,
    date_filter: Option<&str>,
    limit: usize,
    detail: bool,
) -> anyhow::Result<()> {
    let rows = db.get_tracks_for_segue_detection(band)?;

    if rows.is_empty() {
        println!("No tracks with boundary features found. Run `setbreak extract-boundaries` first.");
        return Ok(());
    }

    // Group tracks by (date, disc) to find consecutive pairs
    let segues = detect_all_segues(&rows, min_confidence, date_filter);

    if segues.is_empty() {
        println!("No segues detected above confidence threshold {:.2}.", min_confidence);
        return Ok(());
    }

    let shown = segues.len().min(limit);
    println!(
        "Detected {} audio segues (showing top {}, min confidence {:.2})\n",
        segues.len(),
        shown,
        min_confidence,
    );

    if detail {
        print_detail(&segues[..shown]);
    } else {
        print_compact(&segues[..shown]);
    }

    Ok(())
}

/// Detect segues across all track pairs grouped by show date + disc + set.
fn detect_all_segues(
    rows: &[SegueTrackRow],
    min_confidence: f64,
    date_filter: Option<&str>,
) -> Vec<DetectedSegue> {
    let mut segues = Vec::new();

    // Iterate consecutive pairs within the same date+disc+set group.
    // Rows are pre-sorted by (date, disc, set, track, file_path).
    let mut i = 0;
    while i + 1 < rows.len() {
        let a = &rows[i];
        let b = &rows[i + 1];

        // Skip if not same date, disc, or set
        if a.parsed_date != b.parsed_date || a.parsed_disc != b.parsed_disc || a.parsed_set != b.parsed_set {
            i += 1;
            continue;
        }

        // Apply date filter
        if let Some(df) = date_filter {
            if a.parsed_date != df {
                i += 1;
                continue;
            }
        }

        // Build boundary feature structs for scoring
        let tail = boundary::BoundaryFeatures {
            tail_rms_db: a.tail_rms_db,
            tail_silence_pct: a.tail_silence_pct,
            head_rms_db: a.head_rms_db,
            head_silence_pct: a.head_silence_pct,
        };
        let head = boundary::BoundaryFeatures {
            tail_rms_db: b.tail_rms_db,
            tail_silence_pct: b.tail_silence_pct,
            head_rms_db: b.head_rms_db,
            head_silence_pct: b.head_silence_pct,
        };

        let confidence = boundary::segue_score(&tail, &head);

        if confidence >= min_confidence {
            segues.push(DetectedSegue {
                date: a.parsed_date.clone(),
                band: a.parsed_band.clone(),
                track_a_title: a.title.clone(),
                track_b_title: b.title.clone(),
                confidence,
                track_a_tail_rms: a.tail_rms_db,
                track_a_tail_silence: a.tail_silence_pct,
                track_b_head_rms: b.head_rms_db,
                track_b_head_silence: b.head_silence_pct,
            });
        }

        i += 1;
    }

    // Sort by confidence descending
    segues.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());
    segues
}

fn print_compact(segues: &[DetectedSegue]) {
    // Header
    println!(
        "{:<12} {:<4} {:>5}  {}",
        "Date", "Band", "Conf", "Segue"
    );
    println!("{}", "-".repeat(75));

    for s in segues {
        let band = s.band.as_deref().unwrap_or("??");
        let segue_display = format!("{} -> {}", truncate(&s.track_a_title, 28), truncate(&s.track_b_title, 28));
        println!(
            "{:<12} {:<4} {:>4.0}%  {}",
            s.date,
            band,
            s.confidence * 100.0,
            segue_display,
        );
    }
}

fn print_detail(segues: &[DetectedSegue]) {
    for s in segues {
        let band = s.band.as_deref().unwrap_or("??");
        println!(
            "{} ({}) — {:.0}% confidence",
            s.date,
            band,
            s.confidence * 100.0,
        );
        println!(
            "  {} -> {}",
            s.track_a_title, s.track_b_title,
        );
        println!(
            "    A tail: {:.1} dBFS, {:.0}% silent | B head: {:.1} dBFS, {:.0}% silent",
            s.track_a_tail_rms,
            s.track_a_tail_silence * 100.0,
            s.track_b_head_rms,
            s.track_b_head_silence * 100.0,
        );
        println!();
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max.saturating_sub(3)])
    }
}

/// Get audio-detected segues for a specific date, for integration with chain detection.
/// Returns pairs of (position_a, position_b, confidence) for tracks that appear to be segued.
pub fn detect_segues_for_date(
    db: &Database,
    date: &str,
    min_confidence: f64,
) -> anyhow::Result<Vec<(String, String, f64)>> {
    let rows = db.get_tracks_for_segue_detection(None)?;

    // Filter to this date
    let date_rows: Vec<&SegueTrackRow> = rows.iter().filter(|r| r.parsed_date == date).collect();

    let mut result = Vec::new();
    for pair in date_rows.windows(2) {
        let a = pair[0];
        let b = pair[1];

        if a.parsed_disc != b.parsed_disc || a.parsed_set != b.parsed_set {
            continue;
        }

        let tail = boundary::BoundaryFeatures {
            tail_rms_db: a.tail_rms_db,
            tail_silence_pct: a.tail_silence_pct,
            head_rms_db: a.head_rms_db,
            head_silence_pct: a.head_silence_pct,
        };
        let head = boundary::BoundaryFeatures {
            tail_rms_db: b.tail_rms_db,
            tail_silence_pct: b.tail_silence_pct,
            head_rms_db: b.head_rms_db,
            head_silence_pct: b.head_silence_pct,
        };

        let confidence = boundary::segue_score(&tail, &head);
        if confidence >= min_confidence {
            result.push((a.title.clone(), b.title.clone(), confidence));
        }
    }

    Ok(result)
}
