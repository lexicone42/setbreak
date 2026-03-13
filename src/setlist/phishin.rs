// Phish setlist fetching from phish.in API (https://phish.in, open access).
//
// phish.in is a community-maintained archive of Phish audio with structured
// setlist data. API is open, no authentication required.
//
// Note: phish.in does NOT provide per-track segue information. The `segued`
// field will always be false for phish.in imports. Chain detection for Phish
// shows still relies on filename `->` markers from archive.org titles.

use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use serde::Deserialize;

use super::import::{ImportResult, SetlistEntry};
use crate::db::Database;

// ---------------------------------------------------------------------------
// API response types (only the fields we need)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ShowListResponse {
    total_pages: u32,
    #[allow(dead_code)]
    total_entries: u32,
    shows: Vec<ShowSummary>,
}

#[derive(Debug, Deserialize)]
struct ShowSummary {
    date: String,
}

/// Show detail response — flat (no `data` wrapper), tracks inline.
#[derive(Debug, Deserialize)]
struct ShowDetail {
    date: String,
    venue_name: Option<String>,
    venue: Option<PhishinVenue>,
    #[serde(default)]
    tracks: Vec<PhishinTrack>,
}

#[derive(Debug, Deserialize)]
struct PhishinVenue {
    city: Option<String>,
    state: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PhishinTrack {
    title: String,
    position: u32,
    set_name: String,
    #[allow(dead_code)]
    duration: Option<u64>,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Fetch all Phish show dates from phish.in, then fetch track listings for each.
/// Imports into the setlists table with source = "phish.in".
pub fn fetch_phish_setlists(
    db: &Database,
    rate_limit_ms: u64,
    dry_run: bool,
) -> Result<ImportResult> {
    // Step 1: Get all show dates
    println!("Fetching Phish show dates from phish.in...");
    let dates =
        fetch_all_show_dates(rate_limit_ms).context("Failed to fetch show dates from phish.in")?;
    println!("Found {} shows", dates.len());

    // Step 2: Check which dates we already have
    let existing = db
        .get_setlist_dates_for_source("phish.in")
        .context("Failed to query existing setlist dates")?;
    let existing_set: std::collections::HashSet<&str> =
        existing.iter().map(|s| s.as_str()).collect();

    let to_fetch: Vec<&str> = dates
        .iter()
        .map(|s| s.as_str())
        .filter(|d| !existing_set.contains(d))
        .collect();

    if to_fetch.is_empty() {
        println!("All {} shows already imported", dates.len());
        return Ok(ImportResult {
            shows_imported: 0,
            songs_imported: 0,
            files_processed: dates.len(),
        });
    }

    println!(
        "Fetching setlists for {} new shows ({} already cached)...",
        to_fetch.len(),
        existing_set.len()
    );

    // Step 3: Fetch each show's tracks
    let pb = ProgressBar::new(to_fetch.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template(
                "{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} shows ({eta} remaining) {msg}",
            )
            .unwrap()
            .progress_chars("=>-"),
    );

    let mut all_entries = Vec::new();
    let mut fetch_errors = 0usize;

    for date in &to_fetch {
        pb.set_message(date.to_string());

        match fetch_show_detail(date) {
            Ok(entries) => {
                all_entries.extend(entries);
            }
            Err(e) => {
                fetch_errors += 1;
                log::warn!("Failed to fetch {date}: {e}");
            }
        }

        pb.inc(1);
        thread::sleep(Duration::from_millis(rate_limit_ms));
    }

    pb.finish_with_message("done");

    if fetch_errors > 0 {
        println!("{fetch_errors} shows failed to fetch");
    }

    if dry_run {
        let show_count = all_entries
            .iter()
            .map(|e| &e.date)
            .collect::<std::collections::HashSet<_>>()
            .len();
        println!(
            "Would import {} songs across {} shows",
            all_entries.len(),
            show_count
        );
        return Ok(ImportResult {
            shows_imported: show_count,
            songs_imported: all_entries.len(),
            files_processed: to_fetch.len(),
        });
    }

    // Step 4: Import into DB
    let result = super::import::import_setlists_append(db, &all_entries)
        .context("Failed to import phish.in setlists")?;

    Ok(ImportResult {
        shows_imported: result.shows_imported,
        songs_imported: result.songs_imported,
        files_processed: to_fetch.len(),
    })
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Fetch all show dates from the paginated shows endpoint.
fn fetch_all_show_dates(rate_limit_ms: u64) -> Result<Vec<String>> {
    let mut dates = Vec::new();
    let mut page = 1u32;

    // First request to get total pages
    let first: ShowListResponse = api_get(&format!(
        "https://phish.in/api/v2/shows?per_page=50&sort=date:asc&page={page}"
    ))?;

    let total_pages = first.total_pages;
    for show in &first.shows {
        dates.push(show.date.clone());
    }

    let pb = ProgressBar::new(total_pages as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} pages")
            .unwrap()
            .progress_chars("=>-"),
    );
    pb.inc(1);

    page = 2;
    while page <= total_pages {
        let resp: ShowListResponse = api_get(&format!(
            "https://phish.in/api/v2/shows?per_page=50&sort=date:asc&page={page}"
        ))?;

        for show in &resp.shows {
            dates.push(show.date.clone());
        }

        pb.inc(1);
        page += 1;
        thread::sleep(Duration::from_millis(rate_limit_ms));
    }

    pb.finish_and_clear();
    Ok(dates)
}

/// Fetch track listing for a single show date.
fn fetch_show_detail(date: &str) -> Result<Vec<SetlistEntry>> {
    let show: ShowDetail = api_get(&format!("https://phish.in/api/v2/shows/{date}"))?;

    let city = show.venue.as_ref().and_then(|v| v.city.clone());
    let state = show.venue.as_ref().and_then(|v| v.state.clone());

    let mut entries = Vec::new();
    for track in &show.tracks {
        let set_num = parse_set_name(&track.set_name);

        entries.push(SetlistEntry {
            date: show.date.clone(),
            set_num,
            position: track.position as i32,
            song: track.title.clone(),
            segued: false, // phish.in doesn't provide segue data
            venue: show.venue_name.clone(),
            city: city.clone(),
            state: state.clone(),
            source: "phish.in".to_string(),
        });
    }

    Ok(entries)
}

/// Parse "Set 1" / "Set 2" / "Encore" / "Encore 2" into an integer.
fn parse_set_name(s: &str) -> i32 {
    match s {
        "Set 1" => 1,
        "Set 2" => 2,
        "Set 3" => 3,
        "Set 4" => 4,
        "Encore" => 10,
        "Encore 2" => 11,
        _ => {
            // Try to extract a number
            if let Some(n) = s.strip_prefix("Set ") {
                n.parse().unwrap_or(99)
            } else {
                99
            }
        }
    }
}

/// Make an API GET request and deserialize JSON.
fn api_get<T: serde::de::DeserializeOwned>(url: &str) -> Result<T> {
    log::debug!("GET {url}");
    let mut resp = ureq::get(url)
        .header("Accept", "application/json")
        .call()
        .with_context(|| format!("HTTP request failed: {url}"))?;

    resp.body_mut()
        .read_json()
        .with_context(|| format!("Failed to parse JSON from {url}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_set_name() {
        assert_eq!(parse_set_name("Set 1"), 1);
        assert_eq!(parse_set_name("Set 2"), 2);
        assert_eq!(parse_set_name("Encore"), 10);
        assert_eq!(parse_set_name("Encore 2"), 11);
    }

    #[test]
    fn test_show_list_deserialize() {
        let json = r#"{"total_pages":43,"current_page":1,"total_entries":2112,"shows":[{"id":1,"date":"1983-12-02","audio_status":"complete","duration":123,"venue_name":"Test","venue_location":"A, B","likes_count":0,"performance_gap_value":1,"tags":[],"created_at":"2012-01-01T00:00:00","updated_at":"2012-01-01T00:00:00","previous_show_date":null,"next_show_date":"1983-12-03","cover_art_urls":{},"album_cover_url":null}]}"#;
        let resp: ShowListResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.total_pages, 43);
        assert_eq!(resp.shows[0].date, "1983-12-02");
    }
}
