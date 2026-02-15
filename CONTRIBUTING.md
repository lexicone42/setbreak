# Contributing to setbreak

Thanks for your interest! Here's how to get started.

## Setup

1. Install Rust 1.85+ (for 2024 edition support): https://rustup.rs
2. Install `ffmpeg` (needed for SHN file decoding)
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

All tests run against in-memory state — no music files or database needed.

## Project structure

```
src/
  main.rs              CLI entry point (clap)
  bands.rs             Band registry (single source of truth for all band data)
  config.rs            TOML config loading
  scanner/             File discovery + filename parsing
  analyzer/            Parallel audio analysis + jam scoring
  db/                  SQLite schema, migrations, queries
  setlist/             archive.org metadata lookups
  chains.rs            Segue chain detection
  discovery.rs         Missing show discovery
  similarity.rs        Track similarity
```

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

## Style

- Follow existing patterns in the codebase
- `cargo clippy` should be clean
- Prefer simple, direct code over abstractions

## Versioning

This project follows [Semantic Versioning](https://semver.org/). The version lives in `Cargo.toml` and is automatically used by the `--version` flag.

- **Patch** (0.1.x): Bug fixes, score formula tweaks
- **Minor** (0.x.0): New commands, new features, schema migrations
- **Major** (x.0.0): Breaking changes to CLI interface or database schema without migration
