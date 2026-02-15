use std::collections::HashMap;
use std::sync::OnceLock;

use serde::Deserialize;

/// How to query archive.org for a band's shows.
#[derive(Debug, Clone, PartialEq)]
pub enum ArchiveStrategy {
    /// Band has a dedicated collection (e.g., "GratefulDead")
    Collection(String),
    /// Band uses creator field across multiple collections (e.g., "Phish")
    Creator(String),
}

/// Rules for normalizing archive.org identifiers (directory names).
#[derive(Debug, Clone)]
pub enum NormalizationRule {
    /// Replace a lowercase prefix with a different case: "bts" → "BTS"
    PrefixCase { from: String, to: String },
    /// Expand a short prefix to a longer one: "ph" → "phish"
    PrefixExpand { from: String, to: String },
    /// Expand 2-digit years in identifiers: "gd69-" → "gd1969-"
    YearExpand { prefix: String },
}

/// A single band entry with all its metadata.
#[derive(Debug, Clone)]
pub struct BandEntry {
    pub canonical_name: String,
    pub codes: Vec<String>,
    pub search_names: Vec<String>,
    pub archive_strategy: Option<ArchiveStrategy>,
    pub normalizations: Vec<NormalizationRule>,
    /// Prefix used in search fallback → collection/creator mapping.
    /// e.g., "gd" → "GratefulDead" for collection-based search
    pub search_fallback_prefix: Option<(String, String)>,
}

/// The unified band registry — single source of truth for all band data.
#[derive(Debug)]
pub struct BandRegistry {
    bands: Vec<BandEntry>,
    /// Maps lowercase band code → index in `bands`
    code_to_index: HashMap<String, usize>,
    /// Maps lowercase search name → index in `bands`
    search_to_index: HashMap<String, usize>,
}

/// Config file band definition (deserialized from TOML).
#[derive(Debug, Deserialize, Clone)]
pub struct CustomBandConfig {
    pub name: String,
    pub codes: Vec<String>,
    #[serde(default)]
    pub search: Vec<String>,
    #[serde(default)]
    pub archive: Option<CustomArchiveConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CustomArchiveConfig {
    #[serde(rename = "type")]
    pub strategy_type: String,
    pub value: String,
}

static REGISTRY: OnceLock<BandRegistry> = OnceLock::new();

/// Initialize the global band registry. Must be called once at startup.
/// Panics if called more than once.
pub fn init(custom_bands: &[CustomBandConfig]) {
    let registry = BandRegistry::new(custom_bands);
    REGISTRY
        .set(registry)
        .expect("BandRegistry already initialized");
}

/// Initialize with no custom bands (convenience for tests).
pub fn init_default() {
    // Only init if not already done (safe for tests that run in parallel)
    let _ = REGISTRY.set(BandRegistry::new(&[]));
}

/// Get the global band registry. Panics if not initialized.
pub fn registry() -> &'static BandRegistry {
    REGISTRY.get().expect("BandRegistry not initialized — call bands::init() first")
}

impl BandRegistry {
    fn new(custom_bands: &[CustomBandConfig]) -> Self {
        let mut bands = builtin_bands();

        // Merge custom bands
        for custom in custom_bands {
            let existing = bands.iter_mut().find(|b| {
                b.canonical_name.to_lowercase() == custom.name.to_lowercase()
            });

            if let Some(entry) = existing {
                // Additive merge: append new codes
                for code in &custom.codes {
                    let lower = code.to_lowercase();
                    if !entry.codes.contains(&lower) {
                        entry.codes.push(lower);
                    }
                }
                for name in &custom.search {
                    let lower = name.to_lowercase();
                    if !entry.search_names.contains(&lower) {
                        entry.search_names.push(lower);
                    }
                }
                // Override archive strategy if provided
                if let Some(ref archive) = custom.archive {
                    entry.archive_strategy = Some(parse_archive_strategy(archive));
                }
            } else {
                // New band
                let archive_strategy = custom.archive.as_ref().map(parse_archive_strategy);
                bands.push(BandEntry {
                    canonical_name: custom.name.clone(),
                    codes: custom.codes.iter().map(|c| c.to_lowercase()).collect(),
                    search_names: custom.search.iter().map(|s| s.to_lowercase()).collect(),
                    archive_strategy,
                    normalizations: Vec::new(),
                    search_fallback_prefix: None,
                });
            }
        }

        // Build lookup indices
        let mut code_to_index = HashMap::new();
        let mut search_to_index = HashMap::new();

        for (i, band) in bands.iter().enumerate() {
            for code in &band.codes {
                code_to_index.insert(code.clone(), i);
            }
            for name in &band.search_names {
                search_to_index.insert(name.clone(), i);
            }
        }

        BandRegistry {
            bands,
            code_to_index,
            search_to_index,
        }
    }

    /// Look up a band code (e.g., "gd", "ph") → canonical name.
    /// Replaces `expand_band_code()` in filename.rs.
    pub fn lookup_code(&self, code: &str) -> Option<&str> {
        let lower = code.to_lowercase();
        self.code_to_index
            .get(&lower)
            .map(|&i| self.bands[i].canonical_name.as_str())
    }

    /// Look up a search name (directory name component) → canonical name.
    /// Uses `starts_with` matching like the original known_bands array.
    /// Replaces the known_bands + titlecase_band() combo in filename.rs.
    pub fn lookup_search_name(&self, name: &str) -> Option<&str> {
        let lower = name.to_lowercase().replace('_', " ");

        // Exact match first
        if let Some(&i) = self.search_to_index.get(&lower) {
            return Some(self.bands[i].canonical_name.as_str());
        }

        // Starts-with match (e.g., "king gizzard" matches "king gizzard and the lizard wizard")
        for (search_name, &i) in &self.search_to_index {
            if lower == *search_name || lower.starts_with(search_name.as_str()) {
                return Some(self.bands[i].canonical_name.as_str());
            }
        }

        None
    }

    /// Resolve a band input (code or name) → archive query strategy.
    /// Replaces `resolve_query()` in discovery.rs.
    pub fn resolve_archive_query(&self, input: &str) -> Option<&ArchiveStrategy> {
        let lower = input.to_lowercase();

        // Try code lookup
        if let Some(&i) = self.code_to_index.get(&lower) {
            return self.bands[i].archive_strategy.as_ref();
        }

        // Try search name lookup
        for (name, &i) in &self.search_to_index {
            if lower == *name || lower.replace(' ', "") == name.replace(' ', "") {
                return self.bands[i].archive_strategy.as_ref();
            }
        }

        None
    }

    /// Resolve a band input (code or name) → canonical name for DB queries.
    /// Replaces `resolve_parsed_band()` in discovery.rs.
    pub fn resolve_canonical_name(&self, input: &str) -> String {
        let lower = input.to_lowercase();

        // Try code lookup
        if let Some(&i) = self.code_to_index.get(&lower) {
            return self.bands[i].canonical_name.clone();
        }

        // Try search name lookup (exact and normalized)
        let normalized = lower.replace(' ', "");
        for (name, &i) in &self.search_to_index {
            if lower == *name || normalized == name.replace(' ', "") {
                return self.bands[i].canonical_name.clone();
            }
        }

        // Fallback: return input as-is
        input.to_string()
    }

    /// Normalize a directory name into an archive.org identifier using band-specific rules.
    /// Replaces `normalize_archive_identifier()` in setlist/mod.rs.
    pub fn normalize_identifier(&self, dir_name: &str) -> String {
        for band in &self.bands {
            for rule in &band.normalizations {
                match rule {
                    NormalizationRule::YearExpand { prefix } => {
                        if let Some(rest) = dir_name.strip_prefix(prefix.as_str()) {
                            // Check if followed by 2-digit year + dash
                            if rest.len() >= 3 && rest.as_bytes()[2] == b'-' {
                                if let Ok(yy) = rest[..2].parse::<u32>() {
                                    let century = if yy <= 25 { "20" } else { "19" };
                                    return format!("{prefix}{century}{rest}");
                                }
                            }
                        }
                    }
                    NormalizationRule::PrefixCase { from, to } => {
                        if let Some(rest) = dir_name.strip_prefix(from.as_str()) {
                            if rest.starts_with(|c: char| c.is_ascii_digit()) {
                                return format!("{to}{rest}");
                            }
                        }
                    }
                    NormalizationRule::PrefixExpand { from, to } => {
                        if let Some(rest) = dir_name.strip_prefix(from.as_str()) {
                            if rest.starts_with(|c: char| c.is_ascii_digit()) {
                                // Ensure it has a 4-digit year
                                if rest.len() >= 4 && rest[..4].chars().all(|c| c.is_ascii_digit()) {
                                    return format!("{to}{rest}");
                                }
                            }
                        }
                    }
                }
            }
        }

        dir_name.to_string()
    }

    /// Resolve a directory name prefix to a search creator/collection for fallback lookups.
    /// Replaces the hardcoded prefix map in setlist/mod.rs try_search_fallback().
    pub fn resolve_search_creator(&self, dir_name: &str) -> Option<&str> {
        let lower = dir_name.to_lowercase();
        for band in &self.bands {
            if let Some((prefix, _)) = &band.search_fallback_prefix {
                if lower.starts_with(prefix.as_str()) {
                    return band.search_fallback_prefix.as_ref().map(|(_, v)| v.as_str());
                }
            }
        }
        None
    }

    /// Get all band entries (for iteration).
    pub fn bands(&self) -> &[BandEntry] {
        &self.bands
    }
}

fn parse_archive_strategy(config: &CustomArchiveConfig) -> ArchiveStrategy {
    match config.strategy_type.as_str() {
        "collection" => ArchiveStrategy::Collection(config.value.clone()),
        "creator" => ArchiveStrategy::Creator(config.value.clone()),
        _ => ArchiveStrategy::Creator(config.value.clone()),
    }
}

/// Build the built-in band registry from current hardcoded data.
/// Extracted from: filename.rs expand_band_code(), known_bands, discovery.rs, setlist/mod.rs
fn builtin_bands() -> Vec<BandEntry> {
    vec![
        BandEntry {
            canonical_name: "Grateful Dead".to_string(),
            codes: vec!["gd".into()],
            search_names: vec!["grateful dead".into()],
            archive_strategy: Some(ArchiveStrategy::Collection("GratefulDead".to_string())),
            normalizations: vec![NormalizationRule::YearExpand {
                prefix: "gd".to_string(),
            }],
            search_fallback_prefix: Some(("gd".into(), "GratefulDead".into())),
        },
        BandEntry {
            canonical_name: "Jerry Garcia Band".to_string(),
            codes: vec!["jg".into(), "jgb".into()],
            search_names: vec!["jerry garcia".into()],
            archive_strategy: None,
            normalizations: Vec::new(),
            search_fallback_prefix: None,
        },
        BandEntry {
            canonical_name: "Phish".to_string(),
            codes: vec!["ph".into(), "phish".into()],
            search_names: vec!["phish".into()],
            archive_strategy: Some(ArchiveStrategy::Creator("Phish".to_string())),
            normalizations: vec![NormalizationRule::PrefixExpand {
                from: "ph".to_string(),
                to: "phish".to_string(),
            }],
            search_fallback_prefix: Some(("ph".into(), "Phish".into())),
        },
        BandEntry {
            canonical_name: "Widespread Panic".to_string(),
            codes: vec!["wsp".into(), "panic".into()],
            search_names: vec!["widespread panic".into()],
            archive_strategy: None,
            normalizations: Vec::new(),
            search_fallback_prefix: None,
        },
        BandEntry {
            canonical_name: "moe.".to_string(),
            codes: vec!["moe".into()],
            search_names: vec!["moe.".into()],
            archive_strategy: None,
            normalizations: Vec::new(),
            search_fallback_prefix: None,
        },
        BandEntry {
            canonical_name: "Sound Tribe Sector 9".to_string(),
            codes: vec!["sts9".into(), "s9".into()],
            search_names: vec![],
            archive_strategy: None,
            normalizations: Vec::new(),
            search_fallback_prefix: None,
        },
        BandEntry {
            canonical_name: "Umphrey's McGee".to_string(),
            codes: vec!["um".into(), "ump".into()],
            search_names: vec!["umphrey's mcgee".into()],
            archive_strategy: None,
            normalizations: Vec::new(),
            search_fallback_prefix: None,
        },
        BandEntry {
            canonical_name: "Disco Biscuits".to_string(),
            codes: vec!["bisco".into(), "db".into()],
            search_names: vec!["disco biscuits".into()],
            archive_strategy: None,
            normalizations: Vec::new(),
            search_fallback_prefix: None,
        },
        BandEntry {
            canonical_name: "Ween".to_string(),
            codes: vec!["ween".into()],
            search_names: vec!["ween".into()],
            archive_strategy: None,
            normalizations: Vec::new(),
            search_fallback_prefix: None,
        },
        BandEntry {
            canonical_name: "Gov't Mule".to_string(),
            codes: vec!["mule".into()],
            search_names: vec!["gov't mule".into()],
            archive_strategy: None,
            normalizations: Vec::new(),
            search_fallback_prefix: None,
        },
        BandEntry {
            canonical_name: "Allman Brothers Band".to_string(),
            codes: vec!["abband".into(), "abb".into()],
            search_names: vec!["allman brothers band".into()],
            archive_strategy: None,
            normalizations: Vec::new(),
            search_fallback_prefix: None,
        },
        BandEntry {
            canonical_name: "Dark Star Orchestra".to_string(),
            codes: vec!["dso".into()],
            search_names: vec!["dark star orchestra".into()],
            archive_strategy: None,
            normalizations: Vec::new(),
            search_fallback_prefix: None,
        },
        BandEntry {
            canonical_name: "Led Zeppelin".to_string(),
            codes: vec!["lsz".into(), "led".into()],
            search_names: vec![],
            archive_strategy: None,
            normalizations: Vec::new(),
            search_fallback_prefix: None,
        },
        BandEntry {
            canonical_name: "Goose".to_string(),
            codes: vec!["goose".into()],
            search_names: vec!["goose".into()],
            archive_strategy: None,
            normalizations: Vec::new(),
            search_fallback_prefix: None,
        },
        BandEntry {
            canonical_name: "Built to Spill".to_string(),
            codes: vec!["bts".into()],
            search_names: vec!["built to spill".into()],
            archive_strategy: None,
            normalizations: vec![NormalizationRule::PrefixCase {
                from: "bts".to_string(),
                to: "BTS".to_string(),
            }],
            search_fallback_prefix: Some(("bts".into(), "BuiltToSpill".into())),
        },
        BandEntry {
            canonical_name: "Billy Strings".to_string(),
            codes: vec!["billy".into(), "bs".into(), "bsco".into()],
            search_names: vec!["billy strings".into()],
            archive_strategy: None,
            normalizations: Vec::new(),
            search_fallback_prefix: None,
        },
        BandEntry {
            canonical_name: "King Gizzard & the Lizard Wizard".to_string(),
            codes: vec!["kg".into(), "kglw".into()],
            search_names: vec!["king gizzard".into()],
            archive_strategy: None,
            normalizations: Vec::new(),
            search_fallback_prefix: None,
        },
        BandEntry {
            canonical_name: "Trey Anastasio Band".to_string(),
            codes: vec!["trey".into(), "tab".into()],
            search_names: vec!["trey anastasio band".into()],
            archive_strategy: None,
            normalizations: Vec::new(),
            search_fallback_prefix: None,
        },
        BandEntry {
            canonical_name: "Lotus".to_string(),
            codes: vec!["lotus".into()],
            search_names: vec!["lotus".into()],
            archive_strategy: None,
            normalizations: Vec::new(),
            search_fallback_prefix: None,
        },
        BandEntry {
            canonical_name: "Joe Russo's Almost Dead".to_string(),
            codes: vec!["jrad".into()],
            search_names: vec!["joe russo's almost dead".into()],
            archive_strategy: None,
            normalizations: Vec::new(),
            search_fallback_prefix: None,
        },
        BandEntry {
            canonical_name: "String Cheese Incident".to_string(),
            codes: vec!["sci".into()],
            search_names: vec!["string cheese incident".into()],
            archive_strategy: None,
            normalizations: Vec::new(),
            search_fallback_prefix: None,
        },
        BandEntry {
            canonical_name: "Leftover Salmon".to_string(),
            codes: vec!["lmg".into(), "lemon".into()],
            search_names: vec![],
            archive_strategy: None,
            normalizations: Vec::new(),
            search_fallback_prefix: None,
        },
        BandEntry {
            canonical_name: "Medeski Martin & Wood".to_string(),
            codes: vec!["mmw".into()],
            search_names: vec!["medeski martin".into()],
            archive_strategy: None,
            normalizations: Vec::new(),
            search_fallback_prefix: None,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_registry() -> BandRegistry {
        BandRegistry::new(&[])
    }

    #[test]
    fn test_lookup_code_basic() {
        let reg = test_registry();
        assert_eq!(reg.lookup_code("gd"), Some("Grateful Dead"));
        assert_eq!(reg.lookup_code("GD"), Some("Grateful Dead"));
        assert_eq!(reg.lookup_code("ph"), Some("Phish"));
        assert_eq!(reg.lookup_code("phish"), Some("Phish"));
        assert_eq!(reg.lookup_code("bts"), Some("Built to Spill"));
        assert_eq!(reg.lookup_code("unknown"), None);
    }

    #[test]
    fn test_lookup_code_all_aliases() {
        let reg = test_registry();
        // Verify all codes from the old expand_band_code() work
        assert_eq!(reg.lookup_code("jg"), Some("Jerry Garcia Band"));
        assert_eq!(reg.lookup_code("jgb"), Some("Jerry Garcia Band"));
        assert_eq!(reg.lookup_code("wsp"), Some("Widespread Panic"));
        assert_eq!(reg.lookup_code("panic"), Some("Widespread Panic"));
        assert_eq!(reg.lookup_code("moe"), Some("moe."));
        assert_eq!(reg.lookup_code("sts9"), Some("Sound Tribe Sector 9"));
        assert_eq!(reg.lookup_code("s9"), Some("Sound Tribe Sector 9"));
        assert_eq!(reg.lookup_code("um"), Some("Umphrey's McGee"));
        assert_eq!(reg.lookup_code("ump"), Some("Umphrey's McGee"));
        assert_eq!(reg.lookup_code("bisco"), Some("Disco Biscuits"));
        assert_eq!(reg.lookup_code("db"), Some("Disco Biscuits"));
        assert_eq!(reg.lookup_code("ween"), Some("Ween"));
        assert_eq!(reg.lookup_code("mule"), Some("Gov't Mule"));
        assert_eq!(reg.lookup_code("abband"), Some("Allman Brothers Band"));
        assert_eq!(reg.lookup_code("abb"), Some("Allman Brothers Band"));
        assert_eq!(reg.lookup_code("dso"), Some("Dark Star Orchestra"));
        assert_eq!(reg.lookup_code("lsz"), Some("Led Zeppelin"));
        assert_eq!(reg.lookup_code("led"), Some("Led Zeppelin"));
        assert_eq!(reg.lookup_code("goose"), Some("Goose"));
        assert_eq!(reg.lookup_code("billy"), Some("Billy Strings"));
        assert_eq!(reg.lookup_code("bs"), Some("Billy Strings"));
        assert_eq!(reg.lookup_code("bsco"), Some("Billy Strings"));
        assert_eq!(reg.lookup_code("kg"), Some("King Gizzard & the Lizard Wizard"));
        assert_eq!(reg.lookup_code("kglw"), Some("King Gizzard & the Lizard Wizard"));
        assert_eq!(reg.lookup_code("trey"), Some("Trey Anastasio Band"));
        assert_eq!(reg.lookup_code("tab"), Some("Trey Anastasio Band"));
        assert_eq!(reg.lookup_code("lotus"), Some("Lotus"));
        assert_eq!(reg.lookup_code("jrad"), Some("Joe Russo's Almost Dead"));
        assert_eq!(reg.lookup_code("sci"), Some("String Cheese Incident"));
        assert_eq!(reg.lookup_code("lmg"), Some("Leftover Salmon"));
        assert_eq!(reg.lookup_code("lemon"), Some("Leftover Salmon"));
        assert_eq!(reg.lookup_code("mmw"), Some("Medeski Martin & Wood"));
    }

    #[test]
    fn test_lookup_search_name() {
        let reg = test_registry();
        assert_eq!(reg.lookup_search_name("grateful dead"), Some("Grateful Dead"));
        assert_eq!(reg.lookup_search_name("Grateful Dead"), Some("Grateful Dead"));
        assert_eq!(reg.lookup_search_name("grateful_dead"), Some("Grateful Dead"));
        assert_eq!(reg.lookup_search_name("phish"), Some("Phish"));
        assert_eq!(reg.lookup_search_name("king gizzard"), Some("King Gizzard & the Lizard Wizard"));
        assert_eq!(reg.lookup_search_name("King Gizzard and the Lizard Wizard"), Some("King Gizzard & the Lizard Wizard"));
        assert_eq!(reg.lookup_search_name("medeski martin"), Some("Medeski Martin & Wood"));
        assert_eq!(reg.lookup_search_name("unknown band"), None);
    }

    #[test]
    fn test_resolve_archive_query() {
        let reg = test_registry();
        assert_eq!(
            reg.resolve_archive_query("gd"),
            Some(&ArchiveStrategy::Collection("GratefulDead".to_string()))
        );
        assert_eq!(
            reg.resolve_archive_query("phish"),
            Some(&ArchiveStrategy::Creator("Phish".to_string()))
        );
        assert_eq!(
            reg.resolve_archive_query("grateful dead"),
            Some(&ArchiveStrategy::Collection("GratefulDead".to_string()))
        );
        assert_eq!(reg.resolve_archive_query("bts"), None); // BTS has no archive strategy
        assert_eq!(reg.resolve_archive_query("unknown"), None);
    }

    #[test]
    fn test_resolve_canonical_name() {
        let reg = test_registry();
        assert_eq!(reg.resolve_canonical_name("gd"), "Grateful Dead");
        assert_eq!(reg.resolve_canonical_name("phish"), "Phish");
        assert_eq!(reg.resolve_canonical_name("bts"), "Built to Spill");
        assert_eq!(reg.resolve_canonical_name("grateful dead"), "Grateful Dead");
        assert_eq!(reg.resolve_canonical_name("gratefuldead"), "Grateful Dead");
        assert_eq!(reg.resolve_canonical_name("unknown"), "unknown");
    }

    #[test]
    fn test_normalize_identifier_gd_2digit_year() {
        let reg = test_registry();
        assert_eq!(
            reg.normalize_identifier("gd69-04-22.sbd.miller.88466.sbeok.flac16"),
            "gd1969-04-22.sbd.miller.88466.sbeok.flac16"
        );
        assert_eq!(
            reg.normalize_identifier("gd82-08-08.sbd.wise.7690.shnf"),
            "gd1982-08-08.sbd.wise.7690.shnf"
        );
    }

    #[test]
    fn test_normalize_identifier_gd_4digit_unchanged() {
        let reg = test_registry();
        assert_eq!(
            reg.normalize_identifier("gd1972-04-14.sbd.miller.34552.flac"),
            "gd1972-04-14.sbd.miller.34552.flac"
        );
    }

    #[test]
    fn test_normalize_identifier_bts_case() {
        let reg = test_registry();
        assert_eq!(reg.normalize_identifier("bts1999-03-08"), "BTS1999-03-08");
        assert_eq!(
            reg.normalize_identifier("bts2012-02-25.nt4.flac16"),
            "BTS2012-02-25.nt4.flac16"
        );
    }

    #[test]
    fn test_normalize_identifier_phish_expand() {
        let reg = test_registry();
        assert_eq!(
            reg.normalize_identifier("ph1997-11-16.692.shnf"),
            "phish1997-11-16.692.shnf"
        );
        assert_eq!(
            reg.normalize_identifier("ph2013-10-31dpa4022.flac16"),
            "phish2013-10-31dpa4022.flac16"
        );
    }

    #[test]
    fn test_normalize_identifier_passthrough() {
        let reg = test_registry();
        assert_eq!(
            reg.normalize_identifier("phish2013-10-31"),
            "phish2013-10-31"
        );
        assert_eq!(
            reg.normalize_identifier("grateful_dead_live"),
            "grateful_dead_live"
        );
    }

    #[test]
    fn test_resolve_search_creator() {
        let reg = test_registry();
        assert_eq!(reg.resolve_search_creator("gd1977-05-08"), Some("GratefulDead"));
        assert_eq!(reg.resolve_search_creator("ph1997-11-22"), Some("Phish"));
        assert_eq!(reg.resolve_search_creator("bts1999-03-08"), Some("BuiltToSpill"));
        assert_eq!(reg.resolve_search_creator("unknown-dir"), None);
    }

    #[test]
    fn test_custom_band_new() {
        let custom = vec![CustomBandConfig {
            name: "Lettuce".to_string(),
            codes: vec!["let".into(), "lettuce".into()],
            search: vec!["lettuce".into()],
            archive: Some(CustomArchiveConfig {
                strategy_type: "creator".to_string(),
                value: "Lettuce".to_string(),
            }),
        }];
        let reg = BandRegistry::new(&custom);
        assert_eq!(reg.lookup_code("let"), Some("Lettuce"));
        assert_eq!(reg.lookup_code("lettuce"), Some("Lettuce"));
        assert_eq!(
            reg.resolve_archive_query("let"),
            Some(&ArchiveStrategy::Creator("Lettuce".to_string()))
        );
    }

    #[test]
    fn test_custom_band_merge_codes() {
        let custom = vec![CustomBandConfig {
            name: "Grateful Dead".to_string(),
            codes: vec!["dead".into(), "gdead".into()],
            search: vec![],
            archive: None,
        }];
        let reg = BandRegistry::new(&custom);
        // Original codes still work
        assert_eq!(reg.lookup_code("gd"), Some("Grateful Dead"));
        // New codes also work
        assert_eq!(reg.lookup_code("dead"), Some("Grateful Dead"));
        assert_eq!(reg.lookup_code("gdead"), Some("Grateful Dead"));
    }
}
