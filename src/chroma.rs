//! Chroma-based harmonic analysis: key matching, transposition-aware similarity.
//!
//! Uses the stored `chroma_vector` (12-dim pitch class distribution) to find
//! tracks with similar harmonic content, even across different keys.

use crate::db::Database;

/// Pitch class names for display.
const PITCH_CLASSES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

/// A track with its chroma vector and metadata.
#[derive(Clone)]
pub struct ChromaTrack {
    pub track_id: i64,
    pub title: String,
    pub date: String,
    pub key: String,
    pub chroma: [f64; 12],
    pub duration_min: f64,
}

/// Result of a harmonic similarity search.
pub struct HarmonicMatch {
    pub title: String,
    pub date: String,
    pub key: String,
    pub duration_min: f64,
    pub distance: f64,
    pub transposition: i32,
    pub dominant_pitch_class: String,
}

/// Parse a chroma_vector JSON string into a 12-element array.
fn parse_chroma(json: &str) -> Option<[f64; 12]> {
    let vals: Vec<f64> = serde_json::from_str(json).ok()?;
    if vals.len() != 12 {
        return None;
    }
    let mut arr = [0.0; 12];
    arr.copy_from_slice(&vals);
    Some(arr)
}

/// Normalize a chroma vector to unit length (L2 norm).
fn normalize_chroma(chroma: &[f64; 12]) -> [f64; 12] {
    let norm: f64 = chroma.iter().map(|x| x * x).sum::<f64>().sqrt();
    if norm < 1e-10 {
        return *chroma;
    }
    let mut out = [0.0; 12];
    for (i, &val) in chroma.iter().enumerate() {
        out[i] = val / norm;
    }
    out
}

/// Rotate a chroma vector by `semitones` steps (circular shift).
/// This simulates transposition: rotate(chroma, 2) shifts up a whole step.
fn rotate_chroma(chroma: &[f64; 12], semitones: i32) -> [f64; 12] {
    let mut out = [0.0; 12];
    let shift = semitones.rem_euclid(12); // always positive
    for i in 0..12 {
        out[(i + shift as usize) % 12] = chroma[i];
    }
    out
}

/// Cosine distance between two chroma vectors (0 = identical, 2 = opposite).
fn chroma_cosine_distance(a: &[f64; 12], b: &[f64; 12]) -> f64 {
    let mut dot = 0.0;
    let mut norm_a = 0.0;
    let mut norm_b = 0.0;
    for i in 0..12 {
        dot += a[i] * b[i];
        norm_a += a[i] * a[i];
        norm_b += b[i] * b[i];
    }
    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom < 1e-10 {
        return 2.0;
    }
    1.0 - (dot / denom)
}

/// Find the transposition (0-11 semitones) that minimizes cosine distance.
/// Returns (best_distance, best_transposition).
fn best_transposition(a: &[f64; 12], b: &[f64; 12]) -> (f64, i32) {
    let a_norm = normalize_chroma(a);
    let b_norm = normalize_chroma(b);

    let mut best_dist = f64::MAX;
    let mut best_shift = 0i32;

    for shift in 0..12 {
        let b_rotated = rotate_chroma(&b_norm, shift);
        let dist = chroma_cosine_distance(&a_norm, &b_rotated);
        if dist < best_dist {
            best_dist = dist;
            best_shift = shift;
        }
    }

    (best_dist, best_shift)
}

/// Find the dominant pitch class in a chroma vector.
fn dominant_pitch_class(chroma: &[f64; 12]) -> &'static str {
    let mut max_idx = 0;
    let mut max_val = chroma[0];
    for (i, &val) in chroma.iter().enumerate().skip(1) {
        if val > max_val {
            max_val = val;
            max_idx = i;
        }
    }
    PITCH_CLASSES[max_idx]
}

/// Find harmonically similar tracks (transposition-aware chroma matching).
///
/// If `allow_transposition` is true, tries all 12 rotations and picks the best.
/// Otherwise, compares chroma vectors directly (same key only).
pub fn find_harmonic_matches(
    db: &Database,
    target_id: i64,
    limit: usize,
    allow_transposition: bool,
) -> Result<(ChromaTrack, Vec<HarmonicMatch>), String> {
    let tracks = load_chroma_tracks(db).map_err(|e| format!("DB error: {e}"))?;

    let target = tracks
        .iter()
        .find(|t| t.track_id == target_id)
        .ok_or("Target track has no chroma data")?
        .clone();

    let mut matches: Vec<HarmonicMatch> = tracks
        .iter()
        .filter(|t| t.track_id != target_id)
        .map(|t| {
            let (distance, transposition) = if allow_transposition {
                best_transposition(&target.chroma, &t.chroma)
            } else {
                let a_norm = normalize_chroma(&target.chroma);
                let b_norm = normalize_chroma(&t.chroma);
                (chroma_cosine_distance(&a_norm, &b_norm), 0)
            };

            HarmonicMatch {
                title: t.title.clone(),
                date: t.date.clone(),
                key: t.key.clone(),
                duration_min: t.duration_min,
                distance,
                transposition,
                dominant_pitch_class: dominant_pitch_class(&t.chroma).to_string(),
            }
        })
        .collect();

    matches.sort_by(|a, b| {
        a.distance
            .partial_cmp(&b.distance)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    matches.truncate(limit);

    Ok((target, matches))
}

/// Load all tracks with chroma vectors from the database.
fn load_chroma_tracks(db: &Database) -> crate::db::Result<Vec<ChromaTrack>> {
    db.query_chroma_tracks()
}

// ── Database query support ──────────────────────────────────────────────

impl Database {
    /// Load all tracks that have a chroma_vector.
    pub fn query_chroma_tracks(&self) -> crate::db::Result<Vec<ChromaTrack>> {
        let sql = "SELECT a.track_id,
                          COALESCE(t.parsed_title, t.title, '(untitled)'),
                          COALESCE(t.parsed_date, t.date, '?'),
                          COALESCE(a.estimated_key, '?'),
                          a.chroma_vector,
                          COALESCE(a.duration, 0) / 60.0
                   FROM analysis_results a
                   JOIN tracks t ON t.id = a.track_id
                   WHERE a.chroma_vector IS NOT NULL
                     AND COALESCE(t.data_quality, 'ok') != 'garbage'";

        let mut stmt = self.conn.prepare(sql)?;
        let mut tracks = Vec::new();

        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let track_id: i64 = row.get(0)?;
            let title: String = row.get(1)?;
            let date: String = row.get(2)?;
            let key: String = row.get(3)?;
            let chroma_json: String = row.get(4)?;
            let duration_min: f64 = row.get(5)?;

            if let Some(chroma) = parse_chroma(&chroma_json) {
                tracks.push(ChromaTrack {
                    track_id,
                    title,
                    date,
                    key,
                    chroma,
                    duration_min,
                });
            }
        }

        Ok(tracks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rotate_identity() {
        let chroma = [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        let rotated = rotate_chroma(&chroma, 0);
        assert_eq!(chroma, rotated);
    }

    #[test]
    fn test_rotate_one_semitone() {
        let chroma = [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        let rotated = rotate_chroma(&chroma, 1);
        // C shifted up 1 = C#
        assert_eq!(
            rotated,
            [0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]
        );
    }

    #[test]
    fn test_rotate_full_circle() {
        let chroma = [
            1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0,
        ];
        let rotated = rotate_chroma(&chroma, 12);
        assert_eq!(chroma, rotated);
    }

    #[test]
    fn test_cosine_identical() {
        let a = [
            1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0,
        ];
        let dist = chroma_cosine_distance(&a, &a);
        assert!(dist.abs() < 1e-10);
    }

    #[test]
    fn test_best_transposition_same_key() {
        // C major: strong C, E, G
        let a = [1.0, 0.0, 0.0, 0.0, 0.8, 0.0, 0.0, 0.9, 0.0, 0.0, 0.0, 0.0];
        let (dist, shift) = best_transposition(&a, &a);
        assert!(dist < 1e-10);
        assert_eq!(shift, 0);
    }

    #[test]
    fn test_best_transposition_finds_shift() {
        // C major triad: C, E, G
        let c_major = [1.0, 0.0, 0.0, 0.0, 0.8, 0.0, 0.0, 0.9, 0.0, 0.0, 0.0, 0.0];
        // G major triad: G, B, D — which is C major shifted up 7 semitones
        let g_major = rotate_chroma(&c_major, 7);
        let (dist, shift) = best_transposition(&c_major, &g_major);
        assert!(dist < 1e-10);
        // shift should be 7 (to rotate g_major back to c_major) or 5 (complement)
        assert!(shift == 5 || shift == 7);
    }

    #[test]
    fn test_dominant_pitch_class() {
        let chroma = [0.1, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.9, 0.0, 0.0, 0.0, 0.0];
        assert_eq!(dominant_pitch_class(&chroma), "G");
    }

    #[test]
    fn test_parse_chroma_valid() {
        let json = "[0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0, 1.1, 1.2]";
        let result = parse_chroma(json);
        assert!(result.is_some());
        assert_eq!(result.unwrap()[0], 0.1);
    }

    #[test]
    fn test_parse_chroma_wrong_length() {
        let json = "[0.1, 0.2, 0.3]";
        assert!(parse_chroma(json).is_none());
    }
}
