use regex::Regex;
use std::path::Path;
use std::sync::LazyLock;

/// Parsed metadata extracted from the file path.
#[derive(Debug, Default, PartialEq)]
pub struct ParsedPath {
    pub band: Option<String>,
    pub date: Option<String>,
    pub venue: Option<String>,
    pub disc: Option<i32>,
    pub track: Option<i32>,
    pub set: Option<String>,
    pub title: Option<String>,
}

/// Known band code mappings (archive.org conventions).
fn expand_band_code(code: &str) -> Option<&'static str> {
    match code.to_lowercase().as_str() {
        "gd" => Some("Grateful Dead"),
        "jg" | "jgb" => Some("Jerry Garcia Band"),
        "ph" | "phish" => Some("Phish"),
        "wsp" => Some("Widespread Panic"),
        "moe" => Some("moe."),
        "sts9" | "s9" => Some("Sound Tribe Sector 9"),
        "um" | "ump" => Some("Umphrey's McGee"),
        "bisco" | "db" => Some("Disco Biscuits"),
        "ween" => Some("Ween"),
        "mule" => Some("Gov't Mule"),
        "abband" | "abb" => Some("Allman Brothers Band"),
        "dso" => Some("Dark Star Orchestra"),
        "lsz" | "led" => Some("Led Zeppelin"),
        "goose" => Some("Goose"),
        "billy" | "bs" | "bsco" => Some("Billy Strings"),
        "kg" | "kglw" => Some("King Gizzard & the Lizard Wizard"),
        "trey" | "tab" => Some("Trey Anastasio Band"),
        "lotus" => Some("Lotus"),
        "jrad" => Some("Joe Russo's Almost Dead"),
        "sci" => Some("String Cheese Incident"),
        "lmg" | "lemon" => Some("Leftover Salmon"),
        "mmw" => Some("Medeski Martin & Wood"),
        "panic" => Some("Widespread Panic"),
        _ => None,
    }
}

/// Expand a 2-digit year to 4 digits (30-99 → 19xx, 00-29 → 20xx).
fn expand_year(year: &str) -> String {
    if year.len() == 2 {
        let y: u32 = year.parse().unwrap_or(0);
        if y >= 30 {
            format!("19{year}")
        } else {
            format!("20{year}")
        }
    } else {
        year.to_string()
    }
}

/// Basic date validation: month 1-12, day 1-31.
fn is_valid_date(month: &str, day: &str) -> bool {
    let m: u32 = month.parse().unwrap_or(0);
    let d: u32 = day.parse().unwrap_or(0);
    (1..=12).contains(&m) && (1..=31).contains(&d)
}

/// Build a normalized YYYY-MM-DD string from components.
fn build_date(year: &str, month: &str, day: &str) -> String {
    let m: u32 = month.parse().unwrap_or(0);
    let d: u32 = day.parse().unwrap_or(0);
    format!("{}-{m:02}-{d:02}", expand_year(year))
}

// Pattern 1: Band code + date at start of filename (supports 2-4 digit years)
// e.g., gd1977-05-08d1t01.shn, gd71-04-18d1t04.mp3
static BAND_DATE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?ix)
        ^(?P<band>[a-z]+)
        (?P<year>\d{2,4})-(?P<month>\d{1,2})-(?P<day>\d{1,2})"
    )
    .unwrap()
});

// Extract disc from filename remainder: d + 1-2 digits, not part of a word like "sbd"
// Requires d to be preceded by non-letter (or string start) and followed by non-digit
static REMAINDER_DISC_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:^|[^a-zA-Z])d(?P<disc>\d{1,2})(?:[^0-9]|$)").unwrap()
});

// Extract track: t/tr/track + digits, preceded by non-letter
static REMAINDER_TRACK_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:^|[^a-zA-Z])(?:t(?:rack)?|tr)(?P<track>\d{1,3})").unwrap()
});

// Extract set: s + 1-2 digits, preceded by non-letter, followed by non-digit
static REMAINDER_SET_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:^|[^a-zA-Z])s(?P<set>\d{1,2})(?:[^0-9]|$)").unwrap()
});

// Combined disc+track without separator: d206 = disc 2, track 06
static DISC_TRACK_COMBINED_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:^|[^a-zA-Z])d(?P<disc>\d)(?P<track>\d{2,3})$").unwrap()
});

// Pattern 2: Path-based with band/year/date-venue/disc-track structure
// e.g., Grateful Dead/1977/1977-05-08 Barton Hall/d1t01 - Scarlet Begonias.mp3
static PATH_DATE_VENUE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?ix)
        (?P<year>\d{4})-(?P<month>\d{2})-(?P<day>\d{2})
        \s+
        (?P<venue>.+)"
    )
    .unwrap()
});

static PATH_DISC_TRACK_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?ix)
        ^(?:d(?P<disc>\d+))?t(?P<track>\d+)
        (?:\s*[-–]\s*(?P<title>.+))?$"
    )
    .unwrap()
});

// Pattern 3: Set-based path structure
// e.g., Phish/1997.11.22/Set II/04 - Tweezer.flac
static SET_DIR_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^Set\s+(?P<set>I{1,3}|[1-3]|Encore)$").unwrap()
});

// Pattern 4: Generic date extraction (supports 2-4 digit years)
static GENERIC_DATE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?P<year>\d{2,4})[\.\-/](?P<month>\d{1,2})[\.\-/](?P<day>\d{1,2})"
    )
    .unwrap()
});

// Generic track number + title: "01 - Title", "01. Title", "23 Good Times"
// Allows space-only separator when title starts with a letter
static GENERIC_TRACK_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?P<track>\d{1,3})(?:\s*[\.\-–]\s*|\s+)(?P<title>[A-Za-z].+)$").unwrap()
});

/// Parse a file path to extract jam band metadata using a cascade of patterns.
pub fn parse_path(path: &Path) -> ParsedPath {
    let mut parsed = ParsedPath::default();
    let file_stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default();

    // Try Pattern 1: Band code + date in filename (2-4 digit years)
    if let Some(caps) = BAND_DATE_RE.captures(file_stem) {
        let code = caps.name("band").unwrap().as_str();
        let year = caps.name("year").unwrap().as_str();
        let month = caps.name("month").unwrap().as_str();
        let day = caps.name("day").unwrap().as_str();

        if is_valid_date(month, day) {
            parsed.band = expand_band_code(code).map(|s| s.to_string());
            parsed.date = Some(build_date(year, month, day));

            // Extract disc/track/set from remainder of filename
            let remainder = &file_stem[caps.get(0).unwrap().end()..];

            // Try explicit disc (d + 1-2 digits, followed by non-digit)
            parsed.disc = REMAINDER_DISC_RE
                .captures(remainder)
                .and_then(|c| c.name("disc"))
                .and_then(|m| m.as_str().parse().ok());

            // Try explicit track (t/tr/track + digits)
            parsed.track = REMAINDER_TRACK_RE
                .captures(remainder)
                .and_then(|c| c.name("track"))
                .and_then(|m| m.as_str().parse().ok());

            // If no explicit disc/track, try combined format (d206 = disc 2, track 06)
            if parsed.disc.is_none() && parsed.track.is_none() {
                if let Some(c) = DISC_TRACK_COMBINED_RE.captures(remainder) {
                    parsed.disc = c.name("disc").and_then(|m| m.as_str().parse().ok());
                    parsed.track = c.name("track").and_then(|m| m.as_str().parse().ok());
                }
            }

            // Try set (s + 1-2 digits)
            if parsed.set.is_none() {
                parsed.set = REMAINDER_SET_RE
                    .captures(remainder)
                    .and_then(|c| c.name("set"))
                    .map(|m| m.as_str().to_string());
            }

            return parsed;
        }
    }

    // Build path components for multi-component patterns
    let components: Vec<&str> = path
        .components()
        .filter_map(|c| c.as_os_str().to_str())
        .collect();

    // Walk components for band name (directory-based)
    for comp in &components {
        if let Some(band) = expand_band_code(comp) {
            parsed.band = Some(band.to_string());
            break;
        }
    }

    // If no band code matched, check for full band names in path components
    if parsed.band.is_none() {
        for comp in &components {
            // Normalize underscores to spaces for matching (grateful_dead → grateful dead)
            let lower = comp.to_lowercase().replace('_', " ");
            let known_bands = [
                "grateful dead", "phish", "widespread panic", "goose", "billy strings",
                "umphrey's mcgee", "disco biscuits", "moe.", "string cheese incident",
                "dark star orchestra", "trey anastasio band", "lotus", "ween",
                "gov't mule", "allman brothers band", "joe russo's almost dead",
                "king gizzard", "medeski martin",
            ];
            for band in &known_bands {
                if lower == *band || lower.starts_with(band) {
                    parsed.band = Some(titlecase_band(band));
                    break;
                }
            }
            if parsed.band.is_some() {
                break;
            }
        }
    }

    // Try Pattern 2: Look for date-venue in a directory component
    for comp in &components {
        if let Some(caps) = PATH_DATE_VENUE_RE.captures(comp) {
            let year = caps.name("year").unwrap().as_str();
            let month = caps.name("month").unwrap().as_str();
            let day = caps.name("day").unwrap().as_str();
            parsed.date = Some(format!("{year}-{month}-{day}"));
            parsed.venue = Some(caps.name("venue").unwrap().as_str().to_string());
            break;
        }
    }

    // Try Pattern 3: Look for "Set X" directory
    for comp in &components {
        if let Some(caps) = SET_DIR_RE.captures(comp) {
            parsed.set = Some(caps.name("set").unwrap().as_str().to_string());
            break;
        }
    }

    // Try Pattern 2b: disc/track from filename like "d1t01 - Title"
    if let Some(caps) = PATH_DISC_TRACK_RE.captures(file_stem) {
        parsed.disc = caps.name("disc").and_then(|m| m.as_str().parse().ok());
        parsed.track = caps.name("track").and_then(|m| m.as_str().parse().ok());
        parsed.title = caps.name("title").map(|m| m.as_str().trim().to_string());
        return parsed;
    }

    // Pattern 4: Generic fallback
    // Extract date from any path component (supports 2-4 digit years, validates)
    if parsed.date.is_none() {
        let full_path = path.to_string_lossy();
        for caps in GENERIC_DATE_RE.captures_iter(&full_path) {
            let year = caps.name("year").unwrap().as_str();
            let month = caps.name("month").unwrap().as_str();
            let day = caps.name("day").unwrap().as_str();
            if is_valid_date(month, day) {
                parsed.date = Some(build_date(year, month, day));
                break;
            }
        }
    }

    // Extract track number + title from filename
    if let Some(caps) = GENERIC_TRACK_RE.captures(file_stem) {
        parsed.track = caps.name("track").and_then(|m| m.as_str().parse().ok());
        let title = caps.name("title").map(|m| m.as_str().trim().to_string());
        if parsed.title.is_none() {
            parsed.title = title;
        }
    }

    parsed
}

fn titlecase_band(s: &str) -> String {
    match s {
        "grateful dead" => "Grateful Dead".to_string(),
        "phish" => "Phish".to_string(),
        "widespread panic" => "Widespread Panic".to_string(),
        "goose" => "Goose".to_string(),
        "billy strings" => "Billy Strings".to_string(),
        "umphrey's mcgee" => "Umphrey's McGee".to_string(),
        "disco biscuits" => "Disco Biscuits".to_string(),
        "moe." => "moe.".to_string(),
        "string cheese incident" => "String Cheese Incident".to_string(),
        "dark star orchestra" => "Dark Star Orchestra".to_string(),
        "trey anastasio band" => "Trey Anastasio Band".to_string(),
        "lotus" => "Lotus".to_string(),
        "ween" => "Ween".to_string(),
        "gov't mule" => "Gov't Mule".to_string(),
        "allman brothers band" => "Allman Brothers Band".to_string(),
        "joe russo's almost dead" => "Joe Russo's Almost Dead".to_string(),
        _ => s.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // === Compact format (band code + date in filename) ===

    #[test]
    fn test_compact_gd_4digit_year() {
        let p = PathBuf::from("gd1977-05-08d1t01.shn");
        let r = parse_path(&p);
        assert_eq!(r.band.as_deref(), Some("Grateful Dead"));
        assert_eq!(r.date.as_deref(), Some("1977-05-08"));
        assert_eq!(r.disc, Some(1));
        assert_eq!(r.track, Some(1));
    }

    #[test]
    fn test_compact_gd_2digit_year_70s() {
        let p = PathBuf::from("gd71-04-18d1t04.shn");
        let r = parse_path(&p);
        assert_eq!(r.band.as_deref(), Some("Grateful Dead"));
        assert_eq!(r.date.as_deref(), Some("1971-04-18"));
        assert_eq!(r.disc, Some(1));
        assert_eq!(r.track, Some(4));
    }

    #[test]
    fn test_compact_gd_2digit_year_80s() {
        let p = PathBuf::from("gd80-01-13d2t05.mp3");
        let r = parse_path(&p);
        assert_eq!(r.band.as_deref(), Some("Grateful Dead"));
        assert_eq!(r.date.as_deref(), Some("1980-01-13"));
        assert_eq!(r.disc, Some(2));
        assert_eq!(r.track, Some(5));
    }

    #[test]
    fn test_compact_gd_2digit_year_90s() {
        let p = PathBuf::from("gd93-04-01d1t02.mp3");
        let r = parse_path(&p);
        assert_eq!(r.band.as_deref(), Some("Grateful Dead"));
        assert_eq!(r.date.as_deref(), Some("1993-04-01"));
        assert_eq!(r.disc, Some(1));
        assert_eq!(r.track, Some(2));
    }

    #[test]
    fn test_compact_source_tag_between_date_and_track() {
        // gd74-06-23sbd_t26.mp3 — "sbd" source tag, underscore before track
        let p = PathBuf::from("gd74-06-23sbd_t26.mp3");
        let r = parse_path(&p);
        assert_eq!(r.band.as_deref(), Some("Grateful Dead"));
        assert_eq!(r.date.as_deref(), Some("1974-06-23"));
        assert_eq!(r.disc, None);
        assert_eq!(r.track, Some(26));
    }

    #[test]
    fn test_compact_set_track_notation() {
        // ph2013-12-31.mk5-s2t09.flac — set 2, track 9
        let p = PathBuf::from("ph2013-12-31.mk5-s2t09.flac");
        let r = parse_path(&p);
        assert_eq!(r.band.as_deref(), Some("Phish"));
        assert_eq!(r.date.as_deref(), Some("2013-12-31"));
        assert_eq!(r.set.as_deref(), Some("2"));
        assert_eq!(r.track, Some(9));
    }

    #[test]
    fn test_compact_motb_format() {
        let p = PathBuf::from("gd1979-10-31.motb.0039.s2t10.mp3");
        let r = parse_path(&p);
        assert_eq!(r.band.as_deref(), Some("Grateful Dead"));
        assert_eq!(r.date.as_deref(), Some("1979-10-31"));
        assert_eq!(r.set.as_deref(), Some("2"));
        assert_eq!(r.track, Some(10));
    }

    #[test]
    fn test_compact_combined_disc_track() {
        // d206 = disc 2, track 06 (no t separator)
        let p = PathBuf::from("ph1997-11-14d206.mp3");
        let r = parse_path(&p);
        assert_eq!(r.band.as_deref(), Some("Phish"));
        assert_eq!(r.date.as_deref(), Some("1997-11-14"));
        assert_eq!(r.disc, Some(2));
        assert_eq!(r.track, Some(6));
    }

    #[test]
    fn test_compact_track_word_spelled_out() {
        let p = PathBuf::from("gd71-12-31d2track06.mp3");
        let r = parse_path(&p);
        assert_eq!(r.band.as_deref(), Some("Grateful Dead"));
        assert_eq!(r.date.as_deref(), Some("1971-12-31"));
        assert_eq!(r.disc, Some(2));
        assert_eq!(r.track, Some(6));
    }

    #[test]
    fn test_compact_tr_prefix() {
        let p = PathBuf::from("gd1993-09-24-d1-tr03.wav");
        let r = parse_path(&p);
        assert_eq!(r.band.as_deref(), Some("Grateful Dead"));
        assert_eq!(r.date.as_deref(), Some("1993-09-24"));
        assert_eq!(r.disc, Some(1));
        assert_eq!(r.track, Some(3));
    }

    #[test]
    fn test_compact_single_digit_month() {
        let p = PathBuf::from("gd71-4-22d1t05.mp3");
        let r = parse_path(&p);
        assert_eq!(r.date.as_deref(), Some("1971-04-22"));
        assert_eq!(r.disc, Some(1));
        assert_eq!(r.track, Some(5));
    }

    #[test]
    fn test_compact_phish() {
        let p = PathBuf::from("ph1997-11-22t04.flac");
        let r = parse_path(&p);
        assert_eq!(r.band.as_deref(), Some("Phish"));
        assert_eq!(r.date.as_deref(), Some("1997-11-22"));
        assert_eq!(r.disc, None);
        assert_eq!(r.track, Some(4));
    }

    #[test]
    fn test_compact_no_disc_or_track() {
        let p = PathBuf::from("gd1972-08-27.shn");
        let r = parse_path(&p);
        assert_eq!(r.band.as_deref(), Some("Grateful Dead"));
        assert_eq!(r.date.as_deref(), Some("1972-08-27"));
        assert_eq!(r.disc, None);
        assert_eq!(r.track, None);
    }

    #[test]
    fn test_compact_uppercase() {
        let p = PathBuf::from("GD70-02-06d1t01.shn");
        let r = parse_path(&p);
        assert_eq!(r.band.as_deref(), Some("Grateful Dead"));
        assert_eq!(r.date.as_deref(), Some("1970-02-06"));
        assert_eq!(r.disc, Some(1));
        assert_eq!(r.track, Some(1));
    }

    #[test]
    fn test_compact_invalid_date_rejected() {
        // gd08-06-71 — day 71 is invalid, should not match as a date
        let p = PathBuf::from("gd08-06-71d2t06_vbr.mp3");
        let r = parse_path(&p);
        // BAND_DATE_RE matches but is_valid_date rejects day=71
        // Falls through, no valid date in the filename
        assert!(r.date.is_none());
    }

    // === Path-based patterns ===

    #[test]
    fn test_path_based_grateful_dead() {
        let p = PathBuf::from("Grateful Dead/1977/1977-05-08 Barton Hall/d1t01 - Scarlet Begonias.mp3");
        let r = parse_path(&p);
        assert_eq!(r.band.as_deref(), Some("Grateful Dead"));
        assert_eq!(r.date.as_deref(), Some("1977-05-08"));
        assert_eq!(r.venue.as_deref(), Some("Barton Hall"));
        assert_eq!(r.disc, Some(1));
        assert_eq!(r.track, Some(1));
        assert_eq!(r.title.as_deref(), Some("Scarlet Begonias"));
    }

    #[test]
    fn test_set_based_phish() {
        let p = PathBuf::from("Phish/1997.11.22/Set II/04 - Tweezer.flac");
        let r = parse_path(&p);
        assert_eq!(r.band.as_deref(), Some("Phish"));
        assert_eq!(r.date.as_deref(), Some("1997-11-22"));
        assert_eq!(r.set.as_deref(), Some("II"));
        assert_eq!(r.track, Some(4));
        assert_eq!(r.title.as_deref(), Some("Tweezer"));
    }

    #[test]
    fn test_set_encore() {
        let p = PathBuf::from("Phish/2023.07.14/Set Encore/01 - Tweezer Reprise.flac");
        let r = parse_path(&p);
        assert_eq!(r.set.as_deref(), Some("Encore"));
    }

    #[test]
    fn test_underscore_band_directory() {
        // grateful_dead directory should match "grateful dead"
        let p = PathBuf::from("grateful_dead/some_show/01 - Dark Star.mp3");
        let r = parse_path(&p);
        assert_eq!(r.band.as_deref(), Some("Grateful Dead"));
        assert_eq!(r.track, Some(1));
        assert_eq!(r.title.as_deref(), Some("Dark Star"));
    }

    // === Generic patterns ===

    #[test]
    fn test_generic_fallback_date() {
        let p = PathBuf::from("music/2023.12.31/03 - Midnight Jam.mp3");
        let r = parse_path(&p);
        assert_eq!(r.date.as_deref(), Some("2023-12-31"));
        assert_eq!(r.track, Some(3));
        assert_eq!(r.title.as_deref(), Some("Midnight Jam"));
    }

    #[test]
    fn test_generic_date_2digit_year_in_path() {
        // Date extracted from directory name with 2-digit year
        let p = PathBuf::from("grateful_dead/gd85-11-10/disc207-truckin.mp3");
        let r = parse_path(&p);
        assert_eq!(r.band.as_deref(), Some("Grateful Dead"));
        assert_eq!(r.date.as_deref(), Some("1985-11-10"));
    }

    #[test]
    fn test_generic_track_with_dash() {
        let p = PathBuf::from("01 - Dark Star.mp3");
        let r = parse_path(&p);
        assert_eq!(r.track, Some(1));
        assert_eq!(r.title.as_deref(), Some("Dark Star"));
    }

    #[test]
    fn test_generic_track_space_only() {
        // Baker's Dozen style: "23 Good Times Bad Times.flac"
        let p = PathBuf::from("23 Good Times Bad Times.flac");
        let r = parse_path(&p);
        assert_eq!(r.track, Some(23));
        assert_eq!(r.title.as_deref(), Some("Good Times Bad Times"));
    }

    #[test]
    fn test_generic_track_no_false_match_on_digits() {
        // "2_01.mp3" should NOT match as track 2 title "01"
        let p = PathBuf::from("2_01.mp3");
        let r = parse_path(&p);
        // title should be None (no alpha start) or track should handle differently
        assert!(r.title.is_none());
    }

    #[test]
    fn test_unknown_band() {
        let p = PathBuf::from("Random Band/2020-01-15/01 - Song.mp3");
        let r = parse_path(&p);
        assert_eq!(r.band, None);
        assert_eq!(r.date.as_deref(), Some("2020-01-15"));
        assert_eq!(r.track, Some(1));
    }

    #[test]
    fn test_band_code_goose() {
        let p = PathBuf::from("goose/goose2023-06-10d1t05.flac");
        let r = parse_path(&p);
        assert_eq!(r.band.as_deref(), Some("Goose"));
    }

    #[test]
    fn test_compact_goose_full() {
        let p = PathBuf::from("goose2023-06-10d1t05.flac");
        let r = parse_path(&p);
        assert_eq!(r.band.as_deref(), Some("Goose"));
        assert_eq!(r.date.as_deref(), Some("2023-06-10"));
        assert_eq!(r.disc, Some(1));
        assert_eq!(r.track, Some(5));
    }

    // === Year expansion ===

    #[test]
    fn test_expand_year() {
        assert_eq!(expand_year("71"), "1971");
        assert_eq!(expand_year("99"), "1999");
        assert_eq!(expand_year("68"), "1968");
        assert_eq!(expand_year("30"), "1930");
        assert_eq!(expand_year("00"), "2000");
        assert_eq!(expand_year("25"), "2025");
        assert_eq!(expand_year("29"), "2029");
        assert_eq!(expand_year("1977"), "1977");
        assert_eq!(expand_year("2023"), "2023");
    }

    #[test]
    fn test_date_validation() {
        assert!(is_valid_date("01", "01"));
        assert!(is_valid_date("12", "31"));
        assert!(is_valid_date("6", "8"));
        assert!(!is_valid_date("13", "01"));
        assert!(!is_valid_date("00", "15"));
        assert!(!is_valid_date("06", "32"));
        assert!(!is_valid_date("06", "71"));
    }
}
