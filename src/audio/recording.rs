#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RecordingTagContext<'a> {
    pub(super) category: Option<&'a str>,
    pub(super) source_url: Option<&'a str>,
}

/// Inject richer PulseDeck ID3 metadata into completed local recordings.
pub(super) fn inject_id3_tags_with_context(
    filepath: &str,
    title: &str,
    context: RecordingTagContext<'_>,
) -> Result<(), Box<dyn std::error::Error>> {
    use id3::{Tag, TagLike, Version};

    let (artist, track_title) = split_artist_title(title);

    let mut tag = Tag::new();
    tag.set_artist(artist);
    tag.set_title(track_title);
    tag.set_album(album_label(context.source_url));

    if let Some(category) = context.category {
        if !category.trim().is_empty() {
            tag.set_genre(category.trim().to_string());
        }
    }

    tag.write_to_path(filepath, Version::Id3v24)?;
    Ok(())
}

pub(super) fn split_artist_title(title: &str) -> (String, String) {
    if let Some(pos) = title.find(" - ") {
        let artist = title[..pos].trim();
        let track_title = title[pos + 3..].trim();

        if !artist.is_empty() && !track_title.is_empty() {
            return (artist.to_string(), track_title.to_string());
        }
    }

    ("Unknown Artist".to_string(), title.trim().to_string())
}

pub(super) fn album_label(source_url: Option<&str>) -> String {
    match source_url.filter(|url| !url.trim().is_empty()) {
        Some(url) => format!("PulseDeck live capture · {url}"),
        None => "PulseDeck live capture".to_string(),
    }
}

pub(super) fn recording_target_exists(path: &std::path::Path) -> bool {
    path.exists()
}

pub(super) fn duplicate_skip_message(title: &str) -> String {
    format!("Duplicate recording skipped: {title}")
}

/// Replace invalid filesystem characters to protect host OS filesystems
pub(super) fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            other => other,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_artist_title_parses_artist_and_title() {
        assert_eq!(
            split_artist_title("Artist - Song"),
            ("Artist".to_string(), "Song".to_string())
        );
    }

    #[test]
    fn split_artist_title_falls_back_without_separator() {
        assert_eq!(
            split_artist_title("Mystery Transmission"),
            (
                "Unknown Artist".to_string(),
                "Mystery Transmission".to_string()
            )
        );
    }

    #[test]
    fn album_label_includes_source_url_when_available() {
        assert_eq!(
            album_label(Some("https://example.test/stream")),
            "PulseDeck live capture · https://example.test/stream"
        );
    }

    #[test]
    fn duplicate_skip_message_names_track() {
        assert_eq!(
            duplicate_skip_message("Artist - Song"),
            "Duplicate recording skipped: Artist - Song"
        );
    }

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("normal_file.mp3"), "normal_file.mp3");
        assert_eq!(sanitize_filename("artist/song?.mp3"), "artist-song-.mp3");
        assert_eq!(
            sanitize_filename("windows\\invalid:name*char\".mp3"),
            "windows-invalid-name-char-.mp3"
        );
        assert_eq!(sanitize_filename("<tag> | pipe.mp3"), "-tag- - pipe.mp3");
    }
}
