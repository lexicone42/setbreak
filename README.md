# setbreak

[![Built with Claude Code](https://img.shields.io/badge/Built%20with-Claude%20Code-blueviolet?logo=anthropic)](https://claude.ai/claude-code)
[![Rust](https://img.shields.io/badge/Rust-2024%20edition-orange?logo=rust)](https://www.rust-lang.org/)

A CLI tool for analyzing jam-band music libraries. Scans audio files, extracts spectral/temporal/harmonic features, computes jam-quality scores, and stores everything in SQLite for exploration.

Built for Grateful Dead tape collections, but works with any jam-band library (Phish, Widespread Panic, Goose, Billy Strings, etc.).

## What it does

**Scan** your music library to catalog tracks, parsing band names, show dates, disc/track numbers, and venues from filenames and tags:

```
setbreak scan ~/music/grateful_dead/
# Scan complete: 5761 scanned, 5761 new, 0 updated, 0 skipped, 0 errors
```

**Analyze** audio files to extract 80+ features using DSP (FFT, STFT, pitch detection, beat tracking, onset detection, chord estimation):

```
setbreak analyze -j4
# Analysis complete: 1704 analyzed, 3 failed
```

**Look up song titles** from archive.org metadata, matching directory names to archive identifiers:

```
setbreak setlist --dry-run
# Found 5200 tracks missing titles across 260 directories
# Setlist lookup complete: 255 dirs fetched, 4688 titles updated, 5 errors
```

**Find segue chains** — multi-song jam suites connected by `->` markers, ranked by jam scores:

```
setbreak chains --sort transcendence -n 10
# Dark Star -> St. Stephen -> The Eleven     1969-02-27   3  44.5    82   63   71   79
# Help > Slip > Franklin's                   1977-05-08   3  32.1    78   58   65   72
```

**Discover missing shows** from archive.org, comparing your local library against the full collection:

```
setbreak discover --band gd --year 1977
# Collection: GratefulDead (7700 total shows in archive)
# Local shows: 42 dates | Missing: 38 dates
```

**Rescore** all tracks when scoring formulas evolve, without re-analyzing audio:

```
setbreak rescore
# Rescore complete: 1704 tracks updated
```

## Jam scores

Every analyzed track gets 10 scores (0-100), each computed from multiple audio features:

| Score | Measures | High example |
|-------|----------|-------------|
| **Energy** | RMS + LUFS + spectral brightness | Fire on the Mountain |
| **Intensity** | Spectral flux variance + dynamic range | Not Fade Away |
| **Groove** | Onset rate sweet spot + flux consistency + pattern repetition | China Cat Sunflower |
| **Improvisation** | Non-repetition + chord richness + timbral variety | Dark Star |
| **Tightness** | Pitch stability + flux consistency + beat/onset ratio | Scarlet Begonias |
| **Build Quality** | Crest factor + loudness range + transition density | Dark Star (20+ min) |
| **Exploratory** | Flatness variety + pitch ambiguity + mode ambiguity | Space |
| **Transcendence** | Peak intensity + crest factor + groove-energy synergy | Drums > Space |
| **Valence** | Brightness + tempo + mode + simplicity | Sugar Magnolia |
| **Arousal** | RMS + flux + onset rate + spectral bandwidth | Truckin' |

Scores are designed to differentiate — a 20-minute Dark Star should score very differently from a 3-minute China Cat Sunflower, and they do.

## Building

Requires Rust 1.85+ (2024 edition) and `ffmpeg` (for SHN file decoding).

```bash
# Always build in release mode — debug builds are 10-30x slower for DSP
cargo build --release
```

The DSP engine is a [fork of ferrous-waves](https://github.com/lexicone42/ferrous-waves) (originally by [willibrandon](https://github.com/willibrandon/ferrous-waves)) with optimizations for batch analysis — duplicate STFT elimination and configurable feature skipping. It's pulled automatically as a git dependency.

## Architecture

```
src/
  main.rs              CLI (clap derive)
  lib.rs               Public module exports
  config.rs            XDG paths
  scanner/
    mod.rs             walkdir traversal + lofty tag reading
    filename.rs        Regex-based filename parser (20+ band codes)
    metadata.rs        Tag extraction
  analyzer/
    mod.rs             Parallel analysis (rayon + tokio)
    decode.rs          Audio loading + SHN-via-ffmpeg
    features.rs        Feature extraction from AnalysisResult
    jam_metrics.rs     Score computation (10 scores, ~300 lines)
  db/
    mod.rs             SQLite setup + versioned migrations (v1-v6)
    models.rs          Structs for DB rows
    queries.rs         All SQL (insert, update, query)
  setlist/
    mod.rs             archive.org metadata lookups
  chains.rs            Segue chain detection (multi-song jam suites)
  discovery.rs         archive.org collection discovery (missing shows)
  similarity.rs        Track similarity (cosine distance on feature vectors)
```

**Storage**: SQLite with WAL mode, 8 tables, 100+ feature columns, 4 relational detail tables (chords, segments, tension points, transitions), plus similarity and archive show caches.

**Processing**: Rayon thread pool for parallelism, thread-local tokio runtimes for the async analysis engine, chunked processing for crash recovery.

## Database

The database lives at `~/.local/share/setbreak/setbreak.db` (XDG data dir). Schema uses `PRAGMA user_version` for migrations.

Query examples with `sqlite3`:

```sql
-- Top 10 highest-improvisation tracks
SELECT t.parsed_title, t.parsed_date,
       ROUND(a.improvisation_score, 1) as improv,
       ROUND(a.duration/60.0, 1) as minutes
FROM analysis_results a
JOIN tracks t ON t.id = a.track_id
ORDER BY a.improvisation_score DESC
LIMIT 10;

-- Score comparison across versions of a song
SELECT t.parsed_date,
       ROUND(a.groove_score,1) as groove,
       ROUND(a.improvisation_score,1) as improv,
       ROUND(a.build_quality_score,1) as build
FROM analysis_results a
JOIN tracks t ON t.id = a.track_id
WHERE t.parsed_title = 'Dark Star'
ORDER BY a.duration DESC;

-- Feature distribution
SELECT COUNT(*) as tracks,
       ROUND(MIN(groove_score),1) as min_groove,
       ROUND(AVG(groove_score),1) as avg_groove,
       ROUND(MAX(groove_score),1) as max_groove
FROM analysis_results;
```

## License

MIT OR Apache-2.0
