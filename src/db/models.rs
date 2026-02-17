/// Data for inserting or updating a track (scan phase).
pub struct NewTrack {
    pub file_path: String,
    pub file_size: i64,
    pub file_modified: String,
    pub format: String,

    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub date: Option<String>,
    pub track_number: Option<i32>,
    pub disc_number: Option<i32>,
    pub set_name: Option<String>,
    pub venue: Option<String>,
    pub comment: Option<String>,

    pub parsed_band: Option<String>,
    pub parsed_date: Option<String>,
    pub parsed_venue: Option<String>,
    pub parsed_disc: Option<i32>,
    pub parsed_track: Option<i32>,
    pub parsed_set: Option<String>,
    pub parsed_title: Option<String>,

    pub duration_secs: Option<f64>,
    pub recording_type: Option<String>,
}

/// A track row read from the database.
#[derive(Debug, Clone)]
pub struct Track {
    pub id: i64,
    pub file_path: String,
    pub format: String,
    pub artist: Option<String>,
    pub parsed_band: Option<String>,
    pub parsed_date: Option<String>,
}

/// Analysis results to store for a track.
#[derive(Default)]
pub struct NewAnalysis {
    pub track_id: i64,

    // Summary
    pub duration: Option<f64>,
    pub sample_rate: Option<i32>,
    pub channels: Option<i32>,
    pub peak_amplitude: Option<f64>,
    pub rms_level: Option<f64>,
    pub dynamic_range: Option<f64>,

    // Spectral
    pub spectral_centroid_mean: Option<f64>,
    pub spectral_centroid_std: Option<f64>,
    pub spectral_flux_mean: Option<f64>,
    pub spectral_flux_std: Option<f64>,
    pub spectral_rolloff_mean: Option<f64>,
    pub spectral_rolloff_std: Option<f64>,
    pub spectral_flatness_mean: Option<f64>,
    pub spectral_flatness_std: Option<f64>,
    pub spectral_bandwidth_mean: Option<f64>,
    pub spectral_bandwidth_std: Option<f64>,
    pub zcr_mean: Option<f64>,
    pub zcr_std: Option<f64>,
    pub sub_band_bass_mean: Option<f64>,
    pub sub_band_bass_std: Option<f64>,
    pub sub_band_mid_mean: Option<f64>,
    pub sub_band_mid_std: Option<f64>,
    pub sub_band_high_mean: Option<f64>,
    pub sub_band_high_std: Option<f64>,
    pub sub_band_presence_mean: Option<f64>,
    pub sub_band_presence_std: Option<f64>,
    pub mfcc_0_mean: Option<f64>,
    pub mfcc_0_std: Option<f64>,
    pub mfcc_1_mean: Option<f64>,
    pub mfcc_1_std: Option<f64>,
    pub mfcc_2_mean: Option<f64>,
    pub mfcc_2_std: Option<f64>,
    pub mfcc_3_mean: Option<f64>,
    pub mfcc_3_std: Option<f64>,
    pub mfcc_4_mean: Option<f64>,
    pub mfcc_4_std: Option<f64>,
    pub mfcc_5_mean: Option<f64>,
    pub mfcc_5_std: Option<f64>,
    pub mfcc_6_mean: Option<f64>,
    pub mfcc_6_std: Option<f64>,
    pub mfcc_7_mean: Option<f64>,
    pub mfcc_7_std: Option<f64>,
    pub mfcc_8_mean: Option<f64>,
    pub mfcc_8_std: Option<f64>,
    pub mfcc_9_mean: Option<f64>,
    pub mfcc_9_std: Option<f64>,
    pub mfcc_10_mean: Option<f64>,
    pub mfcc_10_std: Option<f64>,
    pub mfcc_11_mean: Option<f64>,
    pub mfcc_11_std: Option<f64>,
    pub mfcc_12_mean: Option<f64>,
    pub mfcc_12_std: Option<f64>,

    // Temporal
    pub tempo_bpm: Option<f64>,
    pub beat_count: Option<i32>,
    pub onset_count: Option<i32>,
    pub tempo_stability: Option<f64>,
    pub rhythmic_complexity: Option<f64>,

    // Pitch
    pub mean_pitch: Option<f64>,
    pub pitch_range_low: Option<f64>,
    pub pitch_range_high: Option<f64>,
    pub pitch_stability: Option<f64>,
    pub dominant_pitch: Option<f64>,
    pub vibrato_presence: Option<f64>,
    pub vibrato_rate: Option<f64>,
    pub pitch_confidence_mean: Option<f64>,

    // Perceptual
    pub lufs_integrated: Option<f64>,
    pub loudness_range: Option<f64>,
    pub true_peak_dbfs: Option<f64>,
    pub crest_factor: Option<f64>,
    pub energy_level: Option<f64>,

    // Musical
    pub estimated_key: Option<String>,
    pub key_confidence: Option<f64>,
    pub tonality: Option<f64>,
    pub harmonic_complexity: Option<f64>,
    pub chord_count: Option<i32>,
    pub chord_change_rate: Option<f64>,
    pub mode_clarity: Option<f64>,
    pub key_alternatives_count: Option<i32>,
    pub time_sig_numerator: Option<i32>,
    pub time_sig_denominator: Option<i32>,
    pub chroma_vector: Option<String>, // JSON [f64; 12]

    // Quality
    pub recording_quality_score: Option<f64>,
    pub snr_db: Option<f64>,
    pub clipping_ratio: Option<f64>,
    pub noise_floor_db: Option<f64>,

    // Segments
    pub segment_count: Option<i32>,
    pub temporal_complexity: Option<f64>,
    pub coherence_score: Option<f64>,

    // Energy/tension profile
    pub energy_shape: Option<String>,
    pub peak_energy: Option<f64>,
    pub energy_variance: Option<f64>,
    pub tension_build_count: Option<i32>,
    pub tension_release_count: Option<i32>,

    // Structure
    pub repetition_count: Option<i32>,
    pub repetition_similarity: Option<f64>,
    pub solo_section_count: Option<i32>,
    pub solo_section_ratio: Option<f64>,
    pub transition_count: Option<i32>,

    // Classification
    pub classification_music_score: Option<f64>,
    pub hnr: Option<f64>,

    // Emotion (Russell circumplex)
    pub valence_score: Option<f64>,
    pub arousal_score: Option<f64>,

    // Jam-specific derived scores (0-100)
    pub energy_score: Option<f64>,
    pub intensity_score: Option<f64>,
    pub groove_score: Option<f64>,
    pub improvisation_score: Option<f64>,
    pub tightness_score: Option<f64>,
    pub build_quality_score: Option<f64>,
    pub exploratory_score: Option<f64>,
    pub transcendence_score: Option<f64>,
}

/// Chord event for relational storage.
pub struct ChordEvent {
    pub track_id: i64,
    pub chord: String,
    pub start_time: f64,
    pub duration: f64,
    pub confidence: Option<f64>,
}

/// Segment for relational storage.
pub struct SegmentRecord {
    pub track_id: i64,
    pub segment_index: i32,
    pub label: String,
    pub section_type: Option<String>,
    pub start_time: f64,
    pub duration: f64,
    pub energy: Option<f64>,
    pub spectral_centroid: Option<f64>,
    pub zcr: Option<f64>,
    pub key: Option<String>,
    pub tempo: Option<f64>,
    pub dynamic_range: Option<f64>,
    pub confidence: Option<f64>,
    pub harmonic_stability: Option<f64>,
    pub rhythmic_density: Option<f64>,
    pub avg_brightness: Option<f64>,
    pub dynamic_variation: Option<f64>,
}

/// Tension point for relational storage.
pub struct TensionPointRecord {
    pub track_id: i64,
    pub time: f64,
    pub tension: f64,
    pub change_type: String,
}

/// Transition for relational storage.
pub struct TransitionRecord {
    pub track_id: i64,
    pub time: f64,
    pub transition_type: String,
    pub strength: Option<f64>,
    pub duration: Option<f64>,
}

/// A track with its jam scores (for query display).
#[derive(Debug, Clone)]
pub struct TrackScore {
    pub title: String,
    pub date: String,
    pub duration_min: f64,
    pub key: Option<String>,
    pub tempo: Option<f64>,
    pub energy: f64,
    pub intensity: f64,
    pub groove: f64,
    pub improvisation: f64,
    pub tightness: f64,
    pub build_quality: f64,
    pub exploratory: f64,
    pub transcendence: f64,
    pub valence: f64,
    pub arousal: f64,
}

/// A chain of consecutive tracks connected by segue markers (->).
#[derive(Debug, Clone)]
pub struct ChainScore {
    pub date: String,
    pub songs: Vec<String>,
    pub chain_length: usize,
    pub duration_min: f64,
    pub energy: f64,
    pub intensity: f64,
    pub groove: f64,
    pub improvisation: f64,
    pub tightness: f64,
    pub build_quality: f64,
    pub exploratory: f64,
    pub transcendence: f64,
    pub valence: f64,
    pub arousal: f64,
}

impl ChainScore {
    /// Build a ChainScore from a slice of consecutive segued tracks.
    /// Scores are duration-weighted averages (longer jams contribute more).
    pub fn from_tracks(tracks: &[TrackScore]) -> Self {
        let total_dur: f64 = tracks.iter().map(|t| t.duration_min).sum();
        let safe_dur = if total_dur > 0.0 { total_dur } else { 1.0 };

        let wavg = |f: fn(&TrackScore) -> f64| -> f64 {
            tracks.iter().map(|t| f(t) * t.duration_min).sum::<f64>() / safe_dur
        };

        // Strip segue markers from song titles for display
        let songs: Vec<String> = tracks
            .iter()
            .map(|t| strip_segue_marker(&t.title))
            .collect();

        Self {
            date: tracks[0].date.clone(),
            songs,
            chain_length: tracks.len(),
            duration_min: total_dur,
            energy: wavg(|t| t.energy),
            intensity: wavg(|t| t.intensity),
            groove: wavg(|t| t.groove),
            improvisation: wavg(|t| t.improvisation),
            tightness: wavg(|t| t.tightness),
            build_quality: wavg(|t| t.build_quality),
            exploratory: wavg(|t| t.exploratory),
            transcendence: wavg(|t| t.transcendence),
            valence: wavg(|t| t.valence),
            arousal: wavg(|t| t.arousal),
        }
    }

    /// Human-readable chain title: "Dark Star -> St. Stephen -> The Eleven"
    pub fn chain_title(&self) -> String {
        self.songs.join(" -> ")
    }
}

/// Strip trailing segue markers from a song title.
fn strip_segue_marker(title: &str) -> String {
    let t = title.trim_end();
    for marker in &[" -->", "-->", " ->", "->", " >"] {
        if let Some(stripped) = t.strip_suffix(marker) {
            return stripped.trim_end().to_string();
        }
    }
    t.to_string()
}

/// An archive.org show entry (cached in DB).
#[derive(Debug, Clone)]
pub struct ArchiveShow {
    pub identifier: String,
    pub collection: String,
    pub date: String,
    pub title: String,
    pub source_quality: i32, // sbd=3, matrix=2, aud=1
    pub format_quality: i32, // flac=3, shn=2, mp3=1
}

/// A missing show with best available tape info.
#[derive(Debug, Clone)]
pub struct MissingShow {
    pub date: String,
    pub best_identifier: String,
    pub title: String,
    pub source_quality: i32,
    pub format_quality: i32,
    pub tape_count: usize,
}

/// Library statistics.
#[derive(Debug)]
pub struct LibraryStats {
    pub total_tracks: i64,
    pub analyzed_tracks: i64,
    pub total_duration_hours: f64,
    pub formats: Vec<(String, i64)>,
    pub bands: Vec<(String, i64)>,
}

/// A row of calibration data: track scores + LUFS + show grouping key.
pub struct CalibrationRow {
    pub track_id: i64,
    pub lufs: f64,
    pub scores: [Option<f64>; 10], // energy, intensity, groove, improv, tight, build, explor, trans, valence, arousal
    pub parsed_date: String,
    pub parsed_band: Option<String>,
}
