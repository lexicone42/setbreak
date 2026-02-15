use std::collections::HashMap;
use std::path::Path;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use regex::Regex;
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
    /// Some entries use "track" field for ordering.
    /// Deserialized from JSON but not yet used in matching logic.
    #[allow(dead_code)]
    track: Option<String>,
}

/// Archive.org search API response.
#[derive(Debug, Deserialize)]
struct ArchiveSearchResponse {
    response: Option<ArchiveSearchInner>,
}

#[derive(Debug, Deserialize)]
struct ArchiveSearchInner {
    docs: Option<Vec<ArchiveSearchDoc>>,
}

#[derive(Debug, Deserialize)]
struct ArchiveSearchDoc {
    identifier: Option<String>,
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
///
/// Handles common naming mismatches:
/// - 2-digit years in GD dirs (gd69- → gd1969-)
/// - Case differences in BTS dirs (bts → BTS)
/// - Prefix differences in Phish dirs (ph → phish)
/// - Filename differences via disc/track position matching
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

        match fetch_metadata_with_fallbacks(dir_name) {
            Ok(file_map) if !file_map.is_empty() => {
                result.directories_fetched += 1;

                // Build a position-based lookup as a fallback
                let position_map = build_position_map(&file_map);

                for (track_id, filename) in dir_tracks {
                    let title = match_title(filename, &file_map, &position_map);
                    if let Some((title, method)) = title {
                        if !dry_run {
                            db.update_parsed_title(*track_id, &title)
                                .with_context(|| format!("Failed to update title for track {track_id}"))?;
                        }
                        result.titles_updated += 1;
                        log::info!("  {filename} => {title} ({method})");
                    } else {
                        log::debug!("  {filename}: no match in archive.org metadata");
                    }
                }
            }
            Ok(_) => {
                // Empty response — identifier not found on archive.org
                log::debug!("No audio files found for {dir_name}");
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

/// Try to match a local filename to an archive.org title using multiple strategies.
/// Returns (title, match_method) or None.
fn match_title<'a>(
    filename: &str,
    file_map: &'a HashMap<String, String>,
    position_map: &'a HashMap<(u32, u32), String>,
) -> Option<(String, &'static str)> {
    // Strategy 1: Exact filename match
    if let Some(title) = file_map.get(filename) {
        return Some((title.clone(), "exact"));
    }

    // Strategy 2: Stem match (strip extension)
    let stem = Path::new(filename)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string());

    if let Some(ref stem) = stem {
        let found = file_map.iter().find(|(k, _)| {
            Path::new(k.as_str())
                .file_stem()
                .map(|s| s.to_string_lossy() == *stem)
                .unwrap_or(false)
        });
        if let Some((_, title)) = found {
            return Some((title.clone(), "stem"));
        }
    }

    // Strategy 3: Position match (disc/track numbers)
    if let Some(pos) = extract_disc_track(filename) {
        if let Some(title) = position_map.get(&pos) {
            return Some((title.clone(), "position"));
        }
        // If disc extraction failed (disc=0), try matching track-only
        if pos.0 == 0 {
            // Find any entry with matching track number
            let found = position_map.iter().find(|((_, t), _)| *t == pos.1);
            if let Some((_, title)) = found {
                return Some((title.clone(), "track-only"));
            }
        }
    }

    None
}

/// Build a (disc, track) → title map from archive.org file entries.
fn build_position_map(file_map: &HashMap<String, String>) -> HashMap<(u32, u32), String> {
    let mut map = HashMap::new();
    for (name, title) in file_map {
        if let Some(pos) = extract_disc_track(name) {
            // Only insert if not already present (first one wins, usually .flac)
            map.entry(pos).or_insert_with(|| title.clone());
        }
    }
    map
}

/// Extract (disc, track) numbers from a filename.
///
/// Handles patterns like:
/// - `gd69-04-22d1t01.mp3` → (1, 1)
/// - `phish2013-10-31s1t01.flac` → (1, 1)  (set = disc)
/// - `gd74-06-23sbd_t40.mp3` → (0, 40)
/// - `ph150805d1_06_The_Last_Step.mp3` → (1, 6)
/// - `Built To Spill 1999-03-08 Boise11.mp3` → (0, 11)
fn extract_disc_track(filename: &str) -> Option<(u32, u32)> {
    let name = Path::new(filename)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();

    // Pattern 1: d{disc}t{track} or s{set}t{track} (most common)
    let re_dt = Regex::new(r"[ds](\d+)[t_](\d+)").unwrap();
    if let Some(caps) = re_dt.captures(&name) {
        let disc: u32 = caps[1].parse().unwrap_or(0);
        let track: u32 = caps[2].parse().unwrap_or(0);
        if track > 0 {
            return Some((disc, track));
        }
    }

    // Pattern 2: _t{track} or just t{track} preceded by non-alphanumeric
    let re_t = Regex::new(r"[_\-]t(\d+)").unwrap();
    if let Some(caps) = re_t.captures(&name) {
        let track: u32 = caps[1].parse().unwrap_or(0);
        if track > 0 {
            return Some((0, track));
        }
    }

    // Pattern 3: trailing digits (like "Boise11" → track 11)
    let re_trailing = Regex::new(r"(\d{1,2})$").unwrap();
    if let Some(caps) = re_trailing.captures(&name) {
        let track: u32 = caps[1].parse().unwrap_or(0);
        if track > 0 && track <= 50 {
            return Some((0, track));
        }
    }

    None
}

/// Normalize a directory name into an archive.org identifier.
///
/// Handles:
/// - GD 2-digit years: `gd69-04-22...` → `gd1969-04-22...`
/// - BTS lowercase: `bts1999-03-08` → `BTS1999-03-08`
/// - Phish short prefix: `ph1997-08-03...` → `phish1997-08-03...`
fn normalize_archive_identifier(dir_name: &str) -> String {
    // GD with 2-digit year: gd{YY}-... → gd19{YY}-...
    let re_gd_2digit = Regex::new(r"^gd(\d{2})-(.*)$").unwrap();
    if let Some(caps) = re_gd_2digit.captures(dir_name) {
        let yy: u32 = caps[1].parse().unwrap_or(0);
        let century = if yy <= 25 { "20" } else { "19" };
        return format!("gd{century}{}-{}", &caps[1], &caps[2]);
    }

    // BTS lowercase → uppercase
    if let Some(rest) = dir_name.strip_prefix("bts") {
        if rest.starts_with(|c: char| c.is_ascii_digit()) {
            return format!("BTS{rest}");
        }
    }

    // Phish: ph{4-digit year} → phish{4-digit year}
    if let Some(rest) = dir_name.strip_prefix("ph") {
        if rest.starts_with(|c: char| c.is_ascii_digit()) {
            // Check if it starts with a 4-digit year
            if rest.len() >= 4 && rest[..4].chars().all(|c| c.is_ascii_digit()) {
                return format!("phish{rest}");
            }
        }
    }

    // Date-only directories (like "2013-12-31") — can't determine band from dir alone
    // These will be handled by the search fallback

    dir_name.to_string()
}

/// Fetch metadata with identifier normalization and search fallback.
fn fetch_metadata_with_fallbacks(dir_name: &str) -> Result<HashMap<String, String>> {
    // Step 1: Try the normalized identifier
    let normalized = normalize_archive_identifier(dir_name);

    if normalized != dir_name {
        log::debug!("Normalized identifier: {dir_name} → {normalized}");
    }

    let map = fetch_archive_metadata(&normalized)?;
    if !map.is_empty() {
        return Ok(map);
    }

    // Step 2: If normalization changed it, also try the original
    if normalized != dir_name {
        let map = fetch_archive_metadata(dir_name)?;
        if !map.is_empty() {
            return Ok(map);
        }
    }

    // Step 3: Search fallback — extract date and band, search archive.org
    if let Some(map) = try_search_fallback(dir_name)? {
        return Ok(map);
    }

    Ok(HashMap::new())
}

/// Try to find an archive.org identifier by searching for the show date.
fn try_search_fallback(dir_name: &str) -> Result<Option<HashMap<String, String>>> {
    // Extract a date from the directory name
    let re_date = Regex::new(r"(\d{4})-(\d{2})-(\d{2})").unwrap();
    let date = if let Some(caps) = re_date.captures(dir_name) {
        format!("{}-{}-{}", &caps[1], &caps[2], &caps[3])
    } else {
        // Try 2-digit year
        let re_date2 = Regex::new(r"(\d{2})-(\d{2})-(\d{2})").unwrap();
        if let Some(caps) = re_date2.captures(dir_name) {
            let yy: u32 = caps[1].parse().unwrap_or(0);
            let century = if yy <= 25 { "20" } else { "19" };
            format!("{century}{}-{}-{}", &caps[1], &caps[2], &caps[3])
        } else {
            return Ok(None);
        }
    };

    // Determine the band/creator for the search
    let creator = if dir_name.starts_with("gd") {
        "GratefulDead"
    } else if dir_name.starts_with("ph") {
        "Phish"
    } else if dir_name.to_lowercase().starts_with("bts") {
        "BuiltToSpill"
    } else {
        return Ok(None);
    };

    log::debug!("Search fallback: creator={creator} date={date}");

    let url = format!(
        "https://archive.org/advancedsearch.php?q=collection%3A{creator}+date%3A{date}&fl%5B%5D=identifier&rows=5&output=json"
    );

    let response: ArchiveSearchResponse = match ureq::get(&url).call() {
        Ok(mut resp) => resp.body_mut().read_json()
            .with_context(|| format!("Failed to parse search JSON for {dir_name}"))?,
        Err(e) => {
            log::debug!("Search request failed for {dir_name}: {e}");
            return Ok(None);
        }
    };

    let docs = response.response
        .and_then(|r| r.docs)
        .unwrap_or_default();

    if docs.is_empty() {
        log::debug!("No search results for {creator} {date}");
        return Ok(None);
    }

    // Try each search result until we find one with titled audio files
    for doc in &docs {
        if let Some(identifier) = &doc.identifier {
            // Rate limit between attempts
            thread::sleep(Duration::from_millis(300));

            let map = fetch_archive_metadata(identifier)?;
            if !map.is_empty() {
                log::info!("Search fallback found: {dir_name} → {identifier} ({} files)", map.len());
                return Ok(Some(map));
            }
        }
    }

    Ok(None)
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

    #[test]
    fn test_normalize_gd_2digit_year() {
        assert_eq!(
            normalize_archive_identifier("gd69-04-22.sbd.miller.88466.sbeok.flac16"),
            "gd1969-04-22.sbd.miller.88466.sbeok.flac16"
        );
        assert_eq!(
            normalize_archive_identifier("gd82-08-08.sbd.wise.7690.shnf"),
            "gd1982-08-08.sbd.wise.7690.shnf"
        );
    }

    #[test]
    fn test_normalize_gd_4digit_year_unchanged() {
        // Already has 4-digit year — should pass through unchanged
        assert_eq!(
            normalize_archive_identifier("gd1972-04-14.sbd.miller.34552.flac"),
            "gd1972-04-14.sbd.miller.34552.flac"
        );
    }

    #[test]
    fn test_normalize_bts_uppercase() {
        assert_eq!(
            normalize_archive_identifier("bts1999-03-08"),
            "BTS1999-03-08"
        );
        assert_eq!(
            normalize_archive_identifier("bts2012-02-25.nt4.flac16"),
            "BTS2012-02-25.nt4.flac16"
        );
    }

    #[test]
    fn test_normalize_phish_prefix() {
        assert_eq!(
            normalize_archive_identifier("ph1997-11-16.692.shnf"),
            "phish1997-11-16.692.shnf"
        );
        assert_eq!(
            normalize_archive_identifier("ph2013-10-31dpa4022.flac16"),
            "phish2013-10-31dpa4022.flac16"
        );
    }

    #[test]
    fn test_normalize_already_correct() {
        // These should pass through unchanged
        assert_eq!(
            normalize_archive_identifier("phish2013-10-31"),
            "phish2013-10-31"
        );
        assert_eq!(
            normalize_archive_identifier("grateful_dead_live"),
            "grateful_dead_live"
        );
    }

    #[test]
    fn test_extract_disc_track_standard() {
        assert_eq!(extract_disc_track("gd69-04-22d1t01.mp3"), Some((1, 1)));
        assert_eq!(extract_disc_track("gd69-04-22d2t05.flac"), Some((2, 5)));
        assert_eq!(extract_disc_track("phish2013-10-31s1t01.flac"), Some((1, 1)));
    }

    #[test]
    fn test_extract_disc_track_underscore() {
        assert_eq!(extract_disc_track("ph150805d1_06_The_Last_Step.mp3"), Some((1, 6)));
    }

    #[test]
    fn test_extract_disc_track_no_disc() {
        assert_eq!(extract_disc_track("gd74-06-23sbd_t40.mp3"), Some((0, 40)));
    }

    #[test]
    fn test_extract_disc_track_trailing() {
        assert_eq!(extract_disc_track("Built To Spill 1999-03-08 Boise11.mp3"), Some((0, 11)));
    }

    #[test]
    fn test_extract_disc_track_none() {
        assert_eq!(extract_disc_track("README.txt"), None);
    }

    #[test]
    fn test_match_title_exact() {
        let mut file_map = HashMap::new();
        file_map.insert("gd69-04-22d1t01.mp3".to_string(), "Dark Star".to_string());
        let pos_map = build_position_map(&file_map);

        let result = match_title("gd69-04-22d1t01.mp3", &file_map, &pos_map);
        assert_eq!(result, Some(("Dark Star".to_string(), "exact")));
    }

    #[test]
    fn test_match_title_stem() {
        let mut file_map = HashMap::new();
        file_map.insert("gd69-04-22d1t01.flac".to_string(), "Dark Star".to_string());
        let pos_map = build_position_map(&file_map);

        let result = match_title("gd69-04-22d1t01.mp3", &file_map, &pos_map);
        assert_eq!(result, Some(("Dark Star".to_string(), "stem")));
    }

    #[test]
    fn test_match_title_position() {
        let mut file_map = HashMap::new();
        // Archive has phish-prefixed filenames
        file_map.insert("phish1997-11-16d1t02.flac".to_string(), "Tweezer".to_string());
        let pos_map = build_position_map(&file_map);

        // Local has ph-prefixed filenames — different stem but same position
        let result = match_title("ph97-11-16d1t02.mp3", &file_map, &pos_map);
        assert_eq!(result, Some(("Tweezer".to_string(), "position")));
    }

    #[test]
    fn test_search_response_deserialize() {
        let json = r#"{"response":{"docs":[{"identifier":"gd1969-04-22.sbd.miller.88466.sbeok.flac16"}]}}"#;
        let r: ArchiveSearchResponse = serde_json::from_str(json).unwrap();
        let docs = r.response.unwrap().docs.unwrap();
        assert_eq!(docs[0].identifier.as_deref(), Some("gd1969-04-22.sbd.miller.88466.sbeok.flac16"));
    }
}
