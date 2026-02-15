# Rust Patterns in setbreak

Interesting Rust techniques used in this project, with references to where they appear.

## Thread-local tokio runtimes for rayon + async interop

**Problem**: ferrous-waves' `AnalysisEngine::analyze()` is async, but we want CPU-bound parallelism via rayon (not tokio's I/O-oriented work-stealing). Creating a tokio runtime per track is expensive.

**Solution**: `thread_local!` gives each rayon worker its own persistent runtime:

```rust
// src/analyzer/mod.rs:184-189
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

## AtomicU64 counter for race-free temp files

**Problem**: SHN decoding shells out to ffmpeg, writing a temp WAV file. With rayon parallelism, multiple workers can decode SHN files simultaneously. Using `tempfile::NamedTempFile` would work but adds a dependency.

**Solution**: A static atomic counter ensures unique filenames without any locking:

```rust
// src/analyzer/decode.rs:46-49
static COUNTER: AtomicU64 = AtomicU64::new(0);
let id = COUNTER.fetch_add(1, Ordering::Relaxed);
let tmp_wav = tmp_dir.join(format!("setbreak_shn_{}_{}.wav", std::process::id(), id));
```

**Why `Ordering::Relaxed`**: We only need uniqueness, not happens-before ordering. Each call gets a different number — that's all that matters. The process ID prefix handles the (unlikely) case of multiple setbreak instances.

## LazyLock for compiled regexes

**Problem**: The filename parser uses 9 regex patterns. Compiling them on every call would be wasteful, but Rust doesn't allow complex static initialization.

**Solution**: `std::sync::LazyLock` (stabilized in Rust 1.80, no external crate needed):

```rust
// src/scanner/filename.rs:77-84
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

## Versioned SQLite migrations via PRAGMA user_version

**Problem**: The schema evolved across 4 versions as new features were added. Need to migrate existing databases without data loss, and handle "column already exists" gracefully.

**Solution**: SQLite's `user_version` pragma as a migration tracker:

```rust
// src/db/mod.rs:49-69
fn migrate(&self) -> Result<()> {
    let version: i32 = self.conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap_or(0);

    if version < 1 { self.migrate_v1()?; }
    if version < 2 { self.migrate_v2()?; }
    if version < 3 { self.migrate_v3()?; }
    if version < 4 { self.migrate_v4()?; }

    self.conn.pragma_update(None, "user_version", 4)?;
    Ok(())
}
```

For `ALTER TABLE ADD COLUMN`, SQLite doesn't have `IF NOT EXISTS`, so we match on the specific error:

```rust
// src/db/mod.rs:232-243
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
```

**Why not a migration library**: The schema is simple enough that raw SQL + version numbers is clearer than learning a migration framework's abstractions. The version check is O(1) on startup.

## Chunked parallel processing for crash recovery

**Problem**: Analyzing 5,761 tracks takes ~46 hours. If the process crashes at track 4,000, you don't want to lose all progress.

**Solution**: Process in chunks — analyze a chunk in parallel, write results to DB, repeat:

```rust
// src/analyzer/mod.rs:131-175
let chunk_size = jobs * 2;

for chunk in tracks.chunks(chunk_size) {
    // Analyze in parallel with rayon
    let results: Vec<_> = pool.install(|| {
        chunk.par_iter()
            .map(|track| analyze_single_track(track))
            .collect()
    });

    // Write to DB immediately
    for result in results {
        match result {
            Ok(ta) => {
                db.store_full_analysis(/* ... */)?;
                analyzed += 1;
            }
            Err(e) => { failed += 1; }
        }
    }
}
```

**Why `chunks()` instead of `par_iter().for_each()`**: The DB write is single-threaded (SQLite). If we did `for_each`, we'd need a mutex around the DB. Chunking gives natural batch boundaries — analyze N tracks in parallel, then write N results sequentially. If the process dies, only the current chunk (at most `jobs * 2` tracks) is lost.

**Resumability**: On restart, `db.get_unanalyzed_tracks()` returns only tracks without an `analysis_results` row, so completed chunks are automatically skipped.

## thiserror v2 + String error sources

**Problem**: `thiserror` v2 requires `#[source]` fields to implement `std::error::Error`. But many error messages are just strings (e.g., from external libraries that return `String` errors).

**Solution**: Use `#[error("...", 0)]` format strings instead of `#[source]`:

```rust
// src/analyzer/mod.rs:13-21
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

## Regex cascade with early return

**Problem**: Jam-band filenames come in wildly different formats — `gd77-05-08d1t01.shn`, `Grateful Dead/1977/1977-05-08 Barton Hall/d1t01 - Scarlet Begonias.mp3`, `ph2013-12-31.mk5-s2t09.flac`, etc.

**Solution**: A pattern cascade that tries the most specific format first and falls through to progressively more generic patterns:

```rust
// src/scanner/filename.rs:149-293 (simplified)
pub fn parse_path(path: &Path) -> ParsedPath {
    let mut parsed = ParsedPath::default();

    // Pattern 1: Most specific — band code + date in filename
    if let Some(caps) = BAND_DATE_RE.captures(file_stem) {
        if is_valid_date(month, day) {
            // Extract everything from the filename, return early
            return parsed;
        }
    }

    // Pattern 2: Path-based with date-venue directories
    for comp in &components {
        if let Some(caps) = PATH_DATE_VENUE_RE.captures(comp) { /* ... */ }
    }

    // Pattern 3: Set directory ("Set II", "Set Encore")
    // Pattern 4: Generic fallback (any date in path + track number)
    parsed
}
```

**Why early return**: Pattern 1 (compact format) matches 80%+ of the library. The early return avoids running 8 more regexes on those files. The cascade ensures every file gets some metadata, even if it's just a track number and title from `01 - Dark Star.mp3`.

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
