# Changelog

All notable changes to setbreak are documented here.

Format follows [Keep a Changelog](https://keepachangelog.com/). Versioning follows [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
- **v14 features**: Harmonic-percussive ratio, chromagram entropy, spectral contrast slope/range, onset strength contour (DCT), section diversity score
- **v13 features**: Spectral spread, spectral crest, roughness, stereo width, MFCC deltas/delta-deltas, attack/decay times, onset strength stats, swing ratio, microtiming, temporal modulation, chroma self-similarity
- **v12 features**: Spectral contrast (7-band), tonnetz (6D tonal space), chroma flux, sub-band flux, beat patterns, syncopation, pulse clarity, offbeat ratio
- **v9-v11 features**: 26 per-frame derived features (slopes, correlations, entropy, distributional statistics)
- **ANALYZER.md**: Comprehensive reference for all 185 extracted features and 10 jam scores

### Changed
- Analysis column count: 100+ → 185
- Schema version: v6 → v14

## [0.3.0] - 2025-02-17

### Added
- **calibrate** command: LUFS-based score calibration removes recording quality bias via OLS regression
- **quality-check** command: Flags tracks with garbage analysis (DTS bitstreams, low SNR, heavy clipping)
- `data_quality` column on tracks table (ok/suspect/garbage)
- DTS bitstream detection in decoder (statistical check on first 4096 samples)
- All query commands filter out garbage-quality tracks automatically

## [0.2.0] - 2025-02-16

### Added
- **classify** command: Recording type classification (live/studio/live_album/unknown) via tiered heuristic
- **Native SHN decoding** via [shorten-rs](https://github.com/lexicone42/shorten-rs) — eliminates ffmpeg dependency for SHN files
- **Native FLAC decoding** via claxon
- **Native APE decoding** via [ape-rs](https://github.com/lexicone42/ape-rs) — all Monkey's Audio compression levels
- Improvisation score v3: redesigned with timbral variety, structural density, tonal ambiguity
- Build quality score: replaced with segment-level arc detection
- 60-second duration floor for transcendence scoring

### Changed
- ffmpeg now only required for WavPack (.wv) and DSD (.dsf/.dff) files
- Recalibrated 4 jam scores based on 9,984-track analysis

## [0.1.0] - 2025-02-15

Initial public release.

### Added
- **scan**: Walk music directories, parse filenames (band, date, disc, track, set), read audio tags
- **analyze**: Parallel audio analysis via ferrous-waves (FFT, STFT, pitch detection, beat tracking, chord estimation) with 10 jam-quality scores
- **setlist**: Look up song titles from archive.org metadata API
- **chains**: Detect multi-song segue chains ranked by jam scores
- **discover**: Find missing shows from archive.org collections
- **similarity**: Cosine-distance track similarity on feature vectors
- **rescore**: Recompute jam scores without re-analyzing audio
- **top/compare/show**: Query and compare tracks by scores
- **stats**: Library statistics (track counts, formats, bands)
- TOML config file (`~/.config/setbreak/config.toml`) with custom band definitions
- Unified BandRegistry with 23 built-in bands and 37 band codes
- SQLite storage with WAL mode, versioned migrations, 8 tables
- Chunked parallel processing for crash recovery
