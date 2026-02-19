# setbreak

[![Built with Claude Code](https://img.shields.io/badge/Built%20with-Claude%20Code-blueviolet?logo=anthropic)](https://claude.ai/claude-code)
[![Rust](https://img.shields.io/badge/Rust-2024%20edition-orange?logo=rust)](https://www.rust-lang.org/)

A CLI tool for analyzing jam-band music libraries. Scans audio files, extracts 180+ spectral/temporal/harmonic features via DSP, computes jam-quality scores, and stores everything in SQLite for exploration.

Built for Grateful Dead tape collections, but works with any jam-band library (Phish, Widespread Panic, Goose, Billy Strings, etc.).

## What it does

**Scan** your music library to catalog tracks, parsing band names, show dates, disc/track numbers, and venues from filenames and tags:

```
setbreak scan ~/music/grateful_dead/ ~/music/phish/
# Scan complete: 10573 scanned, 10573 new, 0 updated, 0 skipped, 0 errors

# Or configure music_dirs in ~/.config/setbreak/config.toml and just:
setbreak scan
```

**Analyze** audio files to extract 180+ features using DSP (FFT, STFT, pitch detection, beat tracking, onset detection, chord estimation, harmonic-percussive separation):

```
setbreak analyze          # auto-detects worker count from config (cores/2)
setbreak analyze -j4      # or specify explicitly
# Analysis complete: 10573 analyzed, 3 failed
```

**Look up song titles** from archive.org metadata, matching directory names to archive identifiers:

```
setbreak setlist --dry-run
# Found 5200 tracks missing titles across 260 directories
# Setlist lookup complete: 255 dirs fetched, 4688 titles updated, 5 errors
```

**Explore your top tracks** by any jam score:

```
setbreak top --sort transcendence -n 10
setbreak top --sort groove --song "Dark Star" -n 5
```

**Find segue chains** — multi-song jam suites connected by `->` markers, ranked by jam scores:

```
setbreak chains --sort transcendence -n 10
# Dark Star -> St. Stephen -> The Eleven     1969-02-27   3  44.5    82   63   71   79
# Help > Slip > Franklin's                   1977-05-08   3  32.1    78   58   65   72
```

**Compare versions** of a song across shows:

```
setbreak compare "Dark Star"
# Shows every Dark Star in your library with side-by-side scores
```

**Find similar tracks** based on feature-vector cosine distance:

```
setbreak similar "Dark Star" --date 1972-04-14 -n 10
```

**Discover missing shows** from archive.org, comparing your local library against the full collection:

```
setbreak discover --band gd --year 1977
# Collection: GratefulDead (7700 total shows in archive)
# Local shows: 42 dates | Missing: 38 dates
```

**Classify recordings** as live, studio, or live album:

```
setbreak classify
# Classified: 10233 live, 240 studio, 4 live_album, 96 unknown
```

**Calibrate scores** to remove recording quality bias (louder tapes scoring higher):

```
setbreak calibrate --dry-run   # see regression coefficients
setbreak calibrate             # apply LUFS-based correction
```

**Rescore** all tracks when scoring formulas evolve, without re-analyzing audio:

```
setbreak rescore
# Rescore complete: 10573 tracks updated
```

## Jam scores

Every analyzed track gets 10 scores (0-100), each computed from multiple audio features:

| Score | Measures | High example |
|-------|----------|-------------|
| **Energy** | RMS + LUFS + sub-band bass + spectral brightness | Fire on the Mountain |
| **Intensity** | Spectral flux variance + dynamic range + loudness range | Not Fade Away |
| **Groove** | Onset rate sweet spot + flux consistency + bass steadiness + pattern repetition | China Cat Sunflower |
| **Improvisation** | Non-repetition + timbral variety + structural density + tonal ambiguity | Dark Star |
| **Tightness** | Tempo stability + coherence + spectral smoothness + beat strength | Scarlet Begonias |
| **Build Quality** | Energy arc detection + tension build/release + energy variance + transitions | Dark Star (20+ min) |
| **Exploratory** | Spectral flatness variety + pitch ambiguity + mode ambiguity + harmonic complexity | Space |
| **Transcendence** | Peak energy + sustained intensity + peak tension + groove-energy synergy | Drums > Space |
| **Valence** | Brightness + tempo + mode + simplicity | Sugar Magnolia |
| **Arousal** | RMS + flux + onset rate + spectral bandwidth | Truckin' |

Scores are designed to differentiate — a 20-minute Dark Star should score very differently from a 3-minute China Cat Sunflower, and they do. After analysis, `calibrate` removes recording-quality bias via per-show LUFS regression so a pristine 1977 SBD doesn't automatically outscore a muddy 1969 AUD.

For a detailed breakdown of all 185 extracted features, see [ANALYZER.md](ANALYZER.md).

## Audio format support

All decoding is native Rust — no external dependencies required:

| Format | Decoder | Notes |
|--------|---------|-------|
| MP3 | symphonia | MPEG Layer III |
| FLAC | claxon | Lossless |
| SHN (Shorten) | [shorten-rs](https://github.com/lexicone42/shorten-rs) | Common in tape trading |
| APE (Monkey's Audio) | [ape-rs](https://github.com/lexicone42/ape-rs) | All compression levels |
| WAV | symphonia | PCM |
| AIFF | symphonia | |
| OGG Vorbis | symphonia | |
| M4A/AAC | symphonia | |
| Opus | symphonia | |
| WavPack | ffmpeg fallback | Pure Rust decoder in progress |
| DSF/DFF (DSD) | ffmpeg fallback | Niche format |

DTS-encoded tracks (bitstream masquerading as PCM) are automatically detected and flagged as garbage quality to prevent corrupted analysis results.

## Building

Requires Rust 1.85+ (2024 edition). Optional: `ffmpeg` for WavPack and DSD files only.

```bash
# Always build in release mode — debug builds are 10-30x slower for DSP
cargo build --release
```

## Configuration

Optional TOML config at `~/.config/setbreak/config.toml`. Everything works without it — the file is purely for overrides.

```toml
music_dirs = ["/home/you/music/grateful_dead", "/home/you/music/phish"]
# db_path = "/custom/path/setbreak.db"
workers = 0  # 0 = auto (cores / 2)

[archive]
cache_ttl_days = 30
rate_limit_ms = 500

# Custom bands (merged with 23 built-in bands)
# [[bands]]
# name = "Lettuce"
# codes = ["let", "lettuce"]
# search = ["lettuce"]
# archive = { type = "creator", value = "Lettuce" }
```

**Override priority**: CLI argument > config file > built-in default.

The DSP engine is a [fork of ferrous-waves](https://github.com/lexicone42/ferrous-waves) (originally by [willibrandon](https://github.com/willibrandon/ferrous-waves)) with optimizations for batch analysis — duplicate STFT elimination, configurable feature skipping, and extended spectral/temporal feature extraction. It's pulled automatically as a git dependency.

## Architecture

```
src/
  main.rs              CLI (clap derive) — 15 subcommands
  lib.rs               Public module exports
  bands.rs             Unified band registry (23 bands, 37 codes, OnceLock global)
  config.rs            TOML config loading + XDG paths
  calibrate.rs         LUFS-based score calibration (OLS regression)
  scanner/
    mod.rs             walkdir traversal + lofty tag reading
    filename.rs        Regex-based filename parser (uses BandRegistry)
    metadata.rs        Tag extraction
    classify.rs        Recording type classification (live/studio/live_album)
  analyzer/
    mod.rs             Parallel analysis (rayon + tokio)
    decode.rs          Native audio decoding (symphonia, claxon, shorten-rs, ape-rs)
    features.rs        Feature extraction from AnalysisResult → 185 DB columns
    jam_metrics.rs     Score computation (10 scores)
  db/
    mod.rs             SQLite setup + versioned migrations (v1-v14)
    models.rs          Structs for DB rows
    queries.rs         All SQL (insert, update, query)
  setlist/
    mod.rs             archive.org metadata lookups
  chains.rs            Segue chain detection (multi-song jam suites)
  discovery.rs         archive.org collection discovery (missing shows)
  similarity.rs        Track similarity (cosine distance on feature vectors)
```

**Storage**: SQLite with WAL mode. 8 tables, 185 feature columns on `analysis_results`, plus relational detail tables (chords, segments, tension points, transitions), similarity cache, and archive show cache.

**Processing**: Rayon thread pool for parallelism, thread-local tokio runtimes for the async analysis engine, chunked processing for crash recovery.

## Database

The database lives at `~/.local/share/setbreak/setbreak.db` (XDG data dir). Schema uses `PRAGMA user_version` for migrations (currently v14 — migrations run automatically on startup).

Query examples with `sqlite3`:

```sql
-- Top 10 highest-improvisation tracks
SELECT t.parsed_title, t.parsed_date,
       ROUND(a.improvisation_score, 1) as improv,
       ROUND(a.duration/60.0, 1) as minutes
FROM analysis_results a
JOIN tracks t ON t.id = a.track_id
WHERE t.data_quality != 'garbage'
ORDER BY a.improvisation_score DESC
LIMIT 10;

-- Score comparison across versions of a song
SELECT t.parsed_date,
       ROUND(a.groove_score,1) as groove,
       ROUND(a.improvisation_score,1) as improv,
       ROUND(a.transcendence_score,1) as transcend,
       ROUND(a.duration/60.0,1) as min
FROM analysis_results a
JOIN tracks t ON t.id = a.track_id
WHERE t.parsed_title = 'Dark Star'
  AND t.data_quality != 'garbage'
ORDER BY a.duration DESC;

-- Which shows have the highest average transcendence?
SELECT t.parsed_date,
       COUNT(*) as tracks,
       ROUND(AVG(a.transcendence_score),1) as avg_transcend,
       ROUND(SUM(a.duration)/60.0,0) as total_min
FROM analysis_results a
JOIN tracks t ON t.id = a.track_id
WHERE t.data_quality != 'garbage'
GROUP BY t.parsed_date
HAVING tracks >= 8
ORDER BY avg_transcend DESC
LIMIT 10;
```

## License

Apache-2.0
