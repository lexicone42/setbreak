use crate::db::models::NewAnalysis;
use ferrous_waves::analysis::engine::AnalysisResult;
use std::collections::HashSet;

/// Compute all jam-specific derived scores (0-100) and attach them to the analysis.
///
/// Takes both the flat NewAnalysis (for already-extracted scalars) and the full
/// ferrous-waves AnalysisResult (for rich structural/harmonic/temporal data).
pub fn compute_jam_scores(analysis: &mut NewAnalysis, result: &AnalysisResult) {
    analysis.energy_score = Some(energy_score(analysis));
    analysis.intensity_score = Some(intensity_score(analysis));
    analysis.groove_score = Some(groove_score(analysis));
    analysis.improvisation_score = Some(improvisation_score(analysis, result));
    analysis.tightness_score = Some(tightness_score(analysis, result));
    analysis.build_quality_score = Some(build_quality_score(analysis, result));
    analysis.exploratory_score = Some(exploratory_score(analysis, result));
    analysis.transcendence_score = Some(transcendence_score(analysis, result));
}

// ── Energy Score (0-100) ──────────────────────────────────────────────
// How "present" and powerful the music feels.
// Inputs: RMS level, LUFS, spectral centroid mean
fn energy_score(a: &NewAnalysis) -> f64 {
    let rms = a.rms_level.unwrap_or(0.0);
    let lufs = a.lufs_integrated.unwrap_or(-60.0);
    let centroid = a.spectral_centroid_mean.unwrap_or(0.0);

    // RMS contribution (0-1 range, mapped to 0-40 points)
    let rms_contrib = (rms * 40.0).min(40.0);

    // LUFS contribution: -60 LUFS = 0 points, -5 LUFS = 40 points
    let lufs_norm = ((lufs + 60.0) / 55.0).clamp(0.0, 1.0);
    let lufs_contrib = lufs_norm * 40.0;

    // Spectral centroid: higher = brighter = more energetic (up to 20 points)
    let centroid_norm = ((centroid - 500.0) / 4500.0).clamp(0.0, 1.0);
    let centroid_contrib = centroid_norm * 20.0;

    (rms_contrib + lufs_contrib + centroid_contrib).clamp(0.0, 100.0)
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
fn groove_score(a: &NewAnalysis) -> f64 {
    let stability = a.tempo_stability.unwrap_or(0.0);
    let complexity = a.rhythmic_complexity.unwrap_or(0.0);
    let duration = a.duration.unwrap_or(1.0);
    let beat_count = a.beat_count.unwrap_or(0) as f64;

    let stability_contrib = stability * 40.0;

    let complexity_sweet = if complexity < 1.0 {
        complexity
    } else if complexity <= 8.0 {
        1.0
    } else {
        (1.0 - (complexity - 8.0) / 20.0).max(0.0)
    };
    let complexity_contrib = complexity_sweet * 30.0;

    let beat_rate = beat_count / duration.max(1.0);
    let beat_sweet = if beat_rate < 0.5 {
        beat_rate * 2.0
    } else if beat_rate <= 3.0 {
        1.0
    } else {
        (1.0 - (beat_rate - 3.0) / 5.0).max(0.0)
    };
    let beat_contrib = beat_sweet * 30.0;

    (stability_contrib + complexity_contrib + beat_contrib).clamp(0.0, 100.0)
}

// ── Improvisation Score (0-100) ───────────────────────────────────────
// How much the music departs from repetitive structure.
// High score = fewer repetitions, more harmonic complexity, wider pitch exploration.
fn improvisation_score(a: &NewAnalysis, r: &AnalysisResult) -> f64 {
    // 1. Low repetition → high improvisation (30 pts)
    // Invert: many repetitions with high similarity = low improv
    let rep_count = r.segments.patterns.repetitions.len() as f64;
    let avg_similarity = if rep_count > 0.0 {
        r.segments.patterns.repetitions.iter().map(|p| p.similarity as f64).sum::<f64>() / rep_count
    } else {
        0.0
    };
    // More repetitions AND higher similarity → lower improv
    // 0 reps = full marks; 10+ high-similarity reps = 0
    let rep_penalty = (rep_count * avg_similarity / 10.0).clamp(0.0, 1.0);
    let non_repetition = (1.0 - rep_penalty) * 30.0;

    // 2. Harmonic complexity (20 pts) — entropy of pitch class distribution
    let harm = a.harmonic_complexity.unwrap_or(0.0);
    let harm_contrib = harm.clamp(0.0, 1.0) * 20.0;

    // 3. Chord variety (20 pts) — unique chords used
    let unique_chords = match &r.musical.chord_progression {
        Some(prog) => {
            let set: HashSet<&str> = prog.chords.iter().map(|c| c.chord.as_str()).collect();
            set.len() as f64
        }
        None => 0.0,
    };
    // 1-3 chords = low variety, 8+ = high
    let chord_variety = ((unique_chords - 1.0) / 7.0).clamp(0.0, 1.0);
    let chord_contrib = chord_variety * 20.0;

    // 4. Temporal complexity (15 pts) — structural variety across segments
    let temp_complex = a.temporal_complexity.unwrap_or(0.0);
    let temp_contrib = temp_complex.clamp(0.0, 1.0) * 15.0;

    // 5. Pitch exploration (15 pts) — wide pitch range relative to mean
    let range = a.pitch_range_high.unwrap_or(0.0) - a.pitch_range_low.unwrap_or(0.0);
    // 0-500 Hz range = low, 2000+ Hz = high exploration
    let pitch_norm = (range / 2000.0).clamp(0.0, 1.0);
    let pitch_contrib = pitch_norm * 15.0;

    (non_repetition + harm_contrib + chord_contrib + temp_contrib + pitch_contrib).clamp(0.0, 100.0)
}

// ── Tightness Score (0-100) ───────────────────────────────────────────
// How well the band is locked in together.
// High score = steady tempo, coherent segments, smooth spectral changes, strong beats.
fn tightness_score(a: &NewAnalysis, r: &AnalysisResult) -> f64 {
    // 1. Tempo stability (35 pts)
    let stability = a.tempo_stability.unwrap_or(0.0);
    let stability_contrib = stability.clamp(0.0, 1.0) * 35.0;

    // 2. Segment coherence (25 pts) — adjacent segments sound similar
    let coherence = a.coherence_score.unwrap_or(0.0);
    let coherence_contrib = coherence.clamp(0.0, 1.0) * 25.0;

    // 3. Spectral smoothness (20 pts) — low flux std = controlled changes
    // Invert: high flux std = chaotic = low tightness
    let flux_std = a.spectral_flux_std.unwrap_or(0.0);
    let smoothness = 1.0 - (flux_std / 50.0).clamp(0.0, 1.0);
    let smooth_contrib = smoothness * 20.0;

    // 4. Beat strength (20 pts) — how many onsets fall near detected beats
    // Proxy: ratio of beats to onsets. If beats ≈ onsets, the rhythm is well-defined.
    // If onsets >> beats, there's a lot of un-metered activity.
    let beats = r.temporal.beats.len() as f64;
    let onsets = r.temporal.onsets.len().max(1) as f64;
    let beat_ratio = (beats / onsets).clamp(0.0, 1.0);
    // Sweet spot: ratio 0.3-0.7 means structured but not trivial
    let beat_strength = if beat_ratio < 0.1 {
        beat_ratio * 5.0 // Very few beats relative to onsets = loose
    } else if beat_ratio <= 0.8 {
        1.0
    } else {
        // Almost all onsets are beats = might be a metronome-like simplicity
        0.8 + 0.2 * (1.0 - beat_ratio) / 0.2
    };
    let beat_contrib = beat_strength.clamp(0.0, 1.0) * 20.0;

    (stability_contrib + coherence_contrib + smooth_contrib + beat_contrib).clamp(0.0, 100.0)
}

// ── Build Quality Score (0-100) ───────────────────────────────────────
// How well the music builds to peaks — tension/release arcs.
fn build_quality_score(a: &NewAnalysis, r: &AnalysisResult) -> f64 {
    // 1. Energy profile shape (30 pts)
    // Shapes that suggest a build: Increasing, Peak, Complex
    let shape = format!("{:?}", r.segments.patterns.energy_profile.shape);
    let shape_score = match shape.as_str() {
        "Peak" => 1.0,          // Classic build-to-climax
        "Increasing" => 0.8,    // Steady build
        "Complex" => 0.7,       // Multi-peaked, interesting dynamics
        "Oscillating" => 0.5,   // Rising and falling, some build quality
        "Valley" => 0.3,        // Inverse build
        "Decreasing" => 0.2,    // Winding down
        "Flat" => 0.1,          // No dynamics
        _ => 0.4,
    };
    let shape_contrib = shape_score * 30.0;

    // 2. Tension build/release ratio (25 pts)
    // Good builds have tension build-ups followed by releases
    let builds = a.tension_build_count.unwrap_or(0) as f64;
    let releases = a.tension_release_count.unwrap_or(0) as f64;
    let tension_events = builds + releases;
    // Having both builds AND releases is key
    let tension_score = if tension_events < 1.0 {
        0.0 // No tension dynamics at all
    } else {
        let balance = (builds.min(releases) * 2.0) / tension_events; // 1.0 when equal
        let count_factor = (tension_events / 6.0).clamp(0.0, 1.0); // More events = more dynamic
        balance * 0.6 + count_factor * 0.4
    };
    let tension_contrib = tension_score * 25.0;

    // 3. Energy variance (20 pts) — needs dynamic range to have a build
    let e_var = a.energy_variance.unwrap_or(0.0);
    let var_norm = (e_var / 0.1).clamp(0.0, 1.0); // Typical variance 0-0.1 (energy is 0-1)
    let var_contrib = var_norm * 20.0;

    // 4. Transition smoothness (15 pts)
    let transitions = &r.segments.transitions;
    let smooth_ratio = if transitions.is_empty() {
        0.5 // No transitions detected, neutral
    } else {
        let smooth_count = transitions.iter().filter(|t| {
            let tt = format!("{:?}", t.transition_type);
            tt == "Smooth" || tt == "Crossfade" || tt == "BuildUp"
        }).count() as f64;
        smooth_count / transitions.len() as f64
    };
    let transition_contrib = smooth_ratio * 15.0;

    // 5. Structural progression (10 pts)
    // Bonus if there's an identifiable intro → build → peak structure
    let has_intro = r.segments.structure.iter().any(|s| format!("{:?}", s.section_type) == "Intro");
    let has_solo = r.segments.structure.iter().any(|s| {
        let t = format!("{:?}", s.section_type);
        t == "Solo" || t == "Instrumental"
    });
    let has_outro = r.segments.structure.iter().any(|s| format!("{:?}", s.section_type) == "Outro");
    let structure_score = (has_intro as u8 + has_solo as u8 + has_outro as u8) as f64 / 3.0;
    let structure_contrib = structure_score * 10.0;

    (shape_contrib + tension_contrib + var_contrib + transition_contrib + structure_contrib).clamp(0.0, 100.0)
}

// ── Exploratory Score (0-100) ─────────────────────────────────────────
// How much musical territory is covered — harmonic, timbral, structural.
fn exploratory_score(a: &NewAnalysis, r: &AnalysisResult) -> f64 {
    // 1. Pitch range breadth (20 pts)
    let range = a.pitch_range_high.unwrap_or(0.0) - a.pitch_range_low.unwrap_or(0.0);
    let pitch_norm = (range / 2000.0).clamp(0.0, 1.0);
    let pitch_contrib = pitch_norm * 20.0;

    // 2. Chord variety (20 pts) — unique chords / root notes
    let unique_chords = match &r.musical.chord_progression {
        Some(prog) => {
            let set: HashSet<&str> = prog.chords.iter().map(|c| c.chord.as_str()).collect();
            set.len() as f64
        }
        None => 0.0,
    };
    let chord_norm = ((unique_chords - 1.0) / 10.0).clamp(0.0, 1.0);
    let chord_contrib = chord_norm * 20.0;

    // 3. Key ambiguity (20 pts) — low confidence + close alternatives = exploring
    let key_conf = a.key_confidence.unwrap_or(1.0);
    let alt_count = a.key_alternatives_count.unwrap_or(0) as f64;
    // Low confidence + many alternatives = high exploration
    let key_ambiguity = (1.0 - key_conf) * 0.5 + (alt_count / 6.0).clamp(0.0, 0.5);
    let key_contrib = key_ambiguity.clamp(0.0, 1.0) * 20.0;

    // 4. Harmonic complexity (20 pts)
    let harm = a.harmonic_complexity.unwrap_or(0.0);
    let harm_contrib = harm.clamp(0.0, 1.0) * 20.0;

    // 5. Structural transitions (20 pts) — more transitions = more exploration
    let transition_count = r.segments.transitions.len() as f64;
    // Count of key changes and tempo changes specifically
    let modulation_count = r.segments.transitions.iter().filter(|t| {
        let tt = format!("{:?}", t.transition_type);
        tt == "KeyChange" || tt == "TempoChange"
    }).count() as f64;
    // General transitions: 0-10 range
    let trans_norm = (transition_count / 10.0).clamp(0.0, 0.5);
    // Key/tempo changes are extra exploratory
    let mod_norm = (modulation_count / 3.0).clamp(0.0, 0.5);
    let trans_contrib = (trans_norm + mod_norm) * 20.0;

    (pitch_contrib + chord_contrib + key_contrib + harm_contrib + trans_contrib).clamp(0.0, 100.0)
}

// ── Transcendence Score (0-100) ───────────────────────────────────────
// The "peak experience" composite — the moments where everything comes together.
// Sustained high energy + groove + harmonic richness + tension peaks.
fn transcendence_score(a: &NewAnalysis, r: &AnalysisResult) -> f64 {
    // 1. Peak energy magnitude (30 pts) — how high do the peaks go?
    let peak_e = r.segments.patterns.energy_profile.peaks.iter()
        .map(|p| p.1 as f64)
        .fold(0.0f64, f64::max);
    let avg_e = r.segments.patterns.energy_profile.average as f64;
    // Peak relative to average — transcendence needs standout moments
    let peak_ratio = if avg_e > 0.001 { peak_e / avg_e } else { 1.0 };
    // ratio of 2.0+ = strong peaks
    let peak_norm = ((peak_ratio - 1.0) / 2.0).clamp(0.0, 1.0);
    let peak_contrib = peak_norm * 30.0;

    // 2. Sustained high-energy sections (20 pts)
    // Count segments with above-average energy
    let total_segs = r.segments.segments.len().max(1) as f64;
    let high_energy_segs = r.segments.segments.iter()
        .filter(|s| s.energy as f64 > avg_e * 1.2)
        .count() as f64;
    let sustained_ratio = high_energy_segs / total_segs;
    let sustained_contrib = sustained_ratio.clamp(0.0, 1.0) * 20.0;

    // 3. Peak tension reached (20 pts)
    let max_tension = r.segments.patterns.tension_profile.iter()
        .map(|t| t.tension as f64)
        .fold(0.0f64, f64::max);
    let tension_contrib = max_tension.clamp(0.0, 1.0) * 20.0;

    // 4. Groove during high energy (15 pts)
    // If tempo stability is high AND we have energy, the groove is locked in
    let groove = groove_score(a);
    let energy = energy_score(a);
    // Both need to be high for this to matter
    let groove_energy = (groove / 100.0) * (energy / 100.0);
    let groove_contrib = groove_energy.sqrt() * 15.0; // sqrt to not penalize too harshly

    // 5. Harmonic richness during peaks (15 pts)
    let harm = a.harmonic_complexity.unwrap_or(0.0);
    let tonality = a.tonality.unwrap_or(0.0);
    // Strong tonal center + complex harmonics = rich
    let richness = harm * (0.3 + 0.7 * tonality);
    let richness_contrib = richness.clamp(0.0, 1.0) * 15.0;

    (peak_contrib + sustained_contrib + tension_contrib + groove_contrib + richness_contrib).clamp(0.0, 100.0)
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
        // Energy/intensity/groove only need NewAnalysis
        a.energy_score = Some(energy_score(&a));
        a.intensity_score = Some(intensity_score(&a));
        a.groove_score = Some(groove_score(&a));

        assert!((0.0..=100.0).contains(&a.energy_score.unwrap()), "energy={}", a.energy_score.unwrap());
        assert!((0.0..=100.0).contains(&a.intensity_score.unwrap()), "intensity={}", a.intensity_score.unwrap());
        assert!((0.0..=100.0).contains(&a.groove_score.unwrap()), "groove={}", a.groove_score.unwrap());
    }

    #[test]
    fn test_silence_scores_low() {
        let mut a = base_analysis();
        a.rms_level = Some(0.0);
        a.lufs_integrated = Some(-60.0);
        a.spectral_centroid_mean = Some(0.0);
        a.spectral_flux_std = Some(0.0);
        a.dynamic_range = Some(0.0);
        a.loudness_range = Some(0.0);
        a.tempo_stability = Some(0.0);
        a.rhythmic_complexity = Some(0.0);
        a.beat_count = Some(0);

        assert!(energy_score(&a) < 10.0);
        assert!(intensity_score(&a) < 10.0);
        assert!(groove_score(&a) < 10.0);
    }
}
