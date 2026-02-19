# Analyzer Feature Reference

setbreak extracts 185 features from every audio track using pure DSP — no machine learning models. All features are computed from the raw waveform via FFT, STFT, pitch detection, beat tracking, onset detection, and chord estimation, then stored in the `analysis_results` table.

The DSP engine is [ferrous-waves](https://github.com/lexicone42/ferrous-waves), a Rust audio analysis library. setbreak extends the raw analysis output with derived features (section diversity, segue detection, etc.) computed in `features.rs`.

## Core audio properties

Fundamental properties of the audio signal.

| Column | Type | Description |
|--------|------|-------------|
| `duration` | f64 | Track length in seconds |
| `sample_rate` | i32 | Sample rate (Hz) |
| `channels` | i32 | Channel count |
| `peak_amplitude` | f64 | Maximum absolute sample value |
| `rms_level` | f64 | Root mean square energy |
| `dynamic_range` | f64 | Peak-to-RMS ratio (dB) |

## Spectral features

Frequency-domain characteristics computed from STFT frames (2048-sample Hann window, 512 hop), aggregated as mean and standard deviation across all frames.

| Column | Type | Description |
|--------|------|-------------|
| `spectral_centroid_mean/std` | f64 | Brightness — frequency center of mass (Hz) |
| `spectral_flux_mean/std` | f64 | Rate of spectral change between frames |
| `spectral_rolloff_mean/std` | f64 | Frequency below which 85% of energy lies |
| `spectral_bandwidth_mean/std` | f64 | Spread of the spectrum around the centroid |
| `spectral_flatness_mean/std` | f64 | Noisiness vs tonality (0=tonal, 1=noise) |
| `spectral_spread_mean/std` | f64 | Bandwidth of spectral energy distribution |
| `spectral_crest_mean/std` | f64 | Peakedness vs flatness of spectrum |
| `spectral_entropy_mean/std` | f64 | Spectral disorder — low = tonal, high = noisy |
| `spectral_slope_mean` | f64 | Overall spectral tilt (bright vs dark) |
| `spectral_skewness_mean` | f64 | Spectral shape asymmetry |
| `spectral_kurtosis_mean` | f64 | Spectral peakedness |
| `zcr_mean/std` | f64 | Zero crossing rate — rough brightness/noisiness proxy |
| `hnr` | f64 | Harmonics-to-noise ratio |

### Sub-band energy

Energy decomposed into frequency bands.

| Column | Type | Description |
|--------|------|-------------|
| `sub_band_bass_mean/std` | f64 | Energy in bass sub-band |
| `sub_band_mid_mean/std` | f64 | Energy in midrange sub-band |
| `sub_band_high_mean/std` | f64 | Energy in high-frequency sub-band |
| `sub_band_presence_mean/std` | f64 | Energy in presence sub-band |
| `bass_treble_ratio_mean/std` | f64 | Low-to-high frequency energy ratio |
| `sub_band_flux_bass_mean/std` | f64 | Bass-band spectral change rate |
| `sub_band_flux_mid_mean` | f64 | Mid-band spectral change rate |
| `sub_band_flux_high_mean` | f64 | High-band spectral change rate |

### Spectral contrast & texture

Octave-band contrast and higher-level spectral texture measures.

| Column | Type | Description |
|--------|------|-------------|
| `spectral_contrast_json` | text | 7-band octave spectral contrast (JSON array of mean per band) |
| `spectral_contrast_slope` | f64 | Linear regression slope across 7-band contrast means — captures spectral texture tilt |
| `spectral_contrast_range` | f64 | Max minus min of 7-band contrast means — wider = more spectral variation |
| `harmonic_percussive_ratio` | f64 | Ratio of harmonic (sustained) to total energy. High = guitars/vocals/organ, low = drums/percussion. Computed via median filtering (Fitzgerald 2010) |
| `roughness_mean/std` | f64 | Sensory dissonance / perceptual roughness |

### Harmonic-percussive separation

Fitzgerald (2010) median filtering on the spectrogram separates sustained tones (harmonic) from transients (percussive). This is one of the strongest differentiators — Dark Star (0.82) vs Drums (0.41).

## Timbral features (MFCCs)

Mel-frequency cepstral coefficients — compact timbral representation. 13 coefficients (0-12), each stored as mean and standard deviation, plus delta (velocity) and delta-delta (acceleration) for timbral dynamics.

| Columns | Description |
|---------|-------------|
| `mfcc_0_mean` through `mfcc_12_mean` | Mean MFCC per coefficient |
| `mfcc_0_std` through `mfcc_12_std` | Std deviation per coefficient |
| `mfcc_delta_mean_json` | Rate of MFCC change over time — timbral dynamics (JSON, 13 coefficients) |
| `mfcc_delta_delta_mean_json` | Acceleration of timbral change (JSON, 13 coefficients) |
| `mfcc_flux_mean` | Rate of timbral change (mean Euclidean distance between consecutive MFCC frames) |

## Temporal & rhythm

Beat tracking, onset detection, rhythmic analysis, and groove characteristics.

### Beat & tempo

| Column | Type | Description |
|--------|------|-------------|
| `tempo_bpm` | f64 | Estimated tempo |
| `beat_count` | i32 | Number of detected beats |
| `onset_count` | i32 | Number of detected onsets (note attacks) |
| `tempo_stability` | f64 | Consistency of inter-beat intervals |
| `rhythmic_complexity` | f64 | Variety in rhythmic patterns |
| `beat_regularity` | f64 | Regularity of beat intervals |
| `rhythmic_periodicity_strength` | f64 | Strength of rhythmic periodicity |

### Groove & feel

| Column | Type | Description |
|--------|------|-------------|
| `pulse_clarity` | f64 | How clear/strong the beat is (0 = no pulse, 1 = metronomic). One More Saturday Night (0.37) vs Introduction (0.03) |
| `syncopation` | f64 | Off-beat emphasis (high = syncopated rhythm) |
| `offbeat_ratio` | f64 | Fraction of onsets on off-beats |
| `swing_ratio` | f64 | Swing feel (1.0 = straight, >1 = swing, <1 = pushed) |
| `beat_pattern_json` | text | Beat-relative onset histogram (JSON array) |

### Onset dynamics

| Column | Type | Description |
|--------|------|-------------|
| `onset_strength_mean/std` | f64 | Onset detection strength |
| `onset_strength_skewness` | f64 | Asymmetry of onset strengths |
| `onset_density_std` | f64 | Variation in onset density across time windows |
| `onset_interval_entropy` | f64 | Rhythmic unpredictability — high = irregular timing |
| `onset_strength_contour_json` | text | DCT-II coefficients (first 4) of spectral flux — captures temporal shape of rhythmic energy: overall level, trend, curvature, and asymmetry |
| `attack_time_mean/std` | f64 | Onset attack duration |
| `decay_time_mean/std` | f64 | Post-onset decay duration |

### Microtiming

| Column | Type | Description |
|--------|------|-------------|
| `microtiming_deviation_mean/std` | f64 | Deviation from grid timing |
| `microtiming_bias` | f64 | Tendency to play ahead/behind the beat |

## Pitch & harmony

Pitch detection (pYIN), key estimation, chord analysis, and tonal space.

### Pitch

| Column | Type | Description |
|--------|------|-------------|
| `mean_pitch` | f64 | Average detected pitch (Hz) |
| `pitch_range_low/high` | f64 | Pitch range boundaries |
| `pitch_stability` | f64 | Consistency of detected pitch |
| `dominant_pitch` | f64 | Most frequent pitch value |
| `pitch_confidence_mean` | f64 | Mean confidence of pitch detections |
| `pitch_contour_std` | f64 | Pitch movement variability |
| `pitch_clarity_mean` | f64 | How clearly pitched (vs noisy) the signal is |
| `pitched_frame_ratio` | f64 | Fraction of frames with detectable pitch |
| `vibrato_presence` | f64 | Vibrato detection strength |
| `vibrato_rate` | f64 | Vibrato oscillation rate (Hz) |

### Key & tonality

| Column | Type | Description |
|--------|------|-------------|
| `estimated_key` | text | Detected musical key (e.g., "A minor") |
| `key_confidence` | f64 | Confidence in key estimate |
| `tonality` | text | Major or minor |
| `mode_clarity` | f64 | How clearly major vs minor (low = ambiguous, useful for modal music like Mixolydian/Dorian) |
| `key_alternatives_count` | i32 | Number of plausible alternative keys |
| `harmonic_complexity` | f64 | Harmonic richness measure |

### Chords & harmonic motion

| Column | Type | Description |
|--------|------|-------------|
| `chord_count` | i32 | Number of detected chord changes |
| `chord_change_rate` | f64 | Chord changes per second |
| `chroma_vector` | text | 12-bin chroma energy (JSON array) |
| `chromagram_entropy` | f64 | Shannon entropy of mean 12-bin chroma. Low = simple tonality (one key center), high = chromatic/atonal. Max ≈ 2.485. Space (2.43) vs Slipknot (2.16) |
| `chroma_flux_mean` | f64 | Rate of chromatic change |
| `chroma_self_similarity_bandwidth` | f64 | Harmonic structure repetition bandwidth |
| `tonnetz_json` | text | 6D tonal centroid (Tonnetz) — position in harmonic space (JSON) |
| `tonnetz_flux_mean` | f64 | Rate of movement through tonal space |

## Loudness & dynamics

Perceptual loudness measurements per EBU R128 and recording quality indicators.

| Column | Type | Description |
|--------|------|-------------|
| `lufs_integrated` | f64 | Integrated loudness (LUFS) — used for cross-show calibration |
| `loudness_range` | f64 | LRA — dynamic loudness variation (LU) |
| `loudness_std` | f64 | Short-term loudness variation |
| `peak_loudness` | f64 | Maximum short-term loudness |
| `loudness_dynamic_spread` | f64 | Range of short-term loudness values |
| `loudness_buildup_slope` | f64 | Overall loudness trajectory |
| `true_peak_dbfs` | f64 | True peak level (dBFS) |
| `crest_factor` | f64 | Peak-to-RMS ratio |
| `energy_level` | f64 | Overall energy measure |
| `recording_quality_score` | f64 | Composite recording quality estimate |
| `snr_db` | f64 | Signal-to-noise ratio (dB) |
| `clipping_ratio` | f64 | Fraction of samples near clipping |
| `noise_floor_db` | f64 | Estimated noise floor (dB) |

## Spatial

| Column | Type | Description |
|--------|------|-------------|
| `stereo_width_mean/std` | f64 | Stereo field width (0 = mono, 1 = wide stereo) |
| `spectral_loudness_correlation` | f64 | Correlation between brightness and loudness |
| `temporal_modulation_json` | text | Modulation spectrum — periodic amplitude patterns (JSON) |

## Structure & evolution

How the track evolves over time — structural segments, energy arcs, tension dynamics.

### Segment structure

| Column | Type | Description |
|--------|------|-------------|
| `segment_count` | i32 | Number of detected structural segments |
| `temporal_complexity` | f64 | Variety in temporal patterns |
| `coherence_score` | f64 | Self-similarity / structural coherence |
| `transition_count` | i32 | Number of structural transitions |
| `classification_music_score` | f64 | Music vs speech/noise classification |
| `time_sig_numerator/denominator` | i32 | Detected time signature |
| `section_diversity_score` | f64 | Mean pairwise Euclidean distance between normalized segment feature vectors. High = track moves through many distinct textures. Drums (0.65) vs Sugar Magnolia (0.32) |

### Energy arc

| Column | Type | Description |
|--------|------|-------------|
| `energy_shape` | text | Overall energy contour (Flat/Increasing/Decreasing/Peak/Valley/Oscillating/Complex) |
| `peak_energy` | f64 | Maximum energy value |
| `energy_variance` | f64 | Energy variation across track |
| `energy_buildup_ratio` | f64 | Energy in 2nd half vs 1st half — above 1.0 = builds |
| `peak_energy_time` | f64 | Normalized position (0-1) of peak energy in the track |
| `energy_peak_count` | i32 | Number of energy peaks (climaxes) |
| `energy_valley_depth_mean` | f64 | Mean depth of energy valleys (quiet sections) |
| `bass_energy_slope` | f64 | Bass energy trend over time |

### Tension dynamics

| Column | Type | Description |
|--------|------|-------------|
| `tension_build_count` | i32 | Number of tension build-ups |
| `tension_release_count` | i32 | Number of tension releases |
| `peak_tension` | f64 | Maximum tension value |
| `tension_range` | f64 | Tension max minus min |

### Repetition & solos

| Column | Type | Description |
|--------|------|-------------|
| `repetition_count` | i32 | Number of repeated sections |
| `repetition_similarity` | f64 | Similarity of repeated sections |
| `solo_section_count` | i32 | Detected solo sections |
| `solo_section_ratio` | f64 | Fraction of track in solo sections |

### Temporal trends

Feature slopes and distributional statistics that capture how things change over the course of a track.

| Column | Type | Description |
|--------|------|-------------|
| `spectral_centroid_slope` | f64 | Overall brightness trend (brightening vs darkening) |
| `spectral_centroid_kurtosis` | f64 | Peakedness of brightness distribution |
| `spectral_flux_skewness` | f64 | Asymmetry of spectral change — positive = more sudden onsets |
| `spectral_bandwidth_slope` | f64 | Spectral width trend |

## Jam scores

Ten composite scores (0-100) are computed from the raw features above. Scores are designed to differentiate musical qualities relevant to jam-band listening.

| Score | Formula inputs | What high scores mean |
|-------|---------------|----------------------|
| **Energy** | RMS (30pts) + LUFS (30pts) + sub-band bass (20pts) + spectral centroid (20pts) | Loud, full-spectrum, driving |
| **Intensity** | Spectral flux variance + dynamic range + loudness range | Wild dynamic swings, intense moments |
| **Groove** | Onset sweet spot 7-9/sec (20pts) + flux CV (30pts) + bass steadiness (25pts) + repetition (25pts) | Locked-in, danceable, steady pocket |
| **Improvisation** | Non-repetition (30pts) + timbral variety (25pts) + structural density/min (25pts) + tonal ambiguity (20pts) | Unpredictable, exploratory, non-repeating |
| **Tightness** | Tempo stability + coherence + spectral smoothness + beat strength | Precise, well-rehearsed, metronomic |
| **Build Quality** | Energy arc detection + tension build/release + energy variance + transition smoothness | Long arcs, building tension, satisfying peaks |
| **Exploratory** | Spectral flatness variety + pitch ambiguity + mode ambiguity + harmonic complexity + transitions | Weird, spacey, tonally adventurous |
| **Transcendence** | Peak energy + sustained high-energy + peak tension + groove-energy synergy + harmonic richness | Transcendent peaks, sustained intensity, everything clicking |
| **Valence** | Spectral brightness + tempo + mode + harmonic simplicity | Happy, bright, upbeat |
| **Arousal** | RMS + spectral flux + onset rate + spectral bandwidth | Exciting, activating, high-energy |

### Score calibration

Scores can be optionally calibrated via `setbreak calibrate` to remove recording-quality bias. The calibration fits an OLS regression of each score against per-show median LUFS, then adjusts scores so that a pristine 1977 SBD and a muddy 1969 AUD are scored on musical merit rather than recording quality.

### Quality gating

Tracks with `data_quality = 'garbage'` (DTS bitstreams, extremely low SNR, heavy clipping) are excluded from all query commands. The `quality-check` command flags these tracks based on SNR, clipping ratio, and DTS bitstream detection.

## Relational detail tables

Beyond the 185-column `analysis_results` table, setbreak stores per-event detail:

| Table | Contents |
|-------|----------|
| `track_chords` | Per-chord timestamps, chord name, confidence |
| `track_segments` | Structural segments with start/end times, labels, energy profiles |
| `track_tension_points` | Tension build and release events with timestamps |
| `track_transitions` | Structural transition points between sections |
| `track_similarity` | Pairwise cosine distance between feature vectors |

## Processing pipeline

```
Audio file
  → Decode (symphonia / claxon / shorten-rs / ape-rs / ffmpeg)
  → Resample to mono f32
  → ferrous-waves AnalysisEngine.analyze()
    → STFT (2048-sample Hann window, 512 hop)
    → Spectral features (centroid, flux, rolloff, bandwidth, flatness, contrast, MFCCs)
    → Harmonic-percussive separation (median filtering)
    → Chromagram + tonnetz + entropy
    → Beat tracking + onset detection + rhythm analysis
    → Pitch detection (pYIN) + key estimation + chord analysis
    → Structural segmentation + tension/transition detection
    → Perceptual metrics (LUFS, SNR, true peak)
  → features.rs: Extract 185 columns from AnalysisResult
  → jam_metrics.rs: Compute 10 jam scores from features
  → queries.rs: Upsert into SQLite
```

Analysis is parallelized via rayon (configurable worker count), with thread-local tokio runtimes. Processing is chunked — each chunk is written to the DB before the next begins, so analysis is resumable on crash.
