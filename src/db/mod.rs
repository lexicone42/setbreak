pub mod models;
pub mod queries;

use rusqlite::Connection;
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DbError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("Migration failed: {0}")]
    Migration(String),
}

pub type Result<T> = std::result::Result<T, DbError>;

pub struct Database {
    pub conn: Connection,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let conn = Connection::open(path)?;
        let db = Self { conn };
        db.init()?;
        Ok(db)
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let db = Self { conn };
        db.init()?;
        Ok(db)
    }

    fn init(&self) -> Result<()> {
        // WAL mode for better concurrent read performance
        self.conn.pragma_update(None, "journal_mode", "WAL")?;
        self.conn.pragma_update(None, "synchronous", "NORMAL")?;
        self.conn.pragma_update(None, "foreign_keys", "ON")?;
        self.migrate()?;
        Ok(())
    }

    fn migrate(&self) -> Result<()> {
        let version: i32 = self
            .conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap_or(0);

        if version < 1 {
            self.migrate_v1()?;
        }
        if version < 2 {
            self.migrate_v2()?;
        }
        if version < 3 {
            self.migrate_v3()?;
        }
        if version < 4 {
            self.migrate_v4()?;
        }
        if version < 5 {
            self.migrate_v5()?;
        }
        if version < 6 {
            self.migrate_v6()?;
        }
        if version < 7 {
            self.migrate_v7()?;
        }
        if version < 8 {
            self.migrate_v8()?;
        }
        if version < 9 {
            self.migrate_v9()?;
        }
        if version < 10 {
            self.migrate_v10()?;
        }
        if version < 11 {
            self.migrate_v11()?;
        }
        if version < 12 {
            self.migrate_v12()?;
        }
        if version < 13 {
            self.migrate_v13()?;
        }
        if version < 14 {
            self.migrate_v14()?;
        }

        self.conn.pragma_update(None, "user_version", 14)?;
        Ok(())
    }

    /// V1: Original schema — tracks + analysis_results
    fn migrate_v1(&self) -> Result<()> {
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS tracks (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                file_path       TEXT NOT NULL UNIQUE,
                file_size       INTEGER NOT NULL,
                file_modified   TEXT NOT NULL,
                format          TEXT NOT NULL,

                -- Metadata from tags
                title           TEXT,
                artist          TEXT,
                album           TEXT,
                date            TEXT,
                track_number    INTEGER,
                disc_number     INTEGER,
                set_name        TEXT,
                venue           TEXT,
                comment         TEXT,

                -- Metadata from filename parsing
                parsed_band     TEXT,
                parsed_date     TEXT,
                parsed_venue    TEXT,
                parsed_disc     INTEGER,
                parsed_track    INTEGER,
                parsed_set      TEXT,
                parsed_title    TEXT,

                -- Duration from tags (seconds), may be null before analysis
                duration_secs   REAL,

                created_at      TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at      TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE INDEX IF NOT EXISTS idx_tracks_artist ON tracks(artist);
            CREATE INDEX IF NOT EXISTS idx_tracks_parsed_band ON tracks(parsed_band);
            CREATE INDEX IF NOT EXISTS idx_tracks_parsed_date ON tracks(parsed_date);
            CREATE INDEX IF NOT EXISTS idx_tracks_format ON tracks(format);

            CREATE TABLE IF NOT EXISTS analysis_results (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                track_id        INTEGER NOT NULL UNIQUE REFERENCES tracks(id) ON DELETE CASCADE,

                -- Summary
                duration        REAL,
                sample_rate     INTEGER,
                channels        INTEGER,
                peak_amplitude  REAL,
                rms_level       REAL,
                dynamic_range   REAL,

                -- Spectral (aggregated)
                spectral_centroid_mean  REAL,
                spectral_centroid_std   REAL,
                spectral_flux_mean     REAL,
                spectral_flux_std      REAL,

                -- Temporal
                tempo_bpm              REAL,
                beat_count             INTEGER,
                onset_count            INTEGER,
                tempo_stability        REAL,
                rhythmic_complexity    REAL,

                -- Pitch
                mean_pitch             REAL,
                pitch_range_low        REAL,
                pitch_range_high       REAL,
                pitch_stability        REAL,

                -- Perceptual
                lufs_integrated        REAL,
                loudness_range         REAL,
                true_peak_dbfs         REAL,
                crest_factor           REAL,
                energy_level           REAL,

                -- Musical
                estimated_key          TEXT,
                key_confidence         REAL,
                tonality               REAL,
                harmonic_complexity    REAL,

                -- Quality
                recording_quality_score REAL,
                snr_db                  REAL,
                clipping_ratio          REAL,
                noise_floor_db          REAL,

                -- Segments
                segment_count           INTEGER,
                temporal_complexity     REAL,
                coherence_score         REAL,

                -- Jam-specific derived scores (0-100)
                energy_score           REAL,
                intensity_score        REAL,
                groove_score           REAL,
                improvisation_score    REAL,
                tightness_score        REAL,
                build_quality_score    REAL,
                exploratory_score      REAL,
                transcendence_score    REAL,

                analyzed_at     TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE INDEX IF NOT EXISTS idx_analysis_track ON analysis_results(track_id);
            CREATE INDEX IF NOT EXISTS idx_analysis_key ON analysis_results(estimated_key);
            CREATE INDEX IF NOT EXISTS idx_analysis_tempo ON analysis_results(tempo_bpm);
            CREATE INDEX IF NOT EXISTS idx_analysis_energy ON analysis_results(energy_score);
            ",
        )?;
        Ok(())
    }

    /// V2: Extended features + relational detail tables
    fn migrate_v2(&self) -> Result<()> {
        // New scalar columns on analysis_results
        let new_columns = [
            // Spectral (extended)
            "spectral_rolloff_mean REAL",
            "spectral_rolloff_std REAL",
            // Pitch (extended)
            "dominant_pitch REAL",
            "vibrato_presence REAL",
            "vibrato_rate REAL",
            "pitch_confidence_mean REAL",
            // Musical (extended)
            "chord_count INTEGER",
            "chord_change_rate REAL",
            "mode_clarity REAL",
            "key_alternatives_count INTEGER",
            "time_sig_numerator INTEGER",
            "time_sig_denominator INTEGER",
            "chroma_vector TEXT", // JSON [f32; 12]
            // Energy/tension profile
            "energy_shape TEXT",
            "peak_energy REAL",
            "energy_variance REAL",
            "tension_build_count INTEGER",
            "tension_release_count INTEGER",
            // Structure
            "repetition_count INTEGER",
            "repetition_similarity REAL",
            "solo_section_count INTEGER",
            "solo_section_ratio REAL",
            "transition_count INTEGER",
            // Classification
            "classification_music_score REAL",
            "hnr REAL",
        ];

        for col in &new_columns {
            // SQLite doesn't have IF NOT EXISTS for ALTER TABLE ADD COLUMN,
            // so we ignore "duplicate column" errors
            let sql = format!("ALTER TABLE analysis_results ADD COLUMN {col}");
            match self.conn.execute(&sql, []) {
                Ok(_) => {}
                Err(rusqlite::Error::SqliteFailure(err, _))
                    if err.code == rusqlite::ffi::ErrorCode::Unknown
                        || err.extended_code == 1 =>
                {
                    // Column already exists — fine
                }
                Err(e) => return Err(e.into()),
            }
        }

        // Relational detail tables
        self.conn.execute_batch(
            "
            -- Per-track chord events from analysis
            CREATE TABLE IF NOT EXISTS track_chords (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                track_id    INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
                chord       TEXT NOT NULL,
                start_time  REAL NOT NULL,
                duration    REAL NOT NULL,
                confidence  REAL,
                UNIQUE(track_id, start_time)
            );
            CREATE INDEX IF NOT EXISTS idx_chords_track ON track_chords(track_id);
            CREATE INDEX IF NOT EXISTS idx_chords_chord ON track_chords(chord);

            -- Per-track structural segments
            CREATE TABLE IF NOT EXISTS track_segments (
                id                  INTEGER PRIMARY KEY AUTOINCREMENT,
                track_id            INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
                segment_index       INTEGER NOT NULL,
                label               TEXT NOT NULL,
                section_type        TEXT,
                start_time          REAL NOT NULL,
                duration            REAL NOT NULL,
                energy              REAL,
                spectral_centroid   REAL,
                zcr                 REAL,
                key                 TEXT,
                tempo               REAL,
                dynamic_range       REAL,
                confidence          REAL,
                harmonic_stability  REAL,
                rhythmic_density    REAL,
                avg_brightness      REAL,
                dynamic_variation   REAL,
                UNIQUE(track_id, segment_index)
            );
            CREATE INDEX IF NOT EXISTS idx_segments_track ON track_segments(track_id);
            CREATE INDEX IF NOT EXISTS idx_segments_label ON track_segments(label);
            CREATE INDEX IF NOT EXISTS idx_segments_section ON track_segments(section_type);

            -- Per-track tension profile points
            CREATE TABLE IF NOT EXISTS track_tension_points (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                track_id    INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
                time        REAL NOT NULL,
                tension     REAL NOT NULL,
                change_type TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_tension_track ON track_tension_points(track_id);

            -- Per-track transitions between segments
            CREATE TABLE IF NOT EXISTS track_transitions (
                id                INTEGER PRIMARY KEY AUTOINCREMENT,
                track_id          INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
                time              REAL NOT NULL,
                transition_type   TEXT NOT NULL,
                strength          REAL,
                duration          REAL
            );
            CREATE INDEX IF NOT EXISTS idx_transitions_track ON track_transitions(track_id);
            ",
        )?;

        Ok(())
    }

    /// V3: Spectral features for bliss-audio parity (flatness, ZCR, MFCC)
    fn migrate_v3(&self) -> Result<()> {
        let new_columns = [
            "spectral_flatness_mean REAL",
            "spectral_flatness_std REAL",
            "zcr_mean REAL",
            "zcr_std REAL",
            "mfcc_0_mean REAL",
            "mfcc_0_std REAL",
            "mfcc_1_mean REAL",
            "mfcc_1_std REAL",
            "mfcc_2_mean REAL",
            "mfcc_2_std REAL",
            "mfcc_3_mean REAL",
            "mfcc_3_std REAL",
            "mfcc_4_mean REAL",
            "mfcc_4_std REAL",
            "mfcc_5_mean REAL",
            "mfcc_5_std REAL",
            "mfcc_6_mean REAL",
            "mfcc_6_std REAL",
            "mfcc_7_mean REAL",
            "mfcc_7_std REAL",
            "mfcc_8_mean REAL",
            "mfcc_8_std REAL",
            "mfcc_9_mean REAL",
            "mfcc_9_std REAL",
            "mfcc_10_mean REAL",
            "mfcc_10_std REAL",
            "mfcc_11_mean REAL",
            "mfcc_11_std REAL",
            "mfcc_12_mean REAL",
            "mfcc_12_std REAL",
        ];

        for col in &new_columns {
            let sql = format!("ALTER TABLE analysis_results ADD COLUMN {col}");
            match self.conn.execute(&sql, []) {
                Ok(_) => {}
                Err(rusqlite::Error::SqliteFailure(err, _))
                    if err.code == rusqlite::ffi::ErrorCode::Unknown
                        || err.extended_code == 1 =>
                {
                    // Column already exists — fine
                }
                Err(e) => return Err(e.into()),
            }
        }

        Ok(())
    }

    /// V4: Spectral bandwidth, sub-band energy ratios, valence/arousal
    fn migrate_v4(&self) -> Result<()> {
        let new_columns = [
            "spectral_bandwidth_mean REAL",
            "spectral_bandwidth_std REAL",
            "sub_band_bass_mean REAL",
            "sub_band_bass_std REAL",
            "sub_band_mid_mean REAL",
            "sub_band_mid_std REAL",
            "sub_band_high_mean REAL",
            "sub_band_high_std REAL",
            "sub_band_presence_mean REAL",
            "sub_band_presence_std REAL",
            "valence_score REAL",
            "arousal_score REAL",
        ];

        for col in &new_columns {
            let sql = format!("ALTER TABLE analysis_results ADD COLUMN {col}");
            match self.conn.execute(&sql, []) {
                Ok(_) => {}
                Err(rusqlite::Error::SqliteFailure(err, _))
                    if err.code == rusqlite::ffi::ErrorCode::Unknown
                        || err.extended_code == 1 =>
                {
                    // Column already exists — fine
                }
                Err(e) => return Err(e.into()),
            }
        }

        Ok(())
    }

    /// V5: Track similarity table for nearest-neighbor queries
    fn migrate_v5(&self) -> Result<()> {
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS track_similarity (
                track_id         INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
                similar_track_id INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
                distance         REAL NOT NULL,
                rank             INTEGER NOT NULL,
                PRIMARY KEY (track_id, similar_track_id)
            );
            CREATE INDEX IF NOT EXISTS idx_similarity_track ON track_similarity(track_id, rank);
            CREATE INDEX IF NOT EXISTS idx_similarity_similar ON track_similarity(similar_track_id);
            ",
        )?;
        Ok(())
    }

    /// V6: Archive.org show cache for collection discovery
    fn migrate_v6(&self) -> Result<()> {
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS archive_shows (
                identifier      TEXT PRIMARY KEY,
                collection      TEXT NOT NULL,
                date            TEXT NOT NULL,
                title           TEXT NOT NULL DEFAULT '',
                source_quality  INTEGER NOT NULL DEFAULT 0,
                format_quality  INTEGER NOT NULL DEFAULT 0,
                fetched_at      TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_archive_collection ON archive_shows(collection);
            CREATE INDEX IF NOT EXISTS idx_archive_date ON archive_shows(date);
            ",
        )?;
        Ok(())
    }

    /// V7: Recording type classification (live / studio / live_album)
    fn migrate_v7(&self) -> Result<()> {
        try_add_column(&self.conn, "tracks", "recording_type TEXT")?;
        Ok(())
    }

    /// V8: Data quality classification (ok / suspect / garbage)
    fn migrate_v8(&self) -> Result<()> {
        try_add_column(&self.conn, "tracks", "data_quality TEXT DEFAULT 'ok'")?;
        Ok(())
    }

    /// V9: Perceptual loudness dynamics + new per-frame derived features
    fn migrate_v9(&self) -> Result<()> {
        try_add_column(&self.conn, "analysis_results", "loudness_std REAL")?;
        try_add_column(&self.conn, "analysis_results", "peak_loudness REAL")?;
        try_add_column(&self.conn, "analysis_results", "spectral_flux_skewness REAL")?;
        try_add_column(&self.conn, "analysis_results", "spectral_centroid_slope REAL")?;
        try_add_column(&self.conn, "analysis_results", "energy_buildup_ratio REAL")?;
        try_add_column(&self.conn, "analysis_results", "bass_treble_ratio_mean REAL")?;
        try_add_column(&self.conn, "analysis_results", "bass_treble_ratio_std REAL")?;
        try_add_column(&self.conn, "analysis_results", "onset_density_std REAL")?;
        try_add_column(&self.conn, "analysis_results", "loudness_buildup_slope REAL")?;
        try_add_column(&self.conn, "analysis_results", "peak_energy_time REAL")?;
        Ok(())
    }

    /// V10: Pitch-derived + creative per-frame features
    fn migrate_v10(&self) -> Result<()> {
        // Pitch-derived
        try_add_column(&self.conn, "analysis_results", "pitch_contour_std REAL")?;
        try_add_column(&self.conn, "analysis_results", "pitch_clarity_mean REAL")?;
        try_add_column(&self.conn, "analysis_results", "pitched_frame_ratio REAL")?;
        // Creative derivations
        try_add_column(&self.conn, "analysis_results", "mfcc_flux_mean REAL")?;
        try_add_column(&self.conn, "analysis_results", "onset_interval_entropy REAL")?;
        try_add_column(&self.conn, "analysis_results", "spectral_centroid_kurtosis REAL")?;
        try_add_column(&self.conn, "analysis_results", "bass_energy_slope REAL")?;
        try_add_column(&self.conn, "analysis_results", "spectral_bandwidth_slope REAL")?;
        try_add_column(&self.conn, "analysis_results", "loudness_dynamic_spread REAL")?;
        Ok(())
    }

    /// V11: Beat timing, tension/energy depth, periodicity, cross-feature features
    fn migrate_v11(&self) -> Result<()> {
        try_add_column(&self.conn, "analysis_results", "beat_regularity REAL")?;
        try_add_column(&self.conn, "analysis_results", "peak_tension REAL")?;
        try_add_column(&self.conn, "analysis_results", "tension_range REAL")?;
        try_add_column(&self.conn, "analysis_results", "energy_peak_count INTEGER")?;
        try_add_column(&self.conn, "analysis_results", "energy_valley_depth_mean REAL")?;
        try_add_column(&self.conn, "analysis_results", "rhythmic_periodicity_strength REAL")?;
        try_add_column(&self.conn, "analysis_results", "spectral_loudness_correlation REAL")?;
        Ok(())
    }
    /// V12: Spectral shape descriptors, sub-band flux, chroma/Tonnetz, beat-sync rhythm
    fn migrate_v12(&self) -> Result<()> {
        // Spectral shape descriptors (from STFT higher moments)
        try_add_column(&self.conn, "analysis_results", "spectral_skewness_mean REAL")?;
        try_add_column(&self.conn, "analysis_results", "spectral_kurtosis_mean REAL")?;
        try_add_column(&self.conn, "analysis_results", "spectral_entropy_mean REAL")?;
        try_add_column(&self.conn, "analysis_results", "spectral_entropy_std REAL")?;
        try_add_column(&self.conn, "analysis_results", "spectral_slope_mean REAL")?;
        try_add_column(&self.conn, "analysis_results", "spectral_contrast_json TEXT")?;
        // Sub-band spectral flux
        try_add_column(&self.conn, "analysis_results", "sub_band_flux_bass_mean REAL")?;
        try_add_column(&self.conn, "analysis_results", "sub_band_flux_bass_std REAL")?;
        try_add_column(&self.conn, "analysis_results", "sub_band_flux_mid_mean REAL")?;
        try_add_column(&self.conn, "analysis_results", "sub_band_flux_high_mean REAL")?;
        // Chromagram and harmonic features
        try_add_column(&self.conn, "analysis_results", "tonnetz_json TEXT")?;
        try_add_column(&self.conn, "analysis_results", "tonnetz_flux_mean REAL")?;
        try_add_column(&self.conn, "analysis_results", "chroma_flux_mean REAL")?;
        // Beat-synchronous rhythm features
        try_add_column(&self.conn, "analysis_results", "beat_pattern_json TEXT")?;
        try_add_column(&self.conn, "analysis_results", "syncopation REAL")?;
        try_add_column(&self.conn, "analysis_results", "pulse_clarity REAL")?;
        try_add_column(&self.conn, "analysis_results", "offbeat_ratio REAL")?;
        Ok(())
    }

    /// V13: Timbral texture, MFCC dynamics, stereo, attack/decay, rhythm micro, modulation, SSM
    fn migrate_v13(&self) -> Result<()> {
        // Timbral texture descriptors
        try_add_column(&self.conn, "analysis_results", "spectral_spread_mean REAL")?;
        try_add_column(&self.conn, "analysis_results", "spectral_spread_std REAL")?;
        try_add_column(&self.conn, "analysis_results", "spectral_crest_mean REAL")?;
        try_add_column(&self.conn, "analysis_results", "spectral_crest_std REAL")?;
        try_add_column(&self.conn, "analysis_results", "roughness_mean REAL")?;
        try_add_column(&self.conn, "analysis_results", "roughness_std REAL")?;
        // MFCC dynamics
        try_add_column(&self.conn, "analysis_results", "mfcc_delta_mean_json TEXT")?;
        try_add_column(&self.conn, "analysis_results", "mfcc_delta_delta_mean_json TEXT")?;
        // Stereo characteristics
        try_add_column(&self.conn, "analysis_results", "stereo_width_mean REAL")?;
        try_add_column(&self.conn, "analysis_results", "stereo_width_std REAL")?;
        // Onset envelope characteristics
        try_add_column(&self.conn, "analysis_results", "attack_time_mean REAL")?;
        try_add_column(&self.conn, "analysis_results", "attack_time_std REAL")?;
        try_add_column(&self.conn, "analysis_results", "decay_time_mean REAL")?;
        try_add_column(&self.conn, "analysis_results", "decay_time_std REAL")?;
        try_add_column(&self.conn, "analysis_results", "onset_strength_mean REAL")?;
        try_add_column(&self.conn, "analysis_results", "onset_strength_std REAL")?;
        try_add_column(&self.conn, "analysis_results", "onset_strength_skewness REAL")?;
        // Rhythm micro-features
        try_add_column(&self.conn, "analysis_results", "swing_ratio REAL")?;
        try_add_column(&self.conn, "analysis_results", "microtiming_deviation_mean REAL")?;
        try_add_column(&self.conn, "analysis_results", "microtiming_deviation_std REAL")?;
        try_add_column(&self.conn, "analysis_results", "microtiming_bias REAL")?;
        // Temporal modulation spectrum
        try_add_column(&self.conn, "analysis_results", "temporal_modulation_json TEXT")?;
        // Self-similarity structure
        try_add_column(&self.conn, "analysis_results", "chroma_self_similarity_bandwidth REAL")?;
        Ok(())
    }

    /// V14: Music understanding features — H/P ratio, chromagram entropy, contrast stats,
    /// onset contour shape, section diversity
    fn migrate_v14(&self) -> Result<()> {
        try_add_column(&self.conn, "analysis_results", "harmonic_percussive_ratio REAL")?;
        try_add_column(&self.conn, "analysis_results", "chromagram_entropy REAL")?;
        try_add_column(&self.conn, "analysis_results", "spectral_contrast_slope REAL")?;
        try_add_column(&self.conn, "analysis_results", "spectral_contrast_range REAL")?;
        try_add_column(&self.conn, "analysis_results", "onset_strength_contour_json TEXT")?;
        try_add_column(&self.conn, "analysis_results", "section_diversity_score REAL")?;
        Ok(())
    }
}

/// Helper: try to add a column, ignore if it already exists.
#[allow(dead_code)]
fn try_add_column(conn: &Connection, table: &str, column_def: &str) -> Result<()> {
    let sql = format!("ALTER TABLE {table} ADD COLUMN {column_def}");
    match conn.execute(&sql, []) {
        Ok(_) | Err(rusqlite::Error::SqliteFailure(_, _)) => Ok(()),
        Err(e) => Err(e.into()),
    }
}
