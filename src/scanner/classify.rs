/// Classify a track's recording type based on metadata heuristics.
///
/// Returns one of: "live", "studio", "live_album", "unknown".
/// Tiered: first match wins (parsed_date → archive patterns → album tags → fallback).
pub fn classify_recording_type(
    file_path: &str,
    parsed_date: Option<&str>,
    album: Option<&str>,
) -> &'static str {
    // Tier 1: Has a parsed date → archive.org bootleg
    if parsed_date.is_some() {
        return "live";
    }

    // Tier 2: Path contains archive.org naming signatures
    let path_lower = file_path.to_lowercase();
    const ARCHIVE_SIGS: &[&str] = &[
        ".sbeok.", ".sbd.", ".aud.", ".matrix.", ".flac16", ".shnf",
    ];
    if ARCHIVE_SIGS.iter().any(|sig| path_lower.contains(sig)) {
        return "live";
    }

    // Tier 3: Album tag matches known official live album series
    if let Some(alb) = album {
        let alb_lower = alb.to_lowercase();

        const LIVE_ALBUM_SERIES: &[&str] = &[
            "dick's picks",
            "dicks picks",
            "dave's picks",
            "daves picks",
            "from the vault",
            "road trips",
            "without a net",
            "reckoning",
            "dead set",
            "livephish",
            "live phish",
        ];
        if LIVE_ALBUM_SERIES
            .iter()
            .any(|series| alb_lower.contains(series))
        {
            return "live_album";
        }

        // Tier 4: Album contains "live" → generic live recording
        if alb_lower.contains("live") {
            return "live";
        }

        // Tier 5: Has a non-empty album tag → studio
        if !alb.trim().is_empty() {
            return "studio";
        }
    }

    // Tier 6: No metadata
    "unknown"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tier1_parsed_date_is_live() {
        assert_eq!(
            classify_recording_type("/music/gd/gd71-04-18d2t03.shn", Some("1971-04-18"), None),
            "live"
        );
    }

    #[test]
    fn tier2_archive_signatures_are_live() {
        // .sbeok. pattern
        assert_eq!(
            classify_recording_type(
                "/music/gd/gd75-03-xx.sbeok.shnf/track01.shn",
                None,
                None
            ),
            "live"
        );
        // .sbd. pattern
        assert_eq!(
            classify_recording_type("/music/gd/gd83-04-09.sbd.miller.27703/01.shn", None, None),
            "live"
        );
        // .aud. pattern
        assert_eq!(
            classify_recording_type("/music/gd/gd72-05-26.aud.bertha/01.flac", None, None),
            "live"
        );
        // .matrix. pattern
        assert_eq!(
            classify_recording_type("/music/gd/gd77-05-08.matrix.flac16/01.flac", None, None),
            "live"
        );
    }

    #[test]
    fn tier3_official_live_albums() {
        assert_eq!(
            classify_recording_type("/music/gd/vault.flac", None, Some("Two From The Vault")),
            "live_album"
        );
        assert_eq!(
            classify_recording_type("/music/gd/wn.flac", None, Some("Without a Net")),
            "live_album"
        );
        assert_eq!(
            classify_recording_type("/music/gd/dp01.flac", None, Some("Dick's Picks Vol. 1")),
            "live_album"
        );
        assert_eq!(
            classify_recording_type(
                "/music/phish/lp.flac",
                None,
                Some("LivePhish Vol. 04 - 6/14/00")
            ),
            "live_album"
        );
        assert_eq!(
            classify_recording_type("/music/gd/rt.flac", None, Some("Road Trips Vol. 1 No. 1")),
            "live_album"
        );
    }

    #[test]
    fn tier4_album_contains_live() {
        assert_eq!(
            classify_recording_type(
                "/music/bts/rkcndy.flac",
                None,
                Some("Live at RKCNDY, Seattle")
            ),
            "live"
        );
    }

    #[test]
    fn tier5_album_tag_means_studio() {
        assert_eq!(
            classify_recording_type(
                "/music/gd/ab.flac",
                None,
                Some("American Beauty")
            ),
            "studio"
        );
        assert_eq!(
            classify_recording_type(
                "/music/bts/tnwwl.flac",
                None,
                Some("There's Nothing Wrong With Love")
            ),
            "studio"
        );
    }

    #[test]
    fn tier6_no_metadata_is_unknown() {
        assert_eq!(
            classify_recording_type("/music/misc/track01.mp3", None, None),
            "unknown"
        );
    }

    #[test]
    fn tier5_empty_album_is_unknown() {
        assert_eq!(
            classify_recording_type("/music/misc/track01.mp3", None, Some("")),
            "unknown"
        );
        assert_eq!(
            classify_recording_type("/music/misc/track01.mp3", None, Some("  ")),
            "unknown"
        );
    }

    #[test]
    fn tier1_beats_tier5() {
        // Even with a studio album tag, a parsed date wins
        assert_eq!(
            classify_recording_type(
                "/music/gd/ab.flac",
                Some("1970-11-01"),
                Some("American Beauty")
            ),
            "live"
        );
    }
}
