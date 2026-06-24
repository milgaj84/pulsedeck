// Scrobble integration — track metadata parsing and scrobble event logic.

pub mod tracker;

/// Errors that can occur when submitting scrobble data to an external service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScrobbleError {
    Network(String),
    Auth(String),
}

/// Trait for scrobble submission (infrastructure boundary).
pub trait ScrobbleClient: Send + Sync {
    fn now_playing(&self, meta: &TrackMetadata) -> Result<(), ScrobbleError>;
    fn scrobble(&self, meta: &TrackMetadata, timestamp: u64) -> Result<(), ScrobbleError>;
}

/// Parsed track metadata from ICY StreamTitle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackMetadata {
    pub artist: String,
    pub title: String,
}

/// Parse "Artist - Title" from a raw StreamTitle string.
/// Splits on the first occurrence of " - ". If no delimiter is found,
/// artist is empty and title is the full input string.
pub fn parse_track_metadata(stream_title: &str) -> TrackMetadata {
    match stream_title.find(" - ") {
        Some(pos) => TrackMetadata {
            artist: stream_title[..pos].to_string(),
            title: stream_title[pos + 3..].to_string(),
        },
        None => TrackMetadata {
            artist: String::new(),
            title: stream_title.to_string(),
        },
    }
}

/// Format TrackMetadata back to "artist - title" form.
/// When artist is non-empty, produces "artist - title".
/// When artist is empty, produces just the title.
pub fn format_track_metadata(meta: &TrackMetadata) -> String {
    if meta.artist.is_empty() {
        meta.title.clone()
    } else {
        format!("{} - {}", meta.artist, meta.title)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_known_artist_title_split() {
        let result = parse_track_metadata("Radiohead - Creep");
        assert_eq!(result.artist, "Radiohead");
        assert_eq!(result.title, "Creep");
    }

    #[test]
    fn test_parse_splits_on_first_delimiter_only() {
        let result = parse_track_metadata("AC/DC - Back in Black - Remastered");
        assert_eq!(result.artist, "AC/DC");
        assert_eq!(result.title, "Back in Black - Remastered");
    }

    #[test]
    fn test_parse_no_delimiter_artist_empty() {
        let result = parse_track_metadata("Just A Title");
        assert_eq!(result.artist, "");
        assert_eq!(result.title, "Just A Title");
    }

    #[test]
    fn test_parse_empty_input() {
        let result = parse_track_metadata("");
        assert_eq!(result.artist, "");
        assert_eq!(result.title, "");
    }

    #[test]
    fn test_parse_whitespace_only() {
        let result = parse_track_metadata("   ");
        assert_eq!(result.artist, "");
        assert_eq!(result.title, "   ");
    }

    #[test]
    fn test_parse_delimiter_at_start() {
        let result = parse_track_metadata(" - Song Title");
        assert_eq!(result.artist, "");
        assert_eq!(result.title, "Song Title");
    }

    #[test]
    fn test_parse_delimiter_at_end() {
        let result = parse_track_metadata("Artist - ");
        assert_eq!(result.artist, "Artist");
        assert_eq!(result.title, "");
    }

    #[test]
    fn test_format_both_non_empty() {
        let meta = TrackMetadata {
            artist: "Daft Punk".to_string(),
            title: "Around The World".to_string(),
        };
        assert_eq!(format_track_metadata(&meta), "Daft Punk - Around The World");
    }

    #[test]
    fn test_format_empty_artist_returns_title_only() {
        let meta = TrackMetadata {
            artist: String::new(),
            title: "Some Song".to_string(),
        };
        assert_eq!(format_track_metadata(&meta), "Some Song");
    }

    #[test]
    fn test_format_empty_both() {
        let meta = TrackMetadata {
            artist: String::new(),
            title: String::new(),
        };
        assert_eq!(format_track_metadata(&meta), "");
    }

    #[test]
    fn test_roundtrip_parse_format_with_artist() {
        let original = "The Beatles - Hey Jude";
        let parsed = parse_track_metadata(original);
        let formatted = format_track_metadata(&parsed);
        assert_eq!(formatted, original);
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    // Feature: v080-features, Property 4: TrackMetadata parse/format round-trip
    // **Validates: Requirements 3.5, 3.8**
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn roundtrip_format_then_parse(
            artist in "[^-]{1,30}".prop_filter(
                "artist must not contain ' - '",
                |s| !s.contains(" - ") && !s.is_empty()
            ),
            title in "[^-]{1,30}".prop_filter(
                "title must not contain ' - '",
                |s| !s.contains(" - ") && !s.is_empty()
            ),
        ) {
            let original = TrackMetadata {
                artist: artist.clone(),
                title: title.clone(),
            };
            let formatted = format_track_metadata(&original);
            let parsed = parse_track_metadata(&formatted);
            prop_assert_eq!(parsed, original);
        }
    }
}
