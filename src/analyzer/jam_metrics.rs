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
// v3: Rebalanced weights — onset sweet spot barely differentiates (95%+ of tracks
// score full marks), so reduced from 20→10 pts and gave 10 to flux CV (the real
// differentiator). This shifts avg from ~51 to ~45 and widens effective range.
// Note: Baker's Dozen Phish scores low because 24-bit recordings trigger 3x more
// onset detections (~25/sec vs normal ~9/sec) — a ferrous-waves issue to fix later.
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

    // 1. Onset rate sweet spot (10 pts): 6-10/sec is the groove zone
    // Library: avg 9.0/sec. Most tracks in 7-10 range → this barely differentiates,
    // but still correctly penalizes very sparse (Space) and over-detected tracks.
    let onset_rate = onset_count / duration;
    let onset_sweet = if onset_rate < 4.0 {
        onset_rate / 4.0
    } else if onset_rate < 6.0 {
        0.5 + 0.5 * (onset_rate - 4.0) / 2.0
    } else if onset_rate <= 10.0 {
        1.0
    } else if onset_rate <= 13.0 {
        1.0 - 0.5 * (onset_rate - 10.0) / 3.0
    } else {
        (0.5 - (onset_rate - 13.0) / 20.0).max(0.0)
    };
    let onset_contrib = onset_sweet.clamp(0.0, 1.0) * 10.0;

    // 2. Rhythmic consistency (40 pts): flux CV — strongest differentiator
    // Library flux_cv: avg 0.666, range 0.05-4.0
    // Tight grooves: 0.3-0.5, loose jams: 0.8-1.5+
    let flux_cv = if flux_mean > 0.5 { flux_std / flux_mean } else { 2.0 };
    let flux_score = (1.0 - flux_cv).clamp(0.0, 1.0);
    let flux_contrib = flux_score * 40.0;

    // 3. Bass steadiness (25 pts): groove lives in the bass
    // Low bass CV = locked-in bass pattern. Library bass_cv: avg 0.64, range 0.07-1.75
    let bass_cv = if bass_mean > 0.01 { bass_std / bass_mean } else { 1.5 };
    let bass_score = (1.0 - bass_cv * 0.7).clamp(0.0, 1.0);
    let bass_contrib = bass_score * 25.0;

    // 4. Pattern repetition (25 pts): groove IS repetition
    // Library: avg 0.90, range 0.80-0.999
    let rep_score = ((rep_sim - 0.85) / 0.15).clamp(0.0, 1.0);
    let rep_contrib = rep_score * 25.0;

    (onset_contrib + flux_contrib + bass_contrib + rep_contrib).clamp(0.0, 100.0)
}

// ── Improvisation Score (0-100) ───────────────────────────────────────
// How much the music departs from repetitive structure.
// Uses non-repetition, timbral variety, duration-normalized structural density,
// and tonal ambiguity (mode clarity).
fn improvisation_score(a: &NewAnalysis) -> f64 {
    // 1. Non-repetition (30 pts): low repetition similarity = improvised
    // Library range: 0.80-0.99, avg 0.90
    let rep_sim = a.repetition_similarity.unwrap_or(0.9);
    let non_rep = (1.0 - (rep_sim - 0.80) / 0.20).clamp(0.0, 1.0);
    let non_rep_contrib = non_rep * 30.0;

    // 2. Timbral variety (25 pts): high centroid std = exploring tonal space
    // Library range: 144-3595, avg 991
    let centroid_std = a.spectral_centroid_std.unwrap_or(500.0);
    let timbre_variety = ((centroid_std - 200.0) / 3000.0).clamp(0.0, 1.0);
    let timbre_contrib = timbre_variety * 25.0;

    // 3. Structural density (25 pts): transitions per minute, not raw count
    // Normalizes for duration so a 5-min song with 10 transitions scores
    // higher than a 20-min song with 10.
    let transitions = a.transition_count.unwrap_or(0) as f64;
    let duration_secs = a.duration.unwrap_or(180.0).max(30.0);
    let trans_per_min = transitions * 60.0 / duration_secs;
    let trans_norm = (trans_per_min / 5.0).clamp(0.0, 1.0);
    let trans_contrib = trans_norm * 25.0;

    // 4. Tonal ambiguity (20 pts): low mode clarity = harmonically wandering
    // Library range: 0.05-0.51, avg 0.15
    let mode_clarity = a.mode_clarity.unwrap_or(0.15);
    let tonal_ambiguity = (1.0 - (mode_clarity - 0.05) / 0.45).clamp(0.0, 1.0);
    let tonal_contrib = tonal_ambiguity * 20.0;

    (non_rep_contrib + timbre_contrib + trans_contrib + tonal_contrib).clamp(0.0, 100.0)
}

// ── Tightness Score (0-100) ───────────────────────────────────────────
// How well the band is locked in together — rhythmic precision, consistent
// timbral delivery, steady energy. Differentiates tight grooves (Sugar Magnolia,
// Fire on the Mountain) from free-form playing (Drums, Space, Dark Star jams).
//
// v3: Dropped pitch_stability (anti-correlated — ambient/drone scores higher than
// tight grooves) and beat-onset ratio (doesn't differentiate — 97% of tracks in
// the sweet spot). Added ZCR consistency and rhythmic presence.
fn tightness_score(a: &NewAnalysis) -> f64 {
    let duration = a.duration.unwrap_or(1.0).max(1.0);
    let onset_count = a.onset_count.unwrap_or(0) as f64;

    // 1. Flux consistency (30 pts): low CV = steady energy delivery — best differentiator
    // Library: avg CV 0.666, tight songs 0.4-0.7, Drums 0.9-1.7
    let flux_mean = a.spectral_flux_mean.unwrap_or(0.0);
    let flux_std = a.spectral_flux_std.unwrap_or(0.0);
    let flux_cv = if flux_mean > 0.5 { flux_std / flux_mean } else { 2.0 };
    // Map: 0.3 → 1.0, 1.1 → 0.0
    let flux_score = ((1.1 - flux_cv) / 0.8).clamp(0.0, 1.0);
    let flux_contrib = flux_score * 30.0;

    // 2. ZCR consistency (25 pts): low ZCR CV = consistent timbral character
    // Library: avg CV 0.488, tight songs 0.2-0.5, Drums 0.5-1.2
    let zcr_mean = a.zcr_mean.unwrap_or(0.0);
    let zcr_std = a.zcr_std.unwrap_or(0.0);
    let zcr_cv = if zcr_mean > 0.001 { zcr_std / zcr_mean } else { 1.5 };
    // Map: 0.2 → 1.0, 1.0 → 0.0
    let zcr_score = ((1.0 - zcr_cv) / 0.8).clamp(0.0, 1.0);
    let zcr_contrib = zcr_score * 25.0;

    // 3. Spectral flatness consistency (25 pts): low std = consistent tonal character
    // Library: avg 0.088, tight songs 0.025-0.10, Drums 0.05-0.18
    let flat_std = a.spectral_flatness_std.unwrap_or(0.15);
    // Map: 0.02 → 1.0, 0.16 → 0.0
    let flat_score = ((0.16 - flat_std) / 0.14).clamp(0.0, 1.0);
    let flat_contrib = flat_score * 25.0;

    // 4. Rhythmic presence (20 pts): steady onset rate in groove zone = tight
    // Penalizes Space/ambient (sparse onsets) and chaotic playing (too many).
    // Library: avg 9/sec, groove zone 5-11/sec
    let onset_rate = if duration > 0.0 { onset_count / duration } else { 0.0 };
    let rhythm_score = if onset_rate < 3.0 {
        onset_rate / 3.0 // very sparse = not tight
    } else if onset_rate < 5.0 {
        0.5 + 0.5 * (onset_rate - 3.0) / 2.0 // ramping up
    } else if onset_rate <= 11.0 {
        1.0 // groove zone
    } else if onset_rate <= 14.0 {
        1.0 - 0.5 * (onset_rate - 11.0) / 3.0 // tapering
    } else {
        (0.5 - (onset_rate - 14.0) / 20.0).max(0.0) // chaotic
    };
    let rhythm_contrib = rhythm_score.clamp(0.0, 1.0) * 20.0;

    (flux_contrib + zcr_contrib + flat_contrib + rhythm_contrib).clamp(0.0, 100.0)
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
// v2: Added duration gating — short clips (<60s) produce spurious high scores
// because noise/banter has wide spectral variance and low pitch confidence.
fn exploratory_score(a: &NewAnalysis) -> f64 {
    let duration = a.duration.unwrap_or(1.0).max(1.0);

    // Duration gate: <60s tracks can't meaningfully demonstrate exploration
    if duration < 60.0 {
        return 0.0;
    }
    // Gentle ramp 60-120s to avoid cliff edge
    let duration_factor = if duration < 120.0 {
        (duration - 60.0) / 60.0
    } else {
        1.0
    };

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

    let raw = (flat_contrib + pitch_contrib + trans_contrib + mode_contrib).clamp(0.0, 100.0);
    (raw * duration_factor).clamp(0.0, 100.0)
}

// ── Transcendence Score (0-100) ───────────────────────────────────────
// The "peak experience" composite — sustained intensity where everything clicks.
// Measures: overall power, peak moments, groove×energy synergy, spectral richness.
//
// v2: Replaced crest_factor (anti-correlated — peaky/sparse tracks score high,
// sustained jams score low). Recalibrated peak_ratio. Removed sqrt compression
// on synergy. Uses bottleneck approach: both groove AND energy must be high.
// Requires >= 90s of audio. Ramps 90s→240s so brief high-energy moments
// (like NYE Auld Lang Syne at 96s) don't dominate over sustained jams.
fn transcendence_score(a: &NewAnalysis) -> f64 {
    let duration = a.duration.unwrap_or(0.0);
    if duration < 90.0 {
        return 0.0;
    }
    // Gentle ramp: 90s → 0%, 240s → 100%
    let duration_factor = if duration < 240.0 {
        (duration - 90.0) / 150.0
    } else {
        1.0
    };

    // 1. Sustained energy (25 pts): overall power of the track
    // energy_level: library 0.0-0.56, avg 0.375
    let energy_level = a.energy_level.unwrap_or(0.0);
    let energy_norm = (energy_level / 0.45).clamp(0.0, 1.0);
    let energy_contrib = energy_norm * 25.0;

    // 2. Peak intensity ratio (20 pts): how far peaks exceed average
    // Library: 0-2.18, avg 0.21. Top jams: 0.3-0.5.
    let peak_e = a.peak_energy.unwrap_or(0.0);
    let avg_e = a.energy_level.unwrap_or(0.001).max(0.001);
    let peak_ratio = peak_e / avg_e;
    // Map: 0 → 0, 0.6 → 1.0
    let peak_norm = (peak_ratio / 0.6).clamp(0.0, 1.0);
    let peak_contrib = peak_norm * 20.0;

    // 3. Groove × Energy synergy (30 pts): transcendence needs BOTH —
    // use bottleneck approach (weakest link limits the score)
    let groove = groove_score(a);
    let energy = energy_score(a);
    // min(groove, energy) — the lower one constrains transcendence.
    // A high-energy track with no groove (Drums) can't be transcendent,
    // and a groovy quiet track can't either.
    let bottleneck = groove.min(energy);
    // Map: 0 → 0, 70 → full marks (groove/energy rarely exceed 70)
    let synergy_norm = (bottleneck / 70.0).clamp(0.0, 1.0);
    let synergy_contrib = synergy_norm * 30.0;

    // 4. Spectral richness (25 pts): full-band participation, complex overtones
    // Library: 0.5-90, avg 29
    let flux = a.spectral_flux_mean.unwrap_or(0.0);
    let flux_norm = (flux / 60.0).clamp(0.0, 1.0);
    let flux_contrib = flux_norm * 25.0;

    let raw = (energy_contrib + peak_contrib + synergy_contrib + flux_contrib).clamp(0.0, 100.0);
    (raw * duration_factor).clamp(0.0, 100.0)
}

// ── Valence Score (0-100) ──────────────────────────────────────────────
// Russell circumplex horizontal axis: happy/positive (high) ↔ sad/dark (low).
//
// v2: Complete redesign. Previous formula used mode (97.6% minor — modal music
// misclassified), harmonic_complexity (degenerate at 0.99), and tempo_bpm
// (only 28 distinct values, clustered at 190). Those ferrous-waves features need
// fixing at the source. This version uses acoustic correlates that actually
// differentiate: brightness, timbral balance, rhythmic activity, pitch clarity.
fn valence_score(a: &NewAnalysis) -> f64 {
    let duration = a.duration.unwrap_or(1.0).max(1.0);

    // 1. Brightness (30 pts): spectral centroid — strongest single proxy for valence
    // Brighter timbre correlates with perceived positivity.
    // Library: 1190-11067, avg 3764, 3455 distinct values — excellent spread
    let centroid = a.spectral_centroid_mean.unwrap_or(0.0);
    let bright_norm = ((centroid - 1500.0) / 6000.0).clamp(0.0, 1.0);
    let bright_contrib = bright_norm * 30.0;

    // 2. Treble balance (25 pts): high+presence energy vs bass energy
    // Higher ratio = brighter, more "open" sounding = more positive
    // Low ratio = bass-heavy, darker = lower valence
    let bass = a.sub_band_bass_mean.unwrap_or(0.1);
    let high = a.sub_band_high_mean.unwrap_or(0.0);
    let presence = a.sub_band_presence_mean.unwrap_or(0.0);
    let treble_ratio = (high + presence) / (bass + 0.01);
    // Library: ratio typically 1-8, avg ~4
    let treble_norm = ((treble_ratio - 1.0) / 6.0).clamp(0.0, 1.0);
    let treble_contrib = treble_norm * 25.0;

    // 3. Rhythmic activity (25 pts): faster onset rate = more upbeat/positive
    // Library: avg 9/sec, range 1-28. Space/ambient = low, uptempo = high.
    let onset_rate = a.onset_count.unwrap_or(0) as f64 / duration;
    let rhythm_norm = ((onset_rate - 2.0) / 10.0).clamp(0.0, 1.0);
    let rhythm_contrib = rhythm_norm * 25.0;

    // 4. Pitch clarity (20 pts): clear melodic content = more tonal/positive
    // Library: 0-0.949, avg 0.577, 87 distinct — good spread
    let pitch_conf = a.pitch_confidence_mean.unwrap_or(0.5);
    let pitch_contrib = pitch_conf.clamp(0.0, 1.0) * 20.0;

    (bright_contrib + treble_contrib + rhythm_contrib + pitch_contrib).clamp(0.0, 100.0)
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
            loudness_std: None, peak_loudness: None,
            spectral_flux_skewness: None, spectral_centroid_slope: None,
            energy_buildup_ratio: None,
            bass_treble_ratio_mean: None, bass_treble_ratio_std: None,
            onset_density_std: None, loudness_buildup_slope: None,
            peak_energy_time: None,
            pitch_contour_std: None, pitch_clarity_mean: None,
            pitched_frame_ratio: None,
            mfcc_flux_mean: None, onset_interval_entropy: None,
            spectral_centroid_kurtosis: None,
            bass_energy_slope: None, spectral_bandwidth_slope: None,
            loudness_dynamic_spread: None,
            beat_regularity: None,
            peak_tension: None, tension_range: None,
            energy_peak_count: None, energy_valley_depth_mean: None,
            rhythmic_periodicity_strength: None,
            spectral_loudness_correlation: None,
            spectral_skewness_mean: None, spectral_kurtosis_mean: None,
            spectral_entropy_mean: None, spectral_entropy_std: None,
            spectral_slope_mean: None, spectral_contrast_json: None,
            sub_band_flux_bass_mean: None, sub_band_flux_bass_std: None,
            sub_band_flux_mid_mean: None, sub_band_flux_high_mean: None,
            tonnetz_json: None, tonnetz_flux_mean: None, chroma_flux_mean: None,
            beat_pattern_json: None, syncopation: None,
            pulse_clarity: None, offbeat_ratio: None,
            spectral_spread_mean: None, spectral_spread_std: None,
            spectral_crest_mean: None, spectral_crest_std: None,
            roughness_mean: None, roughness_std: None,
            mfcc_delta_mean_json: None, mfcc_delta_delta_mean_json: None,
            stereo_width_mean: None, stereo_width_std: None,
            attack_time_mean: None, attack_time_std: None,
            decay_time_mean: None, decay_time_std: None,
            onset_strength_mean: None, onset_strength_std: None,
            onset_strength_skewness: None,
            swing_ratio: None,
            microtiming_deviation_mean: None, microtiming_deviation_std: None,
            microtiming_bias: None,
            temporal_modulation_json: None,
            chroma_self_similarity_bandwidth: None,
            harmonic_percussive_ratio: None, chromagram_entropy: None,
            spectral_contrast_slope: None, spectral_contrast_range: None,
            onset_strength_contour_json: None, section_diversity_score: None,
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
