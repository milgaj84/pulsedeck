/// Inject ID3 metadata frames into completed local recordings
pub(super) fn inject_id3_tags(
    filepath: &str,
    title: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use id3::{Tag, TagLike, Version};

    let mut artist = "Unknown Artist".to_string();
    let mut track_title = title.to_string();

    if let Some(pos) = title.find(" - ") {
        artist = title[..pos].trim().to_string();
        track_title = title[pos + 3..].trim().to_string();
    }

    let mut tag = Tag::new();
    tag.set_artist(artist);
    tag.set_title(track_title);
    tag.set_album("PulseDeck live capturing");

    tag.write_to_path(filepath, Version::Id3v24)?;
    Ok(())
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
