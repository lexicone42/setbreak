//! Boundary feature extraction for segue detection.
//!
//! Computes audio characteristics at the head (first 1s) and tail (last 3s)
//! of each track. These features enable programmatic segue detection by
//! comparing the tail of track N with the head of track N+1.
//!
//! The core signal is silence: a segued track has music sustaining through
//! its tail, while a clean break fades to crowd noise or silence.

use ferrous_waves::AudioFile;

/// Duration of tail region to analyze (seconds).
const TAIL_DURATION_SECS: f32 = 3.0;

/// Duration of head region to analyze (seconds).
const HEAD_DURATION_SECS: f32 = 1.0;

/// Window size for silence detection (seconds). 50ms windows give ~60 windows
/// in the 3-second tail, enough granularity to detect brief fade-outs.
const WINDOW_SECS: f32 = 0.05;

/// Silence threshold in dBFS. Live recordings typically have crowd noise
/// at -40 to -50 dBFS, while music sits at -20 to -5 dBFS. This threshold
/// distinguishes "no music playing" from active performance.
const SILENCE_THRESHOLD_DB: f64 = -40.0;

/// Boundary features extracted from a track's audio.
#[derive(Debug, Clone)]
pub struct BoundaryFeatures {
    /// RMS of the last 3 seconds in dBFS (-100 = silence, 0 = full scale).
    pub tail_rms_db: f64,
    /// Fraction of 50ms windows in the last 3s with RMS below -40 dBFS.
    /// 0.0 = music sustains throughout (segue candidate).
    /// 1.0 = complete silence (clean break).
    pub tail_silence_pct: f64,
    /// RMS of the first 1 second in dBFS.
    pub head_rms_db: f64,
    /// Fraction of 50ms windows in the first 1s with RMS below -40 dBFS.
    /// 0.0 = starts at full volume (segue continuation).
    /// 1.0 = starts from silence (clean intro).
    pub head_silence_pct: f64,
}

/// Extract boundary features from a decoded audio file.
///
/// Converts to mono, slices the first/last N seconds, and computes
/// RMS energy + silence percentage in short windows.
pub fn extract_from_audio(audio: &AudioFile) -> BoundaryFeatures {
    let mono = audio.buffer.to_mono();
    let sr = audio.buffer.sample_rate as f32;
    let window_samples = (sr * WINDOW_SECS) as usize;

    let tail_samples = (sr * TAIL_DURATION_SECS) as usize;
    let head_samples = (sr * HEAD_DURATION_SECS) as usize;

    let tail_start = mono.len().saturating_sub(tail_samples);
    let tail = &mono[tail_start..];
    let head = &mono[..head_samples.min(mono.len())];

    BoundaryFeatures {
        tail_rms_db: rms_db(tail),
        tail_silence_pct: silence_pct(tail, window_samples),
        head_rms_db: rms_db(head),
        head_silence_pct: silence_pct(head, window_samples),
    }
}

/// Compute RMS level in dBFS.
fn rms_db(samples: &[f32]) -> f64 {
    if samples.is_empty() {
        return -100.0;
    }
    let sum_sq: f64 = samples.iter().map(|&s| (s as f64) * (s as f64)).sum();
    let rms = (sum_sq / samples.len() as f64).sqrt();
    if rms < 1e-10 {
        -100.0
    } else {
        20.0 * rms.log10()
    }
}

/// Fraction of windows where RMS is below the silence threshold.
fn silence_pct(samples: &[f32], window_size: usize) -> f64 {
    if samples.is_empty() || window_size == 0 {
        return 1.0;
    }
    let mut silent = 0u32;
    let mut total = 0u32;
    for chunk in samples.chunks(window_size) {
        total += 1;
        if rms_db(chunk) < SILENCE_THRESHOLD_DB {
            silent += 1;
        }
    }
    if total == 0 {
        1.0
    } else {
        f64::from(silent) / f64::from(total)
    }
}

/// Score the probability of a segue between two consecutive tracks.
///
/// Returns a value in [0.0, 1.0] where:
/// - 0.0 = definitely a clean break (silence between songs)
/// - 1.0 = definitely a segue (music sustains across the cut)
///
/// The score is a weighted combination of:
/// - A's tail silence (low = segue, high = break) — 40%
/// - A's tail RMS (high = music playing) — 20%
/// - B's head silence (low = segue, high = clean intro) — 25%
/// - B's head RMS (high = immediate music) — 15%
pub fn segue_score(tail: &BoundaryFeatures, head: &BoundaryFeatures) -> f64 {
    // Invert silence percentages: 0% silent → 1.0 (segue), 100% silent → 0.0
    let tail_music = 1.0 - tail.tail_silence_pct;
    let head_music = 1.0 - head.head_silence_pct;

    // Sigmoid mapping for RMS: -20 dBFS → 0.5, -10 dBFS → ~0.88, -30 dBFS → ~0.12
    let tail_rms_signal = sigmoid(tail.tail_rms_db, -20.0, 0.15);
    let head_rms_signal = sigmoid(head.head_rms_db, -20.0, 0.15);

    0.40 * tail_music + 0.20 * tail_rms_signal + 0.25 * head_music + 0.15 * head_rms_signal
}

/// Logistic sigmoid centered at `center` with steepness `k`.
fn sigmoid(x: f64, center: f64, k: f64) -> f64 {
    1.0 / (1.0 + (-k * (x - center)).exp())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rms_db_silence() {
        assert_eq!(rms_db(&[]), -100.0);
        assert_eq!(rms_db(&[0.0; 100]), -100.0);
    }

    #[test]
    fn test_rms_db_full_scale() {
        // Sine wave at full scale: RMS ≈ -3 dBFS
        let samples: Vec<f32> = (0..44100)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 44100.0).sin())
            .collect();
        let db = rms_db(&samples);
        assert!((db - (-3.01)).abs() < 0.1, "Expected ~-3 dBFS, got {db}");
    }

    #[test]
    fn test_silence_pct_all_silent() {
        let samples = vec![0.0f32; 4410]; // 100ms at 44.1kHz
        assert_eq!(silence_pct(&samples, 2205), 1.0);
    }

    #[test]
    fn test_silence_pct_all_loud() {
        // Full-scale signal: every window should be above threshold
        let samples: Vec<f32> = (0..4410)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 44100.0).sin())
            .collect();
        assert_eq!(silence_pct(&samples, 2205), 0.0);
    }

    #[test]
    fn test_segue_score_definite_segue() {
        let tail = BoundaryFeatures {
            tail_rms_db: -10.0,
            tail_silence_pct: 0.0,
            head_rms_db: 0.0,
            head_silence_pct: 0.0,
        };
        let head = BoundaryFeatures {
            tail_rms_db: 0.0,
            tail_silence_pct: 0.0,
            head_rms_db: -10.0,
            head_silence_pct: 0.0,
        };
        let score = segue_score(&tail, &head);
        assert!(score > 0.8, "Expected high segue score, got {score}");
    }

    #[test]
    fn test_segue_score_definite_break() {
        let tail = BoundaryFeatures {
            tail_rms_db: -50.0,
            tail_silence_pct: 1.0,
            head_rms_db: 0.0,
            head_silence_pct: 0.0,
        };
        let head = BoundaryFeatures {
            tail_rms_db: 0.0,
            tail_silence_pct: 0.0,
            head_rms_db: -50.0,
            head_silence_pct: 1.0,
        };
        let score = segue_score(&tail, &head);
        assert!(score < 0.2, "Expected low segue score, got {score}");
    }
}
