//! Column name constants and schema inventory for analysis_results.
//!
//! Provides:
//! - `SCORE_COLUMNS`: validated score column names for SQL ORDER BY
//! - `TRACK_SCORE_SELECT`: shared SELECT fragment for TrackScore queries
//! - `NOT_GARBAGE`: common WHERE clause filter
//! - `ANALYSIS_SCHEMA`: full column inventory for the `schema` command

use super::models::TrackScore;

// ---------------------------------------------------------------------------
// Query helper constants
// ---------------------------------------------------------------------------

/// Valid score column names (for SQL injection-safe ORDER BY).
pub const SCORE_COLUMNS: &[&str] = &[
    "energy_score",
    "intensity_score",
    "groove_score",
    "improvisation_score",
    "tightness_score",
    "build_quality_score",
    "exploratory_score",
    "transcendence_score",
    "valence_score",
    "arousal_score",
];

/// SQL SELECT fragment shared by all TrackScore queries.
/// Produces columns 0..14 matching `map_track_score` positional indices.
/// Use with: `FROM analysis_results a JOIN tracks t ON t.id = a.track_id`
pub const TRACK_SCORE_SELECT: &str =
    "COALESCE(t.parsed_title, t.title, '(untitled)'),
     COALESCE(t.parsed_date, t.date, '?'),
     COALESCE(a.duration, 0.0) / 60.0,
     a.estimated_key, a.tempo_bpm,
     COALESCE(a.energy_score, 0), COALESCE(a.intensity_score, 0),
     COALESCE(a.groove_score, 0), COALESCE(a.improvisation_score, 0),
     COALESCE(a.tightness_score, 0), COALESCE(a.build_quality_score, 0),
     COALESCE(a.exploratory_score, 0), COALESCE(a.transcendence_score, 0),
     COALESCE(a.valence_score, 0), COALESCE(a.arousal_score, 0)";

/// Common WHERE clause to exclude garbage-quality tracks.
pub const NOT_GARBAGE: &str = "COALESCE(t.data_quality, 'ok') != 'garbage'";

/// WHERE clause to show only live recordings (excludes studio, live_album, unknown).
pub const LIVE_ONLY: &str = "COALESCE(t.recording_type, 'unknown') = 'live'";

/// Map a rusqlite row (from TRACK_SCORE_SELECT) to a TrackScore.
/// Expects columns 0..14 in the order produced by TRACK_SCORE_SELECT.
pub fn map_track_score(row: &rusqlite::Row) -> rusqlite::Result<TrackScore> {
    Ok(TrackScore {
        title: row.get(0)?,
        date: row.get(1)?,
        duration_min: row.get(2)?,
        key: row.get(3)?,
        tempo: row.get(4)?,
        energy: row.get(5)?,
        intensity: row.get(6)?,
        groove: row.get(7)?,
        improvisation: row.get(8)?,
        tightness: row.get(9)?,
        build_quality: row.get(10)?,
        exploratory: row.get(11)?,
        transcendence: row.get(12)?,
        valence: row.get(13)?,
        arousal: row.get(14)?,
    })
}

// ---------------------------------------------------------------------------
// Schema inventory for `setbreak schema`
// ---------------------------------------------------------------------------

pub struct ColumnDef {
    pub name: &'static str,
    pub sql_type: &'static str,
    pub category: &'static str,
    pub description: &'static str,
}

/// Complete analysis_results schema (excludes id, track_id, analyzed_at).
pub const ANALYSIS_SCHEMA: &[ColumnDef] = &[
    // ── Summary ──────────────────────────────────────────────────────
    ColumnDef { name: "duration",        sql_type: "REAL", category: "Summary",  description: "Track duration in seconds" },
    ColumnDef { name: "sample_rate",     sql_type: "INT",  category: "Summary",  description: "Sample rate in Hz" },
    ColumnDef { name: "channels",        sql_type: "INT",  category: "Summary",  description: "Number of audio channels" },
    ColumnDef { name: "peak_amplitude",  sql_type: "REAL", category: "Summary",  description: "Maximum sample amplitude" },
    ColumnDef { name: "rms_level",       sql_type: "REAL", category: "Summary",  description: "Root mean square level" },
    ColumnDef { name: "dynamic_range",   sql_type: "REAL", category: "Summary",  description: "Dynamic range in dB" },

    // ── Spectral (core) ──────────────────────────────────────────────
    ColumnDef { name: "spectral_centroid_mean",   sql_type: "REAL", category: "Spectral",  description: "Average brightness (Hz)" },
    ColumnDef { name: "spectral_centroid_std",    sql_type: "REAL", category: "Spectral",  description: "Brightness variability" },
    ColumnDef { name: "spectral_flux_mean",       sql_type: "REAL", category: "Spectral",  description: "Average spectral change rate" },
    ColumnDef { name: "spectral_flux_std",        sql_type: "REAL", category: "Spectral",  description: "Spectral change variability" },
    ColumnDef { name: "spectral_rolloff_mean",    sql_type: "REAL", category: "Spectral",  description: "Frequency below which 85% energy" },
    ColumnDef { name: "spectral_rolloff_std",     sql_type: "REAL", category: "Spectral",  description: "Rolloff variability" },
    ColumnDef { name: "spectral_flatness_mean",   sql_type: "REAL", category: "Spectral",  description: "Noise-like vs tonal (0=tonal, 1=noise)" },
    ColumnDef { name: "spectral_flatness_std",    sql_type: "REAL", category: "Spectral",  description: "Flatness variability" },
    ColumnDef { name: "spectral_bandwidth_mean",  sql_type: "REAL", category: "Spectral",  description: "Spectral width in Hz" },
    ColumnDef { name: "spectral_bandwidth_std",   sql_type: "REAL", category: "Spectral",  description: "Bandwidth variability" },
    ColumnDef { name: "zcr_mean",                 sql_type: "REAL", category: "Spectral",  description: "Zero crossing rate (percussiveness)" },
    ColumnDef { name: "zcr_std",                  sql_type: "REAL", category: "Spectral",  description: "ZCR variability" },

    // ── Sub-band energy ──────────────────────────────────────────────
    ColumnDef { name: "sub_band_bass_mean",       sql_type: "REAL", category: "Sub-band",  description: "Bass energy (20-250 Hz)" },
    ColumnDef { name: "sub_band_bass_std",        sql_type: "REAL", category: "Sub-band",  description: "Bass variability" },
    ColumnDef { name: "sub_band_mid_mean",        sql_type: "REAL", category: "Sub-band",  description: "Mid energy (250-2000 Hz)" },
    ColumnDef { name: "sub_band_mid_std",         sql_type: "REAL", category: "Sub-band",  description: "Mid variability" },
    ColumnDef { name: "sub_band_high_mean",       sql_type: "REAL", category: "Sub-band",  description: "High energy (2000-8000 Hz)" },
    ColumnDef { name: "sub_band_high_std",        sql_type: "REAL", category: "Sub-band",  description: "High variability" },
    ColumnDef { name: "sub_band_presence_mean",   sql_type: "REAL", category: "Sub-band",  description: "Presence energy (8000+ Hz)" },
    ColumnDef { name: "sub_band_presence_std",    sql_type: "REAL", category: "Sub-band",  description: "Presence variability" },

    // ── MFCCs ────────────────────────────────────────────────────────
    ColumnDef { name: "mfcc_0_mean",   sql_type: "REAL", category: "MFCC", description: "MFCC 0 mean (overall energy)" },
    ColumnDef { name: "mfcc_0_std",    sql_type: "REAL", category: "MFCC", description: "MFCC 0 std" },
    ColumnDef { name: "mfcc_1_mean",   sql_type: "REAL", category: "MFCC", description: "MFCC 1 mean (spectral slope)" },
    ColumnDef { name: "mfcc_1_std",    sql_type: "REAL", category: "MFCC", description: "MFCC 1 std" },
    ColumnDef { name: "mfcc_2_mean",   sql_type: "REAL", category: "MFCC", description: "MFCC 2 mean" },
    ColumnDef { name: "mfcc_2_std",    sql_type: "REAL", category: "MFCC", description: "MFCC 2 std" },
    ColumnDef { name: "mfcc_3_mean",   sql_type: "REAL", category: "MFCC", description: "MFCC 3 mean" },
    ColumnDef { name: "mfcc_3_std",    sql_type: "REAL", category: "MFCC", description: "MFCC 3 std" },
    ColumnDef { name: "mfcc_4_mean",   sql_type: "REAL", category: "MFCC", description: "MFCC 4 mean" },
    ColumnDef { name: "mfcc_4_std",    sql_type: "REAL", category: "MFCC", description: "MFCC 4 std" },
    ColumnDef { name: "mfcc_5_mean",   sql_type: "REAL", category: "MFCC", description: "MFCC 5 mean" },
    ColumnDef { name: "mfcc_5_std",    sql_type: "REAL", category: "MFCC", description: "MFCC 5 std" },
    ColumnDef { name: "mfcc_6_mean",   sql_type: "REAL", category: "MFCC", description: "MFCC 6 mean" },
    ColumnDef { name: "mfcc_6_std",    sql_type: "REAL", category: "MFCC", description: "MFCC 6 std" },
    ColumnDef { name: "mfcc_7_mean",   sql_type: "REAL", category: "MFCC", description: "MFCC 7 mean" },
    ColumnDef { name: "mfcc_7_std",    sql_type: "REAL", category: "MFCC", description: "MFCC 7 std" },
    ColumnDef { name: "mfcc_8_mean",   sql_type: "REAL", category: "MFCC", description: "MFCC 8 mean" },
    ColumnDef { name: "mfcc_8_std",    sql_type: "REAL", category: "MFCC", description: "MFCC 8 std" },
    ColumnDef { name: "mfcc_9_mean",   sql_type: "REAL", category: "MFCC", description: "MFCC 9 mean" },
    ColumnDef { name: "mfcc_9_std",    sql_type: "REAL", category: "MFCC", description: "MFCC 9 std" },
    ColumnDef { name: "mfcc_10_mean",  sql_type: "REAL", category: "MFCC", description: "MFCC 10 mean" },
    ColumnDef { name: "mfcc_10_std",   sql_type: "REAL", category: "MFCC", description: "MFCC 10 std" },
    ColumnDef { name: "mfcc_11_mean",  sql_type: "REAL", category: "MFCC", description: "MFCC 11 mean" },
    ColumnDef { name: "mfcc_11_std",   sql_type: "REAL", category: "MFCC", description: "MFCC 11 std" },
    ColumnDef { name: "mfcc_12_mean",  sql_type: "REAL", category: "MFCC", description: "MFCC 12 mean" },
    ColumnDef { name: "mfcc_12_std",   sql_type: "REAL", category: "MFCC", description: "MFCC 12 std" },

    // ── Temporal ─────────────────────────────────────────────────────
    ColumnDef { name: "tempo_bpm",            sql_type: "REAL", category: "Temporal",  description: "Estimated tempo (30-300 BPM)" },
    ColumnDef { name: "beat_count",           sql_type: "INT",  category: "Temporal",  description: "Number of detected beats" },
    ColumnDef { name: "onset_count",          sql_type: "INT",  category: "Temporal",  description: "Number of detected onsets" },
    ColumnDef { name: "tempo_stability",      sql_type: "REAL", category: "Temporal",  description: "Tempo consistency (0-1, 1=steady)" },
    ColumnDef { name: "rhythmic_complexity",  sql_type: "REAL", category: "Temporal",  description: "Rhythmic complexity measure" },

    // ── Pitch ────────────────────────────────────────────────────────
    ColumnDef { name: "mean_pitch",            sql_type: "REAL", category: "Pitch", description: "Average detected pitch (Hz)" },
    ColumnDef { name: "pitch_range_low",       sql_type: "REAL", category: "Pitch", description: "Lowest detected pitch (Hz)" },
    ColumnDef { name: "pitch_range_high",      sql_type: "REAL", category: "Pitch", description: "Highest detected pitch (Hz)" },
    ColumnDef { name: "pitch_stability",       sql_type: "REAL", category: "Pitch", description: "Pitch steadiness" },
    ColumnDef { name: "dominant_pitch",        sql_type: "REAL", category: "Pitch", description: "Most common pitch (Hz)" },
    ColumnDef { name: "vibrato_presence",      sql_type: "REAL", category: "Pitch", description: "Vibrato amount (always ~0.7, degenerate)" },
    ColumnDef { name: "vibrato_rate",          sql_type: "REAL", category: "Pitch", description: "Vibrato speed (Hz)" },
    ColumnDef { name: "pitch_confidence_mean", sql_type: "REAL", category: "Pitch", description: "Average pitch detection confidence" },

    // ── Perceptual ───────────────────────────────────────────────────
    ColumnDef { name: "lufs_integrated",   sql_type: "REAL", category: "Perceptual", description: "Integrated loudness (LUFS)" },
    ColumnDef { name: "loudness_range",    sql_type: "REAL", category: "Perceptual", description: "Loudness range (LRA, LU)" },
    ColumnDef { name: "true_peak_dbfs",    sql_type: "REAL", category: "Perceptual", description: "True peak level (dBFS)" },
    ColumnDef { name: "crest_factor",      sql_type: "REAL", category: "Perceptual", description: "Peak-to-RMS ratio" },
    ColumnDef { name: "energy_level",      sql_type: "REAL", category: "Perceptual", description: "Overall energy level" },
    ColumnDef { name: "loudness_std",      sql_type: "REAL", category: "Perceptual", description: "Std of short-term loudness (3s window)" },
    ColumnDef { name: "peak_loudness",     sql_type: "REAL", category: "Perceptual", description: "Maximum momentary loudness" },

    // ── Per-frame derived (v9-v11) ───────────────────────────────────
    ColumnDef { name: "spectral_flux_skewness",      sql_type: "REAL", category: "Derived",     description: "+ve=percussive hits, -ve=sustained" },
    ColumnDef { name: "spectral_centroid_slope",      sql_type: "REAL", category: "Derived",     description: "Brightness trend over time" },
    ColumnDef { name: "energy_buildup_ratio",         sql_type: "REAL", category: "Derived",     description: "Energy last_third / first_third" },
    ColumnDef { name: "bass_treble_ratio_mean",       sql_type: "REAL", category: "Derived",     description: "Mean bass/(high+presence) ratio" },
    ColumnDef { name: "bass_treble_ratio_std",        sql_type: "REAL", category: "Derived",     description: "Bass/treble ratio variability" },
    ColumnDef { name: "onset_density_std",            sql_type: "REAL", category: "Derived",     description: "Onset density variability (10s windows)" },
    ColumnDef { name: "loudness_buildup_slope",       sql_type: "REAL", category: "Derived",     description: "Linear trend of short-term loudness" },
    ColumnDef { name: "peak_energy_time",             sql_type: "REAL", category: "Derived",     description: "Normalized time of peak energy (0-1)" },
    ColumnDef { name: "pitch_contour_std",            sql_type: "REAL", category: "Derived",     description: "Melodic range (std of pitch)" },
    ColumnDef { name: "pitch_clarity_mean",           sql_type: "REAL", category: "Derived",     description: "Mean pitch clarity across frames" },
    ColumnDef { name: "pitched_frame_ratio",          sql_type: "REAL", category: "Derived",     description: "Fraction of frames with detected pitch" },
    ColumnDef { name: "mfcc_flux_mean",               sql_type: "REAL", category: "Derived",     description: "Timbral change rate (frame MFCC distance)" },
    ColumnDef { name: "onset_interval_entropy",       sql_type: "REAL", category: "Derived",     description: "Rhythmic predictability (Shannon entropy)" },
    ColumnDef { name: "spectral_centroid_kurtosis",   sql_type: "REAL", category: "Derived",     description: "Excess kurtosis of centroid" },
    ColumnDef { name: "bass_energy_slope",            sql_type: "REAL", category: "Derived",     description: "Linear trend of bass energy" },
    ColumnDef { name: "spectral_bandwidth_slope",     sql_type: "REAL", category: "Derived",     description: "Bandwidth trend (broadening/narrowing)" },
    ColumnDef { name: "loudness_dynamic_spread",      sql_type: "REAL", category: "Derived",     description: "Short-term loudness range (max-min LUFS)" },
    ColumnDef { name: "beat_regularity",              sql_type: "REAL", category: "Derived",     description: "CV of inter-beat intervals (0=metronome)" },
    ColumnDef { name: "peak_tension",                 sql_type: "REAL", category: "Derived",     description: "Maximum tension value (0-1)" },
    ColumnDef { name: "tension_range",                sql_type: "REAL", category: "Derived",     description: "Max - min tension" },
    ColumnDef { name: "energy_peak_count",            sql_type: "INT",  category: "Derived",     description: "Number of energy climax peaks" },
    ColumnDef { name: "energy_valley_depth_mean",     sql_type: "REAL", category: "Derived",     description: "Mean valley depth vs mean peak" },
    ColumnDef { name: "rhythmic_periodicity_strength", sql_type: "REAL", category: "Derived",    description: "Strongest periodic event strength (0-1)" },
    ColumnDef { name: "spectral_loudness_correlation", sql_type: "REAL", category: "Derived",    description: "Pearson r: centroid vs loudness" },

    // ── Spectral shape (v12) ─────────────────────────────────────────
    ColumnDef { name: "spectral_skewness_mean",  sql_type: "REAL", category: "Spectral shape", description: "+ve=low-freq dominant" },
    ColumnDef { name: "spectral_kurtosis_mean",  sql_type: "REAL", category: "Spectral shape", description: "High=tonal, low=noise-like" },
    ColumnDef { name: "spectral_entropy_mean",   sql_type: "REAL", category: "Spectral shape", description: "0=pure tone, 1=noise" },
    ColumnDef { name: "spectral_entropy_std",    sql_type: "REAL", category: "Spectral shape", description: "Entropy variability" },
    ColumnDef { name: "spectral_slope_mean",     sql_type: "REAL", category: "Spectral shape", description: "Spectral tilt (negative=natural roll-off)" },
    ColumnDef { name: "spectral_contrast_json",  sql_type: "TEXT", category: "Spectral shape", description: "JSON [f32;7] per-band peak/valley ratio" },

    // ── Sub-band flux (v12) ──────────────────────────────────────────
    ColumnDef { name: "sub_band_flux_bass_mean", sql_type: "REAL", category: "Sub-band flux", description: "Bass change rate (kick detection)" },
    ColumnDef { name: "sub_band_flux_bass_std",  sql_type: "REAL", category: "Sub-band flux", description: "Bass change variability" },
    ColumnDef { name: "sub_band_flux_mid_mean",  sql_type: "REAL", category: "Sub-band flux", description: "Mid change rate (harmonic/rhythmic)" },
    ColumnDef { name: "sub_band_flux_high_mean", sql_type: "REAL", category: "Sub-band flux", description: "Treble change rate (hi-hat/cymbal)" },

    // ── Harmonic / chroma (v12) ──────────────────────────────────────
    ColumnDef { name: "tonnetz_json",       sql_type: "TEXT", category: "Harmonic", description: "JSON [f32;6] mean Tonnetz (harmonic center)" },
    ColumnDef { name: "tonnetz_flux_mean",  sql_type: "REAL", category: "Harmonic", description: "Harmonic change rate (HCDF)" },
    ColumnDef { name: "chroma_flux_mean",   sql_type: "REAL", category: "Harmonic", description: "Pitch-class change rate" },

    // ── Beat-sync rhythm (v12) ───────────────────────────────────────
    ColumnDef { name: "beat_pattern_json",  sql_type: "TEXT", category: "Rhythm", description: "JSON sub-band onset patterns" },
    ColumnDef { name: "syncopation",        sql_type: "REAL", category: "Rhythm", description: "Off-beat emphasis (0=on-beat)" },
    ColumnDef { name: "pulse_clarity",      sql_type: "REAL", category: "Rhythm", description: "Beat regularity strength (0-1)" },
    ColumnDef { name: "offbeat_ratio",      sql_type: "REAL", category: "Rhythm", description: "Mid-band offbeat/downbeat energy" },

    // ── Timbral texture (v13) ────────────────────────────────────────
    ColumnDef { name: "spectral_spread_mean", sql_type: "REAL", category: "Timbral", description: "Spectral bandwidth (narrow=tonal, wide=noisy)" },
    ColumnDef { name: "spectral_spread_std",  sql_type: "REAL", category: "Timbral", description: "Spectral width variability" },
    ColumnDef { name: "spectral_crest_mean",  sql_type: "REAL", category: "Timbral", description: "Max/mean spectrum (high=tonal, low=noise)" },
    ColumnDef { name: "spectral_crest_std",   sql_type: "REAL", category: "Timbral", description: "Spectral crest variability" },
    ColumnDef { name: "roughness_mean",       sql_type: "REAL", category: "Timbral", description: "Sensory dissonance (Sethares model)" },
    ColumnDef { name: "roughness_std",        sql_type: "REAL", category: "Timbral", description: "Roughness variability" },

    // ── MFCC dynamics (v13) ──────────────────────────────────────────
    ColumnDef { name: "mfcc_delta_mean_json",       sql_type: "TEXT", category: "MFCC dynamics", description: "JSON [f32;13] mean first derivatives" },
    ColumnDef { name: "mfcc_delta_delta_mean_json",  sql_type: "TEXT", category: "MFCC dynamics", description: "JSON [f32;13] mean second derivatives" },

    // ── Stereo (v13) ─────────────────────────────────────────────────
    ColumnDef { name: "stereo_width_mean", sql_type: "REAL", category: "Stereo", description: "L/R correlation (1=mono, 0=wide)" },
    ColumnDef { name: "stereo_width_std",  sql_type: "REAL", category: "Stereo", description: "Stereo image variability" },

    // ── Onset envelope (v13) ─────────────────────────────────────────
    ColumnDef { name: "attack_time_mean",        sql_type: "REAL", category: "Onset", description: "Mean rise time onset->peak (seconds)" },
    ColumnDef { name: "attack_time_std",         sql_type: "REAL", category: "Onset", description: "Attack time variability" },
    ColumnDef { name: "decay_time_mean",         sql_type: "REAL", category: "Onset", description: "Mean decay time peak->quiet (seconds)" },
    ColumnDef { name: "decay_time_std",          sql_type: "REAL", category: "Onset", description: "Decay time variability" },
    ColumnDef { name: "onset_strength_mean",     sql_type: "REAL", category: "Onset", description: "Mean onset strength (spectral flux peaks)" },
    ColumnDef { name: "onset_strength_std",      sql_type: "REAL", category: "Onset", description: "Onset strength variability" },
    ColumnDef { name: "onset_strength_skewness", sql_type: "REAL", category: "Onset", description: "+ve = rare big hits" },

    // ── Micro-rhythm (v13) ───────────────────────────────────────────
    ColumnDef { name: "swing_ratio",                 sql_type: "REAL", category: "Micro-rhythm", description: "8th-note ratio (1.0=straight, ~1.67=shuffle)" },
    ColumnDef { name: "microtiming_deviation_mean",  sql_type: "REAL", category: "Micro-rhythm", description: "Mean onset-to-grid deviation (seconds)" },
    ColumnDef { name: "microtiming_deviation_std",   sql_type: "REAL", category: "Micro-rhythm", description: "Timing deviation variability" },
    ColumnDef { name: "microtiming_bias",            sql_type: "REAL", category: "Micro-rhythm", description: "Ahead(+) or behind(-) beat tendency" },

    // ── Temporal modulation (v13) ────────────────────────────────────
    ColumnDef { name: "temporal_modulation_json", sql_type: "TEXT", category: "Modulation", description: "JSON [f32;5] energy in 5 modulation bands" },

    // ── Self-similarity (v13) ────────────────────────────────────────
    ColumnDef { name: "chroma_self_similarity_bandwidth", sql_type: "REAL", category: "Structure", description: "Harmonic repetitiveness (wide=repetitive)" },

    // ── Music understanding (v14) ────────────────────────────────────
    ColumnDef { name: "harmonic_percussive_ratio",    sql_type: "REAL", category: "Music",     description: "1.0=harmonic, 0.0=percussive" },
    ColumnDef { name: "chromagram_entropy",           sql_type: "REAL", category: "Music",     description: "Tonal complexity (0=simple, 2.5=chromatic)" },
    ColumnDef { name: "spectral_contrast_slope",      sql_type: "REAL", category: "Music",     description: "Contrast vs frequency trend" },
    ColumnDef { name: "spectral_contrast_range",      sql_type: "REAL", category: "Music",     description: "Contrast variation across bands" },
    ColumnDef { name: "onset_strength_contour_json",  sql_type: "TEXT", category: "Music",     description: "JSON [f32;4] DCT of rhythmic shape" },
    ColumnDef { name: "section_diversity_score",      sql_type: "REAL", category: "Music",     description: "How different sections are from each other" },

    // ── Musical key/chord ────────────────────────────────────────────
    ColumnDef { name: "estimated_key",         sql_type: "TEXT", category: "Musical",   description: "Detected key (e.g. 'D mixolydian')" },
    ColumnDef { name: "key_confidence",        sql_type: "REAL", category: "Musical",   description: "Key detection confidence" },
    ColumnDef { name: "tonality",              sql_type: "REAL", category: "Musical",   description: "Tonality measure" },
    ColumnDef { name: "harmonic_complexity",   sql_type: "REAL", category: "Musical",   description: "Harmonic complexity" },
    ColumnDef { name: "chord_count",           sql_type: "INT",  category: "Musical",   description: "Number of distinct chords" },
    ColumnDef { name: "chord_change_rate",     sql_type: "REAL", category: "Musical",   description: "Chord changes per second" },
    ColumnDef { name: "mode_clarity",          sql_type: "REAL", category: "Musical",   description: "Major/minor distinction clarity" },
    ColumnDef { name: "key_alternatives_count", sql_type: "INT", category: "Musical",   description: "Number of plausible alternative keys" },
    ColumnDef { name: "time_sig_numerator",    sql_type: "INT",  category: "Musical",   description: "Time signature numerator" },
    ColumnDef { name: "time_sig_denominator",  sql_type: "INT",  category: "Musical",   description: "Time signature denominator" },
    ColumnDef { name: "chroma_vector",         sql_type: "TEXT", category: "Musical",   description: "JSON [f64;12] pitch class distribution" },
    ColumnDef { name: "major_frame_ratio",     sql_type: "REAL", category: "Musical",   description: "Fraction of frames classified major (0-1)" },
    ColumnDef { name: "major_chord_ratio",     sql_type: "REAL", category: "Musical",   description: "Fraction of chords that are major (0-1)" },

    // ── Dynamics trajectory (v16) ────────────────────────────────────
    ColumnDef { name: "dynamics_entropy",      sql_type: "REAL", category: "Dynamics",  description: "LUFS histogram entropy (0-1)" },
    ColumnDef { name: "dynamics_slope",        sql_type: "REAL", category: "Dynamics",  description: "LUFS/minute linear trend (+ve=crescendo)" },
    ColumnDef { name: "dynamics_peak_count",   sql_type: "INT",  category: "Dynamics",  description: "Loudness peaks with >=3 LU prominence" },
    ColumnDef { name: "key_change_count",      sql_type: "INT",  category: "Dynamics",  description: "Key changes across 30s windows" },

    // ── Quality ──────────────────────────────────────────────────────
    ColumnDef { name: "recording_quality_score", sql_type: "REAL", category: "Quality", description: "Recording quality score" },
    ColumnDef { name: "snr_db",                  sql_type: "REAL", category: "Quality", description: "Signal-to-noise ratio (dB)" },
    ColumnDef { name: "clipping_ratio",          sql_type: "REAL", category: "Quality", description: "Fraction of clipped samples" },
    ColumnDef { name: "noise_floor_db",          sql_type: "REAL", category: "Quality", description: "Noise floor level (dB)" },

    // ── Segments / structure ─────────────────────────────────────────
    ColumnDef { name: "segment_count",        sql_type: "INT",  category: "Structure", description: "Number of structural segments" },
    ColumnDef { name: "temporal_complexity",   sql_type: "REAL", category: "Structure", description: "Temporal complexity" },
    ColumnDef { name: "coherence_score",       sql_type: "REAL", category: "Structure", description: "Overall coherence" },
    ColumnDef { name: "energy_shape",          sql_type: "TEXT", category: "Structure", description: "Energy profile shape (Flat/Peak/etc)" },
    ColumnDef { name: "peak_energy",           sql_type: "REAL", category: "Structure", description: "Maximum energy level" },
    ColumnDef { name: "energy_variance",       sql_type: "REAL", category: "Structure", description: "Energy level variance" },
    ColumnDef { name: "tension_build_count",   sql_type: "INT",  category: "Structure", description: "Number of tension build-ups" },
    ColumnDef { name: "tension_release_count", sql_type: "INT",  category: "Structure", description: "Number of tension releases" },
    ColumnDef { name: "repetition_count",      sql_type: "INT",  category: "Structure", description: "Number of repetitive sections" },
    ColumnDef { name: "repetition_similarity", sql_type: "REAL", category: "Structure", description: "How similar repeated sections are" },
    ColumnDef { name: "solo_section_count",    sql_type: "INT",  category: "Structure", description: "Number of solo sections (always 0)" },
    ColumnDef { name: "solo_section_ratio",    sql_type: "REAL", category: "Structure", description: "Solo fraction of duration (always 0)" },
    ColumnDef { name: "transition_count",      sql_type: "INT",  category: "Structure", description: "Number of section transitions" },

    // ── Classification ───────────────────────────────────────────────
    ColumnDef { name: "classification_music_score", sql_type: "REAL", category: "Classification", description: "Music vs non-music score" },
    ColumnDef { name: "hnr",                        sql_type: "REAL", category: "Classification", description: "Harmonics-to-noise ratio" },

    // ── Jam scores (0-100) ───────────────────────────────────────────
    ColumnDef { name: "energy_score",         sql_type: "REAL", category: "Score", description: "Energy jam score (0-100)" },
    ColumnDef { name: "intensity_score",      sql_type: "REAL", category: "Score", description: "Intensity v2 jam score (0-100)" },
    ColumnDef { name: "groove_score",         sql_type: "REAL", category: "Score", description: "Groove v5 jam score (0-100)" },
    ColumnDef { name: "improvisation_score",  sql_type: "REAL", category: "Score", description: "Improvisation v4 jam score (0-100)" },
    ColumnDef { name: "tightness_score",      sql_type: "REAL", category: "Score", description: "Tightness v4 jam score (0-100)" },
    ColumnDef { name: "build_quality_score",  sql_type: "REAL", category: "Score", description: "Build Quality v3 jam score (0-100)" },
    ColumnDef { name: "exploratory_score",    sql_type: "REAL", category: "Score", description: "Exploratory v4 jam score (0-100)" },
    ColumnDef { name: "transcendence_score",  sql_type: "REAL", category: "Score", description: "Transcendence v3 jam score (0-100)" },
    ColumnDef { name: "valence_score",        sql_type: "REAL", category: "Score", description: "Valence v5 jam score (0-100)" },
    ColumnDef { name: "arousal_score",        sql_type: "REAL", category: "Score", description: "Arousal v3 jam score (0-100)" },
];
