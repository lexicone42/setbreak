# Changelog

All notable changes to setbreak are documented here.

Format follows [Keep a Changelog](https://keepachangelog.com/). Versioning follows [Semantic Versioning](https://semver.org/).

## [0.1.0] - 2025-02-15

Initial public release.

### Added
- **scan**: Walk music directories, parse filenames (band, date, disc, track, set), read audio tags
- **analyze**: Parallel audio analysis via ferrous-waves (FFT, STFT, pitch detection, beat tracking, chord estimation) with 10 jam-quality scores
- **setlist**: Look up song titles from archive.org metadata API
- **chains**: Detect multi-song segue chains ranked by jam scores
- **discover**: Find missing shows from archive.org collections
- **similarity**: Cosine-distance track similarity on 47-dimensional feature vectors
- **rescore**: Recompute jam scores without re-analyzing audio
- **stats**: Library statistics (track counts, formats, bands)
- **compare**: Compare versions of a song across shows
- TOML config file (`~/.config/setbreak/config.toml`) with custom band definitions
- Unified BandRegistry with 23 built-in bands and 37 band codes
- SQLite storage with WAL mode, versioned migrations (v1-v6), 8 tables
- SHN decoding via ffmpeg subprocess
- Chunked parallel processing for crash recovery
