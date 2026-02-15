use std::collections::HashMap;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use serde::Deserialize;

use crate::db::models::{ArchiveShow, MissingShow};
use crate::db::Database;

/// Cache TTL in days before re-fetching from archive.org.
const CACHE_TTL_DAYS: i64 = 30;

/// Results per page from archive.org search API.
const PAGE_SIZE: usize = 500;

/// Result of a discovery run.
pub struct DiscoveryResult {
    pub collection: String,
    pub archive_count: usize,
    pub local_count: usize,
    pub missing: Vec<MissingShow>,
}

/// Archive.org advanced search response.
#[derive(Debug, Deserialize)]
struct SearchResponse {
    response: SearchInner,
}

#[derive(Debug, Deserialize)]
struct SearchInner {
    #[serde(rename = "numFound")]
    num_found: usize,
    docs: Vec<SearchDoc>,
}

#[derive(Debug, Deserialize)]
struct SearchDoc {
    identifier: String,
    date: Option<String>,
    title: Option<String>,
}

/// Discover missing shows from archive.org for a given band.
pub fn discover_missing_shows(
    db: &Database,
    band: &str,
    force_refresh: bool,
    year_filter: Option<&str>,
    limit: usize,
) -> Result<DiscoveryResult> {
    let query = resolve_query(band)?;
    let cache_key = query_cache_key(&query).to_string();
    let parsed_band = resolve_parsed_band(band);

    // Check cache first
    let archive_shows = if force_refresh {
        None
    } else {
        db.get_cached_archive_shows(&cache_key, CACHE_TTL_DAYS)
            .context("Failed to read cache")?
    };

    let shows = match archive_shows {
        Some(cached) => {
            println!("Using cached data ({} shows, refresh with --refresh)", cached.len());
            cached
        }
        None => {
            let label = match &query {
                ArchiveQuery::Collection(c) => format!("collection '{c}'"),
                ArchiveQuery::Creator(c) => format!("creator '{c}'"),
            };
            println!("Fetching shows from archive.org {}...", label);
            let fetched = fetch_collection_shows(&query)?;
            let count = db.store_archive_shows(&fetched)
                .context("Failed to cache shows")?;
            println!("Cached {} shows from archive.org", count);
            fetched
        }
    };

    let archive_count = shows.len();

    // Get local show dates
    let local_dates: Vec<String> = db.get_local_show_dates(&parsed_band)
        .context("Failed to get local dates")?;
    let local_count = local_dates.len();

    // Build a set of local dates for fast lookup
    let local_set: std::collections::HashSet<&str> = local_dates.iter().map(|d| d.as_str()).collect();

    // Group archive shows by date, keeping the best quality per date
    let mut by_date: HashMap<String, Vec<&ArchiveShow>> = HashMap::new();
    for show in &shows {
        if !show.date.is_empty() {
            by_date.entry(show.date.clone()).or_default().push(show);
        }
    }

    // Find missing dates and build MissingShow entries
    let mut missing: Vec<MissingShow> = Vec::new();
    for (date, tapes) in &by_date {
        if local_set.contains(date.as_str()) {
            continue;
        }

        if let Some(filter) = year_filter {
            if !matches_year_filter(date, filter) {
                continue;
            }
        }

        // Find best tape (highest combined quality)
        let best = tapes.iter()
            .max_by_key(|t| t.source_quality * 10 + t.format_quality)
            .unwrap();

        missing.push(MissingShow {
            date: date.clone(),
            best_identifier: best.identifier.clone(),
            title: best.title.clone(),
            source_quality: best.source_quality,
            format_quality: best.format_quality,
            tape_count: tapes.len(),
        });
    }

    // Sort by date, then truncate
    missing.sort_by(|a, b| a.date.cmp(&b.date));
    missing.truncate(limit);

    Ok(DiscoveryResult {
        collection: cache_key,
        archive_count,
        local_count,
        missing,
    })
}

/// Solr deep pagination limit — archive.org returns errors past this offset.
const MAX_SOLR_OFFSET: usize = 10_000;

/// Year ranges to chunk queries and avoid the Solr 10K result limit.
/// Narrower ranges for peak-density years (GD 1970s had thousands of tapes/year).
const YEAR_RANGES: &[(u32, u32)] = &[
    (1960, 1969),
    (1970, 1971),
    (1972, 1973),
    (1974, 1975),
    (1976, 1977),
    (1978, 1979),
    (1980, 1982),
    (1983, 1985),
    (1986, 1988),
    (1989, 1991),
    (1992, 1995),
    (1996, 2025),
];

/// Fetch all shows from an archive.org collection or creator.
/// Uses year-range chunking to avoid Solr's 10K deep-pagination limit.
fn fetch_collection_shows(query: &ArchiveQuery) -> Result<Vec<ArchiveShow>> {
    let cache_key = query_cache_key(query);
    // First, get total count for progress bar
    let first_resp = fetch_search_page(query, None, 0, 0)?;
    let total = first_resp.response.num_found;

    let pb = ProgressBar::new(total as u64);
    pb.set_style(
        ProgressStyle::with_template(
            "  [{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} shows ({per_sec})"
        )
        .unwrap()
        .progress_chars("##-"),
    );

    let mut all_shows = Vec::new();

    for &(year_start, year_end) in YEAR_RANGES {
        let date_range = Some((year_start, year_end));
        let mut offset = 0;

        loop {
            if offset >= MAX_SOLR_OFFSET {
                log::warn!(
                    "Hit Solr limit for {cache_key} years {year_start}-{year_end} at offset {offset}"
                );
                break;
            }

            thread::sleep(Duration::from_millis(200));

            match fetch_search_page(query, date_range, offset, PAGE_SIZE) {
                Ok(resp) => {
                    let docs = &resp.response.docs;
                    if docs.is_empty() {
                        break;
                    }
                    for doc in docs {
                        if let Some(show) = parse_search_doc(doc, cache_key) {
                            all_shows.push(show);
                        }
                    }
                    pb.set_position(all_shows.len() as u64);

                    if docs.len() < PAGE_SIZE {
                        break; // Last page
                    }
                    offset += PAGE_SIZE;
                }
                Err(e) => {
                    log::warn!("Failed to fetch {cache_key} {year_start}-{year_end} offset {offset}: {e}");
                    break;
                }
            }
        }
    }

    pb.finish_with_message(format!("Fetched {} shows", all_shows.len()));
    Ok(all_shows)
}

/// Fetch a single page from the archive.org advanced search API.
/// If `date_range` is Some, restricts to items with dates in that year range.
fn fetch_search_page(
    query: &ArchiveQuery,
    date_range: Option<(u32, u32)>,
    start: usize,
    rows: usize,
) -> Result<SearchResponse> {
    let q_clause = query_clause(query);
    let date_clause = match date_range {
        Some((y1, y2)) => format!("+date%3A%5B{y1}-01-01+TO+{y2}-12-31%5D"),
        None => String::new(),
    };
    let url = format!(
        "https://archive.org/advancedsearch.php?\
         q={q_clause}{date_clause}&\
         fl%5B%5D=identifier&fl%5B%5D=date&fl%5B%5D=title&\
         sort%5B%5D=date+asc&\
         rows={rows}&start={start}&output=json"
    );

    let resp: SearchResponse = ureq::get(&url)
        .call()
        .with_context(|| format!("HTTP request failed (page at offset {start})"))?
        .body_mut()
        .read_json()
        .with_context(|| format!("Failed to parse JSON (page at offset {start})"))?;

    Ok(resp)
}

/// Parse a search doc into an ArchiveShow.
fn parse_search_doc(doc: &SearchDoc, collection: &str) -> Option<ArchiveShow> {
    let date = extract_date(doc.date.as_deref()?)?;
    let source_q = parse_source_quality(&doc.identifier);
    let format_q = parse_format_quality(&doc.identifier);

    Some(ArchiveShow {
        identifier: doc.identifier.clone(),
        collection: collection.to_string(),
        date,
        title: doc.title.clone().unwrap_or_default(),
        source_quality: source_q,
        format_quality: format_q,
    })
}

/// Extract YYYY-MM-DD date from archive.org date strings.
/// Handles: "1977-05-08T00:00:00Z", "1977-05-08", "1977-05-08T00:00:00"
fn extract_date(raw: &str) -> Option<String> {
    let d = raw.trim();
    if d.len() >= 10 {
        let date_part = &d[..10];
        // Basic validation: YYYY-MM-DD
        if date_part.len() == 10
            && date_part.as_bytes()[4] == b'-'
            && date_part.as_bytes()[7] == b'-'
        {
            return Some(date_part.to_string());
        }
    }
    None
}

/// Parse source quality from identifier string.
/// sbd=3 (soundboard), matrix=2, aud=1 (audience), unknown=0
fn parse_source_quality(identifier: &str) -> i32 {
    let id_lower = identifier.to_lowercase();
    if id_lower.contains(".sbd.") || id_lower.contains("_sbd_") || id_lower.contains("-sbd-") || id_lower.contains(".sbd") {
        3
    } else if id_lower.contains("matrix") || id_lower.contains(".mtx.") {
        2
    } else if id_lower.contains(".aud.") || id_lower.contains("_aud_") || id_lower.contains("-aud-") {
        1
    } else {
        0 // Unknown source
    }
}

/// Parse format quality from identifier string.
/// flac=3, shn=2, mp3=1, unknown=0
fn parse_format_quality(identifier: &str) -> i32 {
    let id_lower = identifier.to_lowercase();
    if id_lower.contains("flac") {
        3
    } else if id_lower.contains("shn") {
        2
    } else if id_lower.contains("mp3") || id_lower.contains("64kb") || id_lower.contains("128kb") || id_lower.contains("vbr") {
        1
    } else {
        0
    }
}

/// How to query archive.org for a band's shows.
enum ArchiveQuery {
    /// Band has a dedicated collection (e.g., GratefulDead)
    Collection(String),
    /// Band uses creator field across multiple collections
    Creator(String),
}

/// Resolve band shorthand to archive.org query strategy.
fn resolve_query(band: &str) -> Result<ArchiveQuery> {
    match band.to_lowercase().as_str() {
        "gd" | "grateful dead" | "gratefuldead" => Ok(ArchiveQuery::Collection("GratefulDead".to_string())),
        "phish" => Ok(ArchiveQuery::Creator("Phish".to_string())),
        "bts" | "built to spill" => {
            anyhow::bail!("Built to Spill archive.org collection not yet mapped. Use --band gd for now.")
        }
        _ => anyhow::bail!("Unknown band '{}'. Supported: gd, phish", band),
    }
}

/// Get the cache key (collection name) for a query.
fn query_cache_key(query: &ArchiveQuery) -> &str {
    match query {
        ArchiveQuery::Collection(c) => c,
        ArchiveQuery::Creator(c) => c,
    }
}

/// Build the search query string for archive.org.
fn query_clause(query: &ArchiveQuery) -> String {
    match query {
        ArchiveQuery::Collection(c) => format!("collection%3A{c}"),
        ArchiveQuery::Creator(c) => format!("creator%3A{c}"),
    }
}

/// Resolve band shorthand to the parsed_band value used in the tracks table.
fn resolve_parsed_band(band: &str) -> String {
    match band.to_lowercase().as_str() {
        "gd" | "grateful dead" | "gratefuldead" => "Grateful Dead".to_string(),
        "phish" => "Phish".to_string(),
        "bts" | "built to spill" => "Built to Spill".to_string(),
        _ => band.to_string(),
    }
}

/// Check if a date matches a year filter.
/// Supports: "1977" (single year) or "1977-1980" (range).
fn matches_year_filter(date: &str, filter: &str) -> bool {
    let date_year: u32 = match date.get(..4).and_then(|y| y.parse().ok()) {
        Some(y) => y,
        None => return false,
    };

    if let Some((start, end)) = filter.split_once('-') {
        // Could be YYYY-YYYY range or just a date like YYYY-MM-DD
        if start.len() == 4 && end.len() == 4 {
            if let (Ok(s), Ok(e)) = (start.parse::<u32>(), end.parse::<u32>()) {
                return date_year >= s && date_year <= e;
            }
        }
    }

    // Single year
    if let Ok(y) = filter.parse::<u32>() {
        return date_year == y;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_date() {
        assert_eq!(extract_date("1977-05-08T00:00:00Z"), Some("1977-05-08".into()));
        assert_eq!(extract_date("1977-05-08"), Some("1977-05-08".into()));
        assert_eq!(extract_date("1977-05-08T00:00:00"), Some("1977-05-08".into()));
        assert_eq!(extract_date("bad"), None);
        assert_eq!(extract_date(""), None);
    }

    #[test]
    fn test_source_quality() {
        assert_eq!(parse_source_quality("gd1977-05-08.sbd.miller.12345.sbeok.shnf"), 3);
        assert_eq!(parse_source_quality("gd1977-05-08.aud.12345.shnf"), 1);
        assert_eq!(parse_source_quality("gd1977-05-08.matrix.12345.flac16"), 2);
        assert_eq!(parse_source_quality("gd1977-05-08.12345.shnf"), 0);
    }

    #[test]
    fn test_format_quality() {
        assert_eq!(parse_format_quality("gd1977-05-08.sbd.miller.12345.flac16"), 3);
        assert_eq!(parse_format_quality("gd1977-05-08.sbd.miller.12345.shnf"), 2);
        assert_eq!(parse_format_quality("gd1977-05-08.sbd.miller.12345.mp3"), 1);
        assert_eq!(parse_format_quality("gd1977-05-08.sbd.miller.12345"), 0);
    }

    #[test]
    fn test_year_filter() {
        assert!(matches_year_filter("1977-05-08", "1977"));
        assert!(!matches_year_filter("1978-05-08", "1977"));
        assert!(matches_year_filter("1977-05-08", "1975-1980"));
        assert!(matches_year_filter("1980-05-08", "1975-1980"));
        assert!(!matches_year_filter("1981-05-08", "1975-1980"));
        assert!(!matches_year_filter("bad-date", "1977"));
    }

    #[test]
    fn test_resolve_query() {
        assert!(matches!(resolve_query("gd").unwrap(), ArchiveQuery::Collection(c) if c == "GratefulDead"));
        assert!(matches!(resolve_query("phish").unwrap(), ArchiveQuery::Creator(c) if c == "Phish"));
        assert!(resolve_query("unknown_band").is_err());
    }

    #[test]
    fn test_resolve_parsed_band() {
        assert_eq!(resolve_parsed_band("gd"), "Grateful Dead");
        assert_eq!(resolve_parsed_band("phish"), "Phish");
    }
}
