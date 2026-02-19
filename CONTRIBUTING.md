# Contributing to setbreak

Thanks for your interest! Here's how to get started.

## Setup

1. Install Rust 1.85+ (for 2024 edition support): https://rustup.rs
2. Optional: Install `ffmpeg` (only needed for WavPack and DSD files)
3. Clone and build in release mode:

```bash
git clone https://github.com/lexicone42/setbreak.git
cd setbreak
cargo build --release
```

**Important**: Always use `--release`. Debug builds are 10-30x slower for audio analysis (unoptimized FFT/SIMD).

## Running tests

```bash
cargo test
```

All tests run against in-memory state — no music files or database needed. Currently 99 tests.

## Project structure

```
src/
  main.rs              CLI entry point (clap) — 15 subcommands
  bands.rs             Band registry (single source of truth for all band data)
  config.rs            TOML config loading
  calibrate.rs         LUFS-based score calibration
  scanner/             File discovery + filename parsing + recording classification
  analyzer/            Parallel audio analysis + feature extraction + jam scoring
  db/                  SQLite schema, migrations (v1-v14), queries
  setlist/             archive.org metadata lookups
  chains.rs            Segue chain detection
  discovery.rs         Missing show discovery
  similarity.rs        Track similarity
```

See [ANALYZER.md](ANALYZER.md) for details on all 185 extracted features and the 10 jam scores.

## Adding a new band

Add a `BandEntry` to the `built_in_bands()` function in `src/bands.rs`. Each band needs:
- A canonical name
- One or more short codes (used in filenames like `gd77-05-08`)
- Search names (for matching directory paths)
- Optionally, an archive.org strategy and normalization rules

Or, users can add bands via config file without any code changes:

```toml
# ~/.config/setbreak/config.toml
[[bands]]
name = "Lettuce"
codes = ["let", "lettuce"]
search = ["lettuce"]
archive = { type = "creator", value = "Lettuce" }
```

## Adding new features

The feature pipeline has 5 files that need updating in order:

1. **ferrous-waves** (`engine.rs`): Add computation to the DSP engine
2. **models.rs**: Add field to `NewAnalysis` struct
3. **mod.rs**: Add migration (`try_add_column`) and bump `user_version`
4. **queries.rs**: Add column to all 6 SQL locations (INSERT columns, VALUES params, ON CONFLICT SET, `params!` macro, `get_analyses_for_rescore`, `minimal_analysis`)
5. **features.rs**: Add extraction from `AnalysisResult` to `NewAnalysis`
6. **jam_metrics.rs**: Add `None` to `base_analysis()` test helper

## Style

- Follow existing patterns in the codebase
- `cargo clippy` should be clean
- Prefer simple, direct code over abstractions
- All features are `Option<T>` in the DB — graceful degradation if a feature can't be computed

## Versioning

This project follows [Semantic Versioning](https://semver.org/). The version lives in `Cargo.toml` and is automatically used by the `--version` flag.

- **Patch** (0.1.x): Bug fixes, score formula tweaks
- **Minor** (0.x.0): New commands, new features, schema migrations
- **Major** (x.0.0): Breaking changes to CLI interface or database schema without migration
