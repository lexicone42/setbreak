# Rust Patterns in setbreak

Interesting Rust techniques used in this project, with references to where they appear.

## Thread-local tokio runtimes for rayon + async interop

**Problem**: ferrous-waves' `AnalysisEngine::analyze()` is async, but we want CPU-bound parallelism via rayon (not tokio's I/O-oriented work-stealing). Creating a tokio runtime per track is expensive.

**Solution**: `thread_local!` gives each rayon worker its own persistent runtime:

```rust
// src/analyzer/mod.rs
thread_local! {
    static THREAD_RT: tokio::runtime::Runtime =
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");
}

// Usage in rayon worker:
let result: AnalysisResult = THREAD_RT
    .with(|rt| rt.block_on(engine.analyze(&audio)))
    .map_err(|e| AnalyzeError::Engine(e.to_string()))?;
```

**Why it works**: Rayon reuses OS threads across tasks. `thread_local!` ensures each thread creates exactly one runtime, which persists for the thread's lifetime. The `new_current_thread()` runtime is lightweight — no background threads, just a single-threaded executor that blocks on the async call.

**Alternative considered**: Wrapping everything in a multi-threaded tokio runtime with `spawn_blocking` for CPU work. This inverts the control — tokio owns the threads and rayon becomes secondary. We chose rayon-primary because the work is 99% CPU-bound FFT, not I/O.

## Unified decoder dispatch via extension matching

**Problem**: setbreak supports 11+ audio formats across 5 different decoders (symphonia, claxon, shorten-rs, ape-rs, wavpack-rs, plus ffmpeg fallback). Each decoder has its own API, error types, and sample format (i32/i16/f32). We need a single entry point that returns `AudioFile` regardless of format.

**Solution**: Match on file extension, normalize all decoders to a common `i32 → f32` pipeline:

```rust
// src/analyzer/decode.rs
pub fn load_audio(path: &Path) -> Result<AudioFile, DecodeError> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();

    let audio = match ext.as_str() {
        "wav" | "mp3" | "aif" | "aiff" | "ogg" | "m4a" | "aac" | "opus" => {
            AudioFile::load(path).map_err(|e| DecodeError::FerrousWaves(e.to_string()))?
        }
        "flac" => load_flac_native(path)?,
        "shn" => load_shn_native(path)?,
        "ape" => load_ape_native(path)?,
        "wv" => load_wv_native(path)?,
        _ => load_via_ffmpeg(path)?,  // DSD and anything else
    };

    if is_dts_bitstream(&audio) {
        return Err(DecodeError::DtsBitstream);
    }
    Ok(audio)
}
```

**The normalization pattern**: Every native decoder follows the same 3-step shape — open reader, collect `i32` samples, scale to `f32`:

```rust
fn load_FORMAT_native(path: &Path) -> Result<AudioFile, DecodeError> {
    let mut reader = FORMAT::open(path).map_err(/* ... */)?;
    let scale = 2_f32.powi(bits_per_sample as i32 - 1);
    let samples_i32: Vec<i32> = reader.samples().collect::<Result<_, _>>()?;
    let samples_f32: Vec<f32> = samples_i32.iter().map(|&s| s as f32 / scale).collect();
    Ok(AudioFile { buffer: AudioBuffer::new(samples_f32, sample_rate, channels), ... })
}
```

This pattern is deliberately repetitive — each function is ~15 lines of nearly identical code. We could extract a generic helper, but the per-format error types and reader APIs are just different enough that the abstraction would be more complex than the repetition. Four copies of a simple pattern beats one copy of a complex generic.

**Post-decode validation**: The DTS bitstream check runs *after* decoding, not before. DTS streams decode "successfully" as PCM — they just produce garbage audio. The statistical check (>25% of first 4096 samples near ±1.0) catches them without needing to understand the DTS sync word format.

## Statistical heuristic for DTS detection

**Problem**: Some archive.org recordings are DTS-encoded (5.1 surround bitstream) but stored as 16-bit PCM WAV/SHN files. Standard decoders read them fine — the file is valid PCM — but the "audio" is actually a DTS bitstream that sounds like white noise. Analyzing it produces garbage feature values.

**Solution**: A statistical heuristic on the decoded samples:

```rust
// src/analyzer/decode.rs
fn is_dts_bitstream(audio: &AudioFile) -> bool {
    let samples = &audio.buffer.samples;
    if samples.len() < 4096 { return false; }

    let check = &samples[..4096];
    let near_max = check.iter().filter(|&&s| s.abs() > 0.9).count();
    let near_max_ratio = near_max as f64 / check.len() as f64;

    // DTS: >30% near ±1.0. Real audio: almost never >10% in first seconds.
    near_max_ratio > 0.25
}
```

**Why it works**: DTS bitstreams, when interpreted as PCM, produce pseudo-random values across the full amplitude range. Real audio has a dynamic range — quiet passages, transients, not a constant barrage of near-max values. The 25% threshold has zero false positives across 10,000+ tracks.

**Why not parse DTS sync words**: The sync word approach (`0x7FFE8001`) requires knowing the exact byte alignment and endianness, which varies by DTS variant. The statistical approach is format-agnostic and catches any bitstream-as-PCM misinterpretation.

## Tiered heuristic with early return

**Problem**: Classifying recordings as live/studio/live_album from heterogeneous metadata. Some tracks have parsed dates, some have archive.org path signatures, some only have album tags, some have nothing.

**Solution**: A priority cascade where each tier returns immediately on match:

```rust
// src/scanner/classify.rs
pub fn classify_recording_type(file_path: &str, parsed_date: Option<&str>, album: Option<&str>) -> &'static str {
    // Tier 1: Has a parsed date → archive.org bootleg
    if parsed_date.is_some() { return "live"; }

    // Tier 2: Path contains archive.org naming signatures
    const ARCHIVE_SIGS: &[&str] = &[".sbeok.", ".sbd.", ".aud.", ".matrix.", ".flac16", ".shnf"];
    if ARCHIVE_SIGS.iter().any(|sig| path_lower.contains(sig)) { return "live"; }

    // Tier 3: Album tag matches known live album series
    // Tier 4: Album contains "live"
    // Tier 5: Has any album tag → studio
    // Tier 6: No metadata → "unknown"
    "unknown"
}
```

**Why `&'static str` instead of an enum**: The values go straight into SQLite as text. An enum would require `impl Display`, `FromStr` for DB round-tripping, and `#[derive(ValueEnum)]` for CLI. The string literals are the source of truth — using them directly eliminates an entire conversion layer. The tradeoff is no compile-time exhaustiveness checking, but there are only 4 values and they're tested.

**Why `const` arrays inside the function**: `ARCHIVE_SIGS` and `LIVE_ALBUM_SERIES` are used only here. Putting them at module scope would scatter related logic across the file. Function-local `const` keeps the data next to the code that uses it, and the compiler treats it identically to a module-level `const`.

## Channels × frames data layout navigation

**Problem**: ferrous-waves stores multi-dimensional time-series data (spectral contrast, chroma, tonnetz) as `Vec<Vec<f32>>` where the outer Vec is channels/bands and the inner Vec is frames over time. This is the opposite of the "frames × features" layout that's more intuitive for per-frame processing.

**Solution**: Helper functions that navigate the channels-first layout:

```rust
// src/analyzer/features.rs

/// Mean per channel for channels × frames data
fn mean_per_channel(channels: &[Vec<f32>]) -> Option<Vec<f32>> {
    if channels.is_empty() { return None; }
    Some(channels.iter().map(|ch| {
        if ch.is_empty() { 0.0 } else { ch.iter().sum::<f32>() / ch.len() as f32 }
    }).collect())
}

/// Mean frame-to-frame Euclidean distance across all dimensions
fn compute_channel_flux(channels: &[Vec<f32>]) -> Option<f64> {
    let nframes = channels[0].len();
    let mut total_dist = 0.0_f64;
    for t in 1..nframes {
        let dist_sq: f64 = channels.iter()
            .filter(|ch| t < ch.len())
            .map(|ch| ((ch[t] - ch[t - 1]) as f64).powi(2))
            .sum();
        total_dist += dist_sq.sqrt();
    }
    Some(total_dist / (nframes - 1) as f64)
}
```

**Why not transpose**: Transposing `Vec<Vec<f32>>` from channels×frames to frames×channels would allocate a new 2D array. For a 10-minute track at 43 fps (512-hop STFT at 22050 Hz), that's ~26,000 frames × 7 channels = 182,000 f32 values. The helpers iterate the existing layout without allocation.

**The `compute_channel_flux` trick**: "Flux" usually means frame-to-frame change in a single dimension. For multi-dimensional data, we compute Euclidean distance between consecutive time steps across *all* channels simultaneously. This gives tonnetz flux (movement through 6D harmonic space) and chroma flux (tonal change across 12 pitch classes) as single scalar values.

## OLS regression without dependencies

**Problem**: LUFS calibration needs per-score regression against show median LUFS. Could pull in a stats crate, but we only need one function: OLS slope.

**Solution**: 10 lines of arithmetic:

```rust
// src/calibrate.rs
fn ols_slope(x: &[f64], y: &[f64]) -> f64 {
    let n = x.len() as f64;
    let x_mean = x.iter().sum::<f64>() / n;
    let y_mean = y.iter().sum::<f64>() / n;

    let mut cov = 0.0;
    let mut var = 0.0;
    for i in 0..x.len() {
        let dx = x[i] - x_mean;
        cov += dx * (y[i] - y_mean);
        var += dx * dx;
    }

    if var < 1e-12 { 0.0 } else { cov / var }
}
```

**Why not `nalgebra` or `statrs`**: OLS slope is `Σ(x-x̄)(y-ȳ) / Σ(x-x̄)²`. That's it. Adding a linear algebra crate for one formula would increase compile time (nalgebra is huge) for zero benefit. The `1e-12` guard prevents division by zero when all x-values are identical (single-show corpus).

**The median-of-medians approach**: The calibration uses median of per-show median LUFS as the corpus reference, not mean of all tracks. This prevents large shows (30+ tracks) from dominating the baseline. Each show gets equal weight regardless of how many tracks it contains.

## Struct update syntax for partial updates

**Problem**: `NewAnalysis` has 185+ fields. The calibration code only updates 10 jam scores — it doesn't touch any feature columns. Constructing a full `NewAnalysis` with 175 dummy fields would be noisy and fragile.

**Solution**: `Default` derive + struct update syntax:

```rust
// src/calibrate.rs
let update = NewAnalysis {
    track_id: row.track_id,
    energy_score: adjusted_scores[0],
    intensity_score: adjusted_scores[1],
    // ... 8 more scores ...
    ..NewAnalysis::default()  // All other 175+ fields = None/0/""
};

db.update_jam_scores(&update)?;
```

**Why `..Default::default()`**: The `update_jam_scores` query only reads the score fields. The rest are ignored. `Default` sets all `Option<f64>` to `None`, all `Option<String>` to `None`, all `i64` to `0`. This is safe because the partial-update query doesn't touch those columns.

**The alternative**: A separate `ScoreUpdate` struct with just 10 fields. This would be cleaner in isolation, but every new struct needs its own query function, its own parameter binding, and its own test helper. Reusing `NewAnalysis` with `Default` is a pragmatic shortcut — one struct, one set of field names, one mental model.

## LazyLock for compiled regexes

**Problem**: The filename parser uses 9 regex patterns. Compiling them on every call would be wasteful, but Rust doesn't allow complex static initialization.

**Solution**: `std::sync::LazyLock` (stabilized in Rust 1.80, no external crate needed):

```rust
// src/scanner/filename.rs
static BAND_DATE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?ix)
        ^(?P<band>[a-z]+)
        (?P<year>\d{2,4})-(?P<month>\d{1,2})-(?P<day>\d{1,2})"
    )
    .unwrap()
});
```

**Before Rust 1.80**: You'd need `once_cell::sync::Lazy` or the `lazy_static!` macro. `LazyLock` is the std equivalent — zero dependencies, same semantics.

## OnceLock for runtime-initialized globals

**Problem**: The `BandRegistry` is a global singleton (every module needs band lookups), but it can't be a compile-time constant because it needs data from the config file (custom bands). `LazyLock` won't work because the initialization closure can't capture runtime data.

**Solution**: `std::sync::OnceLock` — set once at startup, read everywhere:

```rust
// src/bands.rs
static REGISTRY: OnceLock<BandRegistry> = OnceLock::new();

pub fn init(custom_bands: &[CustomBandConfig]) {
    let registry = BandRegistry::new(custom_bands);
    REGISTRY.set(registry).expect("BandRegistry already initialized");
}

pub fn registry() -> &'static BandRegistry {
    REGISTRY.get().expect("BandRegistry not initialized")
}
```

**For tests**: `OnceLock::set()` panics on double-init, but tests run in parallel in one process. The `init_default()` function uses `let _ = REGISTRY.set(...)` to silently succeed if already set, combined with `std::sync::Once` in test setup for exactly-once initialization.

**LazyLock vs OnceLock**: Use `LazyLock` when the value can be computed at first access with no external input (regexes, constants). Use `OnceLock` when initialization depends on runtime data (config files, CLI args) and must happen at a controlled point.

## Versioned SQLite migrations via PRAGMA user_version

**Problem**: The schema evolved across 14 versions as new features were added. Need to migrate existing databases without data loss, and handle "column already exists" gracefully.

**Solution**: SQLite's `user_version` pragma as a migration tracker:

```rust
// src/db/mod.rs (currently at v14)
fn migrate(&self) -> Result<()> {
    let version: i32 = self.conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap_or(0);

    if version < 1 { self.migrate_v1()?; }
    if version < 2 { self.migrate_v2()?; }
    // ... up to v14
    if version < 14 { self.migrate_v14()?; }

    self.conn.pragma_update(None, "user_version", 14)?;
    Ok(())
}
```

For `ALTER TABLE ADD COLUMN`, SQLite doesn't have `IF NOT EXISTS`, so we match on the specific error:

```rust
// src/db/mod.rs
fn try_add_column(&self, table: &str, col: &str, col_type: &str) -> Result<()> {
    let sql = format!("ALTER TABLE {} ADD COLUMN {} {}", table, col, col_type);
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
    Ok(())
}
```

**Why not a migration library**: The schema is simple enough that raw SQL + version numbers is clearer than learning a migration framework's abstractions. The version check is O(1) on startup. At v14, the `if version < N` chain is still readable — it would need to be reconsidered around v50+.

## Chunked parallel processing for crash recovery

**Problem**: Analyzing 10,573 tracks takes hours. If the process crashes at track 8,000, you don't want to lose all progress.

**Solution**: Process in chunks — analyze a chunk in parallel, write results to DB, repeat:

```rust
// src/analyzer/mod.rs
let chunk_size = jobs * 2;

for chunk in tracks.chunks(chunk_size) {
    let results: Vec<_> = pool.install(|| {
        chunk.par_iter()
            .map(|track| analyze_single_track(track))
            .collect()
    });

    for result in results {
        match result {
            Ok(ta) => { db.store_full_analysis(/* ... */)?; analyzed += 1; }
            Err(e) => { failed += 1; }
        }
    }
}
```

**Why `chunks()` instead of `par_iter().for_each()`**: The DB write is single-threaded (SQLite). If we did `for_each`, we'd need a mutex around the DB. Chunking gives natural batch boundaries — analyze N tracks in parallel, then write N results sequentially. If the process dies, only the current chunk (at most `jobs * 2` tracks) is lost.

**Resumability**: On restart, `db.get_unanalyzed_tracks()` returns only tracks without an `analysis_results` row, so completed chunks are automatically skipped. With `--force`, all tracks are re-queued but the chunked write pattern still provides crash recovery.

## AtomicU64 counter for race-free temp files

**Problem**: ffmpeg fallback decoding writes a temp WAV file. With rayon parallelism, multiple workers can decode simultaneously. Using `tempfile::NamedTempFile` would work but adds a dependency.

**Solution**: A static atomic counter ensures unique filenames without any locking:

```rust
// src/analyzer/decode.rs
static COUNTER: AtomicU64 = AtomicU64::new(0);
let id = COUNTER.fetch_add(1, Ordering::Relaxed);
let tmp_wav = tmp_dir.join(format!("setbreak_decode_{}_{}.wav", std::process::id(), id));
```

**Why `Ordering::Relaxed`**: We only need uniqueness, not happens-before ordering. Each call gets a different number — that's all that matters. The process ID prefix handles the (unlikely) case of multiple setbreak instances.

## Pairwise distance on normalized feature vectors

**Problem**: Computing "section diversity" — how different are a track's structural segments from each other? Segments have features at very different scales (energy ~0.01-0.5, spectral centroid ~500-5000 Hz, ZCR ~0-0.3).

**Solution**: Per-track min-max normalization, then mean pairwise Euclidean distance:

```rust
// src/analyzer/features.rs
fn compute_section_diversity(segments: &[AudioSegment]) -> Option<f64> {
    // Build 4D feature vectors per segment
    let features: Vec<[f64; 4]> = segments.iter()
        .map(|s| [s.energy as f64, s.spectral_centroid as f64, s.zcr as f64, s.dynamic_range as f64])
        .collect();

    // Normalize each dimension to [0, 1] within this track
    let mut mins = [f64::INFINITY; 4];
    let mut maxs = [f64::NEG_INFINITY; 4];
    for f in &features { for d in 0..4 { mins[d] = mins[d].min(f[d]); maxs[d] = maxs[d].max(f[d]); } }

    // ... normalize, then:
    let mut total_dist = 0.0;
    let mut count = 0u64;
    for i in 0..n {
        for j in (i+1)..n {
            total_dist += euclidean_distance(&normed[i], &normed[j]);
            count += 1;
        }
    }
    Some(total_dist / count as f64)
}
```

**Why per-track normalization**: Cross-track normalization would require a pre-pass over all segments in the corpus. Per-track normalization answers a different question: "how varied are *this track's* sections relative to each other?" A track with energy range [0.1, 0.2] that uses the full range is just as diverse as one with [0.01, 0.5] — it's the contrast between sections that matters, not absolute values.

**Why O(n²) pairwise**: Segment count per track is typically 4-15. Even at 20 segments, that's only 190 pairs — negligible compute. A more complex approach (e.g., mean distance from centroid) would run in O(n) but captures less information about the distribution of segments in feature space.

## thiserror v2 + String error sources

**Problem**: `thiserror` v2 requires `#[source]` fields to implement `std::error::Error`. But many error messages are just strings (e.g., from external libraries that return `String` errors).

**Solution**: Use `#[error("...", 0)]` format strings instead of `#[source]`:

```rust
// src/analyzer/mod.rs
#[derive(Error, Debug)]
pub enum AnalyzeError {
    #[error("Decode error: {0}")]
    Decode(#[from] decode::DecodeError),
    #[error("Analysis engine error: {0}")]
    Engine(String),              // No #[source] — String doesn't impl Error
    #[error("Database error: {0}")]
    Db(#[from] crate::db::DbError),
}
```

`Engine(String)` stores the error message directly. The `{0}` in `#[error]` formats it for display. No `#[source]` annotation, so `thiserror` doesn't try to call `.source()` on a `String`.

## Profile-guided release builds

**Problem**: Debug builds are 10-30x slower for DSP work (FFT, STFT, pitch detection). Analysis that takes 2 minutes in release takes 30+ minutes in debug.

**Solution**: Aggressive release profile:

```toml
# Cargo.toml
[profile.release]
opt-level = 3
lto = true
```

**The tradeoff**: Release builds take ~90 seconds to compile (LTO is the bottleneck). But for a workload that runs for hours, the compile time is negligible. `opt-level = 3` enables SIMD auto-vectorization which is critical for FFT performance. `lto = true` enables cross-crate inlining, which matters because our hot path crosses from setbreak into ferrous-waves.

**Never benchmark in debug**: This is a general Rust rule, but it's especially important for numerical code. The difference isn't 2x — it's 10-30x because debug builds skip SIMD, add bounds checks on every array access, and disable inlining across crate boundaries.
