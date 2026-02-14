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

// Pattern 1: Compact archive.org filenames
// e.g., gd1977-05-08d1t01.shn
static COMPACT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?ix)
        ^(?P<band>[a-z]+)
        (?P<year>\d{4})-(?P<month>\d{2})-(?P<day>\d{2})
        (?:d(?P<disc>\d+))?
        (?:t(?P<track>\d+))?
        (?:\..+)?$"
    )
    .unwrap()
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

// Pattern 4: Generic date extraction
static GENERIC_DATE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?P<year>\d{4})[\.\-/](?P<month>\d{2})[\.\-/](?P<day>\d{2})"
    )
    .unwrap()
});

// Generic track number at start of filename: "01 - Title.ext" or "01. Title.ext"
static GENERIC_TRACK_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?P<track>\d{1,3})\s*[\.\-–]\s*(?P<title>.+)$").unwrap()
});

/// Parse a file path to extract jam band metadata using a cascade of patterns.
pub fn parse_path(path: &Path) -> ParsedPath {
    let mut parsed = ParsedPath::default();
    let file_stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default();

    // Try Pattern 1: Compact archive.org filename
    if let Some(caps) = COMPACT_RE.captures(file_stem) {
        let code = caps.name("band").unwrap().as_str();
        parsed.band = expand_band_code(code).map(|s| s.to_string());
        let year = caps.name("year").unwrap().as_str();
        let month = caps.name("month").unwrap().as_str();
        let day = caps.name("day").unwrap().as_str();
        parsed.date = Some(format!("{year}-{month}-{day}"));
        parsed.disc = caps.name("disc").and_then(|m| m.as_str().parse().ok());
        parsed.track = caps.name("track").and_then(|m| m.as_str().parse().ok());
        return parsed;
    }

    // Build path components for multi-component patterns
    let components: Vec<&str> = path
        .components()
        .filter_map(|c| c.as_os_str().to_str())
        .collect();

    // Walk components for band name (directory-based)
    // Heuristic: a component that looks like a known band name
    for comp in &components {
        if let Some(band) = expand_band_code(comp) {
            parsed.band = Some(band.to_string());
            break;
        }
    }

    // If no band code matched, check for full band names in path components
    if parsed.band.is_none() {
        for comp in &components {
            // If a path component matches a known full band name
            let lower = comp.to_lowercase();
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
    // Extract date from any path component
    if parsed.date.is_none() {
        let full_path = path.to_string_lossy();
        if let Some(caps) = GENERIC_DATE_RE.captures(&full_path) {
            let year = caps.name("year").unwrap().as_str();
            let month = caps.name("month").unwrap().as_str();
            let day = caps.name("day").unwrap().as_str();
            parsed.date = Some(format!("{year}-{month}-{day}"));
        }
    }

    // Extract track number + title from filename
    if let Some(caps) = GENERIC_TRACK_RE.captures(file_stem) {
        parsed.track = caps.name("track").and_then(|m| m.as_str().parse().ok());
        let title = caps.name("title").map(|m| m.as_str().trim().to_string());
        // Don't overwrite if already set
        if parsed.title.is_none() {
            parsed.title = title;
        }
    }

    parsed
}

fn titlecase_band(s: &str) -> String {
    // Specific known capitalization
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

    #[test]
    fn test_compact_gd() {
        let p = PathBuf::from("gd1977-05-08d1t01.shn");
        let r = parse_path(&p);
        assert_eq!(r.band.as_deref(), Some("Grateful Dead"));
        assert_eq!(r.date.as_deref(), Some("1977-05-08"));
        assert_eq!(r.disc, Some(1));
        assert_eq!(r.track, Some(1));
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
    fn test_generic_fallback_date() {
        let p = PathBuf::from("music/2023.12.31/03 - Midnight Jam.mp3");
        let r = parse_path(&p);
        assert_eq!(r.date.as_deref(), Some("2023-12-31"));
        assert_eq!(r.track, Some(3));
        assert_eq!(r.title.as_deref(), Some("Midnight Jam"));
    }

    #[test]
    fn test_generic_track_title() {
        let p = PathBuf::from("01 - Dark Star.mp3");
        let r = parse_path(&p);
        assert_eq!(r.track, Some(1));
        assert_eq!(r.title.as_deref(), Some("Dark Star"));
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
}
