use super::models::{
    ChordEvent, LibraryStats, NewAnalysis, NewTrack, SegmentRecord, TensionPointRecord, Track,
    TrackScore, TransitionRecord,
};
use super::{Database, Result};
use rusqlite::params;

impl Database {
    /// Insert or update a track. Returns the track id.
    pub fn upsert_track(&self, t: &NewTrack) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO tracks (
                file_path, file_size, file_modified, format,
                title, artist, album, date, track_number, disc_number,
                set_name, venue, comment,
                parsed_band, parsed_date, parsed_venue, parsed_disc,
                parsed_track, parsed_set, parsed_title, duration_secs,
                updated_at
            ) VALUES (
                ?1, ?2, ?3, ?4,
                ?5, ?6, ?7, ?8, ?9, ?10,
                ?11, ?12, ?13,
                ?14, ?15, ?16, ?17,
                ?18, ?19, ?20, ?21,
                datetime('now')
            )
            ON CONFLICT(file_path) DO UPDATE SET
                file_size = excluded.file_size,
                file_modified = excluded.file_modified,
                format = excluded.format,
                title = excluded.title,
                artist = excluded.artist,
                album = excluded.album,
                date = excluded.date,
                track_number = excluded.track_number,
                disc_number = excluded.disc_number,
                set_name = excluded.set_name,
                venue = excluded.venue,
                comment = excluded.comment,
                parsed_band = excluded.parsed_band,
                parsed_date = excluded.parsed_date,
                parsed_venue = excluded.parsed_venue,
                parsed_disc = excluded.parsed_disc,
                parsed_track = excluded.parsed_track,
                parsed_set = excluded.parsed_set,
                parsed_title = excluded.parsed_title,
                duration_secs = excluded.duration_secs,
                updated_at = datetime('now')
            ",
            params![
                t.file_path, t.file_size, t.file_modified, t.format,
                t.title, t.artist, t.album, t.date, t.track_number, t.disc_number,
                t.set_name, t.venue, t.comment,
                t.parsed_band, t.parsed_date, t.parsed_venue, t.parsed_disc,
                t.parsed_track, t.parsed_set, t.parsed_title, t.duration_secs,
            ],
        )?;

        let id: i64 = self.conn.query_row(
            "SELECT id FROM tracks WHERE file_path = ?1",
            params![t.file_path],
            |row| row.get(0),
        )?;
        Ok(id)
    }

    /// Get all tracks that have not been analyzed yet.
    pub fn get_unanalyzed_tracks(&self) -> Result<Vec<Track>> {
        let mut stmt = self.conn.prepare(
            "SELECT t.id, t.file_path, t.format, t.artist, t.parsed_band, t.parsed_date
             FROM tracks t
             LEFT JOIN analysis_results a ON a.track_id = t.id
             WHERE a.id IS NULL
             ORDER BY t.id",
        )?;

        let tracks = stmt
            .query_map([], |row| {
                Ok(Track {
                    id: row.get(0)?,
                    file_path: row.get(1)?,
                    format: row.get(2)?,
                    artist: row.get(3)?,
                    parsed_band: row.get(4)?,
                    parsed_date: row.get(5)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(tracks)
    }

    /// Get all tracks (for --force re-analysis).
    pub fn get_all_tracks(&self) -> Result<Vec<Track>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, file_path, format, artist, parsed_band, parsed_date
             FROM tracks ORDER BY id",
        )?;

        let tracks = stmt
            .query_map([], |row| {
                Ok(Track {
                    id: row.get(0)?,
                    file_path: row.get(1)?,
                    format: row.get(2)?,
                    artist: row.get(3)?,
                    parsed_band: row.get(4)?,
                    parsed_date: row.get(5)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(tracks)
    }

    /// Store all analysis data for a track in a single transaction.
    /// This includes the main analysis row plus relational detail tables.
    pub fn store_full_analysis(
        &self,
        a: &NewAnalysis,
        chords: &[ChordEvent],
        segments: &[SegmentRecord],
        tension: &[TensionPointRecord],
        transitions: &[TransitionRecord],
    ) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;

        // Main analysis row
        Self::store_analysis_row(&tx, a)?;

        // Clear old detail rows (for re-analysis)
        tx.execute("DELETE FROM track_chords WHERE track_id = ?1", params![a.track_id])?;
        tx.execute("DELETE FROM track_segments WHERE track_id = ?1", params![a.track_id])?;
        tx.execute("DELETE FROM track_tension_points WHERE track_id = ?1", params![a.track_id])?;
        tx.execute("DELETE FROM track_transitions WHERE track_id = ?1", params![a.track_id])?;

        // Batch insert chords
        if !chords.is_empty() {
            let mut stmt = tx.prepare_cached(
                "INSERT INTO track_chords (track_id, chord, start_time, duration, confidence)
                 VALUES (?1, ?2, ?3, ?4, ?5)"
            )?;
            for c in chords {
                stmt.execute(params![c.track_id, c.chord, c.start_time, c.duration, c.confidence])?;
            }
        }

        // Batch insert segments
        if !segments.is_empty() {
            let mut stmt = tx.prepare_cached(
                "INSERT INTO track_segments (
                    track_id, segment_index, label, section_type, start_time, duration,
                    energy, spectral_centroid, zcr, key, tempo, dynamic_range, confidence,
                    harmonic_stability, rhythmic_density, avg_brightness, dynamic_variation
                 ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)"
            )?;
            for s in segments {
                stmt.execute(params![
                    s.track_id, s.segment_index, s.label, s.section_type,
                    s.start_time, s.duration, s.energy, s.spectral_centroid, s.zcr,
                    s.key, s.tempo, s.dynamic_range, s.confidence,
                    s.harmonic_stability, s.rhythmic_density, s.avg_brightness, s.dynamic_variation,
                ])?;
            }
        }

        // Batch insert tension points
        if !tension.is_empty() {
            let mut stmt = tx.prepare_cached(
                "INSERT INTO track_tension_points (track_id, time, tension, change_type)
                 VALUES (?1, ?2, ?3, ?4)"
            )?;
            for t in tension {
                stmt.execute(params![t.track_id, t.time, t.tension, t.change_type])?;
            }
        }

        // Batch insert transitions
        if !transitions.is_empty() {
            let mut stmt = tx.prepare_cached(
                "INSERT INTO track_transitions (track_id, time, transition_type, strength, duration)
                 VALUES (?1, ?2, ?3, ?4, ?5)"
            )?;
            for t in transitions {
                stmt.execute(params![t.track_id, t.time, t.transition_type, t.strength, t.duration])?;
            }
        }

        tx.commit()?;
        Ok(())
    }

    /// Store the main analysis_results row (used within a transaction).
    fn store_analysis_row(conn: &rusqlite::Connection, a: &NewAnalysis) -> Result<()> {
        conn.execute(
            "INSERT INTO analysis_results (
                track_id,
                duration, sample_rate, channels, peak_amplitude, rms_level, dynamic_range,
                spectral_centroid_mean, spectral_centroid_std, spectral_flux_mean, spectral_flux_std,
                spectral_rolloff_mean, spectral_rolloff_std,
                spectral_flatness_mean, spectral_flatness_std,
                spectral_bandwidth_mean, spectral_bandwidth_std,
                zcr_mean, zcr_std,
                sub_band_bass_mean, sub_band_bass_std,
                sub_band_mid_mean, sub_band_mid_std,
                sub_band_high_mean, sub_band_high_std,
                sub_band_presence_mean, sub_band_presence_std,
                mfcc_0_mean, mfcc_0_std, mfcc_1_mean, mfcc_1_std,
                mfcc_2_mean, mfcc_2_std, mfcc_3_mean, mfcc_3_std,
                mfcc_4_mean, mfcc_4_std, mfcc_5_mean, mfcc_5_std,
                mfcc_6_mean, mfcc_6_std, mfcc_7_mean, mfcc_7_std,
                mfcc_8_mean, mfcc_8_std, mfcc_9_mean, mfcc_9_std,
                mfcc_10_mean, mfcc_10_std, mfcc_11_mean, mfcc_11_std,
                mfcc_12_mean, mfcc_12_std,
                tempo_bpm, beat_count, onset_count, tempo_stability, rhythmic_complexity,
                mean_pitch, pitch_range_low, pitch_range_high, pitch_stability,
                dominant_pitch, vibrato_presence, vibrato_rate, pitch_confidence_mean,
                lufs_integrated, loudness_range, true_peak_dbfs, crest_factor, energy_level,
                estimated_key, key_confidence, tonality, harmonic_complexity,
                chord_count, chord_change_rate, mode_clarity, key_alternatives_count,
                time_sig_numerator, time_sig_denominator, chroma_vector,
                recording_quality_score, snr_db, clipping_ratio, noise_floor_db,
                segment_count, temporal_complexity, coherence_score,
                energy_shape, peak_energy, energy_variance,
                tension_build_count, tension_release_count,
                repetition_count, repetition_similarity,
                solo_section_count, solo_section_ratio, transition_count,
                classification_music_score, hnr,
                valence_score, arousal_score,
                energy_score, intensity_score, groove_score,
                improvisation_score, tightness_score, build_quality_score,
                exploratory_score, transcendence_score,
                analyzed_at
            ) VALUES (
                ?1,
                ?2, ?3, ?4, ?5, ?6, ?7,
                ?8, ?9, ?10, ?11,
                ?12, ?13,
                ?14, ?15,
                ?16, ?17,
                ?18, ?19,
                ?20, ?21,
                ?22, ?23,
                ?24, ?25,
                ?26, ?27,
                ?28, ?29, ?30, ?31,
                ?32, ?33, ?34, ?35,
                ?36, ?37, ?38, ?39,
                ?40, ?41, ?42, ?43,
                ?44, ?45, ?46, ?47,
                ?48, ?49, ?50, ?51,
                ?52, ?53,
                ?54, ?55, ?56, ?57, ?58,
                ?59, ?60, ?61, ?62,
                ?63, ?64, ?65, ?66,
                ?67, ?68, ?69, ?70, ?71,
                ?72, ?73, ?74, ?75,
                ?76, ?77, ?78, ?79,
                ?80, ?81, ?82,
                ?83, ?84, ?85, ?86,
                ?87, ?88, ?89,
                ?90, ?91, ?92,
                ?93, ?94,
                ?95, ?96,
                ?97, ?98, ?99,
                ?100, ?101,
                ?102, ?103,
                ?104, ?105, ?106,
                ?107, ?108, ?109,
                ?110, ?111,
                datetime('now')
            )
            ON CONFLICT(track_id) DO UPDATE SET
                duration = excluded.duration,
                sample_rate = excluded.sample_rate,
                channels = excluded.channels,
                peak_amplitude = excluded.peak_amplitude,
                rms_level = excluded.rms_level,
                dynamic_range = excluded.dynamic_range,
                spectral_centroid_mean = excluded.spectral_centroid_mean,
                spectral_centroid_std = excluded.spectral_centroid_std,
                spectral_flux_mean = excluded.spectral_flux_mean,
                spectral_flux_std = excluded.spectral_flux_std,
                spectral_rolloff_mean = excluded.spectral_rolloff_mean,
                spectral_rolloff_std = excluded.spectral_rolloff_std,
                spectral_flatness_mean = excluded.spectral_flatness_mean,
                spectral_flatness_std = excluded.spectral_flatness_std,
                spectral_bandwidth_mean = excluded.spectral_bandwidth_mean,
                spectral_bandwidth_std = excluded.spectral_bandwidth_std,
                zcr_mean = excluded.zcr_mean,
                zcr_std = excluded.zcr_std,
                sub_band_bass_mean = excluded.sub_band_bass_mean,
                sub_band_bass_std = excluded.sub_band_bass_std,
                sub_band_mid_mean = excluded.sub_band_mid_mean,
                sub_band_mid_std = excluded.sub_band_mid_std,
                sub_band_high_mean = excluded.sub_band_high_mean,
                sub_band_high_std = excluded.sub_band_high_std,
                sub_band_presence_mean = excluded.sub_band_presence_mean,
                sub_band_presence_std = excluded.sub_band_presence_std,
                mfcc_0_mean = excluded.mfcc_0_mean, mfcc_0_std = excluded.mfcc_0_std,
                mfcc_1_mean = excluded.mfcc_1_mean, mfcc_1_std = excluded.mfcc_1_std,
                mfcc_2_mean = excluded.mfcc_2_mean, mfcc_2_std = excluded.mfcc_2_std,
                mfcc_3_mean = excluded.mfcc_3_mean, mfcc_3_std = excluded.mfcc_3_std,
                mfcc_4_mean = excluded.mfcc_4_mean, mfcc_4_std = excluded.mfcc_4_std,
                mfcc_5_mean = excluded.mfcc_5_mean, mfcc_5_std = excluded.mfcc_5_std,
                mfcc_6_mean = excluded.mfcc_6_mean, mfcc_6_std = excluded.mfcc_6_std,
                mfcc_7_mean = excluded.mfcc_7_mean, mfcc_7_std = excluded.mfcc_7_std,
                mfcc_8_mean = excluded.mfcc_8_mean, mfcc_8_std = excluded.mfcc_8_std,
                mfcc_9_mean = excluded.mfcc_9_mean, mfcc_9_std = excluded.mfcc_9_std,
                mfcc_10_mean = excluded.mfcc_10_mean, mfcc_10_std = excluded.mfcc_10_std,
                mfcc_11_mean = excluded.mfcc_11_mean, mfcc_11_std = excluded.mfcc_11_std,
                mfcc_12_mean = excluded.mfcc_12_mean, mfcc_12_std = excluded.mfcc_12_std,
                tempo_bpm = excluded.tempo_bpm,
                beat_count = excluded.beat_count,
                onset_count = excluded.onset_count,
                tempo_stability = excluded.tempo_stability,
                rhythmic_complexity = excluded.rhythmic_complexity,
                mean_pitch = excluded.mean_pitch,
                pitch_range_low = excluded.pitch_range_low,
                pitch_range_high = excluded.pitch_range_high,
                pitch_stability = excluded.pitch_stability,
                dominant_pitch = excluded.dominant_pitch,
                vibrato_presence = excluded.vibrato_presence,
                vibrato_rate = excluded.vibrato_rate,
                pitch_confidence_mean = excluded.pitch_confidence_mean,
                lufs_integrated = excluded.lufs_integrated,
                loudness_range = excluded.loudness_range,
                true_peak_dbfs = excluded.true_peak_dbfs,
                crest_factor = excluded.crest_factor,
                energy_level = excluded.energy_level,
                estimated_key = excluded.estimated_key,
                key_confidence = excluded.key_confidence,
                tonality = excluded.tonality,
                harmonic_complexity = excluded.harmonic_complexity,
                chord_count = excluded.chord_count,
                chord_change_rate = excluded.chord_change_rate,
                mode_clarity = excluded.mode_clarity,
                key_alternatives_count = excluded.key_alternatives_count,
                time_sig_numerator = excluded.time_sig_numerator,
                time_sig_denominator = excluded.time_sig_denominator,
                chroma_vector = excluded.chroma_vector,
                recording_quality_score = excluded.recording_quality_score,
                snr_db = excluded.snr_db,
                clipping_ratio = excluded.clipping_ratio,
                noise_floor_db = excluded.noise_floor_db,
                segment_count = excluded.segment_count,
                temporal_complexity = excluded.temporal_complexity,
                coherence_score = excluded.coherence_score,
                energy_shape = excluded.energy_shape,
                peak_energy = excluded.peak_energy,
                energy_variance = excluded.energy_variance,
                tension_build_count = excluded.tension_build_count,
                tension_release_count = excluded.tension_release_count,
                repetition_count = excluded.repetition_count,
                repetition_similarity = excluded.repetition_similarity,
                solo_section_count = excluded.solo_section_count,
                solo_section_ratio = excluded.solo_section_ratio,
                transition_count = excluded.transition_count,
                classification_music_score = excluded.classification_music_score,
                hnr = excluded.hnr,
                valence_score = excluded.valence_score,
                arousal_score = excluded.arousal_score,
                energy_score = excluded.energy_score,
                intensity_score = excluded.intensity_score,
                groove_score = excluded.groove_score,
                improvisation_score = excluded.improvisation_score,
                tightness_score = excluded.tightness_score,
                build_quality_score = excluded.build_quality_score,
                exploratory_score = excluded.exploratory_score,
                transcendence_score = excluded.transcendence_score,
                analyzed_at = datetime('now')
            ",
            params![
                a.track_id,
                a.duration, a.sample_rate, a.channels, a.peak_amplitude, a.rms_level, a.dynamic_range,
                a.spectral_centroid_mean, a.spectral_centroid_std, a.spectral_flux_mean, a.spectral_flux_std,
                a.spectral_rolloff_mean, a.spectral_rolloff_std,
                a.spectral_flatness_mean, a.spectral_flatness_std,
                a.spectral_bandwidth_mean, a.spectral_bandwidth_std,
                a.zcr_mean, a.zcr_std,
                a.sub_band_bass_mean, a.sub_band_bass_std,
                a.sub_band_mid_mean, a.sub_band_mid_std,
                a.sub_band_high_mean, a.sub_band_high_std,
                a.sub_band_presence_mean, a.sub_band_presence_std,
                a.mfcc_0_mean, a.mfcc_0_std, a.mfcc_1_mean, a.mfcc_1_std,
                a.mfcc_2_mean, a.mfcc_2_std, a.mfcc_3_mean, a.mfcc_3_std,
                a.mfcc_4_mean, a.mfcc_4_std, a.mfcc_5_mean, a.mfcc_5_std,
                a.mfcc_6_mean, a.mfcc_6_std, a.mfcc_7_mean, a.mfcc_7_std,
                a.mfcc_8_mean, a.mfcc_8_std, a.mfcc_9_mean, a.mfcc_9_std,
                a.mfcc_10_mean, a.mfcc_10_std, a.mfcc_11_mean, a.mfcc_11_std,
                a.mfcc_12_mean, a.mfcc_12_std,
                a.tempo_bpm, a.beat_count, a.onset_count, a.tempo_stability, a.rhythmic_complexity,
                a.mean_pitch, a.pitch_range_low, a.pitch_range_high, a.pitch_stability,
                a.dominant_pitch, a.vibrato_presence, a.vibrato_rate, a.pitch_confidence_mean,
                a.lufs_integrated, a.loudness_range, a.true_peak_dbfs, a.crest_factor, a.energy_level,
                a.estimated_key, a.key_confidence, a.tonality, a.harmonic_complexity,
                a.chord_count, a.chord_change_rate, a.mode_clarity, a.key_alternatives_count,
                a.time_sig_numerator, a.time_sig_denominator, a.chroma_vector,
                a.recording_quality_score, a.snr_db, a.clipping_ratio, a.noise_floor_db,
                a.segment_count, a.temporal_complexity, a.coherence_score,
                a.energy_shape, a.peak_energy, a.energy_variance,
                a.tension_build_count, a.tension_release_count,
                a.repetition_count, a.repetition_similarity,
                a.solo_section_count, a.solo_section_ratio, a.transition_count,
                a.classification_music_score, a.hnr,
                a.valence_score, a.arousal_score,
                a.energy_score, a.intensity_score, a.groove_score,
                a.improvisation_score, a.tightness_score, a.build_quality_score,
                a.exploratory_score, a.transcendence_score,
            ],
        )?;
        Ok(())
    }

    /// Backward-compatible: store just the analysis row without detail tables.
    pub fn store_analysis(&self, a: &NewAnalysis) -> Result<()> {
        Self::store_analysis_row(&self.conn, a)
    }

    /// Load all analysis rows with fields needed for score computation.
    /// Returns NewAnalysis structs with score-relevant fields populated.
    pub fn get_analyses_for_rescore(&self) -> Result<Vec<NewAnalysis>> {
        let mut stmt = self.conn.prepare(
            "SELECT track_id, duration, rms_level, lufs_integrated,
                spectral_centroid_mean, spectral_centroid_std,
                spectral_flux_mean, spectral_flux_std,
                dynamic_range, loudness_range,
                onset_count, beat_count, tempo_bpm,
                tempo_stability, coherence_score,
                pitch_range_low, pitch_range_high,
                harmonic_complexity, key_confidence, key_alternatives_count,
                chord_count, tonality, energy_level,
                estimated_key, energy_shape, peak_energy, energy_variance,
                tension_build_count, tension_release_count,
                repetition_similarity, solo_section_count,
                transition_count, segment_count,
                spectral_flatness_std, pitch_confidence_mean,
                mode_clarity, crest_factor, pitch_stability
             FROM analysis_results",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(NewAnalysis {
                    track_id: row.get(0)?,
                    duration: row.get(1)?,
                    rms_level: row.get(2)?,
                    lufs_integrated: row.get(3)?,
                    spectral_centroid_mean: row.get(4)?,
                    spectral_centroid_std: row.get(5)?,
                    spectral_flux_mean: row.get(6)?,
                    spectral_flux_std: row.get(7)?,
                    dynamic_range: row.get(8)?,
                    loudness_range: row.get(9)?,
                    onset_count: row.get(10)?,
                    beat_count: row.get(11)?,
                    tempo_bpm: row.get(12)?,
                    tempo_stability: row.get(13)?,
                    coherence_score: row.get(14)?,
                    pitch_range_low: row.get(15)?,
                    pitch_range_high: row.get(16)?,
                    harmonic_complexity: row.get(17)?,
                    key_confidence: row.get(18)?,
                    key_alternatives_count: row.get(19)?,
                    chord_count: row.get(20)?,
                    tonality: row.get(21)?,
                    energy_level: row.get(22)?,
                    estimated_key: row.get(23)?,
                    energy_shape: row.get(24)?,
                    peak_energy: row.get(25)?,
                    energy_variance: row.get(26)?,
                    tension_build_count: row.get(27)?,
                    tension_release_count: row.get(28)?,
                    repetition_similarity: row.get(29)?,
                    solo_section_count: row.get(30)?,
                    transition_count: row.get(31)?,
                    segment_count: row.get(32)?,
                    spectral_flatness_std: row.get(33)?,
                    pitch_confidence_mean: row.get(34)?,
                    mode_clarity: row.get(35)?,
                    crest_factor: row.get(36)?,
                    pitch_stability: row.get(37)?,
                    // Fields not needed for scoring — set to None/defaults
                    sample_rate: None, channels: None, peak_amplitude: None,
                    spectral_rolloff_mean: None, spectral_rolloff_std: None,
                    spectral_flatness_mean: None,
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
                    rhythmic_complexity: None, mean_pitch: None,
                    dominant_pitch: None,
                    vibrato_presence: None, vibrato_rate: None,
                    true_peak_dbfs: None,
                    chord_change_rate: None,
                    time_sig_numerator: None, time_sig_denominator: None,
                    chroma_vector: None,
                    recording_quality_score: None, snr_db: None,
                    clipping_ratio: None, noise_floor_db: None,
                    temporal_complexity: None,
                    repetition_count: None, solo_section_ratio: None,
                    classification_music_score: None, hnr: None,
                    valence_score: None, arousal_score: None,
                    energy_score: None, intensity_score: None,
                    groove_score: None, improvisation_score: None,
                    tightness_score: None, build_quality_score: None,
                    exploratory_score: None, transcendence_score: None,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Update only the 10 jam score columns for a given track.
    pub fn update_jam_scores(&self, a: &NewAnalysis) -> Result<()> {
        self.conn.execute(
            "UPDATE analysis_results SET
                energy_score = ?1, intensity_score = ?2, groove_score = ?3,
                improvisation_score = ?4, tightness_score = ?5, build_quality_score = ?6,
                exploratory_score = ?7, transcendence_score = ?8,
                valence_score = ?9, arousal_score = ?10
             WHERE track_id = ?11",
            params![
                a.energy_score, a.intensity_score, a.groove_score,
                a.improvisation_score, a.tightness_score, a.build_quality_score,
                a.exploratory_score, a.transcendence_score,
                a.valence_score, a.arousal_score,
                a.track_id,
            ],
        )?;
        Ok(())
    }

    /// Query top tracks by a given score column.
    /// `score_column` must be one of the valid score column names.
    pub fn query_top(
        &self,
        score_column: &str,
        limit: usize,
        song_filter: Option<&str>,
        min_duration_secs: Option<f64>,
    ) -> Result<Vec<TrackScore>> {
        // Validate score column to prevent SQL injection
        let valid_columns = [
            "energy_score", "intensity_score", "groove_score", "improvisation_score",
            "tightness_score", "build_quality_score", "exploratory_score",
            "transcendence_score", "valence_score", "arousal_score",
        ];
        if !valid_columns.contains(&score_column) {
            return Ok(vec![]);
        }

        let mut sql = format!(
            "SELECT
                COALESCE(t.parsed_title, t.title, '(untitled)'),
                COALESCE(t.parsed_date, t.date, '?'),
                COALESCE(a.duration, 0.0) / 60.0,
                a.estimated_key, a.tempo_bpm,
                COALESCE(a.energy_score, 0), COALESCE(a.intensity_score, 0),
                COALESCE(a.groove_score, 0), COALESCE(a.improvisation_score, 0),
                COALESCE(a.tightness_score, 0), COALESCE(a.build_quality_score, 0),
                COALESCE(a.exploratory_score, 0), COALESCE(a.transcendence_score, 0),
                COALESCE(a.valence_score, 0), COALESCE(a.arousal_score, 0)
             FROM analysis_results a
             JOIN tracks t ON t.id = a.track_id
             WHERE a.{score_column} IS NOT NULL"
        );

        let mut params_vec: Vec<Box<dyn rusqlite::types::ToSql>> = vec![];

        if let Some(song) = song_filter {
            params_vec.push(Box::new(format!("%{song}%")));
            sql += &format!(
                " AND (t.parsed_title LIKE ?{n} OR t.title LIKE ?{n})",
                n = params_vec.len()
            );
        }

        if let Some(min_dur) = min_duration_secs {
            params_vec.push(Box::new(min_dur));
            sql += &format!(" AND a.duration >= ?{}", params_vec.len());
        }

        sql += &format!(" ORDER BY a.{score_column} DESC LIMIT {limit}");

        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            params_vec.iter().map(|p| p.as_ref()).collect();
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt
            .query_map(params_refs.as_slice(), |row| {
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
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Compare versions of a song across shows.
    pub fn query_compare(
        &self,
        song: &str,
        sort_by: &str,
        limit: usize,
    ) -> Result<Vec<TrackScore>> {
        let valid_columns = [
            "energy_score", "intensity_score", "groove_score", "improvisation_score",
            "tightness_score", "build_quality_score", "exploratory_score",
            "transcendence_score", "valence_score", "arousal_score", "duration",
        ];
        let order_col = if valid_columns.contains(&sort_by) { sort_by } else { "duration" };

        let sql = format!(
            "SELECT
                COALESCE(t.parsed_title, t.title, '(untitled)'),
                COALESCE(t.parsed_date, t.date, '?'),
                COALESCE(a.duration, 0.0) / 60.0,
                a.estimated_key, a.tempo_bpm,
                COALESCE(a.energy_score, 0), COALESCE(a.intensity_score, 0),
                COALESCE(a.groove_score, 0), COALESCE(a.improvisation_score, 0),
                COALESCE(a.tightness_score, 0), COALESCE(a.build_quality_score, 0),
                COALESCE(a.exploratory_score, 0), COALESCE(a.transcendence_score, 0),
                COALESCE(a.valence_score, 0), COALESCE(a.arousal_score, 0)
             FROM analysis_results a
             JOIN tracks t ON t.id = a.track_id
             WHERE (t.parsed_title LIKE ?1 OR t.title LIKE ?1)
             ORDER BY a.{order_col} DESC
             LIMIT ?2"
        );

        let pattern = format!("%{song}%");
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt
            .query_map(params![pattern, limit as i64], |row| {
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
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Get all analyzed tracks for a given show date.
    pub fn query_show(&self, date: &str) -> Result<Vec<TrackScore>> {
        let mut stmt = self.conn.prepare(
            "SELECT
                COALESCE(t.parsed_title, t.title, '(untitled)'),
                COALESCE(t.parsed_date, t.date, '?'),
                COALESCE(a.duration, 0.0) / 60.0,
                a.estimated_key, a.tempo_bpm,
                COALESCE(a.energy_score, 0), COALESCE(a.intensity_score, 0),
                COALESCE(a.groove_score, 0), COALESCE(a.improvisation_score, 0),
                COALESCE(a.tightness_score, 0), COALESCE(a.build_quality_score, 0),
                COALESCE(a.exploratory_score, 0), COALESCE(a.transcendence_score, 0),
                COALESCE(a.valence_score, 0), COALESCE(a.arousal_score, 0)
             FROM analysis_results a
             JOIN tracks t ON t.id = a.track_id
             WHERE t.parsed_date = ?1 OR t.date = ?1
             ORDER BY COALESCE(t.parsed_disc, t.disc_number, 1),
                      COALESCE(t.parsed_track, t.track_number, 999)"
        )?;

        let rows = stmt
            .query_map(params![date], |row| {
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
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Load feature vectors for similarity computation.
    /// Returns (track_id, feature_vector) pairs for all analyzed tracks.
    pub fn get_feature_vectors(&self) -> Result<Vec<(i64, Vec<f64>)>> {
        let mut stmt = self.conn.prepare(
            "SELECT track_id,
                -- MFCCs (26 dims)
                COALESCE(mfcc_0_mean, 0), COALESCE(mfcc_0_std, 0),
                COALESCE(mfcc_1_mean, 0), COALESCE(mfcc_1_std, 0),
                COALESCE(mfcc_2_mean, 0), COALESCE(mfcc_2_std, 0),
                COALESCE(mfcc_3_mean, 0), COALESCE(mfcc_3_std, 0),
                COALESCE(mfcc_4_mean, 0), COALESCE(mfcc_4_std, 0),
                COALESCE(mfcc_5_mean, 0), COALESCE(mfcc_5_std, 0),
                COALESCE(mfcc_6_mean, 0), COALESCE(mfcc_6_std, 0),
                COALESCE(mfcc_7_mean, 0), COALESCE(mfcc_7_std, 0),
                COALESCE(mfcc_8_mean, 0), COALESCE(mfcc_8_std, 0),
                COALESCE(mfcc_9_mean, 0), COALESCE(mfcc_9_std, 0),
                COALESCE(mfcc_10_mean, 0), COALESCE(mfcc_10_std, 0),
                COALESCE(mfcc_11_mean, 0), COALESCE(mfcc_11_std, 0),
                COALESCE(mfcc_12_mean, 0), COALESCE(mfcc_12_std, 0),
                -- Spectral (10 dims)
                COALESCE(spectral_centroid_mean, 0), COALESCE(spectral_centroid_std, 0),
                COALESCE(spectral_flux_mean, 0), COALESCE(spectral_flux_std, 0),
                COALESCE(spectral_flatness_mean, 0), COALESCE(spectral_flatness_std, 0),
                COALESCE(spectral_bandwidth_mean, 0), COALESCE(spectral_bandwidth_std, 0),
                COALESCE(spectral_rolloff_mean, 0), COALESCE(spectral_rolloff_std, 0),
                -- Sub-band energy (8 dims)
                COALESCE(sub_band_bass_mean, 0), COALESCE(sub_band_bass_std, 0),
                COALESCE(sub_band_mid_mean, 0), COALESCE(sub_band_mid_std, 0),
                COALESCE(sub_band_high_mean, 0), COALESCE(sub_band_high_std, 0),
                COALESCE(sub_band_presence_mean, 0), COALESCE(sub_band_presence_std, 0),
                -- ZCR (2 dims)
                COALESCE(zcr_mean, 0), COALESCE(zcr_std, 0),
                -- Tempo (1 dim)
                COALESCE(tempo_bpm, 0)
             FROM analysis_results"
        )?;

        let dim = 47; // 26 + 10 + 8 + 2 + 1
        let rows = stmt
            .query_map([], |row| {
                let track_id: i64 = row.get(0)?;
                let mut vec = Vec::with_capacity(dim);
                for i in 1..=dim {
                    vec.push(row.get::<_, f64>(i)?);
                }
                Ok((track_id, vec))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Store similarity results (bulk insert within a transaction).
    pub fn store_similarities(&self, similarities: &[(i64, i64, f64, i32)]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM track_similarity", [])?;

        let mut stmt = tx.prepare_cached(
            "INSERT INTO track_similarity (track_id, similar_track_id, distance, rank)
             VALUES (?1, ?2, ?3, ?4)"
        )?;

        for &(track_id, similar_id, distance, rank) in similarities {
            stmt.execute(params![track_id, similar_id, distance, rank])?;
        }
        drop(stmt);
        tx.commit()?;
        Ok(())
    }

    /// Query similar tracks for a given track.
    pub fn query_similar(&self, track_id: i64, limit: usize) -> Result<Vec<(TrackScore, f64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT
                COALESCE(t.parsed_title, t.title, '(untitled)'),
                COALESCE(t.parsed_date, t.date, '?'),
                COALESCE(a.duration, 0.0) / 60.0,
                a.estimated_key, a.tempo_bpm,
                COALESCE(a.energy_score, 0), COALESCE(a.intensity_score, 0),
                COALESCE(a.groove_score, 0), COALESCE(a.improvisation_score, 0),
                COALESCE(a.tightness_score, 0), COALESCE(a.build_quality_score, 0),
                COALESCE(a.exploratory_score, 0), COALESCE(a.transcendence_score, 0),
                COALESCE(a.valence_score, 0), COALESCE(a.arousal_score, 0),
                s.distance
             FROM track_similarity s
             JOIN tracks t ON t.id = s.similar_track_id
             JOIN analysis_results a ON a.track_id = s.similar_track_id
             WHERE s.track_id = ?1
             ORDER BY s.rank
             LIMIT ?2"
        )?;

        let rows = stmt
            .query_map(params![track_id, limit as i64], |row| {
                Ok((
                    TrackScore {
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
                    },
                    row.get::<_, f64>(15)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Find a track ID by song title and optional date.
    pub fn find_track_id(&self, song: &str, date: Option<&str>) -> Result<Option<(i64, String, String)>> {
        let (sql, pattern) = if let Some(_d) = date {
            (
                "SELECT t.id, COALESCE(t.parsed_title, t.title, '?'), COALESCE(t.parsed_date, t.date, '?')
                 FROM tracks t
                 JOIN analysis_results a ON a.track_id = t.id
                 WHERE (t.parsed_title LIKE ?1 OR t.title LIKE ?1)
                   AND (t.parsed_date = ?2 OR t.date = ?2)
                 LIMIT 1",
                format!("%{song}%"),
            )
        } else {
            (
                "SELECT t.id, COALESCE(t.parsed_title, t.title, '?'), COALESCE(t.parsed_date, t.date, '?')
                 FROM tracks t
                 JOIN analysis_results a ON a.track_id = t.id
                 WHERE (t.parsed_title LIKE ?1 OR t.title LIKE ?1)
                 ORDER BY a.duration DESC
                 LIMIT 1",
                format!("%{song}%"),
            )
        };

        let result = if date.is_some() {
            self.conn.query_row(sql, params![pattern, date.unwrap()], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
        } else {
            self.conn.query_row(sql, params![pattern], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
        };

        match result {
            Ok(row) => Ok(Some(row)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Get library statistics.
    pub fn stats(&self) -> Result<LibraryStats> {
        let total_tracks: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM tracks",
            [],
            |row| row.get(0),
        )?;

        let analyzed_tracks: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM analysis_results",
            [],
            |row| row.get(0),
        )?;

        let total_duration_hours: f64 = self.conn.query_row(
            "SELECT COALESCE(SUM(duration), 0.0) / 3600.0 FROM analysis_results",
            [],
            |row| row.get(0),
        )?;

        let mut fmt_stmt = self.conn.prepare(
            "SELECT format, COUNT(*) FROM tracks GROUP BY format ORDER BY COUNT(*) DESC",
        )?;
        let formats: Vec<(String, i64)> = fmt_stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        let mut band_stmt = self.conn.prepare(
            "SELECT COALESCE(parsed_band, artist, 'Unknown'), COUNT(*)
             FROM tracks
             GROUP BY COALESCE(parsed_band, artist, 'Unknown')
             ORDER BY COUNT(*) DESC
             LIMIT 20",
        )?;
        let bands: Vec<(String, i64)> = band_stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(LibraryStats {
            total_tracks,
            analyzed_tracks,
            total_duration_hours,
            formats,
            bands,
        })
    }

    /// Get tracks missing titles (no parsed_title AND no tag title).
    /// Returns (track_id, file_path) pairs.
    pub fn get_tracks_missing_titles(&self) -> Result<Vec<(i64, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, file_path FROM tracks
             WHERE parsed_title IS NULL AND title IS NULL
             ORDER BY file_path",
        )?;

        let rows = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(rows)
    }

    /// Update parsed_title for a single track.
    pub fn update_parsed_title(&self, track_id: i64, title: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE tracks SET parsed_title = ?1, updated_at = datetime('now') WHERE id = ?2",
            params![title, track_id],
        )?;
        Ok(())
    }

    /// Check if a file path already exists and hasn't changed (same size+mtime).
    pub fn track_unchanged(&self, file_path: &str, file_size: i64, file_modified: &str) -> Result<bool> {
        let result: std::result::Result<(i64, String), _> = self.conn.query_row(
            "SELECT file_size, file_modified FROM tracks WHERE file_path = ?1",
            params![file_path],
            |row| Ok((row.get(0)?, row.get(1)?)),
        );

        match result {
            Ok((size, mtime)) => Ok(size == file_size && mtime == file_modified),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(false),
            Err(e) => Err(e.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::NewTrack;

    fn test_track() -> NewTrack {
        NewTrack {
            file_path: "/music/gd1977-05-08d1t01.shn".to_string(),
            file_size: 12345678,
            file_modified: "1700000000".to_string(),
            format: "shn".to_string(),
            title: Some("Scarlet Begonias".to_string()),
            artist: Some("Grateful Dead".to_string()),
            album: Some("1977-05-08 Barton Hall".to_string()),
            date: Some("1977-05-08".to_string()),
            track_number: Some(1),
            disc_number: Some(1),
            set_name: None,
            venue: Some("Barton Hall".to_string()),
            comment: None,
            parsed_band: Some("Grateful Dead".to_string()),
            parsed_date: Some("1977-05-08".to_string()),
            parsed_venue: None,
            parsed_disc: Some(1),
            parsed_track: Some(1),
            parsed_set: None,
            parsed_title: None,
            duration_secs: Some(300.0),
        }
    }

    fn minimal_analysis(track_id: i64) -> NewAnalysis {
        NewAnalysis {
            track_id,
            duration: Some(300.0),
            sample_rate: Some(44100),
            channels: Some(2),
            peak_amplitude: None, rms_level: None, dynamic_range: None,
            spectral_centroid_mean: None, spectral_centroid_std: None,
            spectral_flux_mean: None, spectral_flux_std: None,
            spectral_rolloff_mean: None, spectral_rolloff_std: None,
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
            tempo_bpm: None, beat_count: None, onset_count: None,
            tempo_stability: None, rhythmic_complexity: None,
            mean_pitch: None, pitch_range_low: None, pitch_range_high: None,
            pitch_stability: None, dominant_pitch: None,
            vibrato_presence: None, vibrato_rate: None, pitch_confidence_mean: None,
            lufs_integrated: None, loudness_range: None, true_peak_dbfs: None,
            crest_factor: None, energy_level: None,
            estimated_key: None, key_confidence: None, tonality: None,
            harmonic_complexity: None, chord_count: None, chord_change_rate: None,
            mode_clarity: None, key_alternatives_count: None,
            time_sig_numerator: None, time_sig_denominator: None, chroma_vector: None,
            recording_quality_score: None, snr_db: None, clipping_ratio: None,
            noise_floor_db: None,
            segment_count: None, temporal_complexity: None, coherence_score: None,
            energy_shape: None, peak_energy: None, energy_variance: None,
            tension_build_count: None, tension_release_count: None,
            repetition_count: None, repetition_similarity: None,
            solo_section_count: None, solo_section_ratio: None, transition_count: None,
            classification_music_score: None, hnr: None,
            valence_score: None, arousal_score: None,
            energy_score: None, intensity_score: None, groove_score: None,
            improvisation_score: None, tightness_score: None,
            build_quality_score: None, exploratory_score: None,
            transcendence_score: None,
        }
    }

    #[test]
    fn test_upsert_and_retrieve() {
        let db = Database::open_in_memory().unwrap();
        let t = test_track();
        let id = db.upsert_track(&t).unwrap();
        assert!(id > 0);

        let tracks = db.get_unanalyzed_tracks().unwrap();
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].file_path, t.file_path);
        assert_eq!(tracks[0].artist.as_deref(), Some("Grateful Dead"));
    }

    #[test]
    fn test_upsert_is_idempotent() {
        let db = Database::open_in_memory().unwrap();
        let t = test_track();
        let id1 = db.upsert_track(&t).unwrap();
        let id2 = db.upsert_track(&t).unwrap();
        assert_eq!(id1, id2);

        let stats = db.stats().unwrap();
        assert_eq!(stats.total_tracks, 1);
    }

    #[test]
    fn test_track_unchanged() {
        let db = Database::open_in_memory().unwrap();
        let t = test_track();
        db.upsert_track(&t).unwrap();

        assert!(db.track_unchanged(&t.file_path, t.file_size, &t.file_modified).unwrap());
        assert!(!db.track_unchanged(&t.file_path, 999, &t.file_modified).unwrap());
        assert!(!db.track_unchanged("/nonexistent", 0, "").unwrap());
    }

    #[test]
    fn test_stats_empty() {
        let db = Database::open_in_memory().unwrap();
        let stats = db.stats().unwrap();
        assert_eq!(stats.total_tracks, 0);
        assert_eq!(stats.analyzed_tracks, 0);
    }

    #[test]
    fn test_get_unanalyzed_excludes_analyzed() {
        let db = Database::open_in_memory().unwrap();
        let t = test_track();
        let id = db.upsert_track(&t).unwrap();

        assert_eq!(db.get_unanalyzed_tracks().unwrap().len(), 1);

        let analysis = minimal_analysis(id);
        db.store_analysis(&analysis).unwrap();

        assert_eq!(db.get_unanalyzed_tracks().unwrap().len(), 0);
        assert_eq!(db.stats().unwrap().analyzed_tracks, 1);
    }

    #[test]
    fn test_store_full_analysis_with_details() {
        let db = Database::open_in_memory().unwrap();
        let t = test_track();
        let id = db.upsert_track(&t).unwrap();

        let analysis = minimal_analysis(id);
        let chords = vec![
            ChordEvent { track_id: id, chord: "Am".into(), start_time: 0.0, duration: 2.0, confidence: Some(0.8) },
            ChordEvent { track_id: id, chord: "G".into(), start_time: 2.0, duration: 2.0, confidence: Some(0.7) },
        ];
        let segments = vec![
            SegmentRecord {
                track_id: id, segment_index: 0, label: "Music".into(), section_type: Some("Intro".into()),
                start_time: 0.0, duration: 30.0, energy: Some(0.5), spectral_centroid: Some(2000.0),
                zcr: Some(0.1), key: Some("Am".into()), tempo: Some(120.0), dynamic_range: Some(15.0),
                confidence: Some(0.9), harmonic_stability: Some(0.8), rhythmic_density: Some(0.6),
                avg_brightness: Some(2000.0), dynamic_variation: Some(5.0),
            },
        ];
        let tension = vec![
            TensionPointRecord { track_id: id, time: 15.0, tension: 0.6, change_type: "BuildUp".into() },
        ];
        let transitions = vec![
            TransitionRecord { track_id: id, time: 30.0, transition_type: "Smooth".into(), strength: Some(0.7), duration: Some(2.0) },
        ];

        db.store_full_analysis(&analysis, &chords, &segments, &tension, &transitions).unwrap();

        // Verify counts
        let chord_count: i64 = db.conn.query_row(
            "SELECT COUNT(*) FROM track_chords WHERE track_id = ?1", params![id], |r| r.get(0)
        ).unwrap();
        assert_eq!(chord_count, 2);

        let seg_count: i64 = db.conn.query_row(
            "SELECT COUNT(*) FROM track_segments WHERE track_id = ?1", params![id], |r| r.get(0)
        ).unwrap();
        assert_eq!(seg_count, 1);

        assert_eq!(db.stats().unwrap().analyzed_tracks, 1);
    }
}
