// Setlist import from gdshowsdb (https://github.com/jefmsmit/gdshowsdb, MIT license).
//
// Data attribution: Grateful Dead setlist data from jefmsmit/gdshowsdb,
// a community-maintained database of Grateful Dead setlists.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::db::Database;

/// A single song in a set.
#[derive(Debug, Deserialize)]
struct GdSong {
    #[serde(rename = ":name")]
    name: String,
    #[serde(rename = ":segued")]
    segued: bool,
    #[serde(rename = ":uuid")]
    #[allow(dead_code)]
    uuid: String,
}

/// A set within a show (array of songs).
#[derive(Debug, Deserialize)]
struct GdSet {
    #[serde(rename = ":songs")]
    songs: Vec<GdSong>,
    #[serde(rename = ":uuid")]
    #[allow(dead_code)]
    uuid: String,
}

/// A show entry in a year file.
#[derive(Debug, Deserialize)]
struct GdShow {
    #[serde(rename = ":venue")]
    venue: Option<String>,
    #[serde(rename = ":city")]
    city: Option<String>,
    #[serde(rename = ":state")]
    state: Option<String>,
    #[serde(rename = ":country")]
    #[allow(dead_code)]
    country: Option<String>,
    #[serde(rename = ":sets")]
    sets: Vec<GdSet>,
    #[serde(rename = ":uuid")]
    #[allow(dead_code)]
    uuid: String,
}

/// A single setlist entry ready for DB insertion.
#[derive(Debug)]
pub struct SetlistEntry {
    pub date: String,
    pub set_num: i32,
    pub position: i32,
    pub song: String,
    pub segued: bool,
    pub venue: Option<String>,
    pub city: Option<String>,
    pub state: Option<String>,
    pub source: String,
}

/// Result of an import operation.
pub struct ImportResult {
    pub shows_imported: usize,
    pub songs_imported: usize,
    pub files_processed: usize,
}

/// Parse all gdshowsdb YAML files from a directory and return setlist entries.
pub fn parse_gdshowsdb(data_dir: &Path) -> Result<Vec<SetlistEntry>> {
    let mut entries = Vec::new();
    let mut yaml_files: Vec<_> = std::fs::read_dir(data_dir)
        .with_context(|| format!("Failed to read directory: {}", data_dir.display()))?
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            // Only year files (1965.yaml .. 1995.yaml), skip song_refs.yaml and debug
            name.ends_with(".yaml")
                && name.chars().next().map_or(false, |c| c.is_ascii_digit())
        })
        .collect();

    yaml_files.sort_by_key(|e| e.file_name());

    for entry in &yaml_files {
        let path = entry.path();
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read {}", path.display()))?;

        let shows: BTreeMap<String, GdShow> = serde_yaml::from_str(&content)
            .with_context(|| format!("Failed to parse YAML: {}", path.display()))?;

        for (date_key, show) in &shows {
            // Normalize YYYY/MM/DD → YYYY-MM-DD
            let date = date_key.replace('/', "-");

            for (set_idx, set) in show.sets.iter().enumerate() {
                let set_num = (set_idx + 1) as i32;

                for (song_idx, song) in set.songs.iter().enumerate() {
                    entries.push(SetlistEntry {
                        date: date.clone(),
                        set_num,
                        position: (song_idx + 1) as i32,
                        song: song.name.clone(),
                        segued: song.segued,
                        venue: show.venue.clone(),
                        city: show.city.clone(),
                        state: show.state.clone(),
                        source: "gdshowsdb".to_string(),
                    });
                }
            }
        }
    }

    Ok(entries)
}

/// Import setlist entries into the database (bulk insert in a transaction).
/// Clears existing entries for the given source before inserting.
pub fn import_setlists(db: &Database, entries: &[SetlistEntry], source: &str) -> Result<ImportResult> {
    let tx = db.conn.unchecked_transaction()
        .context("Failed to start transaction")?;

    // Clear existing entries for this source
    tx.execute("DELETE FROM setlists WHERE source = ?1", rusqlite::params![source])
        .context("Failed to clear existing setlist entries")?;

    let mut stmt = tx.prepare(
        "INSERT INTO setlists (date, set_num, position, song, segued, venue, city, state, source)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"
    ).context("Failed to prepare insert statement")?;

    let mut shows = std::collections::HashSet::new();
    let mut songs = 0usize;

    for e in entries {
        stmt.execute(rusqlite::params![
            e.date, e.set_num, e.position, e.song, e.segued,
            e.venue, e.city, e.state, e.source,
        ]).with_context(|| format!("Failed to insert setlist entry: {} {} set {} pos {}",
            e.date, e.song, e.set_num, e.position))?;
        shows.insert(e.date.clone());
        songs += 1;
    }

    drop(stmt);
    tx.commit().context("Failed to commit setlist import")?;

    Ok(ImportResult {
        shows_imported: shows.len(),
        songs_imported: songs,
        files_processed: 0, // set by caller
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_gdshowsdb_sample() {
        // Create a temp YAML that mimics the gdshowsdb format
        let yaml = r#"---
1977/05/08:
  :uuid: 40cb4e0a-7c88-4f44-bf87-2596c174b699
  :venue: Barton Hall, Cornell University
  :city: Ithaca
  :state: NY
  :country: US
  :sets:
  - :uuid: b87cfc08-cf5d-4352-8a49-dc59052f54d2
    :songs:
    - :uuid: fb8957db-6ab7-4509-9e0b-93956ed117d0
      :name: Minglewood Blues
      :segued: false
    - :uuid: 0cc886b9-cc1f-4d71-aa58-02dfab36c590
      :name: Scarlet Begonias
      :segued: true
  - :uuid: 9feb1c81-00dd-437a-9498-6e0a35d631a8
    :songs:
    - :uuid: c171f74a-ba46-409e-931d-6c6b38e23e9d
      :name: Fire On The Mountain
      :segued: false
"#;
        let shows: BTreeMap<String, GdShow> = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(shows.len(), 1);

        let show = &shows["1977/05/08"];
        assert_eq!(show.venue.as_deref(), Some("Barton Hall, Cornell University"));
        assert_eq!(show.sets.len(), 2);
        assert_eq!(show.sets[0].songs.len(), 2);
        assert_eq!(show.sets[0].songs[0].name, "Minglewood Blues");
        assert!(!show.sets[0].songs[0].segued);
        assert_eq!(show.sets[0].songs[1].name, "Scarlet Begonias");
        assert!(show.sets[0].songs[1].segued);
    }

    #[test]
    fn test_date_normalization() {
        let date_key = "1977/05/08";
        let normalized = date_key.replace('/', "-");
        assert_eq!(normalized, "1977-05-08");
    }

    #[test]
    fn test_import_roundtrip() {
        let db = Database::open_in_memory().unwrap();

        let entries = vec![
            SetlistEntry {
                date: "1977-05-08".into(),
                set_num: 1,
                position: 1,
                song: "Minglewood Blues".into(),
                segued: false,
                venue: Some("Barton Hall".into()),
                city: Some("Ithaca".into()),
                state: Some("NY".into()),
                source: "gdshowsdb".into(),
            },
            SetlistEntry {
                date: "1977-05-08".into(),
                set_num: 2,
                position: 1,
                song: "Scarlet Begonias".into(),
                segued: true,
                venue: Some("Barton Hall".into()),
                city: Some("Ithaca".into()),
                state: Some("NY".into()),
                source: "gdshowsdb".into(),
            },
            SetlistEntry {
                date: "1977-05-08".into(),
                set_num: 2,
                position: 2,
                song: "Fire On The Mountain".into(),
                segued: false,
                venue: Some("Barton Hall".into()),
                city: Some("Ithaca".into()),
                state: Some("NY".into()),
                source: "gdshowsdb".into(),
            },
        ];

        let result = import_setlists(&db, &entries, "gdshowsdb").unwrap();
        assert_eq!(result.shows_imported, 1);
        assert_eq!(result.songs_imported, 3);

        // Verify data in DB
        let count: i64 = db.conn
            .query_row("SELECT COUNT(*) FROM setlists", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 3);

        // Check segue data
        let segued: bool = db.conn
            .query_row(
                "SELECT segued FROM setlists WHERE song = 'Scarlet Begonias'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(segued);
    }
}
