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

        self.conn.pragma_update(None, "user_version", 4)?;
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
