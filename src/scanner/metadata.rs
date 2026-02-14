use lofty::file::TaggedFileExt;
use lofty::prelude::*;
use std::path::Path;

/// Tags extracted from audio file metadata.
pub struct TagInfo {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub date: Option<String>,
    pub track_number: Option<i32>,
    pub disc_number: Option<i32>,
    pub venue: Option<String>,
    pub comment: Option<String>,
    pub duration_secs: Option<f64>,
}

/// Read tags from an audio file. Returns empty tags on failure (e.g., SHN files).
pub fn read_tags(path: &Path) -> TagInfo {
    let empty = TagInfo {
        title: None,
        artist: None,
        album: None,
        date: None,
        track_number: None,
        disc_number: None,
        venue: None,
        comment: None,
        duration_secs: None,
    };

    let tagged_file = match lofty::read_from_path(path) {
        Ok(f) => f,
        Err(e) => {
            log::debug!("Could not read tags from {}: {}", path.display(), e);
            return empty;
        }
    };

    // Try primary tag, then fall back
    let tag = tagged_file
        .primary_tag()
        .or_else(|| tagged_file.first_tag());

    let tag = match tag {
        Some(t) => t,
        None => return empty,
    };

    let duration_secs = {
        let props = tagged_file.properties();
        let dur = props.duration();
        let secs = dur.as_secs_f64();
        if secs > 0.0 { Some(secs) } else { None }
    };

    // Extract venue from comment field (common jam band convention)
    let comment = tag.get_string(&ItemKey::Comment).map(|s| s.to_string());
    let venue = tag
        .get_string(&ItemKey::ContentGroup)
        .or_else(|| tag.get_string(&ItemKey::EncoderSettings))
        .map(|s| s.to_string());

    TagInfo {
        title: tag.title().map(|s| s.to_string()),
        artist: tag.artist().map(|s| s.to_string()),
        album: tag.album().map(|s| s.to_string()),
        date: tag.year().map(|y| y.to_string()).or_else(|| {
            tag.get_string(&ItemKey::RecordingDate).map(|s| s.to_string())
        }),
        track_number: tag.track().map(|t| t as i32),
        disc_number: tag.disk().map(|d| d as i32),
        venue,
        comment,
        duration_secs,
    }
}
