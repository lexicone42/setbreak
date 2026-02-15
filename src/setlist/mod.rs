use std::collections::HashMap;
use std::path::Path;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use serde::Deserialize;

use crate::db::Database;

/// Archive.org metadata API response (partial — we only need `files`).
#[derive(Debug, Deserialize)]
struct ArchiveMetadata {
    files: Option<Vec<ArchiveFile>>,
}

/// A single file entry in archive.org metadata.
#[derive(Debug, Deserialize)]
struct ArchiveFile {
    name: Option<String>,
    title: Option<String>,
    /// Some entries use "track" field for ordering
    #[allow(dead_code)]
    track: Option<String>,
}

/// Result of a setlist lookup run.
pub struct SetlistResult {
    pub directories_fetched: usize,
    pub titles_updated: usize,
    pub fetch_errors: usize,
    pub tracks_already_titled: usize,
}

/// Run setlist lookups against archive.org to populate song titles.
///
/// Groups tracks by parent directory, uses directory name as archive.org identifier,
/// fetches metadata, and matches filenames to get song titles.
pub fn lookup_setlists(db: &Database, dry_run: bool) -> Result<SetlistResult> {
    // Get all tracks missing titles (no parsed_title AND no tag title)
    let tracks = db.get_tracks_missing_titles()
        .context("Failed to query tracks missing titles")?;

    if tracks.is_empty() {
        log::info!("All tracks already have titles");
        return Ok(SetlistResult {
            directories_fetched: 0,
            titles_updated: 0,
            fetch_errors: 0,
            tracks_already_titled: 0,
        });
    }

    // Group tracks by parent directory name (= archive.org identifier)
    let mut by_dir: HashMap<String, Vec<(i64, String)>> = HashMap::new();
    let mut no_dir_count = 0;

    for (track_id, file_path) in &tracks {
        let path = Path::new(file_path);
        if let Some(parent) = path.parent() {
            if let Some(dir_name) = parent.file_name() {
                let dir = dir_name.to_string_lossy().to_string();
                let filename = path.file_name()
                    .map(|f| f.to_string_lossy().to_string())
                    .unwrap_or_default();
                by_dir.entry(dir).or_default().push((*track_id, filename));
            } else {
                no_dir_count += 1;
            }
        } else {
            no_dir_count += 1;
        }
    }

    if no_dir_count > 0 {
        log::warn!("{no_dir_count} tracks have no parent directory, skipping");
    }

    println!(
        "Found {} tracks missing titles across {} directories",
        tracks.len(),
        by_dir.len()
    );

    let pb = ProgressBar::new(by_dir.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} dirs ({eta} remaining) {msg}")
            .unwrap()
            .progress_chars("=>-"),
    );

    let mut result = SetlistResult {
        directories_fetched: 0,
        titles_updated: 0,
        fetch_errors: 0,
        tracks_already_titled: 0,
    };

    // Sort directories for deterministic ordering
    let mut dirs: Vec<_> = by_dir.into_iter().collect();
    dirs.sort_by(|a, b| a.0.cmp(&b.0));

    for (dir_name, dir_tracks) in &dirs {
        pb.set_message(dir_name.clone());

        match fetch_archive_metadata(dir_name) {
            Ok(file_map) => {
                result.directories_fetched += 1;

                for (track_id, filename) in dir_tracks {
                    // Try exact filename match first
                    if let Some(title) = file_map.get(filename.as_str()) {
                        if !dry_run {
                            db.update_parsed_title(*track_id, title)
                                .with_context(|| format!("Failed to update title for track {track_id}"))?;
                        }
                        result.titles_updated += 1;
                        log::info!("  {filename} => {title}");
                    } else {
                        // Try matching without extension (archive.org may list .flac
                        // while local file is .shn or .mp3)
                        let stem = Path::new(filename)
                            .file_stem()
                            .map(|s| s.to_string_lossy().to_string());

                        if let Some(stem) = stem {
                            let found = file_map.iter().find(|(k, _)| {
                                Path::new(k)
                                    .file_stem()
                                    .map(|s| s.to_string_lossy() == stem)
                                    .unwrap_or(false)
                            });

                            if let Some((_, title)) = found {
                                if !dry_run {
                                    db.update_parsed_title(*track_id, title)
                                        .with_context(|| format!("Failed to update title for track {track_id}"))?;
                                }
                                result.titles_updated += 1;
                                log::info!("  {filename} => {title} (stem match)");
                            } else {
                                log::debug!("  {filename}: no match in archive.org metadata");
                            }
                        }
                    }
                }
            }
            Err(e) => {
                result.fetch_errors += 1;
                log::warn!("Failed to fetch metadata for {dir_name}: {e}");
            }
        }

        pb.inc(1);

        // Rate limit: ~500ms between requests to be polite
        thread::sleep(Duration::from_millis(500));
    }

    pb.finish_with_message("done");
    Ok(result)
}

/// Percent-encode characters that break archive.org URLs (spaces, parens, etc.)
fn encode_identifier(id: &str) -> String {
    let mut out = String::with_capacity(id.len());
    for c in id.chars() {
        match c {
            ' ' => out.push_str("%20"),
            '(' => out.push_str("%28"),
            ')' => out.push_str("%29"),
            '[' => out.push_str("%5B"),
            ']' => out.push_str("%5D"),
            _ => out.push(c),
        }
    }
    out
}

/// Fetch archive.org metadata for an identifier and return a filename -> title map.
fn fetch_archive_metadata(identifier: &str) -> Result<HashMap<String, String>> {
    let encoded = encode_identifier(identifier);
    let url = format!("https://archive.org/metadata/{encoded}");
    log::debug!("Fetching {url}");

    let response: ArchiveMetadata = ureq::get(&url)
        .call()
        .with_context(|| format!("HTTP request failed for {identifier}"))?
        .body_mut()
        .read_json()
        .with_context(|| format!("Failed to parse JSON for {identifier}"))?;

    let mut map = HashMap::new();
    if let Some(files) = response.files {
        for f in files {
            if let (Some(name), Some(title)) = (f.name, f.title) {
                // Only include audio files (skip metadata, checksums, etc.)
                let ext = Path::new(&name)
                    .extension()
                    .map(|e| e.to_string_lossy().to_lowercase())
                    .unwrap_or_default();

                if matches!(ext.as_str(), "mp3" | "flac" | "ogg" | "shn" | "wav") {
                    map.insert(name, title);
                }
            }
        }
    }

    log::debug!("  Got {} titled audio files for {identifier}", map.len());
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_archive_file_deserialize() {
        let json = r#"{"name": "gd83-04-09d1t01.mp3", "title": "Scarlet Begonias", "track": "01"}"#;
        let f: ArchiveFile = serde_json::from_str(json).unwrap();
        assert_eq!(f.name.as_deref(), Some("gd83-04-09d1t01.mp3"));
        assert_eq!(f.title.as_deref(), Some("Scarlet Begonias"));
    }

    #[test]
    fn test_archive_metadata_empty_files() {
        let json = r#"{"files": []}"#;
        let m: ArchiveMetadata = serde_json::from_str(json).unwrap();
        assert_eq!(m.files.unwrap().len(), 0);
    }

    #[test]
    fn test_archive_metadata_no_files() {
        let json = r#"{}"#;
        let m: ArchiveMetadata = serde_json::from_str(json).unwrap();
        assert!(m.files.is_none());
    }
}
