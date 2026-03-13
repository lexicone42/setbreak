# Forking SetBreak: Building Your Own Audio Analysis Tool

SetBreak is a Rust CLI that extracts **193 audio features** from music files, stores them
in SQLite, computes domain-specific scores, and provides similarity search. It was built
for jam-band analysis (Grateful Dead, Phish), but the core engine is genre-agnostic.

This guide explains how to fork the project and repurpose it for your own domain —
whether that's a recommendation engine, a riddim classifier, a DJ set analyzer, a
podcast quality scorer, or anything else that benefits from rich audio feature extraction.

---

## Table of Contents

1. [Architecture Overview](#architecture-overview)
2. [What You Get for Free](#what-you-get-for-free)
3. [The Complete Feature Inventory](#the-complete-feature-inventory)
4. [What to Keep, Modify, or Remove](#what-to-keep-modify-or-remove)
5. [Step-by-Step Fork Guide](#step-by-step-fork-guide)
6. [Example: Recommendation Engine ("Find Songs That Feel Like This")](#example-recommendation-engine)
7. [Example: Reggae/Dub Riddim Finder](#example-reggae-dub-riddim-finder)
8. [Example: DJ Transition Planner](#example-dj-transition-planner)
9. [Designing Your Own Scores](#designing-your-own-scores)
10. [Score Lab: Interactive Formula Testing](#score-lab-interactive-formula-testing)
11. [Feature Reference by Use Case](#feature-reference-by-use-case)
12. [Working with the Similarity System](#working-with-the-similarity-system)
13. [Chroma Vectors and Harmonic Similarity](#chroma-vectors-and-harmonic-similarity)
14. [Performance and Memory Notes](#performance-and-memory-notes)
15. [Database Schema and Queries](#database-schema-and-queries)
16. [Dependencies and Build](#dependencies-and-build)

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                        YOUR DOMAIN                              │
│  Scores, queries, discovery, setlist logic, chain detection     │
│  ← This is what you rewrite for your use case                  │
├─────────────────────────────────────────────────────────────────┤
│                    REUSABLE CORE                                │
│  Decode → Analyze → Extract Features → Store → Similarity      │
│  ← This works for any audio, any genre, any use case           │
├─────────────────────────────────────────────────────────────────┤
│                    INFRASTRUCTURE                               │
│  SQLite (WAL), rayon parallelism, CLI (clap), progress bars    │
│  ← Generic plumbing you'll keep as-is                          │
└─────────────────────────────────────────────────────────────────┘
```

The codebase has a clean separation between three layers:

**Layer 1 — Audio Pipeline (keep as-is).** Decodes audio files in any format (MP3,
FLAC, WAV, SHN, APE, WavPack, OGG, M4A, AAC, OPUS — all via pure Rust, no ffmpeg
needed), runs them through the ferrous-waves DSP engine, extracts 193 features, and
stores them in SQLite. This layer is completely genre-agnostic.

**Layer 2 — Scoring & Analysis (rewrite for your domain).** The 10 "jam scores"
(energy, groove, improvisation, etc.) are pure formulas that combine raw features into
domain-specific metrics. These are just weighted sums in a single file
(`jam_metrics.rs`). You replace these with your own scores.

**Layer 3 — Domain Logic (rewrite or remove).** Band registry, filename parsing,
archive.org discovery, setlist import, segue chain detection — this is all specific to
the jam-band use case. You'll replace or remove most of it.

### File Map

```
src/
├── main.rs                    # CLI commands — adapt for your commands
├── lib.rs                     # Module declarations — update as you add/remove modules
│
├── analyzer/
│   ├── mod.rs                 # Analysis pipeline orchestration     ← KEEP
│   ├── decode.rs              # Multi-format audio decoding         ← KEEP
│   ├── features.rs            # Feature extraction from DSP output  ← KEEP
│   ├── jam_metrics.rs         # Domain-specific scores (1122 lines) ← REWRITE
│   └── boundary.rs            # Track boundary features (segues)    ← KEEP or REMOVE
│
├── scanner/
│   ├── mod.rs                 # Filesystem scanner                  ← KEEP (adapt paths)
│   ├── filename.rs            # Regex filename parsing              ← REWRITE for your naming
│   ├── classify.rs            # Live/studio classification          ← REWRITE or REMOVE
│   └── metadata.rs            # Tag reading (lofty)                 ← KEEP
│
├── db/
│   ├── mod.rs                 # SQLite setup + migrations           ← KEEP (add your migrations)
│   ├── models.rs              # Data structs (NewTrack, etc.)       ← ADAPT
│   ├── columns.rs             # Schema inventory + query helpers    ← ADAPT
│   └── queries.rs             # All SQL queries                     ← ADAPT
│
├── similarity.rs              # Cosine similarity + k-NN            ← KEEP
├── chroma.rs                  # Transposition-aware harmonic match  ← KEEP
├── score_lab.rs               # Interactive formula testing         ← KEEP (your main iteration tool)
├── chains.rs                  # Segue chain detection               ← REMOVE (jam-specific)
├── segues.rs                  # Audio boundary segue detection      ← REMOVE or REPURPOSE
├── discovery.rs               # archive.org integration             ← REMOVE or REPLACE
├── calibrate.rs               # LUFS bias regression                ← KEEP (useful for any corpus)
├── bands.rs                   # Band registry (23 jam bands)        ← REWRITE or REMOVE
├── config.rs                  # TOML configuration                  ← KEEP (adapt fields)
└── setlist/                   # Setlist import (GD, Phish)          ← REMOVE
```

---

## What You Get for Free

### Audio Decoding (zero work)
Every common audio format decoded natively in Rust:
- **Symphonia** (via ferrous-waves): MP3, WAV, AIFF, OGG, M4A, AAC, OPUS
- **Claxon**: FLAC
- **shorten-rs**: SHN (Shorten, common in bootleg archives)
- **ape-rs**: APE (Monkey's Audio)
- **wavpack-rs**: WavPack
- **ffmpeg fallback**: DSD (DSF/DFF) only — the one format without a Rust decoder

Automatic 96 kHz → 48 kHz downsampling prevents memory explosions (ferrous-waves
uses O(n^2) memory at high sample rates). DTS bitstream detection rejects corrupt files.

### 193 Audio Features per Track (zero work)
All features extracted automatically during analysis. See the
[complete inventory](#the-complete-feature-inventory) below.

### Similarity Search (zero work)
Z-score normalized cosine similarity across all features, with top-20 nearest neighbors
stored per track. Query: "find me 20 tracks that sound most like this one."

### SQLite Storage with WAL mode (zero work)
Concurrent-read-safe database with versioned migrations. All features stored as typed
columns (not JSON blobs), so you can write arbitrary SQL queries against them.

### Parallel Analysis Pipeline (zero work)
Rayon thread pool with chunked processing, memory management (malloc_trim after each
chunk), and crash-resumable progress. Processes ~85 tracks/hour sustained on a 6-core
machine at 2 workers.

### LUFS Calibration (zero work)
Post-processing regression that removes recording-quality bias from scores. If your
corpus has variable recording quality (live recordings, different microphones, different
mastering), this corrects for it automatically.

### Score Lab (zero work)
Interactive formula testing against stored features, powered by evalexpr. Test score
formulas in seconds without recompiling. See [Score Lab](#score-lab-interactive-formula-testing).

### Chroma-Based Harmonic Matching (zero work)
Transposition-aware harmonic similarity using stored 12-dimensional chroma vectors.
Find tracks with similar chord progressions even in different keys. See
[Chroma Vectors](#chroma-vectors-and-harmonic-similarity).

### Schema Introspection (zero work)
`schema --json` exports the complete feature inventory as machine-readable JSON.
Useful for building tooling, generating documentation, or feeding into other systems.

### CLI Framework (minimal adaptation)
Clap v4 with derive macros, progress bars (indicatif), structured logging (env_logger).
You just change the subcommands.

---

## The Complete Feature Inventory

These 193 features are extracted for every track, regardless of genre. They're stored as
typed SQLite columns and available for scoring, similarity, and direct SQL queries.

### Summary (6 features)
| Feature | Type | Description |
|---------|------|-------------|
| `duration` | REAL | Track duration in seconds |
| `sample_rate` | INT | Sample rate in Hz |
| `channels` | INT | Number of audio channels |
| `peak_amplitude` | REAL | Maximum sample amplitude (0.0–1.0) |
| `rms_level` | REAL | Root mean square level |
| `dynamic_range` | REAL | Dynamic range in dB |

### Spectral Core (12 features, mean + std = 24 columns)
These are the workhorses of timbral analysis. Each is a time-series computed per-frame
by ferrous-waves, then aggregated to mean and standard deviation for storage.

| Feature | What It Measures | Musical Meaning |
|---------|-----------------|-----------------|
| `spectral_centroid` | Center of spectral mass (Hz) | **Brightness**. High = bright/trebly, low = dark/bassy. A reggae dub track will have low centroid; a cymbal-heavy jazz track will have high centroid. |
| `spectral_flux` | Frame-to-frame spectral change | **Texture change rate**. High = rapidly changing timbre (e.g., drum solo). Low = sustained texture (e.g., drone, pad). |
| `spectral_rolloff` | Frequency below which 85% of energy sits | **Spectral weight**. Correlates with centroid but more robust to outlier frequencies. |
| `spectral_flatness` | Geometric/arithmetic mean ratio (0–1) | **Noise vs. tone**. 0 = pure tone, 1 = white noise. Distorted guitars ≈ 0.3–0.5; clean piano ≈ 0.05. |
| `spectral_bandwidth` | Width of the spectrum (Hz) | **Spectral spread**. Wide = full-frequency sound; narrow = focused pitch. |
| `zcr` | Zero-crossing rate | **Percussiveness / noise content**. High ZCR = hi-hats, noise, speech sibilants. Low = bass, sustained tones. |

### Sub-Band Energy (8 features)
Energy decomposed into frequency bands, mean + std each:

| Band | Range | What It Captures |
|------|-------|------------------|
| `sub_band_bass` | 20–250 Hz | Kick drum, bass guitar, sub-bass |
| `sub_band_mid` | 250–2000 Hz | Vocals, guitars, most melodic content |
| `sub_band_high` | 2000–8000 Hz | Presence, consonants, cymbals |
| `sub_band_presence` | 8000+ Hz | Air, shimmer, recording artifacts |

### Sub-Band Flux (4 features)
How fast energy changes within each frequency band:

| Feature | Musical Meaning |
|---------|-----------------|
| `sub_band_flux_bass_mean/std` | Kick drum detection, bass rhythmic activity |
| `sub_band_flux_mid_mean` | Harmonic/melodic change rate |
| `sub_band_flux_high_mean` | Hi-hat/cymbal activity, sibilance |

### MFCCs (26 features)
Mel-Frequency Cepstral Coefficients — the standard representation for timbral similarity.
13 coefficients, each with mean and std. These are the features that make similarity
search work well, because they capture the "shape" of the frequency spectrum in a way
that correlates with human perception.

| Feature | Perceptual Meaning |
|---------|-------------------|
| `mfcc_0` | Overall spectral energy (loudness proxy) |
| `mfcc_1` | Spectral slope (bright vs. dark balance) |
| `mfcc_2` | Spectral curvature |
| `mfcc_3–12` | Progressively finer timbral details |

Higher MFCCs capture subtle timbral nuances. The std (variability) of each coefficient
captures how much the timbre *changes* over the track.

### MFCC Dynamics (2 JSON columns)
| Feature | Description |
|---------|-------------|
| `mfcc_delta_mean_json` | [f32; 13] — First derivatives of MFCCs, capturing timbral velocity |
| `mfcc_delta_delta_mean_json` | [f32; 13] — Second derivatives, capturing timbral acceleration |

### Temporal (5 features)
| Feature | Type | Description |
|---------|------|-------------|
| `tempo_bpm` | REAL | Estimated tempo, 30–300 BPM range. Octave ambiguity check at >160 BPM. |
| `beat_count` | INT | Total detected beats |
| `onset_count` | INT | Total detected note onsets |
| `tempo_stability` | REAL | 0–1, where 1 = perfectly steady tempo (metronome-like) |
| `rhythmic_complexity` | REAL | Higher = more complex rhythmic patterns |

### Pitch (8 features)
| Feature | Description |
|---------|-------------|
| `mean_pitch` | Average detected pitch in Hz |
| `pitch_range_low/high` | Lowest/highest detected pitches |
| `pitch_stability` | How steady the pitch is (high for drones, low for melodies) |
| `dominant_pitch` | Most common pitch frequency |
| `vibrato_presence` | Vibrato amount (note: currently degenerate, always ~0.7) |
| `vibrato_rate` | Vibrato speed in Hz |
| `pitch_confidence_mean` | How confident the pitch detection is (low for noise/drums) |

### Perceptual / Loudness (7 features)
| Feature | Description |
|---------|-------------|
| `lufs_integrated` | Integrated loudness in LUFS (the broadcast standard) |
| `loudness_range` | Loudness Range (LRA) in LU — how dynamic the track is |
| `true_peak_dbfs` | True peak level in dBFS |
| `crest_factor` | Peak-to-RMS ratio (high = punchy transients, low = compressed) |
| `energy_level` | Overall energy level |
| `loudness_std` | Standard deviation of short-term loudness (3-second windows) |
| `peak_loudness` | Maximum momentary loudness |

### Derived Per-Frame Features (22 features)
These are computed from the time-series vectors during extraction — they capture
*temporal dynamics* that the mean/std aggregation would miss.

| Feature | Description | Why It Matters |
|---------|-------------|----------------|
| `spectral_flux_skewness` | +ve = percussive hits, -ve = sustained | Distinguishes drum-heavy vs pad-heavy |
| `spectral_centroid_slope` | Brightness trend over time | Tracks that get brighter vs darker |
| `energy_buildup_ratio` | Energy in last third / first third | >1 = builds up, <1 = fades out |
| `bass_treble_ratio_mean/std` | Low/high frequency balance | Genre fingerprint (dub = high, metal = low) |
| `onset_density_std` | Variability of onset density (10s windows) | Steady rhythm vs stop-start |
| `loudness_buildup_slope` | Linear trend of loudness over time | Crescendo detection |
| `peak_energy_time` | Normalized time of peak energy (0–1) | 0.3 = early peak, 0.8 = late climax |
| `pitch_contour_std` | Standard deviation of pitch | Wide melodic range vs monotone |
| `pitch_clarity_mean` | Average pitch detection confidence | Noisy/percussive vs tonal |
| `pitched_frame_ratio` | Fraction of frames with clear pitch | 0.9 = melodic, 0.2 = percussive |
| `mfcc_flux_mean` | Timbral change rate (frame-to-frame MFCC distance) | High = rapidly shifting timbre |
| `onset_interval_entropy` | Shannon entropy of inter-onset intervals | Low = regular rhythm, high = complex |
| `spectral_centroid_kurtosis` | Excess kurtosis of brightness | Peaky vs flat brightness distribution |
| `bass_energy_slope` | Linear trend of bass energy | Bass that builds or drops |
| `spectral_bandwidth_slope` | Bandwidth broadening/narrowing trend | Sound opening up vs closing in |
| `loudness_dynamic_spread` | Max - min short-term LUFS | Total loudness excursion |
| `beat_regularity` | CV of inter-beat intervals (0 = metronome) | Tight vs loose rhythm |
| `peak_tension` | Maximum tension value (0–1) | How much tension the track reaches |
| `tension_range` | Max - min tension | Total tension excursion |
| `energy_peak_count` | Number of energy climaxes | Multi-peak (jam) vs single-arc (pop) |
| `energy_valley_depth_mean` | Mean valley depth vs mean peak | How far energy drops between peaks |
| `rhythmic_periodicity_strength` | Strongest periodic component (0–1) | Clear beat vs free rhythm |
| `spectral_loudness_correlation` | Pearson r: brightness vs loudness | +ve = louder=brighter (typical), -ve = inverse |

### Spectral Shape (5 features + 1 JSON)
| Feature | Description |
|---------|-------------|
| `spectral_skewness_mean` | +ve = low-frequency dominant spectrum |
| `spectral_kurtosis_mean` | High = tonal (peaked spectrum), low = noise-like |
| `spectral_entropy_mean/std` | 0 = pure tone, 1 = white noise |
| `spectral_slope_mean` | Spectral tilt (negative = natural high-frequency roll-off) |
| `spectral_contrast_json` | [f32; 7] per-band peak/valley ratios |

### Harmonic / Chroma (5 features + 1 JSON)
| Feature | Description |
|---------|-------------|
| `tonnetz_json` | [f32; 6] Tonnetz harmonic center (pitch-class geometry) |
| `tonnetz_flux_mean` | Harmonic Change Detection Function (HCDF) |
| `chroma_flux_mean` | Pitch-class change rate |
| `harmonic_percussive_ratio` | 1.0 = purely harmonic, 0.0 = purely percussive |
| `chromagram_entropy` | 0 = one pitch class dominates, 2.5 = all pitch classes equal |

### Beat-Sync Rhythm (4 features + 1 JSON)
| Feature | Description |
|---------|-------------|
| `beat_pattern_json` | Sub-band onset patterns aligned to beat grid |
| `syncopation` | Off-beat emphasis (0 = straight on-beat) |
| `pulse_clarity` | How clear/strong the beat is (0–1) |
| `offbeat_ratio` | Mid-band offbeat/downbeat energy ratio |

### Timbral Texture (6 features)
| Feature | Description |
|---------|-------------|
| `spectral_spread_mean/std` | Spectral bandwidth (narrow = tonal, wide = noisy) |
| `spectral_crest_mean/std` | Max/mean spectrum ratio (high = tonal, low = noise) |
| `roughness_mean/std` | Sensory dissonance (Sethares model). Higher = harsher, more distorted. |

### Onset / Attack Envelope (7 features)
| Feature | Description |
|---------|-------------|
| `attack_time_mean/std` | Mean rise time from onset to peak (seconds) |
| `decay_time_mean/std` | Mean decay time from peak to quiet |
| `onset_strength_mean/std` | How strong the onsets are (loud transients vs soft entries) |
| `onset_strength_skewness` | +ve = rare strong hits (sparse drumming), -ve = consistently strong |

### Micro-Rhythm (4 features)
| Feature | Description |
|---------|-------------|
| `swing_ratio` | 8th-note duration ratio. 1.0 = straight, ~1.67 = shuffle/swing, ~2.0 = triplet |
| `microtiming_deviation_mean/std` | How far onsets land from the grid (seconds) |
| `microtiming_bias` | Positive = consistently ahead of beat, negative = behind |

### Temporal Modulation (1 JSON)
| Feature | Description |
|---------|-------------|
| `temporal_modulation_json` | [f32; 5] Energy in 5 modulation frequency bands |

### Self-Similarity (1 feature)
| Feature | Description |
|---------|-------------|
| `chroma_self_similarity_bandwidth` | How repetitive the harmonic content is (wide = more repetitive) |

### Music Understanding (6 features + 1 JSON)
| Feature | Description |
|---------|-------------|
| `section_diversity_score` | How different the sections are from each other |
| `spectral_contrast_slope` | How contrast changes across frequency bands |
| `spectral_contrast_range` | Total contrast variation |
| `onset_strength_contour_json` | [f32; 4] DCT of the rhythmic shape |

### Musical Key / Chord (13 features + 1 JSON)
| Feature | Description |
|---------|-------------|
| `estimated_key` | Detected key, e.g., "A minor", "D mixolydian" (modal detection works) |
| `key_confidence` | Confidence of key detection |
| `tonality` | Tonality measure |
| `harmonic_complexity` | Harmonic complexity |
| `chord_count` | Number of distinct chords used |
| `chord_change_rate` | Chord transitions per second |
| `mode_clarity` | How clearly major vs minor (ambiguous modes score low) |
| `key_alternatives_count` | Number of plausible alternative keys |
| `time_sig_numerator/denominator` | Detected time signature |
| `chroma_vector` | [f64; 12] pitch class distribution (C, C#, D, ..., B) |
| `major_frame_ratio` | Fraction of frames classified as major (0–1) |
| `major_chord_ratio` | Fraction of chords that are major types (0–1) |

### Dynamics Trajectory (4 features)
| Feature | Description |
|---------|-------------|
| `dynamics_entropy` | LUFS histogram entropy (0–1). Low = mostly one loudness level. |
| `dynamics_slope` | LUFS/minute linear trend. Positive = crescendo, negative = diminuendo. |
| `dynamics_peak_count` | Loudness peaks with >=3 LU prominence |
| `key_change_count` | Key changes detected across 30-second windows |

### Quality (4 features)
| Feature | Description |
|---------|-------------|
| `recording_quality_score` | Overall quality score |
| `snr_db` | Signal-to-noise ratio in dB |
| `clipping_ratio` | Fraction of clipped samples |
| `noise_floor_db` | Noise floor level in dB |

### Structure / Segments (13 features)
| Feature | Description |
|---------|-------------|
| `segment_count` | Number of structural segments detected |
| `temporal_complexity` | Temporal complexity measure |
| `coherence_score` | Overall coherence (how well the track holds together) |
| `energy_shape` | Energy profile shape: Flat, Peak, Increasing, Decreasing, Valley, Oscillating, Complex |
| `peak_energy` | Maximum energy level reached |
| `energy_variance` | How much energy varies across the track |
| `tension_build_count` | Number of tension build-ups |
| `tension_release_count` | Number of tension releases |
| `repetition_count` | Number of repetitive sections |
| `repetition_similarity` | How similar the repeated sections are to each other |
| `transition_count` | Number of section transitions |
| `solo_section_count/ratio` | Solo sections (note: always 0, upstream limitation) |

### Classification (2 features)
| Feature | Description |
|---------|-------------|
| `classification_music_score` | Music vs. non-music confidence |
| `hnr` | Harmonics-to-noise ratio |

### Boundary (4 features)
| Feature | Description |
|---------|-------------|
| `tail_rms_db` | RMS of the last 3 seconds in dBFS |
| `tail_silence_pct` | Fraction of last 3 seconds below -40 dBFS |
| `head_rms_db` | RMS of the first 1 second in dBFS |
| `head_silence_pct` | Fraction of first 1 second below -40 dBFS |

### Relational Detail Tables (stored per-track, not as columns)

In addition to the 193 scalar/JSON features, four detail tables store event-level data:

**track_chords** — Per-chord timestamps with confidence:
```sql
SELECT chord, start_time, duration, confidence FROM track_chords WHERE track_id = ?
-- Example: ("Am", 12.5, 3.2, 0.87), ("G", 15.7, 2.8, 0.91), ...
```

**track_segments** — Structural sections with per-section features:
```sql
SELECT label, section_type, start_time, duration, energy, spectral_centroid, key
FROM track_segments WHERE track_id = ?
-- Example: ("Intro", "intro", 0.0, 32.5, 0.4, 1200.0, "Am"), ...
```

**track_tension_points** — Build/release moments:
```sql
SELECT time, tension, change_type FROM track_tension_points WHERE track_id = ?
-- Example: (45.2, 0.8, "Build"), (72.1, 0.3, "Release"), ...
```

**track_transitions** — Section boundaries:
```sql
SELECT time, transition_type, strength, duration FROM track_transitions WHERE track_id = ?
```

---

## What to Keep, Modify, or Remove

### Keep As-Is (the free lunch)
| File | Why |
|------|-----|
| `analyzer/mod.rs` | Analysis pipeline orchestration, memory management, chunked processing |
| `analyzer/decode.rs` | Multi-format decoding with 96kHz safety, DTS detection |
| `analyzer/features.rs` | All 193 features extracted here — genre-agnostic |
| `analyzer/boundary.rs` | Track head/tail features (useful beyond segues) |
| `similarity.rs` | Z-score normalization + cosine similarity + k-NN |
| `score_lab.rs` | Interactive formula testing (evalexpr, your main iteration tool) |
| `chroma.rs` | Transposition-aware harmonic matching (chroma vectors) |
| `calibrate.rs` | LUFS bias regression (useful for any mixed-quality corpus) |
| `scanner/mod.rs` | Directory walking + file detection |
| `scanner/metadata.rs` | Tag reading via lofty crate |
| `db/mod.rs` | SQLite setup, WAL mode, migration framework |
| `config.rs` | TOML config loading (change the fields, keep the pattern) |

### Modify
| File | What to Change |
|------|---------------|
| `main.rs` | Replace CLI commands with your domain's commands |
| `analyzer/jam_metrics.rs` | **This is the big one.** Replace all 10 jam scores with your own domain scores. The file is self-contained: pure functions that take feature values and return 0–100 scores. |
| `db/models.rs` | Update `NewTrack` struct if you add/remove parsed fields |
| `db/columns.rs` | Update score column names, WHERE clause helpers |
| `db/queries.rs` | Adapt queries for your domain (top, compare, show, etc.) |
| `scanner/filename.rs` | Rewrite regex patterns for your directory structure |
| `config.rs` | Change config fields for your domain |
| `Cargo.toml` | Change name, description, keywords |

### Remove (jam-band specific)
| File | What It Does |
|------|-------------|
| `bands.rs` | 23 jam-band definitions with archive.org strategies |
| `chains.rs` | Multi-song segue chain detection |
| `segues.rs` | Audio-based segue detection |
| `discovery.rs` | archive.org show discovery |
| `setlist/` | Setlist import from gdshowsdb + phish.in |
| `scanner/classify.rs` | Live/studio classification (or rewrite for your needs) |

---

## Step-by-Step Fork Guide

### Step 1: Clone and Verify the Build

```bash
git clone <your-fork-url>
cd setbreak

# You need ferrous-waves as a sibling directory
git clone https://github.com/lexicone42/ferrous-waves.git ../ferrous-waves

# Also needed (pure Rust audio decoders):
git clone https://github.com/lexicone42/shorten-rs.git ../shorten-rs
git clone https://github.com/lexicone42/ape-rs.git ../ape-rs
git clone https://github.com/lexicone42/wavpack-rs.git ../wavpack-rs

# Build (always release — debug is 10-30x slower due to unoptimized FFT)
cargo build --release

# Verify it works
./target/release/setbreak --help
```

### Step 2: Rename the Project

In `Cargo.toml`:
```toml
[package]
name = "your-tool-name"
description = "Your description"
keywords = ["your", "keywords"]
```

### Step 3: Gut the Domain-Specific Code

Remove or empty out:
- `src/bands.rs` — Replace with your own taxonomy (or remove entirely)
- `src/chains.rs` — Remove
- `src/segues.rs` — Remove
- `src/discovery.rs` — Remove (or replace with your data source)
- `src/setlist/` — Remove the entire directory
- `src/scanner/classify.rs` — Simplify or remove

Update `src/lib.rs` to remove the deleted module declarations.

### Step 4: Simplify the Scanner

In `src/scanner/filename.rs`, replace the jam-band regex patterns with patterns that
match your directory structure. The scanner extracts metadata from file paths —
adapt this to however your files are organized.

If your files have good ID3/Vorbis tags and you don't need path parsing, you can
simplify `filename.rs` to just return the filename and rely on `metadata.rs` for
everything else.

### Step 5: Design Your Scores

This is the creative part. Open `src/analyzer/jam_metrics.rs` and study the pattern:

```rust
pub fn compute_jam_scores(analysis: &mut NewAnalysis, segment_energies: &[f32]) {
    let energy = compute_energy_score(analysis);
    let groove = compute_groove_score(analysis);
    // ... etc
    analysis.energy_score = Some(energy);
    analysis.groove_score = Some(groove);
}

fn compute_energy_score(a: &NewAnalysis) -> f64 {
    let rms_part = (a.rms_level.unwrap_or(0.0) / 0.18).clamp(0.0, 1.0) * 30.0;
    let lufs_part = ((a.lufs_integrated.unwrap_or(-55.0) + 55.0) / 22.0).clamp(0.0, 1.0) * 30.0;
    let bass_part = (a.sub_band_bass_mean.unwrap_or(0.0) / 0.15).clamp(0.0, 1.0) * 20.0;
    let bright_part = ((a.spectral_centroid_mean.unwrap_or(2000.0) - 2000.0) / 6000.0)
        .clamp(0.0, 1.0) * 20.0;
    (rms_part + lufs_part + bass_part + bright_part).round()
}
```

The pattern is always: read features from the analysis struct → apply normalization
and weighting → sum to 0–100. Replace these with your own scores. See the
[Designing Your Own Scores](#designing-your-own-scores) section for detailed guidance.

### Step 6: Update the Database Schema

In `src/db/mod.rs`, add a new migration function for your schema changes:

```rust
fn migrate_v19(conn: &Connection) -> Result<()> {
    // Rename score columns or add new ones
    conn.execute_batch("
        ALTER TABLE analysis_results ADD COLUMN my_score_1 REAL;
        ALTER TABLE analysis_results ADD COLUMN my_score_2 REAL;
    ")?;
    conn.pragma_update(None, "user_version", 19)?;
    Ok(())
}
```

Update `models.rs` to match, and update `columns.rs` with your new score names.

### Step 7: Update CLI Commands

In `main.rs`, replace the jam-specific commands with your domain's commands. The
`top`, `compare`, `show`, `similar` pattern generalizes well — most analysis tools
want "top N by score X" and "find similar to track Y".

### Step 8: Test with Your Music

```bash
# Scan your music library
./target/release/your-tool scan /path/to/your/music

# Run analysis (2 workers is safe; increase if you have RAM)
./target/release/your-tool analyze -j 2

# Experiment with score formulas interactively (no recompile needed!)
./target/release/your-tool score-lab "pulse_clarity * 25 + (1 - beat_regularity) * 25" -n 20

# Once formulas are stable in jam_metrics.rs, check compiled scores
./target/release/your-tool top your_score -n 20

# Build similarity index
./target/release/your-tool similarity -j 4

# Find similar tracks
./target/release/your-tool similar "Track Name"

# Find harmonically similar tracks (transposition-aware)
./target/release/your-tool harmonic-match "Track Name"

# Export schema as JSON for tooling
./target/release/your-tool schema --json > features.json
```

---

## Example: Recommendation Engine

**Goal:** "Find songs that feel like this one" — a Spotify-Discover-like experience
for a personal music library, based entirely on acoustic features rather than
collaborative filtering.

### What to Keep
Everything in the reusable core. The similarity system is already a recommendation
engine — it finds the 20 most acoustically similar tracks using cosine distance over
all 193 features.

### Scores to Design

Replace the 10 jam scores with **mood/energy dimensions** that users can filter by:

```rust
// Mood dimensions (0-100)
fn compute_danceability(a: &NewAnalysis) -> f64 {
    // Strong, steady beat + moderate tempo + rhythmic consistency
    let tempo_part = tempo_curve(a.tempo_bpm.unwrap_or(0.0), 100.0, 130.0) * 25.0;
    let pulse_part = (a.pulse_clarity.unwrap_or(0.0) * 100.0).clamp(0.0, 1.0) * 25.0;
    let beat_reg = (1.0 - a.beat_regularity.unwrap_or(1.0)).clamp(0.0, 1.0) * 20.0;
    let onset_part = onset_rate_curve(a, 4.0, 8.0) * 15.0; // sweet spot 4-8/sec
    let groove_flux = (1.2 - flux_cv(a)).clamp(0.0, 1.0) / 1.0 * 15.0;
    (tempo_part + pulse_part + beat_reg + onset_part + groove_flux).round()
}

fn compute_chill(a: &NewAnalysis) -> f64 {
    // Low energy + smooth texture + steady dynamics + warm timbre
    let low_energy = (1.0 - a.rms_level.unwrap_or(0.0) / 0.18).clamp(0.0, 1.0) * 25.0;
    let smooth = (1.0 - a.roughness_mean.unwrap_or(0.0) / 5.0).clamp(0.0, 1.0) * 25.0;
    let steady = (1.0 - a.loudness_std.unwrap_or(0.0) / 10.0).clamp(0.0, 1.0) * 20.0;
    let warm = (1.0 - (a.spectral_centroid_mean.unwrap_or(2000.0) - 1500.0) / 5000.0)
        .clamp(0.0, 1.0) * 15.0;
    let slow_tempo = (1.0 - a.tempo_bpm.unwrap_or(80.0) / 140.0).clamp(0.0, 1.0) * 15.0;
    (low_energy + smooth + steady + warm + slow_tempo).round()
}

fn compute_intensity(a: &NewAnalysis) -> f64 {
    // High energy + high flux + loud + rough texture
    let energy = (a.rms_level.unwrap_or(0.0) / 0.18).clamp(0.0, 1.0) * 25.0;
    let flux = (a.spectral_flux_std.unwrap_or(0.0) / 50.0).clamp(0.0, 1.0) * 25.0;
    let loud = ((a.lufs_integrated.unwrap_or(-55.0) + 55.0) / 22.0).clamp(0.0, 1.0) * 25.0;
    let rough = (a.roughness_mean.unwrap_or(0.0) / 5.0).clamp(0.0, 1.0) * 25.0;
    (energy + flux + loud + rough).round()
}

fn compute_melancholy(a: &NewAnalysis) -> f64 {
    // Minor mode + low brightness + slow + smooth
    let minor = (1.0 - a.major_chord_ratio.unwrap_or(0.5)) * 30.0;
    let dark = (1.0 - (a.spectral_centroid_mean.unwrap_or(2000.0) - 1000.0) / 6000.0)
        .clamp(0.0, 1.0) * 25.0;
    let slow = (1.0 - a.tempo_bpm.unwrap_or(80.0) / 150.0).clamp(0.0, 1.0) * 20.0;
    let smooth = (1.0 - a.roughness_mean.unwrap_or(0.0) / 5.0).clamp(0.0, 1.0) * 15.0;
    let sustained = a.pitched_frame_ratio.unwrap_or(0.5).clamp(0.0, 1.0) * 10.0;
    (minor + dark + slow + smooth + sustained).round()
}

fn compute_complexity(a: &NewAnalysis) -> f64 {
    // Harmonic complexity + key changes + section diversity + rhythmic complexity
    let harmonic = (a.harmonic_complexity.unwrap_or(0.0) / 1.5).clamp(0.0, 1.0) * 25.0;
    let keys = (a.key_change_count.unwrap_or(0) as f64 / 15.0).clamp(0.0, 1.0) * 25.0;
    let sections = (a.section_diversity_score.unwrap_or(0.0)).clamp(0.0, 1.0) * 25.0;
    let rhythm = (a.rhythmic_complexity.unwrap_or(0.0) / 1.0).clamp(0.0, 1.0) * 25.0;
    (harmonic + keys + sections + rhythm).round()
}
```

### Weighted Similarity

The default similarity uses all features equally. For a recommendation engine, you
might want to weight certain features more heavily. Modify `similarity.rs`:

```rust
// Instead of uniform Z-score normalization, apply weights
const FEATURE_WEIGHTS: &[(&str, f64)] = &[
    // MFCCs are the core of timbral similarity — weight heavily
    ("mfcc_*", 2.0),
    // Tempo matters for mood matching
    ("tempo_bpm", 1.5),
    // Key/mode matters for emotional matching
    ("major_chord_ratio", 1.5),
    ("estimated_key", 1.5),
    // Energy features matter for intensity matching
    ("rms_level", 1.2),
    ("lufs_integrated", 1.2),
];
```

### CLI Commands

```
your-tool scan /path/to/music         # Index library
your-tool analyze -j 2                # Extract all features
your-tool mood "Song Title"           # Show mood scores for a track
your-tool recommend "Song Title" -n 10 # Find 10 similar-feeling tracks
your-tool playlist --mood chill -n 30  # Build a chill playlist
your-tool top danceability -n 20       # Most danceable tracks
```

---

## Example: Reggae/Dub Riddim Finder

**Goal:** Index a reggae/dub collection, identify which riddim (instrumental backing
track) each song uses, find all songs over the same riddim, and score tracks for
qualities specific to the genre (bass weight, dub space, skank tightness, etc.).

### The Insight: Riddims Have Acoustic Fingerprints

A riddim is a specific instrumental arrangement that many different artists voice
(sing/DJ) over. The same riddim across different voicings will share:
- Nearly identical `tempo_bpm` (within ±2 BPM)
- Very similar `chroma_vector` (same chord progression)
- Similar `sub_band_bass_mean` (same bass line)
- Similar `beat_pattern_json` (same drum pattern)
- Similar `swing_ratio` (same feel)
- Different `spectral_centroid_mean` (different vocalists)
- Different `pitch_contour_std` (different melodies)

This means the similarity system can cluster riddims if you:
1. **Down-weight vocal features** (MFCCs 1-12, pitch features, spectral centroid)
2. **Up-weight rhythmic/harmonic features** (tempo, chroma, bass, beat pattern, swing)

### Scores to Design

```rust
fn compute_bass_weight(a: &NewAnalysis) -> f64 {
    // How much the bass dominates the mix
    let bass_energy = (a.sub_band_bass_mean.unwrap_or(0.0) / 0.12).clamp(0.0, 1.0) * 35.0;
    let bass_ratio = a.bass_treble_ratio_mean.unwrap_or(1.0);
    let ratio_part = (bass_ratio / 3.0).clamp(0.0, 1.0) * 30.0;
    let sub_flux = (a.sub_band_flux_bass_mean.unwrap_or(0.0) / 20.0).clamp(0.0, 1.0) * 20.0;
    let low_centroid = (1.0 - (a.spectral_centroid_mean.unwrap_or(2000.0) - 800.0) / 4000.0)
        .clamp(0.0, 1.0) * 15.0;
    (bass_energy + ratio_part + sub_flux + low_centroid).round()
}

fn compute_dub_space(a: &NewAnalysis) -> f64 {
    // Spaciousness: wide stereo, reverb (high decay), dynamic range, sparse texture
    let stereo = (1.0 - a.stereo_width_mean.unwrap_or(1.0)).clamp(0.0, 1.0) * 25.0;
    let decay = (a.decay_time_mean.unwrap_or(0.0) / 0.5).clamp(0.0, 1.0) * 25.0;
    let dynamics = (a.loudness_range.unwrap_or(0.0) / 15.0).clamp(0.0, 1.0) * 25.0;
    let sparse = (1.0 - a.onset_count.unwrap_or(100) as f64 / 500.0).clamp(0.0, 1.0) * 25.0;
    (stereo + decay + dynamics + sparse).round()
}

fn compute_skank_tightness(a: &NewAnalysis) -> f64 {
    // How tight and regular the offbeat guitar/keyboard chop is
    let offbeat = (a.offbeat_ratio.unwrap_or(0.0) / 0.8).clamp(0.0, 1.0) * 30.0;
    let regularity = (1.0 - a.beat_regularity.unwrap_or(1.0)).clamp(0.0, 1.0) * 25.0;
    let syncopation_part = (a.syncopation.unwrap_or(0.0) / 0.5).clamp(0.0, 1.0) * 25.0;
    let mid_flux_steady = (1.0 - flux_cv_mid(a)).clamp(0.0, 1.0) * 20.0;
    (offbeat + regularity + syncopation_part + mid_flux_steady).round()
}

fn compute_steppers(a: &NewAnalysis) -> f64 {
    // "Steppers" riddim feel: four-on-the-floor kick, steady tempo, driving bass
    let pulse = (a.pulse_clarity.unwrap_or(0.0)).clamp(0.0, 1.0) * 25.0;
    let tempo_lock = (a.tempo_stability.unwrap_or(0.0) - 0.3).max(0.0) / 0.6 * 25.0;
    let bass_steady = (1.0 - a.sub_band_bass_std.unwrap_or(0.1) / 0.1)
        .clamp(0.0, 1.0) * 25.0;
    let beat_reg = (1.0 - a.beat_regularity.unwrap_or(1.0)).clamp(0.0, 1.0) * 25.0;
    (pulse + tempo_lock + bass_steady + beat_reg).round()
}

fn compute_roots_feel(a: &NewAnalysis) -> f64 {
    // Classic roots reggae: natural instruments, warm, major-leaning, moderate tempo
    let natural = (a.harmonic_percussive_ratio.unwrap_or(0.5) - 0.3).max(0.0) / 0.4 * 20.0;
    let warm = (1.0 - a.roughness_mean.unwrap_or(0.0) / 3.0).clamp(0.0, 1.0) * 20.0;
    let major = a.major_chord_ratio.unwrap_or(0.5).clamp(0.0, 1.0) * 20.0;
    let tempo_range = tempo_curve(a.tempo_bpm.unwrap_or(0.0), 65.0, 85.0) * 20.0;
    let swing = (a.swing_ratio.unwrap_or(1.0) - 1.0).abs().min(0.5) / 0.5 * 20.0;
    (natural + warm + major + tempo_range + swing).round()
}
```

### Riddim Clustering

To find songs on the same riddim, modify the similarity computation to use only
backing-track features:

```rust
// In queries.rs, add a riddim-specific feature vector query
const RIDDIM_FEATURES: &str = "
    tempo_bpm, tempo_stability, beat_regularity,
    sub_band_bass_mean, sub_band_bass_std,
    offbeat_ratio, syncopation, pulse_clarity, swing_ratio,
    chord_change_rate, major_chord_ratio,
    key_change_count, estimated_key
";

// Then in similarity.rs, add a riddim_similarity mode that:
// 1. Loads only RIDDIM_FEATURES
// 2. Requires tempo_bpm to be within ±3 BPM (pre-filter)
// 3. Runs cosine similarity on remaining features
// 4. Clusters results: songs with distance < threshold = same riddim
```

### CLI Commands

```
riddimfinder scan /path/to/reggae
riddimfinder analyze -j 2
riddimfinder top bass_weight -n 20       # Heaviest bass tracks
riddimfinder top dub_space -n 20         # Most spacious dub mixes
riddimfinder riddim "Real Rock"          # Find all songs on this riddim
riddimfinder similar "Satta Massagana"   # Acoustically similar tracks
riddimfinder cluster --min-size 3        # Auto-detect riddim groups
```

---

## Example: DJ Transition Planner

**Goal:** Given a music library, suggest smooth transitions between tracks based on
tempo compatibility, key compatibility, energy matching, and timbral similarity.

### Key Features for DJs

| Feature | DJ Use |
|---------|--------|
| `tempo_bpm` | Beatmatching — tracks within ±3% BPM can be mixed |
| `estimated_key` | Harmonic mixing (Camelot wheel compatibility) |
| `energy_score` (or your version) | Energy flow management |
| `tail_rms_db` / `head_rms_db` | Natural transition points |
| `spectral_centroid_mean` | Timbral compatibility (avoid jarring brightness jumps) |
| `sub_band_bass_mean` | Bass compatibility (avoid bass clashes) |
| `dynamics_slope` | Tracks building up (+) vs coming down (-) |
| `peak_energy_time` | Where the energy peak is (avoid mixing into a drop) |

### Transition Score

```rust
fn compute_transition_score(track_a: &Analysis, track_b: &Analysis) -> f64 {
    // How smoothly track_b can follow track_a
    let bpm_a = track_a.tempo_bpm.unwrap_or(120.0);
    let bpm_b = track_b.tempo_bpm.unwrap_or(120.0);
    let bpm_diff_pct = ((bpm_a - bpm_b) / bpm_a).abs();

    // BPM compatibility (40 points): within 3% is perfect, 6% is passable
    let bpm_score = if bpm_diff_pct < 0.03 { 40.0 }
        else if bpm_diff_pct < 0.06 { 40.0 * (1.0 - (bpm_diff_pct - 0.03) / 0.03) }
        else { 0.0 };

    // Key compatibility (25 points): parse Camelot wheel positions
    let key_score = camelot_compatibility(
        &track_a.estimated_key, &track_b.estimated_key
    ) * 25.0;

    // Energy continuity (20 points): similar energy or smooth ramp
    let energy_diff = (track_a.energy - track_b.energy).abs();
    let energy_score = (1.0 - energy_diff / 30.0).clamp(0.0, 1.0) * 20.0;

    // Timbral compatibility (15 points): similar spectral character
    let centroid_diff = (track_a.spectral_centroid_mean.unwrap_or(2000.0)
        - track_b.spectral_centroid_mean.unwrap_or(2000.0)).abs();
    let timbre_score = (1.0 - centroid_diff / 3000.0).clamp(0.0, 1.0) * 15.0;

    bpm_score + key_score + energy_score + timbre_score
}
```

### CLI Commands

```
djplan scan /path/to/music
djplan analyze -j 4
djplan transitions "Track A" -n 10       # Best 10 tracks to mix into
djplan setlist --start "Track A" -n 15   # Auto-generate a 15-track set
djplan energy-flow                       # Visualize energy curve of a set
```

---

## Score Lab: Interactive Formula Testing

The `score-lab` command is your primary tool for iterating on score formulas. It
evaluates arbitrary mathematical expressions against all stored features **without
recompiling** — collapsing the feedback loop from "edit code → build → rescore → check"
to just "type formula → see results."

### Basic Usage

```bash
# List all available variables (every numeric column in the database)
setbreak score-lab --list

# Test a formula — shows top 20 tracks sorted by the computed value
setbreak score-lab "rms_level / 0.18 * 30 + (lufs_integrated + 55) / 22 * 30"

# Show bottom results (useful for finding what scores LOW)
setbreak score-lab "tempo_stability" --bottom -n 10

# Filter to live recordings over 5 minutes
setbreak score-lab "groove_score * energy_score / 100" --live-only --min-duration 5
```

### Expression Language

Score-lab uses [evalexpr](https://docs.rs/evalexpr), which supports:

| Category | Operators / Functions |
|----------|---------------------|
| Arithmetic | `+`, `-`, `*`, `/`, `^` (power), `%` (modulo) |
| Comparison | `<`, `>`, `<=`, `>=`, `==`, `!=` |
| Logic | `&&`, `\|\|`, `!` |
| Conditional | `if(condition, then_value, else_value)` |
| Math functions | `min(a, b)`, `max(a, b)`, `floor(x)`, `ceil(x)`, `sqrt(x)`, `abs(x)` |

### The Iteration Workflow

```
1. Start with a hypothesis: "danceability = steady beat + moderate tempo + low roughness"
2. Write the formula:
   setbreak score-lab "pulse_clarity * 25 + if(tempo_bpm > 100, if(tempo_bpm < 130, 25, 0), 0) + (1 - roughness_mean / 5) * 25 + (1 - beat_regularity) * 25"
3. Check the top results — do they match your intuition?
4. Adjust weights, thresholds, and features
5. Repeat until satisfied
6. Move the final formula into jam_metrics.rs, rebuild, rescore
```

This is dramatically faster than the compile-rescore cycle. A formula that takes 10
iterations to calibrate would cost ~15 minutes of compile time without score-lab.

### Variable Names

Every numeric column in `analysis_results` is available as a variable. Use
`--list` to see them all. Key ones for score design:

- `rms_level`, `lufs_integrated`, `dynamic_range` — loudness/energy
- `spectral_centroid_mean`, `roughness_mean` — timbre
- `tempo_bpm`, `tempo_stability`, `beat_regularity` — rhythm
- `major_chord_ratio`, `harmonic_complexity`, `key_change_count` — harmony
- `onset_count`, `duration` — derived (onset_rate = onset_count / duration)

NULL values in the database are treated as 0.0.

---

## Designing Your Own Scores

### The Pattern

Every score in setbreak follows the same pattern:

1. **Read features** from the `NewAnalysis` struct (all `Option<f64>` fields)
2. **Normalize** each feature to 0.0–1.0 range
3. **Weight** each normalized feature (weights sum to 100)
4. **Sum** to get a 0–100 score
5. **Optionally gate** on duration or other prerequisites

```rust
fn compute_your_score(a: &NewAnalysis) -> f64 {
    // Gate: skip tracks shorter than 60 seconds
    let dur = a.duration.unwrap_or(0.0);
    if dur < 60.0 { return 0.0; }

    // Feature 1 (30 points): normalize to 0-1, multiply by weight
    let f1 = (a.some_feature.unwrap_or(DEFAULT) / EXPECTED_MAX).clamp(0.0, 1.0) * 30.0;

    // Feature 2 (25 points): inverse relationship (higher feature = lower score)
    let f2 = (1.0 - a.some_feature.unwrap_or(DEFAULT) / EXPECTED_MAX).clamp(0.0, 1.0) * 25.0;

    // Feature 3 (25 points): sweet spot (optimal range, penalized outside)
    let f3 = sweet_spot(a.tempo_bpm.unwrap_or(0.0), 70.0, 90.0) * 25.0;

    // Feature 4 (20 points): ratio of two features
    let ratio = a.feature_a.unwrap_or(1.0) / a.feature_b.unwrap_or(1.0).max(0.01);
    let f4 = (ratio / EXPECTED_MAX_RATIO).clamp(0.0, 1.0) * 20.0;

    (f1 + f2 + f3 + f4).round()
}

/// Sweet spot: returns 1.0 when value is in [low, high], ramps to 0.0 outside.
fn sweet_spot(val: f64, low: f64, high: f64) -> f64 {
    if val >= low && val <= high { return 1.0; }
    if val < low { return (val / low).clamp(0.0, 1.0); }
    // val > high
    (1.0 - (val - high) / high).clamp(0.0, 1.0)
}
```

### Calibration Strategy

1. **Start with `score-lab`** — test formulas interactively before writing Rust code:
   ```bash
   setbreak score-lab "pulse_clarity * 25 + (1 - beat_regularity) * 25 + ..." -n 20
   ```
2. **Check distributions** with SQL to find normalization ranges:
   ```bash
   setbreak sql "SELECT AVG(rms_level), MAX(rms_level), MIN(rms_level) FROM analysis_results"
   ```
3. **Once the formula is stable**, move it into `jam_metrics.rs` as a proper Rust function
4. **Use `rescore`** to apply the compiled formula without re-analyzing
5. **Check with `top`** and `compare` — do the top-scoring tracks match your intuition?
6. **Use `calibrate`** if recording quality varies across your corpus

### Tips

- **Use `unwrap_or(sensible_default)`** for every feature — some tracks may have NULL
  values (e.g., very short tracks, corrupt files, features that require minimum duration)
- **`.clamp(0.0, 1.0)`** before multiplying by weight — prevents negative scores
  and overshoot
- **Duration gates** prevent meaningless scores on very short tracks (intros, applause)
- **Weights should sum to 100** for easy interpretation (score = percentage)
- **Start simple** — 3-4 features per score. Add complexity only when the simple
  version is clearly wrong.
- **The `rescore` command** lets you iterate on formulas without re-analyzing. Change
  the formula, `cargo build --release`, `./target/release/your-tool rescore`, check results.
  This cycle takes seconds, not hours.

---

## Feature Reference by Use Case

### "I want to find songs with similar vibes"
**Primary:** MFCCs (all 26), `spectral_centroid_mean`, `tempo_bpm`, `major_chord_ratio`,
`roughness_mean`, `energy_level`

**Why:** MFCCs capture timbral fingerprint (what instruments sound like), tempo sets the
pace, major/minor ratio sets the emotional tone, roughness captures distortion/harshness,
energy captures loudness.

### "I want to classify genres"
**Primary:** `tempo_bpm`, `swing_ratio`, `sub_band_bass_mean`, `spectral_centroid_mean`,
`offbeat_ratio`, `pulse_clarity`, `roughness_mean`, `harmonic_percussive_ratio`

**Why:** Genres have distinctive tempos (house ≈ 120-130, DnB ≈ 170-180, reggae ≈ 65-85),
swing patterns (jazz = high swing, techno = straight), bass levels (dub = high, folk = low),
brightness (metal = high, ambient = low), offbeat patterns (reggae/ska = high), and
harmonic vs percussive balance.

### "I want to detect energy levels and mood"
**Primary:** `rms_level`, `lufs_integrated`, `roughness_mean`, `spectral_centroid_mean`,
`major_chord_ratio`, `tempo_bpm`, `dynamics_slope`, `valence_score` (built-in), `arousal_score` (built-in)

**Secondary:** `dynamics_entropy`, `loudness_range`, `crest_factor`

### "I want to analyze song structure"
**Primary:** `segment_count`, `section_diversity_score`, `transition_count`, `energy_shape`,
`tension_build_count`, `tension_release_count`, `energy_peak_count`

**Detail tables:** `track_segments` (per-section data), `track_transitions` (section boundaries),
`track_tension_points` (build/release moments)

### "I want to find rhythmically similar tracks"
**Primary:** `tempo_bpm`, `tempo_stability`, `swing_ratio`, `beat_regularity`,
`onset_interval_entropy`, `offbeat_ratio`, `syncopation`, `pulse_clarity`,
`microtiming_deviation_mean`, `rhythmic_periodicity_strength`

**JSON:** `beat_pattern_json`, `temporal_modulation_json`

### "I want to analyze harmonic content"
**Primary:** `estimated_key`, `key_confidence`, `chord_count`, `chord_change_rate`,
`harmonic_complexity`, `chromagram_entropy`, `chroma_flux_mean`, `key_change_count`,
`major_chord_ratio`, `mode_clarity`

**JSON:** `tonnetz_json` (6D harmonic space), `chroma_vector` (12D pitch class distribution)

**Detail table:** `track_chords` (per-chord with timestamps and confidence)

### "I want to assess recording quality"
**Primary:** `snr_db`, `clipping_ratio`, `noise_floor_db`, `recording_quality_score`

**The `data_quality` column** on the tracks table flags garbage (DTS bitstreams, severely
clipped) and suspect (low SNR) recordings. The quality-check command populates this.

### "I want to detect transitions / mix points"
**Primary:** `tail_rms_db`, `tail_silence_pct`, `head_rms_db`, `head_silence_pct`

**The boundary features** measure the energy profile at the start and end of each track.
Tracks that sustain music through the end (high tail RMS, low silence) were likely in a
segue or continuous mix. Tracks with silence at the end had a clean break.

### "I want to work with dynamics and loudness over time"
**Primary:** `dynamics_entropy`, `dynamics_slope`, `dynamics_peak_count`, `loudness_range`,
`loudness_buildup_slope`, `loudness_dynamic_spread`, `energy_buildup_ratio`,
`peak_energy_time`

---

## Working with the Similarity System

### How It Works

`similarity.rs` implements a simple but effective pipeline:

1. **Load feature vectors** — all numerical columns from `analysis_results` are loaded
   as a `Vec<f64>` per track (~150 dimensions after excluding text/JSON columns)
2. **Z-score normalize** — each dimension is centered (subtract mean) and scaled
   (divide by std) so that all features contribute equally regardless of their natural
   scale (e.g., tempo in 30-300 vs. MFCC in -20 to +20)
3. **Cosine similarity** — for each track pair, compute
   `dot(a, b) / (||a|| * ||b||)`. Range: -1 (opposite) to +1 (identical).
4. **Store top-K** — keep only the 20 most similar tracks per track (distance =
   1 - similarity, so 0 = identical)

### Customizing Similarity

**Option 1: Feature weighting.** Multiply certain normalized features by a weight
before computing cosine similarity. This makes those features matter more.

**Option 2: Feature selection.** Create a domain-specific feature vector query that
only selects features relevant to your use case. For example, a "rhythmic similarity"
query would only include tempo, beat, onset, and swing features.

**Option 3: Multiple similarity indices.** Store multiple similarity results in
different tables — one for timbral similarity (MFCC-heavy), one for rhythmic similarity
(tempo/beat-heavy), one for harmonic similarity (key/chord-heavy).

Example: adding a second similarity table:

```rust
// In db/mod.rs, add migration:
conn.execute_batch("
    CREATE TABLE IF NOT EXISTS track_rhythmic_similarity (
        track_id INTEGER NOT NULL,
        similar_track_id INTEGER NOT NULL,
        distance REAL NOT NULL,
        rank INTEGER NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_rhythmic_sim ON track_rhythmic_similarity(track_id, rank);
")?;
```

```rust
// In queries.rs, add a rhythmic-only feature vector query:
pub fn get_rhythmic_feature_vectors(&self) -> Result<Vec<(i64, Vec<f64>)>> {
    let sql = "SELECT a.track_id,
        COALESCE(a.tempo_bpm, 0), COALESCE(a.tempo_stability, 0),
        COALESCE(a.beat_regularity, 0), COALESCE(a.swing_ratio, 0),
        COALESCE(a.syncopation, 0), COALESCE(a.pulse_clarity, 0),
        COALESCE(a.offbeat_ratio, 0), COALESCE(a.onset_interval_entropy, 0),
        COALESCE(a.rhythmic_periodicity_strength, 0),
        COALESCE(a.microtiming_deviation_mean, 0)
    FROM analysis_results a
    JOIN tracks t ON t.id = a.track_id
    WHERE COALESCE(t.data_quality, 'ok') != 'garbage'";
    // ... same pattern as get_feature_vectors()
}
```

### Similarity Performance

- **O(n^2) computation** — 10,000 tracks = 100 million pairwise comparisons.
  Takes ~10 minutes at 4 workers on a 6-core machine.
- **Partial sort** (`select_nth_unstable_by`) avoids full sort — only top-K are found.
- **Storage:** 20 neighbors × 10,000 tracks = 200,000 rows. Tiny in SQLite.
- For much larger libraries (100K+ tracks), consider approximate nearest neighbors
  (ANN) via a crate like `hnsw` or `annoy-rs`.

---

## Chroma Vectors and Harmonic Similarity

The `harmonic-match` command finds tracks with similar chord progressions using the
stored 12-dimensional chroma vectors — even when tracks are in different keys.

### How It Works

Each track's `chroma_vector` is a 12-element array representing the energy in each
pitch class (C, C#, D, ..., B). Two tracks playing the same chord progression in
different keys will have the same chroma shape, just rotated.

The `harmonic-match` command:
1. Loads the target track's chroma vector
2. For each other track, tries all 12 rotations (transpositions)
3. Picks the rotation that minimizes cosine distance
4. Returns the best matches sorted by distance

```bash
# Find tracks harmonically similar to a specific performance
setbreak harmonic-match "Dark Star" -d 1972-08-27

# Same key only (no transposition)
setbreak harmonic-match "Scarlet Begonias" --same-key

# More results
setbreak harmonic-match "Eyes of the World" -n 30
```

### Using Chroma for Custom Analysis

The chroma module (`src/chroma.rs`) provides reusable functions:

- `rotate_chroma(chroma, semitones)` — circular shift for transposition
- `best_transposition(a, b)` — find the optimal alignment between two chroma vectors
- `chroma_cosine_distance(a, b)` — distance metric for harmonic similarity

These are useful for building features like:
- **Riddim clustering** — group tracks by chord progression regardless of key
- **Key-aware playlists** — ensure adjacent tracks are in compatible keys
- **Harmonic complexity scoring** — measure how "far" a track's chroma is from a
  simple major/minor template

### The Schema Command

Use `schema --json` to get machine-readable column metadata for building tooling:

```bash
# Full schema as JSON
setbreak schema --json > features.json

# Just rhythm features
setbreak schema --category rhythm --json

# Search for bass-related features
setbreak schema --grep bass --json
```

---

## Performance and Memory Notes

### Analysis Speed
- **~8x realtime**: a 16-minute track takes ~2 minutes to analyze
- **~85 tracks/hour** sustained at 2 workers on a 6-core machine
- Bottleneck is ferrous-waves DSP (FFT, MFCC, beat tracking), not decode or I/O

### Memory
- Each track being analyzed uses **300-500 MB** of RAM (ferrous-waves intermediate
  data structures)
- With 2 workers: ~1 GB peak. With 4 workers: ~2 GB peak.
- The 96 kHz → 48 kHz downsampling is critical — without it, a single 96 kHz track
  can use 25+ GB
- `malloc_trim(0)` after each chunk returns memory to the OS (glibc-specific,
  Linux only — on macOS this is a no-op but memory is still freed normally)

### Chunk-and-Resume
- Tracks are analyzed in chunks (chunk_size = number of workers)
- Each chunk is written to the DB immediately after analysis
- If the process is killed, it resumes from the last unanalyzed track
- No explicit checkpointing needed — the DB is the checkpoint

### Build
- **Always build in release mode**: `cargo build --release`
- Debug builds are 10-30x slower due to unoptimized FFT
- Release build with LTO takes ~1.5 minutes to compile
- Total binary size: ~15 MB (includes bundled SQLite)

### Database Size
- ~10 KB per analyzed track (193 columns + detail tables)
- 10,000 tracks ≈ 100 MB database
- WAL mode means reads never block writes and vice versa

---

## Database Schema and Queries

### Direct SQL Access

The `sql` command gives you raw SQL access to the database:

```bash
# Distribution of a feature
your-tool sql "SELECT
    ROUND(tempo_bpm/10)*10 as bpm_bucket,
    COUNT(*) as count
FROM analysis_results
WHERE tempo_bpm IS NOT NULL
GROUP BY bpm_bucket
ORDER BY bpm_bucket"

# Correlation between features
your-tool sql "SELECT
    ROUND(100 * (
        AVG(roughness_mean * spectral_centroid_mean) -
        AVG(roughness_mean) * AVG(spectral_centroid_mean)
    ) / (
        STDEV(roughness_mean) * STDEV(spectral_centroid_mean)
    )) / 100.0 as correlation
FROM analysis_results"

# Feature ranges for calibrating score normalization
your-tool sql "SELECT
    MIN(swing_ratio), AVG(swing_ratio), MAX(swing_ratio),
    MIN(offbeat_ratio), AVG(offbeat_ratio), MAX(offbeat_ratio)
FROM analysis_results a
JOIN tracks t ON t.id = a.track_id
WHERE COALESCE(t.data_quality, 'ok') != 'garbage'"
```

### Adding Queries

In `db/queries.rs`, the pattern for all queries is:

```rust
pub fn query_your_thing(&self, params: YourParams) -> Result<Vec<YourResult>> {
    let sql = format!("
        SELECT {TRACK_SCORE_SELECT}
        FROM analysis_results a
        JOIN tracks t ON t.id = a.track_id
        WHERE {NOT_GARBAGE}
        AND your_condition = ?1
        ORDER BY a.your_score DESC
        LIMIT ?2
    ");
    let mut stmt = self.conn.prepare(&sql)?;
    let rows = stmt.query_map(params![params.value, params.limit], |row| {
        map_track_score(row)
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}
```

### The Migration Pattern

Schema changes are versioned via `PRAGMA user_version`. Each migration is a function:

```rust
// In db/mod.rs
fn migrate_v19(conn: &Connection) -> Result<()> {
    // Add new columns
    conn.execute_batch("
        ALTER TABLE analysis_results ADD COLUMN my_new_score REAL;
        CREATE INDEX IF NOT EXISTS idx_my_score ON analysis_results(my_new_score);
    ")?;
    conn.pragma_update(None, "user_version", 19)?;
    Ok(())
}

// Called from ensure_schema():
pub fn ensure_schema(conn: &Connection) -> Result<()> {
    let version: i32 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if version < 19 { migrate_v19(conn)?; }
    // ... etc
    Ok(())
}
```

This is idempotent — running the same migration twice is safe because
`ALTER TABLE ADD COLUMN` fails silently if the column already exists (and the
`PRAGMA user_version` check skips it entirely).

---

## Dependencies and Build

### Cargo.toml Dependencies

| Crate | Purpose | Keep? |
|-------|---------|-------|
| `ferrous-waves` | All DSP (FFT, MFCC, beat tracking, etc.) | **Required** |
| `rusqlite` | SQLite database | **Required** |
| `clap` | CLI argument parsing | **Required** |
| `indicatif` | Progress bars | Keep (nice UX) |
| `lofty` | Audio tag reading (ID3, Vorbis, etc.) | **Required** for metadata |
| `rayon` | Parallel analysis | **Required** for performance |
| `serde` + `serde_json` | Serialization | **Required** |
| `serde_yaml` | YAML setlist import | Remove if no YAML |
| `toml` | Config file parsing | Keep |
| `walkdir` | Directory traversal | **Required** |
| `claxon` | Native FLAC decoding | Keep (common format) |
| `shorten-rs` | Native SHN decoding | Remove if not needed |
| `ape-rs` | Native APE decoding | Remove if not needed |
| `wavpack-rs` | Native WavPack decoding | Remove if not needed |
| `anyhow` + `thiserror` | Error handling | Keep |
| `chrono` | Timestamps | Keep |
| `regex` | Filename parsing | Keep if parsing paths |
| `directories` | XDG paths | Keep |
| `ureq` | HTTP client | Remove if no API calls |
| `log` + `env_logger` | Logging | Keep |
| `tokio` | Async runtime (ferrous-waves is async) | **Required** |
| `libc` | malloc_trim (Linux memory management) | Keep on Linux |
| `evalexpr` | Expression evaluation (score-lab) | Keep (enables formula iteration) |

### Minimum Viable Fork

If you want the absolute minimum — just scan, analyze, store features, and query:

**Keep:** ferrous-waves, rusqlite, clap, rayon, serde/serde_json, toml, walkdir,
claxon, lofty, anyhow/thiserror, chrono, directories, log/env_logger, tokio, libc

**Remove:** serde_yaml, ureq, shorten-rs, ape-rs, wavpack-rs (unless you have
those formats), regex (if not parsing filenames)

This gives you a ~12 MB binary that can scan, analyze, and query any MP3/FLAC/WAV/OGG
library with 193 features per track.

---

## FAQ

### Can I use this with Python / a web app / not-Rust?

Yes. The analysis pipeline writes everything to SQLite. Once analyzed, you can query
the database from any language:

```python
import sqlite3

conn = sqlite3.connect("your.db")
# Get all features for a track
row = conn.execute("""
    SELECT a.*, t.title, t.parsed_date
    FROM analysis_results a
    JOIN tracks t ON t.id = a.track_id
    WHERE t.title LIKE '%Dark Star%'
""").fetchone()
```

You could build a Python web UI that reads the SQLite database, or export to CSV/JSON
for use in Jupyter notebooks, or pipe into a machine learning pipeline.

### Can I add new features beyond the 193?

Yes. ferrous-waves returns rich `AnalysisResult` structs with time-series data. The
feature extraction in `features.rs` aggregates these to scalars. You can add new
aggregations:

```rust
// In features.rs, inside the extract() function:
// Example: compute the 95th percentile of spectral flux (instead of just mean/std)
let flux_95 = percentile(&result.spectral.spectral_flux, 0.95);
analysis.my_new_feature = Some(flux_95);
```

Then add the column to the DB schema (migration) and the `NewAnalysis` struct.

### Can I use a different DSP library instead of ferrous-waves?

Yes, but it's a bigger change. The interface point is `features.rs` — it reads from
`AnalysisResult` and writes to `NewAnalysis`. If you swap the DSP library, you need
to adapt `features.rs` to read from your library's output format. Everything downstream
(scores, similarity, queries) remains unchanged because it only sees `NewAnalysis`.

### What if I have 100,000+ tracks?

The analysis pipeline scales linearly — 100K tracks at 85/hour = ~1,200 hours = 50 days
at 2 workers. You'd want to:
- Increase workers (if you have RAM: 4-8 workers)
- Run on a machine with more cores
- Use the chunked-resume feature to run overnight and resume

The similarity computation is O(n^2), which becomes a bottleneck at 100K tracks. At that
scale, replace the brute-force cosine similarity with approximate nearest neighbors
(e.g., `hnsw` crate).

SQLite handles millions of rows fine. WAL mode keeps reads fast during writes.

### Do I need to understand DSP to use this?

No. The DSP is entirely handled by ferrous-waves. You work with the extracted features,
which are just numbers with intuitive meanings (brightness, tempo, loudness, etc.).
Designing scores requires understanding what the features measure (this guide covers
that), but not how they're computed.

---

## License

SetBreak is Apache-2.0 licensed. The pure Rust decoder crates (shorten-rs, ape-rs,
wavpack-rs) are MIT licensed. ferrous-waves is a fork — check its license.

---

*This guide was written for Claude Code instances and human developers alike. If you're
a Claude helping someone fork setbreak, the key insight is: the analysis pipeline is
genre-agnostic. Focus your changes on `jam_metrics.rs` (scores), `filename.rs`
(scanner), and `main.rs` (CLI). Everything else either works as-is or needs minor
adaptation.*
