use crate::db::models::{
    ChordEvent, NewAnalysis, SegmentRecord, TensionPointRecord, TransitionRecord,
};
use ferrous_waves::analysis::engine::AnalysisResult;
use ferrous_waves::analysis::pitch::PitchFrame;
use std::collections::HashSet;

/// Full extraction result: flat scalars + relational detail records.
pub struct ExtractionResult {
    pub analysis: NewAnalysis,
    pub chords: Vec<ChordEvent>,
    pub segments: Vec<SegmentRecord>,
    pub tension_points: Vec<TensionPointRecord>,
    pub transitions: Vec<TransitionRecord>,
}

/// Extract all available features from a ferrous-waves AnalysisResult.
pub fn extract(track_id: i64, r: &AnalysisResult) -> ExtractionResult {
    // Aggregate spectral vectors into mean/std
    let (centroid_mean, centroid_std) = mean_std(&r.spectral.spectral_centroid);
    let (flux_mean, flux_std) = mean_std(&r.spectral.spectral_flux);
    let (rolloff_mean, rolloff_std) = mean_std(&r.spectral.spectral_rolloff);
    let (flatness_mean, flatness_std) = mean_std(&r.spectral.spectral_flatness);
    let (bandwidth_mean, bandwidth_std) = mean_std(&r.spectral.spectral_bandwidth);
    let (zcr_mean, zcr_std) = mean_std(&r.spectral.zero_crossing_rate);
    let (sub_bass_mean, sub_bass_std) = mean_std(&r.spectral.sub_band_energy_bass);
    let (sub_mid_mean, sub_mid_std) = mean_std(&r.spectral.sub_band_energy_mid);
    let (sub_high_mean, sub_high_std) = mean_std(&r.spectral.sub_band_energy_high);
    let (sub_presence_mean, sub_presence_std) = mean_std(&r.spectral.sub_band_energy_presence);

    // MFCC: per-coefficient mean/std (13 coefficients)
    let mfcc_stats: Vec<(f64, f64)> = (0..13)
        .map(|i| {
            if i < r.spectral.mfcc.len() {
                mean_std(&r.spectral.mfcc[i])
            } else {
                (0.0, 0.0)
            }
        })
        .collect();

    // Pitch confidence: mean of all frame confidences
    let pitch_confidence_mean = if r.pitch.pitch_track.frames.is_empty() {
        None
    } else {
        let sum: f64 = r.pitch.pitch_track.frames.iter().map(|f| f.confidence as f64).sum();
        Some(sum / r.pitch.pitch_track.frames.len() as f64)
    };

    // Vibrato
    let (vibrato_presence, vibrato_rate) = match &r.pitch.vibrato {
        Some(v) => (Some(v.presence as f64), Some(v.rate as f64)),
        None => (None, None),
    };

    // Chord extraction
    let (chord_count, chord_change_rate, chord_records) = extract_chords(track_id, r);

    // Musical extended
    let key_alternatives_count = Some(r.musical.key.alternatives.len() as i32);
    let (time_sig_num, time_sig_den) = match &r.musical.time_signature {
        Some(ts) => (Some(ts.numerator as i32), Some(ts.denominator as i32)),
        None => (None, None),
    };

    // Chroma vector as JSON
    let chroma_json = serde_json::to_string(&r.musical.chroma_vector.values).ok();

    // Energy profile
    let energy_shape = Some(format!("{:?}", r.segments.patterns.energy_profile.shape));
    let peak_energy = r.segments.patterns.energy_profile.peaks.first().map(|p| p.1 as f64);
    let energy_variance = Some(r.segments.patterns.energy_profile.variance as f64);

    // Tension counts
    let mut tension_build_count = 0i32;
    let mut tension_release_count = 0i32;
    let tension_records: Vec<TensionPointRecord> = r.segments.patterns.tension_profile
        .iter()
        .map(|tp| {
            let change_str = format!("{:?}", tp.change_type);
            if change_str.contains("Build") { tension_build_count += 1; }
            if change_str.contains("Release") { tension_release_count += 1; }
            TensionPointRecord {
                track_id,
                time: tp.time as f64,
                tension: tp.tension as f64,
                change_type: change_str,
            }
        })
        .collect();

    // Repetition patterns
    let repetition_count = Some(r.segments.patterns.repetitions.len() as i32);
    let repetition_similarity = if r.segments.patterns.repetitions.is_empty() {
        None
    } else {
        let sum: f64 = r.segments.patterns.repetitions.iter().map(|p| p.similarity as f64).sum();
        Some(sum / r.segments.patterns.repetitions.len() as f64)
    };

    // Solo/instrumental sections
    let duration = r.summary.duration as f64;
    let (solo_count, solo_ratio) = count_solo_sections(&r.segments.structure, duration);

    // Transitions
    let transition_records: Vec<TransitionRecord> = r.segments.transitions
        .iter()
        .map(|t| TransitionRecord {
            track_id,
            time: t.time as f64,
            transition_type: format!("{:?}", t.transition_type),
            strength: Some(t.strength as f64),
            duration: Some(t.duration as f64),
        })
        .collect();

    // Segments
    let segment_records = extract_segments(track_id, r);

    // Classification
    let classification_music_score = Some(r.classification.scores.music as f64);
    let hnr = Some(r.classification.features.hnr as f64);

    let analysis = NewAnalysis {
        track_id,

        // Summary
        duration: Some(duration),
        sample_rate: Some(r.summary.sample_rate as i32),
        channels: Some(r.summary.channels as i32),
        peak_amplitude: Some(r.summary.peak_amplitude as f64),
        rms_level: Some(r.summary.rms_level as f64),
        dynamic_range: Some(r.summary.dynamic_range as f64),

        // Spectral
        spectral_centroid_mean: Some(centroid_mean),
        spectral_centroid_std: Some(centroid_std),
        spectral_flux_mean: Some(flux_mean),
        spectral_flux_std: Some(flux_std),
        spectral_rolloff_mean: Some(rolloff_mean),
        spectral_rolloff_std: Some(rolloff_std),
        spectral_flatness_mean: Some(flatness_mean),
        spectral_flatness_std: Some(flatness_std),
        spectral_bandwidth_mean: Some(bandwidth_mean),
        spectral_bandwidth_std: Some(bandwidth_std),
        zcr_mean: Some(zcr_mean),
        zcr_std: Some(zcr_std),
        sub_band_bass_mean: Some(sub_bass_mean),
        sub_band_bass_std: Some(sub_bass_std),
        sub_band_mid_mean: Some(sub_mid_mean),
        sub_band_mid_std: Some(sub_mid_std),
        sub_band_high_mean: Some(sub_high_mean),
        sub_band_high_std: Some(sub_high_std),
        sub_band_presence_mean: Some(sub_presence_mean),
        sub_band_presence_std: Some(sub_presence_std),
        mfcc_0_mean: Some(mfcc_stats[0].0),
        mfcc_0_std: Some(mfcc_stats[0].1),
        mfcc_1_mean: Some(mfcc_stats[1].0),
        mfcc_1_std: Some(mfcc_stats[1].1),
        mfcc_2_mean: Some(mfcc_stats[2].0),
        mfcc_2_std: Some(mfcc_stats[2].1),
        mfcc_3_mean: Some(mfcc_stats[3].0),
        mfcc_3_std: Some(mfcc_stats[3].1),
        mfcc_4_mean: Some(mfcc_stats[4].0),
        mfcc_4_std: Some(mfcc_stats[4].1),
        mfcc_5_mean: Some(mfcc_stats[5].0),
        mfcc_5_std: Some(mfcc_stats[5].1),
        mfcc_6_mean: Some(mfcc_stats[6].0),
        mfcc_6_std: Some(mfcc_stats[6].1),
        mfcc_7_mean: Some(mfcc_stats[7].0),
        mfcc_7_std: Some(mfcc_stats[7].1),
        mfcc_8_mean: Some(mfcc_stats[8].0),
        mfcc_8_std: Some(mfcc_stats[8].1),
        mfcc_9_mean: Some(mfcc_stats[9].0),
        mfcc_9_std: Some(mfcc_stats[9].1),
        mfcc_10_mean: Some(mfcc_stats[10].0),
        mfcc_10_std: Some(mfcc_stats[10].1),
        mfcc_11_mean: Some(mfcc_stats[11].0),
        mfcc_11_std: Some(mfcc_stats[11].1),
        mfcc_12_mean: Some(mfcc_stats[12].0),
        mfcc_12_std: Some(mfcc_stats[12].1),

        // Temporal
        tempo_bpm: r.temporal.tempo.map(|t| t as f64),
        beat_count: Some(r.temporal.beats.len() as i32),
        onset_count: Some(r.temporal.onsets.len() as i32),
        tempo_stability: Some(r.temporal.tempo_stability as f64),
        rhythmic_complexity: Some(r.temporal.rhythmic_complexity as f64),

        // Pitch
        mean_pitch: r.pitch.mean_pitch.map(|p| p as f64),
        pitch_range_low: Some(r.pitch.pitch_range.0 as f64),
        pitch_range_high: Some(r.pitch.pitch_range.1 as f64),
        pitch_stability: Some(r.pitch.pitch_stability as f64),
        dominant_pitch: r.pitch.dominant_pitch.map(|p| p as f64),
        vibrato_presence,
        vibrato_rate,
        pitch_confidence_mean,

        // Perceptual
        lufs_integrated: Some(r.perceptual.loudness_lufs as f64),
        loudness_range: Some(r.perceptual.loudness_range as f64),
        true_peak_dbfs: Some(r.perceptual.true_peak_dbfs as f64),
        crest_factor: Some(r.perceptual.crest_factor as f64),
        energy_level: Some(r.perceptual.energy_level as f64),
        loudness_std: {
            let stl = &r.perceptual.short_term_loudness;
            if stl.is_empty() { None } else {
                let mean = stl.iter().sum::<f32>() / stl.len() as f32;
                let var = stl.iter().map(|&v| (v - mean).powi(2)).sum::<f32>() / stl.len() as f32;
                Some(var.sqrt() as f64)
            }
        },
        peak_loudness: {
            let ml = &r.perceptual.momentary_loudness;
            if ml.is_empty() { None } else {
                Some(ml.iter().cloned().fold(f32::NEG_INFINITY, f32::max) as f64)
            }
        },

        // Per-frame derived features
        spectral_flux_skewness: compute_skewness(&r.spectral.spectral_flux),
        spectral_centroid_slope: compute_linear_slope(&r.spectral.spectral_centroid),
        energy_buildup_ratio: compute_buildup_ratio(&r.spectral.spectral_flux),
        bass_treble_ratio_mean: compute_bass_treble_ratio_mean(
            &r.spectral.sub_band_energy_bass,
            &r.spectral.sub_band_energy_high,
            &r.spectral.sub_band_energy_presence,
        ),
        bass_treble_ratio_std: compute_bass_treble_ratio_std(
            &r.spectral.sub_band_energy_bass,
            &r.spectral.sub_band_energy_high,
            &r.spectral.sub_band_energy_presence,
        ),
        onset_density_std: compute_onset_density_std(&r.temporal.onsets, r.summary.duration),
        loudness_buildup_slope: compute_linear_slope_f32(&r.perceptual.short_term_loudness),
        peak_energy_time: compute_peak_time(&r.perceptual.short_term_loudness),

        // Pitch-derived per-frame features
        pitch_contour_std: compute_pitch_contour_std(&r.pitch.pitch_track.frames),
        pitch_clarity_mean: compute_pitch_clarity_mean(&r.pitch.pitch_track.frames),
        pitched_frame_ratio: compute_pitched_frame_ratio(&r.pitch.pitch_track.frames),

        // Creative per-frame derivations
        mfcc_flux_mean: compute_mfcc_flux_mean(&r.spectral.mfcc),
        onset_interval_entropy: compute_onset_interval_entropy(&r.temporal.onsets),
        spectral_centroid_kurtosis: compute_kurtosis(&r.spectral.spectral_centroid),
        bass_energy_slope: compute_linear_slope(&r.spectral.sub_band_energy_bass),
        spectral_bandwidth_slope: compute_linear_slope(&r.spectral.spectral_bandwidth),
        loudness_dynamic_spread: {
            let stl = &r.perceptual.short_term_loudness;
            if stl.is_empty() { None } else {
                let min = stl.iter().cloned().fold(f32::INFINITY, f32::min);
                let max = stl.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
                Some((max - min) as f64)
            }
        },

        // Beat timing features (using onsets, not grid-snapped beats)
        beat_regularity: compute_beat_regularity(&r.temporal.onsets),

        // Tension/energy profile features
        peak_tension: r.segments.patterns.tension_profile.iter()
            .map(|t| t.tension as f64)
            .fold(None, |acc, v| Some(acc.map_or(v, |a: f64| a.max(v)))),
        tension_range: {
            let tensions: Vec<f64> = r.segments.patterns.tension_profile.iter()
                .map(|t| t.tension as f64).collect();
            if tensions.is_empty() { None } else {
                let min = tensions.iter().cloned().fold(f64::INFINITY, f64::min);
                let max = tensions.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                Some(max - min)
            }
        },
        energy_peak_count: Some(r.segments.patterns.energy_profile.peaks.len() as i32),
        energy_valley_depth_mean: {
            let peaks = &r.segments.patterns.energy_profile.peaks;
            let valleys = &r.segments.patterns.energy_profile.valleys;
            if peaks.is_empty() || valleys.is_empty() { None } else {
                let mean_peak: f64 = peaks.iter().map(|p| p.1 as f64).sum::<f64>() / peaks.len() as f64;
                let mean_valley: f64 = valleys.iter().map(|v| v.1 as f64).sum::<f64>() / valleys.len() as f64;
                if mean_peak > 1e-10 { Some(mean_valley / mean_peak) } else { None }
            }
        },

        // Periodicity features
        rhythmic_periodicity_strength: r.segments.patterns.periodic_events.iter()
            .map(|e| e.strength as f64)
            .fold(None, |acc, v| Some(acc.map_or(v, |a: f64| a.max(v)))),

        // Cross-feature correlations
        spectral_loudness_correlation: compute_pearson_correlation(
            &r.spectral.spectral_centroid, &r.perceptual.short_term_loudness,
        ),

        // Musical
        estimated_key: Some(r.musical.key.key.clone()),
        key_confidence: Some(r.musical.key.confidence as f64),
        tonality: Some(r.musical.tonality as f64),
        harmonic_complexity: Some(r.musical.harmonic_complexity as f64),
        chord_count,
        chord_change_rate,
        mode_clarity: Some(r.musical.mode_clarity as f64),
        key_alternatives_count,
        time_sig_numerator: time_sig_num,
        time_sig_denominator: time_sig_den,
        chroma_vector: chroma_json,

        // Quality
        recording_quality_score: Some(r.quality.overall_score as f64),
        snr_db: Some(r.quality.metrics.snr_db as f64),
        clipping_ratio: Some(r.quality.metrics.clipping_ratio as f64),
        noise_floor_db: Some(r.quality.metrics.noise_floor_db as f64),

        // Segments
        segment_count: Some(r.segments.segments.len() as i32),
        temporal_complexity: Some(r.segments.temporal_complexity as f64),
        coherence_score: Some(r.segments.coherence_score as f64),

        // Energy/tension profile
        energy_shape,
        peak_energy,
        energy_variance,
        tension_build_count: Some(tension_build_count),
        tension_release_count: Some(tension_release_count),

        // Structure
        repetition_count,
        repetition_similarity,
        solo_section_count: Some(solo_count),
        solo_section_ratio: Some(solo_ratio),
        transition_count: Some(r.segments.transitions.len() as i32),

        // Classification
        classification_music_score,
        hnr,

        // Emotion scores — computed by jam_metrics
        valence_score: None,
        arousal_score: None,

        // Jam scores — computed separately by jam_metrics
        energy_score: None,
        intensity_score: None,
        groove_score: None,
        improvisation_score: None,
        tightness_score: None,
        build_quality_score: None,
        exploratory_score: None,
        transcendence_score: None,
    };

    ExtractionResult {
        analysis,
        chords: chord_records,
        segments: segment_records,
        tension_points: tension_records,
        transitions: transition_records,
    }
}

fn extract_chords(track_id: i64, r: &AnalysisResult) -> (Option<i32>, Option<f64>, Vec<ChordEvent>) {
    let progression = match &r.musical.chord_progression {
        Some(p) => p,
        None => return (Some(0), Some(0.0), vec![]),
    };

    let records: Vec<ChordEvent> = progression.chords.iter().map(|c| {
        ChordEvent {
            track_id,
            chord: c.chord.clone(),
            start_time: c.start_time as f64,
            duration: c.duration as f64,
            confidence: Some(c.confidence as f64),
        }
    }).collect();

    let unique_chords: HashSet<&str> = records.iter().map(|c| c.chord.as_str()).collect();
    let chord_count = Some(unique_chords.len() as i32);

    // Count actual chord transitions (label changes), not total chord events.
    // The old formula (events/duration) was deterministic from song length since
    // chords are detected at fixed 250ms intervals.
    let duration = r.summary.duration as f64;
    let transitions = records
        .windows(2)
        .filter(|w| w[0].chord != w[1].chord)
        .count();
    let chord_change_rate = if duration > 0.0 {
        Some(transitions as f64 / (duration / 60.0))
    } else {
        Some(0.0)
    };

    (chord_count, chord_change_rate, records)
}

fn extract_segments(track_id: i64, r: &AnalysisResult) -> Vec<SegmentRecord> {
    // Merge AudioSegment with StructuralSection data where indices match
    r.segments.segments.iter().enumerate().map(|(i, seg)| {
        // Find matching structural section for this segment index
        let section = r.segments.structure.iter().find(|s| s.segment_indices.contains(&i));

        SegmentRecord {
            track_id,
            segment_index: i as i32,
            label: format!("{:?}", seg.label),
            section_type: section.map(|s| format!("{:?}", s.section_type)),
            start_time: seg.start_time as f64,
            duration: seg.duration as f64,
            energy: Some(seg.energy as f64),
            spectral_centroid: Some(seg.spectral_centroid as f64),
            zcr: Some(seg.zcr as f64),
            key: seg.key.clone(),
            tempo: seg.tempo.map(|t| t as f64),
            dynamic_range: Some(seg.dynamic_range as f64),
            confidence: Some(seg.confidence as f64),
            harmonic_stability: section.map(|s| s.features.harmonic_stability as f64),
            rhythmic_density: section.map(|s| s.features.rhythmic_density as f64),
            avg_brightness: section.map(|s| s.features.avg_brightness as f64),
            dynamic_variation: section.map(|s| s.features.dynamic_variation as f64),
        }
    }).collect()
}

fn count_solo_sections(structure: &[ferrous_waves::analysis::segments::StructuralSection], total_duration: f64) -> (i32, f64) {
    let mut count = 0i32;
    let mut solo_duration = 0.0f64;

    for section in structure {
        let section_type = format!("{:?}", section.section_type);
        if section_type == "Solo" || section_type == "Instrumental" {
            count += 1;
            solo_duration += (section.end_time - section.start_time) as f64;
        }
    }

    let ratio = if total_duration > 0.0 { solo_duration / total_duration } else { 0.0 };
    (count, ratio)
}

fn mean_std(values: &[f32]) -> (f64, f64) {
    if values.is_empty() {
        return (0.0, 0.0);
    }
    let n = values.len() as f64;
    let mean = values.iter().map(|&v| v as f64).sum::<f64>() / n;
    let variance = values.iter().map(|&v| {
        let diff = v as f64 - mean;
        diff * diff
    }).sum::<f64>() / n;
    (mean, variance.sqrt())
}

/// Skewness of a time series. Positive = right-tailed (occasional big spikes),
/// negative = left-tailed (sustained high values). For spectral flux: positive skew
/// means percussive accents, negative means sustained spectral change.
fn compute_skewness(values: &[f32]) -> Option<f64> {
    if values.len() < 3 {
        return None;
    }
    let n = values.len() as f64;
    let mean = values.iter().map(|&v| v as f64).sum::<f64>() / n;
    let m2 = values.iter().map(|&v| (v as f64 - mean).powi(2)).sum::<f64>() / n;
    let m3 = values.iter().map(|&v| (v as f64 - mean).powi(3)).sum::<f64>() / n;
    let std = m2.sqrt();
    if std < 1e-10 {
        return Some(0.0);
    }
    Some(m3 / std.powi(3))
}

/// Linear regression slope of a time series (normalized to per-unit-time).
/// For spectral centroid: positive slope = track gets brighter over time.
fn compute_linear_slope(values: &[f32]) -> Option<f64> {
    if values.len() < 2 {
        return None;
    }
    let n = values.len() as f64;
    let mut sum_x = 0.0;
    let mut sum_y = 0.0;
    let mut sum_xy = 0.0;
    let mut sum_x2 = 0.0;
    for (i, &v) in values.iter().enumerate() {
        let x = i as f64 / n; // normalize x to [0, 1]
        let y = v as f64;
        sum_x += x;
        sum_y += y;
        sum_xy += x * y;
        sum_x2 += x * x;
    }
    let denom = n * sum_x2 - sum_x * sum_x;
    if denom.abs() < 1e-10 {
        return Some(0.0);
    }
    Some((n * sum_xy - sum_x * sum_y) / denom)
}

/// Same as compute_linear_slope but for data that might have extreme values
/// (like LUFS loudness which can be -inf for silence).
fn compute_linear_slope_f32(values: &[f32]) -> Option<f64> {
    if values.len() < 2 {
        return None;
    }
    // Filter out non-finite values (silence frames produce -inf LUFS)
    let valid: Vec<(f64, f64)> = values
        .iter()
        .enumerate()
        .filter(|(_, v)| v.is_finite())
        .map(|(i, v)| (i as f64 / values.len() as f64, *v as f64))
        .collect();
    if valid.len() < 2 {
        return None;
    }
    let n = valid.len() as f64;
    let sum_x: f64 = valid.iter().map(|&(x, _)| x).sum();
    let sum_y: f64 = valid.iter().map(|&(_, y)| y).sum();
    let sum_xy: f64 = valid.iter().map(|&(x, y)| x * y).sum();
    let sum_x2: f64 = valid.iter().map(|&(x, _)| x * x).sum();
    let denom = n * sum_x2 - sum_x * sum_x;
    if denom.abs() < 1e-10 {
        return Some(0.0);
    }
    Some((n * sum_xy - sum_x * sum_y) / denom)
}

/// Ratio of energy in the last third vs first third of the track.
/// >1.0 = track builds in energy, <1.0 = track decays.
fn compute_buildup_ratio(values: &[f32]) -> Option<f64> {
    if values.len() < 6 {
        return None;
    }
    let third = values.len() / 3;
    let first_energy: f64 = values[..third].iter().map(|&v| v as f64).sum::<f64>() / third as f64;
    let last_energy: f64 = values[values.len() - third..].iter().map(|&v| v as f64).sum::<f64>() / third as f64;
    if first_energy < 1e-10 {
        Some(if last_energy > 1e-10 { 10.0 } else { 1.0 })
    } else {
        Some((last_energy / first_energy).min(10.0))
    }
}

/// Mean of bass/(high+presence) ratio per frame. High = groove-heavy, low = treble-heavy.
fn compute_bass_treble_ratio_mean(
    bass: &[f32],
    high: &[f32],
    presence: &[f32],
) -> Option<f64> {
    if bass.is_empty() || high.is_empty() || presence.is_empty() {
        return None;
    }
    let n = bass.len().min(high.len()).min(presence.len());
    let mut sum = 0.0;
    for i in 0..n {
        let treble = (high[i] + presence[i]) as f64;
        let ratio = if treble > 1e-10 { bass[i] as f64 / treble } else { 1.0 };
        sum += ratio;
    }
    Some(sum / n as f64)
}

/// Std of bass/(high+presence) ratio per frame. High = tonal balance shifts a lot.
fn compute_bass_treble_ratio_std(
    bass: &[f32],
    high: &[f32],
    presence: &[f32],
) -> Option<f64> {
    if bass.is_empty() || high.is_empty() || presence.is_empty() {
        return None;
    }
    let n = bass.len().min(high.len()).min(presence.len());
    let mut ratios: Vec<f64> = Vec::with_capacity(n);
    for i in 0..n {
        let treble = (high[i] + presence[i]) as f64;
        ratios.push(if treble > 1e-10 { bass[i] as f64 / treble } else { 1.0 });
    }
    let mean = ratios.iter().sum::<f64>() / ratios.len() as f64;
    let var = ratios.iter().map(|&r| (r - mean).powi(2)).sum::<f64>() / ratios.len() as f64;
    Some(var.sqrt())
}

/// Std of onset density across 10-second windows.
/// High = rhythmically varied (builds/drops/breaks), low = steady groove.
fn compute_onset_density_std(onsets: &[f32], duration: f32) -> Option<f64> {
    if onsets.is_empty() || duration < 20.0 {
        return None;
    }
    let window = 10.0; // 10-second windows
    let num_windows = (duration / window).ceil() as usize;
    if num_windows < 2 {
        return None;
    }
    let mut counts = vec![0.0_f64; num_windows];
    for &onset in onsets {
        let idx = ((onset / window) as usize).min(num_windows - 1);
        counts[idx] += 1.0;
    }
    let mean = counts.iter().sum::<f64>() / counts.len() as f64;
    let var = counts.iter().map(|&c| (c - mean).powi(2)).sum::<f64>() / counts.len() as f64;
    Some(var.sqrt())
}

/// Normalized time (0.0-1.0) at which peak energy occurs.
/// Near 0 = front-loaded, ~0.5 = climax in middle, near 1.0 = builds to end.
fn compute_peak_time(values: &[f32]) -> Option<f64> {
    if values.len() < 10 {
        return None;
    }
    // Smooth with a sliding window (~3s of short-term loudness at typical hop sizes)
    // to find the peak *region* rather than a single noisy frame.
    let win = (values.len() / 20).max(3); // ~5% of duration
    let mut best_idx = 0;
    let mut best_sum = f64::NEG_INFINITY;
    for i in 0..=(values.len() - win) {
        let sum: f64 = values[i..i + win].iter().map(|&v| v as f64).sum();
        if sum > best_sum {
            best_sum = sum;
            best_idx = i + win / 2; // center of window
        }
    }
    Some(best_idx as f64 / values.len() as f64)
}

/// Excess kurtosis of a per-frame vector. High kurtosis = heavy-tailed distribution
/// (occasional extreme values, like sudden brightness peaks during climaxes).
fn compute_kurtosis(values: &[f32]) -> Option<f64> {
    if values.len() < 4 {
        return None;
    }
    let n = values.len() as f64;
    let mean = values.iter().map(|v| *v as f64).sum::<f64>() / n;
    let m2 = values.iter().map(|v| (*v as f64 - mean).powi(2)).sum::<f64>() / n;
    if m2 < 1e-12 {
        return None;
    }
    let m4 = values.iter().map(|v| (*v as f64 - mean).powi(4)).sum::<f64>() / n;
    Some(m4 / (m2 * m2) - 3.0) // excess kurtosis (normal = 0)
}

/// Std of detected pitch frequencies across frames (melodic range over time).
/// Only considers frames where pitch was confidently detected.
fn compute_pitch_contour_std(frames: &[PitchFrame]) -> Option<f64> {
    let pitches: Vec<f64> = frames
        .iter()
        .filter(|f| f.confidence > 0.5)
        .filter_map(|f| f.frequency.map(|hz| hz as f64))
        .filter(|hz| *hz > 50.0 && *hz < 4000.0) // reasonable pitch range
        .collect();
    if pitches.len() < 10 {
        return None;
    }
    let mean = pitches.iter().sum::<f64>() / pitches.len() as f64;
    let var = pitches.iter().map(|p| (p - mean).powi(2)).sum::<f64>() / pitches.len() as f64;
    Some(var.sqrt())
}

/// Mean pitch clarity across all frames (how cleanly the pitch detector tracked).
fn compute_pitch_clarity_mean(frames: &[PitchFrame]) -> Option<f64> {
    if frames.is_empty() {
        return None;
    }
    let sum: f64 = frames.iter().map(|f| f.clarity as f64).sum();
    Some(sum / frames.len() as f64)
}

/// Fraction of frames where pitch was detected (confidence > 0.5).
/// High = tonal/melodic music, low = noise/drums/ambient.
fn compute_pitched_frame_ratio(frames: &[PitchFrame]) -> Option<f64> {
    if frames.is_empty() {
        return None;
    }
    let pitched = frames.iter().filter(|f| f.confidence > 0.5 && f.frequency.is_some()).count();
    Some(pitched as f64 / frames.len() as f64)
}

/// Mean frame-to-frame MFCC distance (timbral change rate).
/// High = rapidly changing timbre (improvisation, exploration).
/// Low = consistent timbre (steady groove, held chord).
fn compute_mfcc_flux_mean(mfcc: &[Vec<f32>]) -> Option<f64> {
    if mfcc.len() < 2 {
        return None;
    }
    // mfcc[coeff_idx][frame_idx] — compute Euclidean distance between consecutive frames
    let n_coeffs = mfcc.len();
    let n_frames = mfcc[0].len();
    if n_frames < 2 {
        return None;
    }
    let mut total_dist = 0.0_f64;
    for frame in 1..n_frames {
        let mut dist_sq = 0.0_f64;
        for coeff in 0..n_coeffs {
            if frame < mfcc[coeff].len() && frame - 1 < mfcc[coeff].len() {
                let d = (mfcc[coeff][frame] - mfcc[coeff][frame - 1]) as f64;
                dist_sq += d * d;
            }
        }
        total_dist += dist_sq.sqrt();
    }
    Some(total_dist / (n_frames - 1) as f64)
}

/// Shannon entropy of inter-onset intervals (rhythmic predictability).
/// Low entropy = regular/predictable rhythm (groove). High = free/chaotic.
fn compute_onset_interval_entropy(onsets: &[f32]) -> Option<f64> {
    if onsets.len() < 10 {
        return None;
    }
    // Compute IOIs
    let iois: Vec<f64> = onsets
        .windows(2)
        .map(|w| (w[1] - w[0]) as f64)
        .filter(|ioi| *ioi > 0.01 && *ioi < 5.0) // reasonable range
        .collect();
    if iois.len() < 5 {
        return None;
    }

    // Bin IOIs into 20 bins (10ms-500ms range, 24.5ms each)
    let bin_count = 20;
    let bin_width = 0.5 / bin_count as f64;
    let mut bins = vec![0u32; bin_count];
    for ioi in &iois {
        let idx = ((ioi / bin_width) as usize).min(bin_count - 1);
        bins[idx] += 1;
    }

    // Shannon entropy
    let total = iois.len() as f64;
    let entropy: f64 = bins
        .iter()
        .filter(|&&c| c > 0)
        .map(|&c| {
            let p = c as f64 / total;
            -p * p.ln()
        })
        .sum();

    // Normalize by max entropy (log(bin_count))
    let max_entropy = (bin_count as f64).ln();
    Some(entropy / max_entropy)
}

/// Coefficient of variation of inter-beat intervals (rhythmic regularity).
/// 0 = perfectly metronomic. High = irregular tempo (rubato, free jazz, Drums/Space).
fn compute_beat_regularity(beats: &[f32]) -> Option<f64> {
    if beats.len() < 4 {
        return None;
    }
    let intervals: Vec<f64> = beats.windows(2).map(|w| (w[1] - w[0]) as f64).collect();
    let mean = intervals.iter().sum::<f64>() / intervals.len() as f64;
    if mean < 1e-10 {
        return None;
    }
    let variance = intervals.iter().map(|&i| (i - mean).powi(2)).sum::<f64>() / intervals.len() as f64;
    Some(variance.sqrt() / mean) // CV = std/mean
}

/// Pearson correlation between two per-frame vectors (aligned by truncating to shorter).
/// Range [-1, 1]. Positive = co-moving, negative = inverse, near 0 = independent.
fn compute_pearson_correlation(a: &[f32], b: &[f32]) -> Option<f64> {
    let n = a.len().min(b.len());
    if n < 10 {
        return None;
    }
    let a = &a[..n];
    let b = &b[..n];
    let n_f = n as f64;
    let mean_a = a.iter().map(|&v| v as f64).sum::<f64>() / n_f;
    let mean_b = b.iter().map(|&v| v as f64).sum::<f64>() / n_f;
    let mut cov = 0.0_f64;
    let mut var_a = 0.0_f64;
    let mut var_b = 0.0_f64;
    for i in 0..n {
        let da = a[i] as f64 - mean_a;
        let db = b[i] as f64 - mean_b;
        cov += da * db;
        var_a += da * da;
        var_b += db * db;
    }
    let denom = (var_a * var_b).sqrt();
    if denom < 1e-10 {
        return Some(0.0);
    }
    Some(cov / denom)
}
