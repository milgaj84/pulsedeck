/// Parse the `StreamTitle` field from an ICY metadata string.
pub(super) fn parse_stream_title(meta: &str) -> Option<String> {
    let key = "StreamTitle='";
    if let Some(start_idx) = meta.find(key) {
        let value_start = start_idx + key.len();
        if let Some(end_idx) = meta[value_start..].find("';") {
            let title = &meta[value_start..value_start + end_idx];
            return Some(clean_title(title));
        }
    }
    None
}

/// Tidy a "now playing" title from a stream.
///
/// Some stations send their own bookkeeping instead of a track: iHeart wraps the
/// artist in `text="..."` with a `song_spot` flag (and sends station promos the
/// same way), and others send placeholders such as `_` or `9999999 - 9999999`.
/// Those would show up as songs, in the history and in desktop notifications,
/// so they become an empty title, which means "no track".
pub(super) fn clean_title(raw: &str) -> String {
    let title = raw.trim();
    let title = match iheart_title(title) {
        Some(Some(cleaned)) => cleaned,
        Some(None) => return String::new(),
        None => title.to_string(),
    };
    if is_placeholder(&title) {
        String::new()
    } else {
        title
    }
}

/// An iHeart-style title. Two shapes are in use:
///
/// - `Artist - text="Song" song_spot="M" MediaBaseId=...`, where `song_spot` is
///   `M` for music and `T` for station promos and podcast ads; a blank artist
///   (`- text="Spot Block End" ...`) is an ad-block marker;
/// - `title="Song",artist="Artist",url="song_spot="F" ...`.
///
/// `None` if it is neither; `Some(None)` if it is not a track.
fn iheart_title(title: &str) -> Option<Option<String>> {
    if title.starts_with("title=\"") {
        let song = attribute(title, "title")?.trim();
        let artist = attribute(title, "artist").unwrap_or("").trim();
        return Some(match (artist.is_empty(), song.is_empty()) {
            (_, true) => None,
            (true, false) => Some(song.to_string()),
            (false, false) => Some(format!("{artist} - {song}")),
        });
    }

    const MARKER: &str = "- text=\"";
    let at = title.find(MARKER)?;
    let (artist, rest) = (&title[..at], &title[at + MARKER.len()..]);
    // The dash must stand on its own: the start of the title, or after a space.
    if !(artist.is_empty() || artist.ends_with(' ')) {
        return None;
    }
    let (song, after) = rest.split_once('"')?;
    if attribute(after, "song_spot") == Some("T") {
        return Some(None);
    }
    let (artist, song) = (artist.trim(), song.trim());
    if artist.is_empty() {
        return Some(None); // a marker, not a song: a song has an artist
    }
    Some(Some(if song.is_empty() {
        artist.to_string()
    } else {
        format!("{artist} - {song}")
    }))
}

/// The value of `name="..."` in `text`.
fn attribute<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    let start = text.find(&format!("{name}=\""))? + name.len() + 2;
    let len = text[start..].find('"')?;
    Some(&text[start..start + len])
}

/// Not a track name: only punctuation (`_`, `-`), or nothing but numbers
/// separated by dashes (`9999999 - 9999999`, ids some stations send). A lone
/// number is kept, since it could be a song title.
fn is_placeholder(title: &str) -> bool {
    if title.chars().any(char::is_alphabetic) {
        return false;
    }
    if !title.chars().any(char::is_numeric) {
        return true;
    }
    let parts: Vec<&str> = title.split(" - ").map(str::trim).collect();
    parts.len() >= 2
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iheart_music_titles_read_artist_dash_song() {
        // As sent by real iHeart streams.
        let cases = [
            ("Olivia Rodrigo - text=\"Stupid Song\" song_spot=\"M\" MediaBaseId=\"3233895\" itunesTrackId=\"0\" amgTrackId=\"-1\"", "Olivia Rodrigo - Stupid Song"),
            ("Kendrick Lamar - text=\"Swimming Pools\" song_spot=\"M\" spotInstanceId=\"-1\" length=\"00:04:06\"", "Kendrick Lamar - Swimming Pools"),
            ("Men At Work - text=\"Down Under\" song_spot=\"M\"", "Men At Work - Down Under"),
            // The other shape; its song_spot is F for real songs too.
            ("title=\"Folded\",artist=\"KEHLANI\",url=\"song_spot=\"F\" MediaBaseId=\"0\" itunesTrackId=\"0\"", "KEHLANI - Folded"),
            // No flag at all still counts as music.
            ("Adele - text=\"Hello\"", "Adele - Hello"),
        ];
        for (raw, expected) in cases {
            assert_eq!(clean_title(raw), expected, "{raw}");
        }
        assert_eq!(
            clean_title("Only Artist - text=\"\" song_spot=\"M\""),
            "Only Artist"
        );
        assert_eq!(
            clean_title("title=\"Just A Song\",artist=\"\",url=\"x\""),
            "Just A Song"
        );
    }

    #[test]
    fn iheart_promos_and_ad_markers_are_not_tracks() {
        let not_tracks = [
            "LA's #1 Hit Music Station - text=\"102.7 KIIS FM\" song_spot=\"T\" MediaBaseId=\"0\" itunesTrackId=\"0\"",
            "Pro Tools - text=\"iHeart Podcast Promo 2026 - Suite 305 - 30 BC FINAL\" song_spot=\"T\" spotInstanceId=\"-1\" length=\"00:00:30\"",
            " - text=\"Spot Block End\" amgTrackId=\"9876543\" length=\"00:00:00\"",
            "- text=\"Spot Block End\"",
            " - text=\"\"",
            "title=\"\",artist=\"Someone\",url=\"x\"",
        ];
        for raw in not_tracks {
            assert_eq!(clean_title(raw), "", "{raw}");
        }
    }

    #[test]
    fn placeholders_become_no_track() {
        for junk in [
            "",
            "   ",
            "_",
            "-",
            "...",
            "- _ -",
            "9999999 - 9999999",
            "286855 - 403318",
            "2089 - 2089",
            "12 - 34 - 56",
        ] {
            assert_eq!(clean_title(junk), "", "{junk:?}");
        }
    }

    #[test]
    fn real_titles_are_left_alone() {
        for title in [
            "Prince - 1999",
            "1999",
            "21",
            "99 Luftballons",
            "Track 1",
            "Daft Punk - Around the World",
            "Sigur Rós - Hoppípolla",
            "日本語テスト",
            "Ünder - Ö",
            "Artist - 12 - Live Version",
            "text=\"not iheart\"",
        ] {
            assert_eq!(clean_title(title), title, "{title:?}");
        }
        assert_eq!(clean_title("  spaced - out  "), "spaced - out");
    }

    #[test]
    fn icy_blocks_are_cleaned_when_parsed() {
        assert_eq!(
            parse_stream_title("StreamTitle='_';StreamUrl='';"),
            Some(String::new())
        );
        assert_eq!(
            parse_stream_title("StreamTitle='9999999 - 9999999';"),
            Some(String::new())
        );
        assert_eq!(
            parse_stream_title("StreamTitle='Adele - text=\"Hello\" song_spot=\"M\"';"),
            Some("Adele - Hello".to_string())
        );
    }

    #[test]
    fn test_parse_stream_title() {
        assert_eq!(
            parse_stream_title("StreamTitle='Lazerhawk - King of The Streets';StreamUrl='';"),
            Some("Lazerhawk - King of The Streets".to_string())
        );

        assert_eq!(
            parse_stream_title("StreamTitle='  Kavinsky - Nightcall  ';StreamUrl='';"),
            Some("Kavinsky - Nightcall".to_string())
        );

        assert_eq!(parse_stream_title("StreamUrl='';"), None);
        assert_eq!(parse_stream_title("StreamTitle='';"), Some("".to_string()));
    }

    #[test]
    fn test_parse_stream_title_multiple_pairs_extracts_first() {
        let meta = "StreamTitle='First Title';StreamTitle='Second Title';";
        assert_eq!(parse_stream_title(meta), Some("First Title".to_string()));
    }

    #[test]
    fn test_parse_stream_title_missing_closing_delimiter_returns_none() {
        assert_eq!(parse_stream_title("StreamTitle='some text"), None);
        assert_eq!(parse_stream_title("StreamTitle='no closing quote"), None);
    }

    #[test]
    fn test_parse_stream_title_unicode_preserved() {
        let meta = "StreamTitle='日本語テスト 🎵🎶 café';";
        assert_eq!(
            parse_stream_title(meta),
            Some("日本語テスト 🎵🎶 café".to_string())
        );
    }

    #[test]
    fn test_parse_stream_title_embedded_single_quote() {
        // StreamTitle='Rock n' Roll'; — the '; after "Roll" is the first '; sequence,
        // so the extracted value includes the embedded single quote.
        let meta = "StreamTitle='Rock n' Roll';";
        assert_eq!(parse_stream_title(meta), Some("Rock n' Roll".to_string()));

        // When the embedded quote IS followed by ';', that terminates the value early.
        let meta2 = "StreamTitle='Rock n';Roll';";
        assert_eq!(parse_stream_title(meta2), Some("Rock n".to_string()));
    }
}
