pub mod filename;
pub mod metadata;

use crate::db::models::NewTrack;
use crate::db::Database;
use crate::SUPPORTED_EXTENSIONS;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::Path;
use thiserror::Error;
use walkdir::WalkDir;

#[derive(Error, Debug)]
pub enum ScanError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Database error: {0}")]
    Db(#[from] crate::db::DbError),
    #[error("Metadata error for {path}: {message}")]
    Metadata { path: String, message: String },
}

pub struct ScanResult {
    pub scanned: u64,
    pub new: u64,
    pub updated: u64,
    pub skipped: u64,
    pub errors: u64,
}

/// Scan directories for audio files and insert/update tracks in the database.
pub fn scan(db: &Database, paths: &[String], force: bool) -> std::result::Result<ScanResult, ScanError> {
    // First pass: collect all audio file paths
    let mut audio_files: Vec<walkdir::DirEntry> = Vec::new();

    for path in paths {
        for entry in WalkDir::new(path).follow_links(true).into_iter().filter_map(|e| e.ok()) {
            if !entry.file_type().is_file() {
                continue;
            }
            let ext = entry
                .path()
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            if SUPPORTED_EXTENSIONS.contains(&ext.as_str()) {
                audio_files.push(entry);
            }
        }
    }

    let total = audio_files.len() as u64;
    let pb = ProgressBar::new(total);
    pb.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} ({per_sec}) ({eta}) {msg}"
        )
        .unwrap()
        .progress_chars("#>-"),
    );
    pb.set_message("Scanning...");

    let mut result = ScanResult {
        scanned: 0,
        new: 0,
        updated: 0,
        skipped: 0,
        errors: 0,
    };

    // Wrap all inserts in a single transaction for dramatic speedup
    let tx = db.conn.unchecked_transaction().map_err(crate::db::DbError::from)?;

    for entry in &audio_files {
        let path = entry.path();
        result.scanned += 1;

        match process_file(&tx, path, force) {
            Ok(FileAction::New) => result.new += 1,
            Ok(FileAction::Updated) => result.updated += 1,
            Ok(FileAction::Skipped) => result.skipped += 1,
            Err(e) => {
                log::warn!("Error scanning {}: {}", path.display(), e);
                result.errors += 1;
            }
        }

        pb.inc(1);
    }

    tx.commit().map_err(crate::db::DbError::from)?;

    pb.finish_with_message(format!(
        "Done: {} new, {} updated, {} skipped, {} errors",
        result.new, result.updated, result.skipped, result.errors
    ));

    Ok(result)
}

enum FileAction {
    New,
    Updated,
    Skipped,
}

fn process_file(
    conn: &rusqlite::Connection,
    path: &Path,
    force: bool,
) -> std::result::Result<FileAction, ScanError> {
    let meta = std::fs::metadata(path)?;
    let file_size = meta.len() as i64;
    let file_modified = format_mtime(&meta);
    let file_path = path.to_string_lossy().to_string();

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    // Single query: check if track exists AND if it's unchanged
    let existing: Option<(i64, String)> = conn
        .query_row(
            "SELECT file_size, file_modified FROM tracks WHERE file_path = ?1",
            rusqlite::params![file_path],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .ok();

    let is_new = existing.is_none();

    // Skip if unchanged and not forced
    if !force {
        if let Some((size, mtime)) = &existing {
            if *size == file_size && *mtime == file_modified {
                return Ok(FileAction::Skipped);
            }
        }
    }

    // Read tags
    let tags = metadata::read_tags(path);

    // Parse filename/path for jam band metadata
    let parsed = filename::parse_path(path);

    let new_track = NewTrack {
        file_path,
        file_size,
        file_modified,
        format: ext,
        title: tags.title,
        artist: tags.artist,
        album: tags.album,
        date: tags.date,
        track_number: tags.track_number,
        disc_number: tags.disc_number,
        set_name: None,
        venue: tags.venue,
        comment: tags.comment,
        parsed_band: parsed.band,
        parsed_date: parsed.date,
        parsed_venue: parsed.venue,
        parsed_disc: parsed.disc,
        parsed_track: parsed.track,
        parsed_set: parsed.set,
        parsed_title: parsed.title,
        duration_secs: tags.duration_secs,
    };

    // Use the transaction connection directly
    conn.execute(
        "INSERT INTO tracks (
            file_path, file_size, file_modified, format,
            title, artist, album, date, track_number, disc_number,
            set_name, venue, comment,
            parsed_band, parsed_date, parsed_venue, parsed_disc,
            parsed_track, parsed_set, parsed_title, duration_secs,
            updated_at
        ) VALUES (
            ?1, ?2, ?3, ?4,
            ?5, ?6, ?7, ?8, ?9, ?10,
            ?11, ?12, ?13,
            ?14, ?15, ?16, ?17,
            ?18, ?19, ?20, ?21,
            datetime('now')
        )
        ON CONFLICT(file_path) DO UPDATE SET
            file_size = excluded.file_size,
            file_modified = excluded.file_modified,
            format = excluded.format,
            title = excluded.title,
            artist = excluded.artist,
            album = excluded.album,
            date = excluded.date,
            track_number = excluded.track_number,
            disc_number = excluded.disc_number,
            set_name = excluded.set_name,
            venue = excluded.venue,
            comment = excluded.comment,
            parsed_band = excluded.parsed_band,
            parsed_date = excluded.parsed_date,
            parsed_venue = excluded.parsed_venue,
            parsed_disc = excluded.parsed_disc,
            parsed_track = excluded.parsed_track,
            parsed_set = excluded.parsed_set,
            parsed_title = excluded.parsed_title,
            duration_secs = excluded.duration_secs,
            updated_at = datetime('now')
        ",
        rusqlite::params![
            new_track.file_path, new_track.file_size, new_track.file_modified, new_track.format,
            new_track.title, new_track.artist, new_track.album, new_track.date,
            new_track.track_number, new_track.disc_number,
            new_track.set_name, new_track.venue, new_track.comment,
            new_track.parsed_band, new_track.parsed_date, new_track.parsed_venue, new_track.parsed_disc,
            new_track.parsed_track, new_track.parsed_set, new_track.parsed_title, new_track.duration_secs,
        ],
    ).map_err(|e| crate::db::DbError::from(e))?;

    if is_new { Ok(FileAction::New) } else { Ok(FileAction::Updated) }
}

fn format_mtime(meta: &std::fs::Metadata) -> String {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs().to_string())
        .unwrap_or_default()
}
