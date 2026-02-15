use crate::db::models::{ChainScore, TrackScore};

/// Check if a track title ends with a segue marker.
/// Matches: " ->", "->", " -->", "-->", " >" (with trailing whitespace tolerance).
/// Does NOT match bare ">" without a space prefix (avoids false positives on titles like "Alligator>").
fn has_segue_marker(title: &str) -> bool {
    let t = title.trim_end();
    t.ends_with("-->")
        || t.ends_with("->")
        || t.ends_with(" >")
}

/// Detect segue chains from an ordered list of tracks within a single show.
/// Tracks must be pre-sorted by disc/track order.
/// `min_length` is the minimum number of songs to form a chain (typically 2).
pub fn detect_chains(tracks: &[TrackScore], min_length: usize) -> Vec<ChainScore> {
    let mut chains = Vec::new();
    let mut current_chain: Vec<&TrackScore> = Vec::new();

    for track in tracks {
        if current_chain.is_empty() {
            // Start potential chain
            current_chain.push(track);
        } else if has_segue_marker(&current_chain.last().unwrap().title) {
            // Previous track segued into this one
            current_chain.push(track);
        } else {
            // Previous track did NOT segue — emit chain if long enough, start new
            if current_chain.len() >= min_length {
                chains.push(ChainScore::from_tracks(
                    &current_chain.iter().copied().cloned().collect::<Vec<_>>(),
                ));
            }
            current_chain.clear();
            current_chain.push(track);
        }
    }

    // Don't forget the last chain
    if current_chain.len() >= min_length {
        chains.push(ChainScore::from_tracks(
            &current_chain.iter().copied().cloned().collect::<Vec<_>>(),
        ));
    }

    chains
}

/// Filter and sort chains by various criteria.
pub fn filter_and_sort_chains(
    mut chains: Vec<ChainScore>,
    min_duration: Option<f64>,
    song_filter: Option<&str>,
    sort_column: &str,
    limit: usize,
) -> Vec<ChainScore> {
    if let Some(min_dur) = min_duration {
        chains.retain(|c| c.duration_min >= min_dur);
    }

    if let Some(pattern) = song_filter {
        let p = pattern.to_lowercase();
        chains.retain(|c| {
            c.songs.iter().any(|s| s.to_lowercase().contains(&p))
        });
    }

    // Sort by the requested column (descending)
    let score_fn: fn(&ChainScore) -> f64 = match sort_column {
        "energy_score" => |c| c.energy,
        "intensity_score" => |c| c.intensity,
        "groove_score" => |c| c.groove,
        "improvisation_score" => |c| c.improvisation,
        "tightness_score" => |c| c.tightness,
        "build_quality_score" => |c| c.build_quality,
        "exploratory_score" => |c| c.exploratory,
        "transcendence_score" => |c| c.transcendence,
        "valence_score" => |c| c.valence,
        "arousal_score" => |c| c.arousal,
        "duration" => |c| c.duration_min,
        _ => |c| c.transcendence, // default
    };

    chains.sort_by(|a, b| score_fn(b).partial_cmp(&score_fn(a)).unwrap_or(std::cmp::Ordering::Equal));
    chains.truncate(limit);
    chains
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_track(title: &str, duration_min: f64, transcendence: f64) -> TrackScore {
        TrackScore {
            title: title.to_string(),
            date: "1977-05-08".to_string(),
            duration_min,
            key: None,
            tempo: None,
            energy: 50.0,
            intensity: 50.0,
            groove: 50.0,
            improvisation: 50.0,
            tightness: 50.0,
            build_quality: 50.0,
            exploratory: 50.0,
            transcendence,
            valence: 50.0,
            arousal: 50.0,
        }
    }

    #[test]
    fn test_segue_detection() {
        assert!(has_segue_marker("Dark Star ->"));
        assert!(has_segue_marker("Dark Star->"));
        assert!(has_segue_marker("Dark Star -->"));
        assert!(has_segue_marker("Dark Star-->"));
        assert!(has_segue_marker("Dark Star >"));
        assert!(has_segue_marker("Dark Star ->  ")); // trailing whitespace

        assert!(!has_segue_marker("Dark Star"));
        assert!(!has_segue_marker("Dark Star!"));
        // Bare > without space is not a segue
        assert!(!has_segue_marker("Alligator>"));
    }

    #[test]
    fn test_simple_two_song_chain() {
        let tracks = vec![
            make_track("Scarlet Begonias ->", 8.0, 60.0),
            make_track("Fire on the Mountain", 12.0, 80.0),
        ];

        let chains = detect_chains(&tracks, 2);
        assert_eq!(chains.len(), 1);
        assert_eq!(chains[0].chain_length, 2);
        assert_eq!(chains[0].songs, vec!["Scarlet Begonias", "Fire on the Mountain"]);
        // Duration-weighted: (60*8 + 80*12) / 20 = 1440/20 = 72
        assert!((chains[0].transcendence - 72.0).abs() < 0.01);
    }

    #[test]
    fn test_three_song_chain() {
        let tracks = vec![
            make_track("Dark Star ->", 20.0, 90.0),
            make_track("St. Stephen ->", 10.0, 70.0),
            make_track("The Eleven", 15.0, 80.0),
        ];

        let chains = detect_chains(&tracks, 2);
        assert_eq!(chains.len(), 1);
        assert_eq!(chains[0].chain_length, 3);
        assert_eq!(chains[0].chain_title(), "Dark Star -> St. Stephen -> The Eleven");
    }

    #[test]
    fn test_multiple_chains_per_show() {
        let tracks = vec![
            make_track("Bertha", 6.0, 40.0),            // standalone
            make_track("Scarlet Begonias ->", 8.0, 60.0),
            make_track("Fire on the Mountain", 12.0, 80.0),
            make_track("Estimated Prophet", 9.0, 55.0),  // standalone
            make_track("Dark Star ->", 20.0, 90.0),
            make_track("Drums ->", 15.0, 30.0),
            make_track("Space ->", 12.0, 85.0),
            make_track("The Other One", 10.0, 75.0),
        ];

        let chains = detect_chains(&tracks, 2);
        assert_eq!(chains.len(), 2);
        assert_eq!(chains[0].chain_length, 2); // Scarlet -> Fire
        assert_eq!(chains[1].chain_length, 4); // Dark Star -> Drums -> Space -> Other One
    }

    #[test]
    fn test_min_length_filter() {
        let tracks = vec![
            make_track("Scarlet Begonias ->", 8.0, 60.0),
            make_track("Fire on the Mountain", 12.0, 80.0),
        ];

        let chains = detect_chains(&tracks, 3);
        assert!(chains.is_empty()); // 2-song chain doesn't meet min_length=3
    }

    #[test]
    fn test_no_chains() {
        let tracks = vec![
            make_track("Bertha", 6.0, 40.0),
            make_track("Sugaree", 10.0, 50.0),
            make_track("Tennessee Jed", 8.0, 45.0),
        ];

        let chains = detect_chains(&tracks, 2);
        assert!(chains.is_empty());
    }

    #[test]
    fn test_filter_by_song() {
        let chains = vec![
            ChainScore {
                date: "1977-05-08".into(),
                songs: vec!["Scarlet Begonias".into(), "Fire on the Mountain".into()],
                chain_length: 2, duration_min: 20.0,
                energy: 50.0, intensity: 50.0, groove: 50.0, improvisation: 50.0,
                tightness: 50.0, build_quality: 50.0, exploratory: 50.0,
                transcendence: 70.0, valence: 50.0, arousal: 50.0,
            },
            ChainScore {
                date: "1977-05-08".into(),
                songs: vec!["Dark Star".into(), "The Eleven".into()],
                chain_length: 2, duration_min: 35.0,
                energy: 50.0, intensity: 50.0, groove: 50.0, improvisation: 50.0,
                tightness: 50.0, build_quality: 50.0, exploratory: 50.0,
                transcendence: 90.0, valence: 50.0, arousal: 50.0,
            },
        ];

        let filtered = filter_and_sort_chains(chains, None, Some("dark star"), "transcendence_score", 10);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].songs[0], "Dark Star");
    }

    #[test]
    fn test_duration_weighted_averaging() {
        // 10 min track at 100, 10 min track at 0 — should average to 50
        let tracks = vec![
            make_track("A ->", 10.0, 100.0),
            make_track("B", 10.0, 0.0),
        ];
        let chains = detect_chains(&tracks, 2);
        assert!((chains[0].transcendence - 50.0).abs() < 0.01);

        // Unequal weights: 30 min at 100, 10 min at 0 — should be 75
        let tracks2 = vec![
            make_track("A ->", 30.0, 100.0),
            make_track("B", 10.0, 0.0),
        ];
        let chains2 = detect_chains(&tracks2, 2);
        assert!((chains2[0].transcendence - 75.0).abs() < 0.01);
    }
}
