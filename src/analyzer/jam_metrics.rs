use crate::db::models::NewAnalysis;
use ferrous_waves::analysis::engine::AnalysisResult;

/// Compute all jam-specific derived scores (0-100) and attach them to the analysis.
///
/// During initial analysis, extracts segment energies directly from the AnalysisResult
/// so the build quality score uses segment data even before segments are stored in DB.
pub fn compute_jam_scores(analysis: &mut NewAnalysis, result: &AnalysisResult) {
    // Extract (start_time, energy) pairs from raw analysis segments
    let segment_energies: Vec<(f64, f64)> = result
        .segments
        .segments
        .iter()
        .map(|seg| (seg.start_time as f64, seg.energy as f64))
        .collect();
    let segments = if segment_energies.is_empty() {
        None
    } else {
        Some(segment_energies.as_slice())
    };
    compute_jam_scores_from_scalars(analysis, segments);
}

/// Compute all jam scores from DB scalars plus optional segment energy data.
/// Used by the rescore command (with DB-loaded segments) and by compute_jam_scores.
pub fn compute_jam_scores_from_scalars(
    analysis: &mut NewAnalysis,
    segment_energies: Option<&[(f64, f64)]>,
) {
    analysis.energy_score = Some(energy_score(analysis));
    analysis.intensity_score = Some(intensity_score(analysis));
    analysis.groove_score = Some(groove_score(analysis));
    analysis.improvisation_score = Some(improvisation_score(analysis));
    analysis.tightness_score = Some(tightness_score(analysis));
    analysis.build_quality_score = Some(build_quality_score(analysis, segment_energies));
    analysis.exploratory_score = Some(exploratory_score(analysis));
    analysis.transcendence_score = Some(transcendence_score(analysis));
    analysis.valence_score = Some(valence_score(analysis));
    analysis.arousal_score = Some(arousal_score(analysis));
}

// ── Energy Score (0-100) ──────────────────────────────────────────────
// How "present" and powerful the music feels.
// Calibrated to live tape recordings (quieter than mastered commercial audio).
// Inputs: RMS level, LUFS, sub-band bass energy, spectral centroid
fn energy_score(a: &NewAnalysis) -> f64 {
    let rms = a.rms_level.unwrap_or(0.0);
    let lufs = a.lufs_integrated.unwrap_or(-60.0);
    let bass = a.sub_band_bass_mean.unwrap_or(0.0);
    let centroid = a.spectral_centroid_mean.unwrap_or(0.0);

    // RMS (30 pts): calibrated to live tape range
    // Library: 0.003-0.31, avg 0.10. Old formula used rms*40 (max ~12/40).
    let rms_norm = (rms / 0.18).clamp(0.0, 1.0);
    let rms_contrib = rms_norm * 30.0;

    // LUFS (30 pts): calibrated to library loudness
    // Library: -68 to -31, avg -41. Old formula used -60..-5 range (max ~21/40).
    let lufs_norm = ((lufs + 55.0) / 22.0).clamp(0.0, 1.0);
    let lufs_contrib = lufs_norm * 30.0;

    // Sub-band bass energy (20 pts): low-frequency power = felt energy
    // Library: 0.009-0.65, avg 0.10
    let bass_norm = (bass / 0.15).clamp(0.0, 1.0);
    let bass_contrib = bass_norm * 20.0;

    // Spectral centroid brightness (20 pts): brighter = more perceived energy
    // Library: 1917-11067, avg 3808
    let centroid_norm = ((centroid - 2000.0) / 6000.0).clamp(0.0, 1.0);
    let centroid_contrib = centroid_norm * 20.0;

    (rms_contrib + lufs_contrib + bass_contrib + centroid_contrib).clamp(0.0, 100.0)
}

// ── Intensity Score (0-100) ───────────────────────────────────────────
// How much the music *varies* in energy — flux and dynamics.
fn intensity_score(a: &NewAnalysis) -> f64 {
    let flux_std = a.spectral_flux_std.unwrap_or(0.0);
    let dynamic_range = a.dynamic_range.unwrap_or(0.0);
    let loudness_range = a.loudness_range.unwrap_or(0.0);

    let flux_norm = (flux_std / 50.0).clamp(0.0, 1.0);
    let flux_contrib = flux_norm * 40.0;

    let dr_norm = (dynamic_range / 30.0).clamp(0.0, 1.0);
    let dr_contrib = dr_norm * 30.0;

    let lr_norm = (loudness_range / 20.0).clamp(0.0, 1.0);
    let lr_contrib = lr_norm * 30.0;

    (flux_contrib + dr_contrib + lr_contrib).clamp(0.0, 100.0)
}

// ── Groove Score (0-100) ──────────────────────────────────────────────
// How steady and compelling the rhythm is.
// v2: recalibrated for real differentiation. Old formula gave 88.6 avg with 33% at 100.
// Key change: replaced centroid_std with bass steadiness, tightened all thresholds.
fn groove_score(a: &NewAnalysis) -> f64 {
    let duration = a.duration.unwrap_or(1.0).max(1.0);
    let onset_count = a.onset_count.unwrap_or(0) as f64;

    // No onsets = no rhythm = no groove
    if onset_count < 1.0 {
        return 0.0;
    }

    let flux_mean = a.spectral_flux_mean.unwrap_or(0.0);
    let flux_std = a.spectral_flux_std.unwrap_or(0.0);
    let bass_mean = a.sub_band_bass_mean.unwrap_or(0.0);
    let bass_std = a.sub_band_bass_std.unwrap_or(0.0);
    let rep_sim = a.repetition_similarity.unwrap_or(0.85);

    // 1. Onset rate sweet spot (20 pts): 7-9/sec is the groove zone (tighter than v1's 6-10)
    // Library: 1.1-13.2/sec, avg 8.3
    let onset_rate = onset_count / duration;
    let onset_sweet = if onset_rate < 5.0 {
        onset_rate / 5.0
    } else if onset_rate < 7.0 {
        0.6 + 0.4 * (onset_rate - 5.0) / 2.0
    } else if onset_rate <= 9.0 {
        1.0
    } else if onset_rate <= 11.0 {
        1.0 - 0.4 * (onset_rate - 9.0) / 2.0
    } else {
        (0.6 - (onset_rate - 11.0) / 5.0).max(0.0)
    };
    let onset_contrib = onset_sweet.clamp(0.0, 1.0) * 20.0;

    // 2. Rhythmic consistency (30 pts): flux CV — strongest differentiator
    // Library flux_cv: avg 0.706, range 0.05-3.31
    // v1 used (1-(cv-0.3)/1.2) which was too generous. Now: direct (1-cv).
    let flux_cv = if flux_mean > 0.5 { flux_std / flux_mean } else { 2.0 };
    let flux_score = (1.0 - flux_cv).clamp(0.0, 1.0);
    let flux_contrib = flux_score * 30.0;

    // 3. Bass steadiness (25 pts): groove lives in the bass
    // Low bass CV = locked-in bass pattern. Library bass_cv: avg 0.64, range 0.07-1.75
    let bass_cv = if bass_mean > 0.01 { bass_std / bass_mean } else { 1.5 };
    let bass_score = (1.0 - bass_cv * 0.7).clamp(0.0, 1.0);
    let bass_contrib = bass_score * 25.0;

    // 4. Pattern repetition (25 pts): groove IS repetition
    // Library: avg 0.90, range 0.80-0.999
    // v1 mapped 0.75-1.0, now 0.85-1.0 — only high repetition scores well
    let rep_score = ((rep_sim - 0.85) / 0.15).clamp(0.0, 1.0);
    let rep_contrib = rep_score * 25.0;

    (onset_contrib + flux_contrib + bass_contrib + rep_contrib).clamp(0.0, 100.0)
}

// ── Improvisation Score (0-100) ───────────────────────────────────────
// How much the music departs from repetitive structure.
// Uses non-repetition, chord richness, timbral variety, and structural transitions.
fn improvisation_score(a: &NewAnalysis) -> f64 {
    // 1. Non-repetition (25 pts): low repetition similarity = improvised
    // Library range: 0.80-0.99, avg 0.90
    let rep_sim = a.repetition_similarity.unwrap_or(0.9);
    let non_rep = (1.0 - (rep_sim - 0.75) / 0.25).clamp(0.0, 1.0);
    let non_rep_contrib = non_rep * 25.0;

    // 2. Chord richness (25 pts): more unique chords = more harmonic exploration
    // Library range: 5-24 chords, avg 20
    let chords = a.chord_count.unwrap_or(0) as f64;
    let chord_norm = ((chords - 3.0) / 18.0).clamp(0.0, 1.0);
    let chord_contrib = chord_norm * 25.0;

    // 3. Timbral variety (25 pts): high centroid std = exploring tonal space
    // Library range: 388-3414, avg 970 (inverse of groove's timbral consistency)
    let centroid_std = a.spectral_centroid_std.unwrap_or(500.0);
    let timbre_variety = ((centroid_std - 400.0) / 2500.0).clamp(0.0, 1.0);
    let timbre_contrib = timbre_variety * 25.0;

    // 4. Structural variety (25 pts): transitions between sections
    // Library range: 0-190, avg 11
    let transitions = a.transition_count.unwrap_or(0) as f64;
    let trans_norm = (transitions / 30.0).clamp(0.0, 1.0);
    let trans_contrib = trans_norm * 25.0;

    (non_rep_contrib + chord_contrib + timbre_contrib + trans_contrib).clamp(0.0, 100.0)
}

// ── Tightness Score (0-100) ───────────────────────────────────────────
// How well the band is locked in together.
// Uses pitch stability, flux consistency, beat structure, and tonal consistency.
fn tightness_score(a: &NewAnalysis) -> f64 {
    // 1. Pitch stability (25 pts): steady pitch = musicians locked in
    // Library range: 0.33-0.91, avg 0.65
    let pitch_stab = a.pitch_stability.unwrap_or(0.5);
    let pitch_contrib = pitch_stab.clamp(0.0, 1.0) * 25.0;

    // 2. Spectral flux consistency (25 pts): low flux CV = consistent energy
    // Same concept as groove but measuring "tightness" not "groove feel"
    let flux_mean = a.spectral_flux_mean.unwrap_or(0.0);
    let flux_std = a.spectral_flux_std.unwrap_or(0.0);
    let flux_cv = if flux_mean > 0.5 { flux_std / flux_mean } else { 2.0 };
    let flux_score = (1.0 - (flux_cv - 0.3) / 1.2).clamp(0.0, 1.0);
    let flux_contrib = flux_score * 25.0;

    // 3. Beat-onset ratio (25 pts): if most onsets align with beats, rhythm is tight
    let beats = a.beat_count.unwrap_or(0) as f64;
    let onsets = a.onset_count.unwrap_or(1).max(1) as f64;
    let beat_ratio = (beats / onsets).clamp(0.0, 1.0);
    let beat_strength = if beat_ratio < 0.1 {
        beat_ratio * 5.0
    } else if beat_ratio <= 0.8 {
        1.0
    } else {
        0.8 + 0.2 * (1.0 - beat_ratio) / 0.2
    };
    let beat_contrib = beat_strength.clamp(0.0, 1.0) * 25.0;

    // 4. Tonal consistency (25 pts): low spectral flatness std = consistent character
    // Library range: 0.05-0.26, avg 0.09
    let flat_std = a.spectral_flatness_std.unwrap_or(0.15);
    let tonal_score = (1.0 - (flat_std - 0.04) / 0.22).clamp(0.0, 1.0);
    let tonal_contrib = tonal_score * 25.0;

    (pitch_contrib + flux_contrib + beat_contrib + tonal_contrib).clamp(0.0, 100.0)
}

// ── Build Quality Score (0-100) ───────────────────────────────────────
// How well the music builds to peaks — dynamic arcs and tension.
//
// Primary: segment-level arc detection (when segment data available and track >= 90s).
// Analyzes the energy contour in 30-second windows to find build→peak arcs,
// scores each arc on magnitude/duration/peak height, rewards multiple arcs.
//
// Fallback: whole-track aggregates (crest factor, loudness range, energy variance,
// transition density) for short tracks or when no segment data exists.
fn build_quality_score(a: &NewAnalysis, segment_energies: Option<&[(f64, f64)]>) -> f64 {
    let duration = a.duration.unwrap_or(0.0);

    // Use segment-based scoring when we have data and track is long enough for arcs
    if let Some(energies) = segment_energies {
        if duration >= 90.0 && energies.len() >= 3 {
            return build_quality_from_segments(energies, duration);
        }
    }

    build_quality_score_fallback(a)
}

/// Fallback: whole-track aggregate formula (original build_quality_score).
fn build_quality_score_fallback(a: &NewAnalysis) -> f64 {
    let duration = a.duration.unwrap_or(1.0).max(1.0);

    // 1. Crest factor (30 pts): peak-to-RMS ratio = dynamic peak character
    let crest = a.crest_factor.unwrap_or(5.0);
    let crest_norm = ((crest - 3.0) / 25.0).clamp(0.0, 1.0);
    let crest_contrib = crest_norm * 30.0;

    // 2. Loudness range (25 pts): wide LRA = dynamic builds
    let lra = a.loudness_range.unwrap_or(0.0);
    let lra_norm = ((lra - 1.0) / 20.0).clamp(0.0, 1.0);
    let lra_contrib = lra_norm * 25.0;

    // 3. Energy variance (20 pts): variation in energy = dynamic movement
    let e_var = a.energy_variance.unwrap_or(0.0);
    let var_norm = (e_var / 0.01).clamp(0.0, 1.0);
    let var_contrib = var_norm * 20.0;

    // 4. Transition density (25 pts): transitions per minute = structural dynamism
    let transitions = a.transition_count.unwrap_or(0) as f64;
    let trans_per_min = transitions / (duration / 60.0);
    let trans_norm = (trans_per_min / 5.0).clamp(0.0, 1.0);
    let trans_contrib = trans_norm * 25.0;

    (crest_contrib + lra_contrib + var_contrib + trans_contrib).clamp(0.0, 100.0)
}

/// A detected build arc in the energy contour.
#[derive(Debug)]
struct BuildArc {
    /// Index of the arc's lowest energy (trough)
    start_idx: usize,
    /// Index of the arc's highest energy (peak)
    peak_idx: usize,
    /// Energy value at the trough
    trough_energy: f64,
    /// Energy value at the peak
    peak_energy: f64,
}

/// Bucket ~1-second segments into 30-second windows, then apply 3-window rolling average.
fn bucket_and_smooth(energies: &[(f64, f64)], duration: f64) -> Vec<f64> {
    let window_secs = 30.0;
    let n_windows = ((duration / window_secs).ceil() as usize).max(1);
    let mut buckets = vec![Vec::new(); n_windows];

    for &(time, energy) in energies {
        let idx = ((time / window_secs) as usize).min(n_windows - 1);
        buckets[idx].push(energy);
    }

    // Mean energy per window (empty windows get 0.0)
    let raw: Vec<f64> = buckets
        .iter()
        .map(|b| {
            if b.is_empty() {
                0.0
            } else {
                b.iter().sum::<f64>() / b.len() as f64
            }
        })
        .collect();

    // 3-window rolling average to smooth noise
    if raw.len() < 3 {
        return raw;
    }
    let mut smoothed = Vec::with_capacity(raw.len());
    smoothed.push((raw[0] + raw[1]) / 2.0);
    for i in 1..raw.len() - 1 {
        smoothed.push((raw[i - 1] + raw[i] + raw[i + 1]) / 3.0);
    }
    smoothed.push((raw[raw.len() - 2] + raw[raw.len() - 1]) / 2.0);
    smoothed
}

/// Detect build arcs in smoothed energy windows.
///
/// An arc starts when energy begins rising. It continues as long as energy doesn't
/// drop more than 15% below the running max (tolerance for brief dips). The arc ends
/// when energy drops >15% below running max, or at end of track.
fn detect_arcs(windows: &[f64]) -> Vec<BuildArc> {
    if windows.len() < 2 {
        return vec![];
    }

    let mut arcs = Vec::new();
    let mut in_arc = false;
    let mut arc_start = 0;
    let mut running_max = 0.0_f64;
    let mut peak_idx = 0;
    let mut trough_energy = 0.0_f64;

    for i in 1..windows.len() {
        let rising = windows[i] > windows[i - 1] * 0.95; // tolerant rise detection

        if !in_arc && rising {
            // Start a new arc
            in_arc = true;
            arc_start = i - 1;
            trough_energy = windows[i - 1];
            running_max = windows[i];
            peak_idx = i;
        } else if in_arc {
            if windows[i] > running_max {
                running_max = windows[i];
                peak_idx = i;
            }

            // Check if energy has dropped >15% below the running max
            let drop_threshold = running_max * 0.85;
            if windows[i] < drop_threshold {
                // Arc ended — record if the build was meaningful
                let magnitude = running_max - trough_energy;
                if magnitude > 0.0 && peak_idx > arc_start {
                    arcs.push(BuildArc {
                        start_idx: arc_start,
                        peak_idx,
                        trough_energy,
                        peak_energy: running_max,
                    });
                }
                in_arc = false;
            }
        }
    }

    // Close any in-progress arc at end of track
    if in_arc {
        let magnitude = running_max - trough_energy;
        if magnitude > 0.0 && peak_idx > arc_start {
            arcs.push(BuildArc {
                start_idx: arc_start,
                peak_idx,
                trough_energy,
                peak_energy: running_max,
            });
        }
    }

    arcs
}

/// Score a single build arc (0-100).
fn score_arc(arc: &BuildArc, track_avg: f64, track_range: f64) -> f64 {
    let duration_windows = (arc.peak_idx - arc.start_idx) as f64;
    let magnitude = arc.peak_energy - arc.trough_energy;

    // 1. Magnitude (40 pts): energy delta normalized to track energy range
    // A build that covers the full dynamic range of the track scores 40.
    let mag_norm = if track_range > 0.0 {
        (magnitude / track_range).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let mag_contrib = mag_norm * 40.0;

    // 2. Duration (30 pts): longer builds score higher
    // 1 window (30s) = low score, ~10 windows (5 min) = max
    // Minimum 2 windows to register, ramps up to 10
    let dur_norm = ((duration_windows - 1.0) / 9.0).clamp(0.0, 1.0);
    let dur_contrib = dur_norm * 30.0;

    // 3. Peak height (30 pts): how high the peak is relative to track average
    // Peaks well above average energy are more impressive builds
    let peak_ratio = if track_avg > 0.0 {
        arc.peak_energy / track_avg
    } else {
        1.0
    };
    // ratio of 1.0 = at average (low score), 2.0+ = well above (high score)
    let peak_norm = ((peak_ratio - 1.0) / 1.5).clamp(0.0, 1.0);
    let peak_contrib = peak_norm * 30.0;

    (mag_contrib + dur_contrib + peak_contrib).clamp(0.0, 100.0)
}

/// Segment-level build quality scoring via arc detection.
fn build_quality_from_segments(energies: &[(f64, f64)], duration: f64) -> f64 {
    let windows = bucket_and_smooth(energies, duration);
    if windows.len() < 2 {
        return 0.0;
    }

    let track_avg = windows.iter().sum::<f64>() / windows.len() as f64;
    let track_min = windows.iter().cloned().fold(f64::INFINITY, f64::min);
    let track_max = windows.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let track_range = track_max - track_min;

    let arcs = detect_arcs(&windows);
    if arcs.is_empty() {
        return 0.0;
    }

    // Score each arc, keep the best
    let mut arc_scores: Vec<f64> = arcs
        .iter()
        .map(|arc| score_arc(arc, track_avg, track_range))
        .collect();
    arc_scores.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));

    let best_score = arc_scores[0];

    // Multi-arc bonus: reward tracks with multiple good build arcs
    // Filter to arcs scoring >= 20 (meaningful builds, not noise)
    let good_arc_count = arc_scores.iter().filter(|&&s| s >= 20.0).count();
    let multi_arc_bonus = match good_arc_count {
        0 | 1 => 0.0,
        2 => 40.0,
        3 => 70.0,
        _ => 100.0, // 4+ good arcs = max bonus
    };

    // Track score = best arc (70%) + multi-arc bonus (30%)
    (best_score * 0.7 + multi_arc_bonus * 0.3).clamp(0.0, 100.0)
}

// ── Exploratory Score (0-100) ─────────────────────────────────────────
// How much musical territory is covered — timbral, textural, structural.
// Uses spectral flatness variety, pitch confidence, transition density, mode ambiguity.
fn exploratory_score(a: &NewAnalysis) -> f64 {
    let duration = a.duration.unwrap_or(1.0).max(1.0);

    // 1. Spectral flatness variety (25 pts): variation between tonal and noisy moments
    // Library range: 0.05-0.26, avg 0.09
    let flat_std = a.spectral_flatness_std.unwrap_or(0.05);
    let flat_norm = ((flat_std - 0.04) / 0.22).clamp(0.0, 1.0);
    let flat_contrib = flat_norm * 25.0;

    // 2. Pitch confidence inverse (25 pts): uncertain pitch = exploring tonal space
    // Library range: 0.028-0.845, avg 0.58
    let pitch_conf = a.pitch_confidence_mean.unwrap_or(0.5);
    let pitch_explore = (1.0 - pitch_conf).clamp(0.0, 1.0);
    let pitch_contrib = pitch_explore * 25.0;

    // 3. Transition density (25 pts): transitions per minute
    // Library range: 0-27/min
    let transitions = a.transition_count.unwrap_or(0) as f64;
    let trans_per_min = transitions / (duration / 60.0);
    let trans_norm = (trans_per_min / 5.0).clamp(0.0, 1.0);
    let trans_contrib = trans_norm * 25.0;

    // 4. Mode ambiguity (25 pts): unclear mode = harmonically adventurous
    // Library range: 0.064-0.22, avg 0.131 (lower = more ambiguous)
    let mode_clar = a.mode_clarity.unwrap_or(0.15);
    let mode_ambig = (1.0 - (mode_clar - 0.05) / 0.20).clamp(0.0, 1.0);
    let mode_contrib = mode_ambig * 25.0;

    (flat_contrib + pitch_contrib + trans_contrib + mode_contrib).clamp(0.0, 100.0)
}

// ── Transcendence Score (0-100) ───────────────────────────────────────
// The "peak experience" composite — everything comes together.
// Uses peak intensity, crest factor, groove×energy synergy, spectral richness.
// Requires >= 60s of audio — shorter clips produce extreme variance/peak ratios
// that inflate the score unreliably.
fn transcendence_score(a: &NewAnalysis) -> f64 {
    let duration = a.duration.unwrap_or(0.0);
    if duration < 60.0 {
        return 0.0;
    }

    // 1. Peak intensity ratio (25 pts): how far peaks exceed average
    // peak_energy range: 0-0.38, energy_level range: 0-0.51
    let peak_e = a.peak_energy.unwrap_or(0.0);
    let avg_e = a.energy_level.unwrap_or(0.001).max(0.001);
    let peak_ratio = peak_e / avg_e;
    let peak_norm = ((peak_ratio - 0.05) / 0.8).clamp(0.0, 1.0);
    let peak_contrib = peak_norm * 25.0;

    // 2. Dynamic peak character (25 pts): crest factor
    // Library range: 3.15-35.6, avg 9.56
    let crest = a.crest_factor.unwrap_or(5.0);
    let crest_norm = ((crest - 3.0) / 25.0).clamp(0.0, 1.0);
    let crest_contrib = crest_norm * 25.0;

    // 3. Groove × Energy synergy (30 pts): transcendence needs both
    let groove = groove_score(a);
    let energy = energy_score(a);
    let groove_energy = (groove / 100.0) * (energy / 100.0);
    let synergy_contrib = groove_energy.sqrt() * 30.0;

    // 4. Spectral richness (20 pts): high flux = lots of spectral activity
    // Library range: 0.73-69, avg 23
    let flux = a.spectral_flux_mean.unwrap_or(0.0);
    let flux_norm = (flux / 50.0).clamp(0.0, 1.0);
    let flux_contrib = flux_norm * 20.0;

    (peak_contrib + crest_contrib + synergy_contrib + flux_contrib).clamp(0.0, 100.0)
}

// ── Valence Score (0-100) ──────────────────────────────────────────────
// Russell circumplex horizontal axis: happy (high) ↔ sad (low).
// Inputs: mode (major/minor), tempo, spectral centroid brightness, harmonic simplicity.
fn valence_score(a: &NewAnalysis) -> f64 {
    // Mode component (30 pts): major → happy, minor → sad
    let mode_val = match &a.estimated_key {
        Some(k) if k.contains("major") => 1.0,
        Some(k) if k.contains("minor") => 0.0,
        _ => 0.5, // Unknown/ambiguous
    };
    let mode_contrib = mode_val * 30.0;

    // Tempo component (25 pts): faster → happier. 60 bpm = 0, 180 bpm = 1
    let tempo = a.tempo_bpm.unwrap_or(120.0);
    let tempo_norm = ((tempo - 60.0) / 120.0).clamp(0.0, 1.0);
    let tempo_contrib = tempo_norm * 25.0;

    // Brightness component (25 pts): brighter timbre → happier
    let centroid = a.spectral_centroid_mean.unwrap_or(0.0);
    let bright_norm = ((centroid - 500.0) / 4500.0).clamp(0.0, 1.0);
    let bright_contrib = bright_norm * 25.0;

    // Simplicity component (20 pts): simpler harmony → happier
    let complexity = a.harmonic_complexity.unwrap_or(0.5);
    let simplicity = 1.0 - complexity.clamp(0.0, 1.0);
    let simple_contrib = simplicity * 20.0;

    (mode_contrib + tempo_contrib + bright_contrib + simple_contrib).clamp(0.0, 100.0)
}

// ── Arousal Score (0-100) ──────────────────────────────────────────────
// Russell circumplex vertical axis: energetic (high) ↔ calm (low).
// Inputs: energy level, tempo, spectral flux, loudness.
fn arousal_score(a: &NewAnalysis) -> f64 {
    // Energy component (30 pts)
    let energy = a.energy_level.unwrap_or(0.0);
    let energy_contrib = energy.clamp(0.0, 1.0) * 30.0;

    // Tempo component (25 pts): faster → more aroused. 60 bpm = 0, 180 bpm = 1
    let tempo = a.tempo_bpm.unwrap_or(120.0);
    let tempo_norm = ((tempo - 60.0) / 120.0).clamp(0.0, 1.0);
    let tempo_contrib = tempo_norm * 25.0;

    // Spectral flux component (20 pts): more change → more arousal
    let flux = a.spectral_flux_mean.unwrap_or(0.0);
    let flux_norm = (flux / 50.0).clamp(0.0, 1.0);
    let flux_contrib = flux_norm * 20.0;

    // Loudness component (25 pts): -40 LUFS = 0, 0 LUFS = 1
    let lufs = a.lufs_integrated.unwrap_or(-40.0);
    let lufs_norm = ((lufs + 40.0) / 40.0).clamp(0.0, 1.0);
    let lufs_contrib = lufs_norm * 25.0;

    (energy_contrib + tempo_contrib + flux_contrib + lufs_contrib).clamp(0.0, 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_analysis() -> NewAnalysis {
        NewAnalysis {
            track_id: 1,
            duration: Some(300.0),
            sample_rate: Some(44100),
            channels: Some(2),
            peak_amplitude: Some(0.95),
            rms_level: Some(0.3),
            dynamic_range: Some(15.0),
            spectral_centroid_mean: Some(2000.0),
            spectral_centroid_std: Some(500.0),
            spectral_flux_mean: Some(10.0),
            spectral_flux_std: Some(15.0),
            spectral_rolloff_mean: Some(4000.0),
            spectral_rolloff_std: Some(800.0),
            spectral_flatness_mean: None, spectral_flatness_std: None,
            spectral_bandwidth_mean: None, spectral_bandwidth_std: None,
            zcr_mean: None, zcr_std: None,
            sub_band_bass_mean: None, sub_band_bass_std: None,
            sub_band_mid_mean: None, sub_band_mid_std: None,
            sub_band_high_mean: None, sub_band_high_std: None,
            sub_band_presence_mean: None, sub_band_presence_std: None,
            mfcc_0_mean: None, mfcc_0_std: None,
            mfcc_1_mean: None, mfcc_1_std: None,
            mfcc_2_mean: None, mfcc_2_std: None,
            mfcc_3_mean: None, mfcc_3_std: None,
            mfcc_4_mean: None, mfcc_4_std: None,
            mfcc_5_mean: None, mfcc_5_std: None,
            mfcc_6_mean: None, mfcc_6_std: None,
            mfcc_7_mean: None, mfcc_7_std: None,
            mfcc_8_mean: None, mfcc_8_std: None,
            mfcc_9_mean: None, mfcc_9_std: None,
            mfcc_10_mean: None, mfcc_10_std: None,
            mfcc_11_mean: None, mfcc_11_std: None,
            mfcc_12_mean: None, mfcc_12_std: None,
            tempo_bpm: Some(120.0),
            beat_count: Some(600),
            onset_count: Some(1200),
            tempo_stability: Some(0.8),
            rhythmic_complexity: Some(4.0),
            mean_pitch: Some(440.0),
            pitch_range_low: Some(100.0),
            pitch_range_high: Some(2000.0),
            pitch_stability: Some(0.6),
            dominant_pitch: Some(440.0),
            vibrato_presence: None,
            vibrato_rate: None,
            pitch_confidence_mean: Some(0.7),
            lufs_integrated: Some(-14.0),
            loudness_range: Some(10.0),
            true_peak_dbfs: Some(-1.0),
            crest_factor: Some(1.4),
            energy_level: Some(0.8),
            estimated_key: Some("A minor".to_string()),
            key_confidence: Some(0.7),
            tonality: Some(0.6),
            harmonic_complexity: Some(0.5),
            chord_count: Some(6),
            chord_change_rate: Some(8.0),
            mode_clarity: Some(0.7),
            key_alternatives_count: Some(2),
            time_sig_numerator: Some(4),
            time_sig_denominator: Some(4),
            chroma_vector: None,
            recording_quality_score: Some(0.8),
            snr_db: Some(45.0),
            clipping_ratio: Some(0.0),
            noise_floor_db: Some(-50.0),
            segment_count: Some(10),
            temporal_complexity: Some(0.5),
            coherence_score: Some(0.7),
            energy_shape: Some("Peak".to_string()),
            peak_energy: Some(0.9),
            energy_variance: Some(0.05),
            tension_build_count: Some(3),
            tension_release_count: Some(2),
            repetition_count: Some(2),
            repetition_similarity: Some(0.6),
            solo_section_count: Some(1),
            solo_section_ratio: Some(0.3),
            transition_count: Some(4),
            classification_music_score: Some(0.95),
            hnr: Some(12.0),
            valence_score: None,
            arousal_score: None,
            energy_score: None,
            intensity_score: None,
            groove_score: None,
            improvisation_score: None,
            tightness_score: None,
            build_quality_score: None,
            exploratory_score: None,
            transcendence_score: None,
        }
    }

    #[test]
    fn test_all_scores_in_range() {
        let mut a = base_analysis();
        compute_jam_scores_from_scalars(&mut a, None);

        for (name, val) in [
            ("energy", a.energy_score), ("intensity", a.intensity_score),
            ("groove", a.groove_score), ("improvisation", a.improvisation_score),
            ("tightness", a.tightness_score), ("build_quality", a.build_quality_score),
            ("exploratory", a.exploratory_score), ("transcendence", a.transcendence_score),
            ("valence", a.valence_score), ("arousal", a.arousal_score),
        ] {
            let v = val.unwrap();
            assert!((0.0..=100.0).contains(&v), "{name}={v}");
        }
    }

    #[test]
    fn test_silence_scores_low() {
        let mut a = base_analysis();
        // Zero all signal-dependent features
        a.rms_level = Some(0.0);
        a.lufs_integrated = Some(-60.0);
        a.spectral_centroid_mean = Some(0.0);
        a.spectral_centroid_std = Some(0.0);
        a.spectral_flux_mean = Some(0.0);
        a.spectral_flux_std = Some(0.0);
        a.spectral_flatness_std = Some(0.0);
        a.dynamic_range = Some(0.0);
        a.loudness_range = Some(0.0);
        a.tempo_stability = Some(0.0);
        a.rhythmic_complexity = Some(0.0);
        a.beat_count = Some(0);
        a.onset_count = Some(0);
        a.chord_count = Some(0);
        a.transition_count = Some(0);
        a.repetition_similarity = Some(1.0);
        a.crest_factor = Some(1.0);
        a.energy_variance = Some(0.0);
        a.energy_level = Some(0.0);
        a.peak_energy = Some(0.0);
        a.pitch_stability = Some(0.0);
        a.pitch_confidence_mean = Some(0.0);
        a.sub_band_bass_mean = Some(0.0);
        a.sub_band_bass_std = Some(0.0);

        assert!(energy_score(&a) < 10.0, "energy={}", energy_score(&a));
        assert!(intensity_score(&a) < 10.0, "intensity={}", intensity_score(&a));
        assert!(groove_score(&a) < 10.0, "groove={}", groove_score(&a));
    }

    // ── Arc detection tests ──────────────────────────────────────────

    #[test]
    fn test_detect_single_build_arc() {
        // Steady build from 0.1 to 0.9 over 8 windows then drop
        let windows = vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 0.3];
        let arcs = detect_arcs(&windows);
        assert_eq!(arcs.len(), 1, "expected 1 arc, got {}", arcs.len());
        assert!(arcs[0].peak_energy > 0.85);
        assert!(arcs[0].trough_energy < 0.15);
    }

    #[test]
    fn test_detect_multiple_arcs() {
        // Two build→peak→release cycles
        let windows = vec![
            0.1, 0.3, 0.5, 0.7, 0.9, // first build
            0.3, 0.2,                   // drop
            0.2, 0.4, 0.6, 0.8, 1.0,   // second build
            0.4,                        // drop
        ];
        let arcs = detect_arcs(&windows);
        assert!(arcs.len() >= 2, "expected >= 2 arcs, got {}", arcs.len());
    }

    #[test]
    fn test_detect_no_arcs_flat() {
        // Flat energy — no build arcs
        let windows = vec![0.5, 0.5, 0.5, 0.5, 0.5, 0.5];
        let arcs = detect_arcs(&windows);
        assert!(arcs.is_empty(), "flat energy should have no arcs");
    }

    #[test]
    fn test_detect_arcs_with_dip_tolerance() {
        // Build with a brief 10% dip (within 15% tolerance) that shouldn't break the arc
        let windows = vec![0.2, 0.4, 0.6, 0.55, 0.7, 0.8, 0.9, 0.3];
        let arcs = detect_arcs(&windows);
        assert_eq!(arcs.len(), 1, "dip within tolerance should not split arc");
        assert!(arcs[0].peak_energy > 0.85);
    }

    #[test]
    fn test_score_arc_full_range_long_build() {
        // Perfect arc: covers full track range, long duration, peaks well above average
        let arc = BuildArc {
            start_idx: 0,
            peak_idx: 9,
            trough_energy: 0.1,
            peak_energy: 0.9,
        };
        let score = score_arc(&arc, 0.4, 0.8);
        assert!(score > 80.0, "perfect arc should score > 80, got {score}");
    }

    #[test]
    fn test_score_arc_short_small() {
        // Weak arc: small magnitude, short duration
        let arc = BuildArc {
            start_idx: 0,
            peak_idx: 1,
            trough_energy: 0.4,
            peak_energy: 0.5,
        };
        let score = score_arc(&arc, 0.45, 0.8);
        assert!(score < 30.0, "weak arc should score < 30, got {score}");
    }

    #[test]
    fn test_build_quality_from_segments_multi_arc() {
        // 10-minute track with 3 build arcs (simulated as (time, energy) pairs)
        let duration = 600.0;
        let mut energies = Vec::new();
        // Arc 1: 0-120s, build from 0.1 to 0.8
        for t in 0..120 {
            let e = 0.1 + 0.7 * (t as f64 / 120.0);
            energies.push((t as f64, e));
        }
        // Drop: 120-180s
        for t in 120..180 {
            energies.push((t as f64, 0.2));
        }
        // Arc 2: 180-300s, build from 0.15 to 0.85
        for t in 180..300 {
            let e = 0.15 + 0.7 * ((t - 180) as f64 / 120.0);
            energies.push((t as f64, e));
        }
        // Drop: 300-360s
        for t in 300..360 {
            energies.push((t as f64, 0.2));
        }
        // Arc 3: 360-480s, build from 0.1 to 0.9
        for t in 360..480 {
            let e = 0.1 + 0.8 * ((t - 360) as f64 / 120.0);
            energies.push((t as f64, e));
        }
        // Outro: 480-600s
        for t in 480..600 {
            energies.push((t as f64, 0.3));
        }

        let score = build_quality_from_segments(&energies, duration);
        assert!(score > 50.0, "multi-arc 10-min jam should score > 50, got {score}");
    }

    #[test]
    fn test_build_quality_fallback_for_short_track() {
        let mut a = base_analysis();
        a.duration = Some(60.0); // short track
        // Even with segment data, should use fallback for < 90s
        let segments = vec![(0.0, 0.5), (30.0, 0.7), (60.0, 0.9)];
        let score = build_quality_score(&a, Some(&segments));
        let fallback = build_quality_score_fallback(&a);
        assert!((score - fallback).abs() < 0.01, "short track should use fallback");
    }

    #[test]
    fn test_build_quality_no_segments_uses_fallback() {
        let a = base_analysis();
        let score = build_quality_score(&a, None);
        let fallback = build_quality_score_fallback(&a);
        assert!((score - fallback).abs() < 0.01, "no segments should use fallback");
    }

    #[test]
    fn test_bucket_and_smooth_basic() {
        // 90 seconds of energy data, should produce 3 windows of 30s each
        let energies: Vec<(f64, f64)> = (0..90)
            .map(|t| (t as f64, if t < 30 { 0.2 } else if t < 60 { 0.5 } else { 0.8 }))
            .collect();
        let windows = bucket_and_smooth(&energies, 90.0);
        assert_eq!(windows.len(), 3, "90s / 30s = 3 windows");
        // After smoothing, middle window should be roughly average of all three
        assert!(windows[1] > 0.3 && windows[1] < 0.7, "middle should be blended");
    }
}
