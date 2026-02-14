use crate::db::models::{
    ChordEvent, NewAnalysis, SegmentRecord, TensionPointRecord, TransitionRecord,
};
use ferrous_waves::analysis::engine::AnalysisResult;
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

    let duration = r.summary.duration as f64;
    let chord_change_rate = if duration > 0.0 && !records.is_empty() {
        Some(records.len() as f64 / (duration / 60.0))
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
