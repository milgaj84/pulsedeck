//! HLS playlist parsing (RFC 8216). Pure: no I/O, no clock.
//!
//! Only what PulseDeck needs for audio-only radio streams is understood.
//! Encrypted streams, fMP4/CMAF segments and video-only variants are rejected
//! with an error whose text says "not supported", so the app can stop
//! reconnecting instead of retrying something that can never work.

use reqwest::Url;
use std::fmt;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HlsError {
    Encrypted,
    Fmp4,
    VideoOnly,
    /// Audio variants exist but none uses a codec we can decode.
    UnsupportedCodec(String),
    NoAudioVariant,
    Malformed(&'static str),
}

impl fmt::Display for HlsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Encrypted => write!(f, "HLS: encrypted streams are not supported"),
            Self::Fmp4 => write!(f, "HLS: fMP4/CMAF segments are not supported"),
            Self::VideoOnly => write!(f, "HLS: video-only streams are not supported"),
            Self::UnsupportedCodec(codec) => {
                write!(f, "HLS: {codec} audio is not supported")
            }
            Self::NoAudioVariant => write!(f, "HLS: no audio stream found, not supported"),
            Self::Malformed(why) => write!(f, "HLS: malformed playlist ({why})"),
        }
    }
}

impl std::error::Error for HlsError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Variant {
    pub uri: Url,
    pub bandwidth: u64,
    pub codecs: Option<String>,
}

/// An `#EXT-X-MEDIA:TYPE=AUDIO` rendition that has its own playlist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Rendition {
    pub uri: Url,
    pub default: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Master {
    pub variants: Vec<Variant>,
    pub renditions: Vec<Rendition>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Segment {
    /// Media sequence number (`EXT-X-MEDIA-SEQUENCE` + index).
    pub seq: u64,
    pub uri: Url,
    /// A discontinuity precedes this segment.
    pub discontinuity: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MediaPlaylist {
    pub target_duration: Duration,
    pub end_list: bool,
    pub segments: Vec<Segment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Playlist {
    Master(Master),
    Media(MediaPlaylist),
}

/// Parse an attribute list such as `BANDWIDTH=64000,CODECS="mp4a.40.2"`.
/// Keys are upper-cased; quotes around values are removed.
fn parse_attributes(list: &str) -> Vec<(String, String)> {
    let mut attrs = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;

    let mut flush = |current: &mut String| {
        if let Some((key, value)) = current.split_once('=') {
            attrs.push((
                key.trim().to_ascii_uppercase(),
                value.trim().trim_matches('"').to_string(),
            ));
        }
        current.clear();
    };

    for ch in list.chars() {
        match ch {
            '"' => {
                in_quotes = !in_quotes;
                current.push(ch);
            }
            ',' if !in_quotes => flush(&mut current),
            _ => current.push(ch),
        }
    }
    flush(&mut current);
    attrs
}

fn attr<'a>(attrs: &'a [(String, String)], key: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.as_str())
}

fn resolve(base: &Url, uri: &str) -> Result<Url, HlsError> {
    base.join(uri)
        .map_err(|_| HlsError::Malformed("invalid URI"))
}

fn is_fmp4_uri(uri: &Url) -> bool {
    let path = uri.path().to_ascii_lowercase();
    [".mp4", ".m4s", ".m4a", ".cmfa", ".cmfv"]
        .iter()
        .any(|ext| path.ends_with(ext))
}

/// Parse a playlist body fetched from `base` (the final URL after redirects).
pub(crate) fn parse(text: &str, base: &Url) -> Result<Playlist, HlsError> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut lines = text.lines().map(str::trim).filter(|line| !line.is_empty());

    if lines.next() != Some("#EXTM3U") {
        return Err(HlsError::Malformed("missing #EXTM3U"));
    }
    let lines: Vec<&str> = lines.collect();

    if lines
        .iter()
        .any(|line| line.starts_with("#EXT-X-STREAM-INF"))
    {
        parse_master(&lines, base).map(Playlist::Master)
    } else {
        parse_media(&lines, base).map(Playlist::Media)
    }
}

fn parse_master(lines: &[&str], base: &Url) -> Result<Master, HlsError> {
    let mut variants = Vec::new();
    let mut renditions = Vec::new();
    let mut pending: Option<Vec<(String, String)>> = None;

    for line in lines {
        if let Some(rest) = line.strip_prefix("#EXT-X-STREAM-INF:") {
            pending = Some(parse_attributes(rest));
        } else if let Some(rest) = line.strip_prefix("#EXT-X-MEDIA:") {
            let attrs = parse_attributes(rest);
            if attr(&attrs, "TYPE") == Some("AUDIO") {
                if let Some(uri) = attr(&attrs, "URI") {
                    renditions.push(Rendition {
                        uri: resolve(base, uri)?,
                        default: attr(&attrs, "DEFAULT") == Some("YES"),
                    });
                }
            }
        } else if !line.starts_with('#') {
            if let Some(attrs) = pending.take() {
                variants.push(Variant {
                    uri: resolve(base, line)?,
                    bandwidth: attr(&attrs, "BANDWIDTH")
                        .and_then(|value| value.parse().ok())
                        .unwrap_or(u64::MAX),
                    codecs: attr(&attrs, "CODECS").map(str::to_string),
                });
            }
        }
    }

    Ok(Master {
        variants,
        renditions,
    })
}

fn parse_media(lines: &[&str], base: &Url) -> Result<MediaPlaylist, HlsError> {
    let mut target_duration = None;
    let mut media_sequence = 0u64;
    let mut end_list = false;

    // First pass: header tags (their position relative to segments is irrelevant
    // for the values we use).
    for line in lines {
        if let Some(value) = line.strip_prefix("#EXT-X-TARGETDURATION:") {
            target_duration = value.trim().parse::<u64>().ok();
        } else if let Some(value) = line.strip_prefix("#EXT-X-MEDIA-SEQUENCE:") {
            media_sequence = value.trim().parse().unwrap_or(0);
        } else if *line == "#EXT-X-ENDLIST" {
            end_list = true;
        } else if let Some(value) = line.strip_prefix("#EXT-X-KEY:") {
            let attrs = parse_attributes(value);
            if attr(&attrs, "METHOD").is_some_and(|method| method != "NONE") {
                return Err(HlsError::Encrypted);
            }
        } else if line.starts_with("#EXT-X-MAP:") {
            return Err(HlsError::Fmp4);
        }
    }

    let target_duration = target_duration
        .filter(|seconds| *seconds > 0)
        .ok_or(HlsError::Malformed("missing #EXT-X-TARGETDURATION"))?;

    let mut segments = Vec::new();
    let mut discontinuity = false;

    for line in lines {
        if *line == "#EXT-X-DISCONTINUITY" {
            discontinuity = true;
        } else if !line.starts_with('#') {
            let uri = resolve(base, line)?;
            if is_fmp4_uri(&uri) {
                return Err(HlsError::Fmp4);
            }
            segments.push(Segment {
                seq: media_sequence + segments.len() as u64,
                uri,
                discontinuity: std::mem::take(&mut discontinuity),
            });
        }
    }

    Ok(MediaPlaylist {
        target_duration: Duration::from_secs(target_duration),
        end_list,
        segments,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CodecClass {
    /// AAC-LC or MP3: decodes correctly.
    Good,
    /// HE-AAC: Symphonia decodes only the LC core (half rate, muffled).
    HeAac,
    Video,
    /// Audio we cannot decode (AC-3, E-AC-3, Opus in mp4, ...).
    Other,
}

fn classify_codecs(codecs: &str) -> CodecClass {
    let mut class = CodecClass::Good;
    for codec in codecs.split(',').map(|c| c.trim().to_ascii_lowercase()) {
        let this = if ["avc", "hvc", "hev", "vp0", "av01", "dvh"]
            .iter()
            .any(|prefix| codec.starts_with(prefix))
        {
            CodecClass::Video
        } else if codec == "mp3"
            || codec == "mp4a.40.34"
            || codec == "mp4a.6b"
            || codec == "mp4a.69"
        {
            CodecClass::Good
        } else if codec == "mp4a.40.5" || codec == "mp4a.40.29" {
            CodecClass::HeAac
        } else if codec.starts_with("mp4a.40.") {
            CodecClass::Good
        } else {
            CodecClass::Other
        };
        class = match (class, this) {
            (CodecClass::Video, _) | (_, CodecClass::Video) => CodecClass::Video,
            (CodecClass::Other, _) | (_, CodecClass::Other) => CodecClass::Other,
            (CodecClass::HeAac, _) | (_, CodecClass::HeAac) => CodecClass::HeAac,
            _ => CodecClass::Good,
        };
    }
    class
}

/// Pick the playlist to play from a master playlist.
///
/// An audio rendition with its own URI wins (default first). Otherwise the
/// lowest-bandwidth audio-only variant is chosen, preferring AAC-LC/MP3 over
/// HE-AAC. A variant without a `CODECS` attribute is accepted as a last resort
/// and the demuxer decides what it really contains.
pub(crate) fn select_variant(master: &Master) -> Result<Url, HlsError> {
    if let Some(rendition) = master
        .renditions
        .iter()
        .find(|rendition| rendition.default)
        .or_else(|| master.renditions.first())
    {
        return Ok(rendition.uri.clone());
    }

    if master.variants.is_empty() {
        return Err(HlsError::NoAudioVariant);
    }

    let classified: Vec<(&Variant, Option<CodecClass>)> = master
        .variants
        .iter()
        .map(|variant| (variant, variant.codecs.as_deref().map(classify_codecs)))
        .collect();

    let lowest = |class: Option<CodecClass>| {
        classified
            .iter()
            .filter(|(_, c)| *c == class)
            .min_by_key(|(variant, _)| variant.bandwidth)
            .map(|(variant, _)| variant.uri.clone())
    };

    if let Some(uri) = lowest(Some(CodecClass::Good))
        .or_else(|| lowest(Some(CodecClass::HeAac)))
        .or_else(|| lowest(None))
    {
        return Ok(uri);
    }

    if classified
        .iter()
        .all(|(_, class)| *class == Some(CodecClass::Video))
    {
        return Err(HlsError::VideoOnly);
    }

    let codec = classified
        .iter()
        .find(|(_, class)| *class == Some(CodecClass::Other))
        .and_then(|(variant, _)| variant.codecs.clone())
        .unwrap_or_else(|| "unknown".to_string());
    Err(HlsError::UnsupportedCodec(codec))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Url {
        Url::parse("https://radio.example/live/index.m3u8").unwrap()
    }

    fn media(text: &str) -> MediaPlaylist {
        match parse(text, &base()).unwrap() {
            Playlist::Media(media) => media,
            other => panic!("expected media playlist, got {other:?}"),
        }
    }

    fn master(text: &str) -> Master {
        match parse(text, &base()).unwrap() {
            Playlist::Master(master) => master,
            other => panic!("expected master playlist, got {other:?}"),
        }
    }

    const LIVE: &str = "#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-TARGETDURATION:6\n#EXT-X-MEDIA-SEQUENCE:100\n#EXTINF:6.0,\nseg100.ts\n#EXTINF:6.0,\nseg101.ts\n";

    #[test]
    fn parses_a_live_media_playlist() {
        let playlist = media(LIVE);

        assert_eq!(playlist.target_duration, Duration::from_secs(6));
        assert!(!playlist.end_list);
        assert_eq!(playlist.segments.len(), 2);
        assert_eq!(playlist.segments[0].seq, 100);
        assert_eq!(playlist.segments[1].seq, 101);
        assert_eq!(
            playlist.segments[0].uri.as_str(),
            "https://radio.example/live/seg100.ts"
        );
    }

    #[test]
    fn endlist_marks_a_vod_playlist() {
        let playlist = media(&format!("{LIVE}#EXT-X-ENDLIST\n"));
        assert!(playlist.end_list);
    }

    #[test]
    fn media_sequence_defaults_to_zero() {
        let playlist = media("#EXTM3U\n#EXT-X-TARGETDURATION:4\n#EXTINF:4,\na.aac\n");
        assert_eq!(playlist.segments[0].seq, 0);
    }

    #[test]
    fn resolves_relative_absolute_and_parent_uris() {
        let playlist = media(
            "#EXTM3U\n#EXT-X-TARGETDURATION:4\n#EXTINF:4,\n../audio/a.aac\n#EXTINF:4,\nhttps://cdn.example/b.aac\n#EXTINF:4,\n/root/c.aac\n#EXTINF:4,\nd.aac?token=1\n",
        );
        let uris: Vec<&str> = playlist.segments.iter().map(|s| s.uri.as_str()).collect();

        assert_eq!(
            uris,
            [
                "https://radio.example/audio/a.aac",
                "https://cdn.example/b.aac",
                "https://radio.example/root/c.aac",
                "https://radio.example/live/d.aac?token=1",
            ]
        );
    }

    #[test]
    fn discontinuity_applies_to_the_next_segment_only() {
        let playlist = media(
            "#EXTM3U\n#EXT-X-TARGETDURATION:4\n#EXTINF:4,\na.ts\n#EXT-X-DISCONTINUITY\n#EXTINF:4,\nb.ts\n#EXTINF:4,\nc.ts\n",
        );
        let flags: Vec<bool> = playlist.segments.iter().map(|s| s.discontinuity).collect();
        assert_eq!(flags, [false, true, false]);
    }

    #[test]
    fn handles_crlf_bom_and_blank_lines() {
        let text = "\u{feff}#EXTM3U\r\n\r\n#EXT-X-TARGETDURATION:5\r\n#EXTINF:5,\r\n\r\nseg.ts\r\n";
        let playlist = media(text);
        assert_eq!(playlist.segments.len(), 1);
        assert_eq!(playlist.segments[0].uri.path(), "/live/seg.ts");
    }

    #[test]
    fn rejects_text_without_extm3u() {
        assert_eq!(
            parse("<html>nope</html>", &base()),
            Err(HlsError::Malformed("missing #EXTM3U"))
        );
        assert_eq!(
            parse("", &base()),
            Err(HlsError::Malformed("missing #EXTM3U"))
        );
    }

    #[test]
    fn media_playlist_requires_target_duration() {
        assert_eq!(
            parse("#EXTM3U\n#EXTINF:4,\na.ts\n", &base()),
            Err(HlsError::Malformed("missing #EXT-X-TARGETDURATION"))
        );
        assert!(parse(
            "#EXTM3U\n#EXT-X-TARGETDURATION:0\n#EXTINF:4,\na.ts\n",
            &base()
        )
        .is_err());
    }

    #[test]
    fn encrypted_playlists_are_rejected_but_method_none_is_fine() {
        let encrypted = "#EXTM3U\n#EXT-X-TARGETDURATION:4\n#EXT-X-KEY:METHOD=AES-128,URI=\"k.key\"\n#EXTINF:4,\na.ts\n";
        assert_eq!(parse(encrypted, &base()), Err(HlsError::Encrypted));

        let sample_aes = encrypted.replace("AES-128", "SAMPLE-AES");
        assert_eq!(parse(&sample_aes, &base()), Err(HlsError::Encrypted));

        let none = "#EXTM3U\n#EXT-X-TARGETDURATION:4\n#EXT-X-KEY:METHOD=NONE\n#EXTINF:4,\na.ts\n";
        assert_eq!(media(none).segments.len(), 1);
    }

    #[test]
    fn fmp4_is_rejected_via_map_or_segment_extension() {
        let with_map =
            "#EXTM3U\n#EXT-X-TARGETDURATION:4\n#EXT-X-MAP:URI=\"init.mp4\"\n#EXTINF:4,\na.m4s\n";
        assert_eq!(parse(with_map, &base()), Err(HlsError::Fmp4));

        let by_extension = "#EXTM3U\n#EXT-X-TARGETDURATION:4\n#EXTINF:4,\nA.M4S\n";
        assert_eq!(parse(by_extension, &base()), Err(HlsError::Fmp4));
    }

    #[test]
    fn error_text_says_not_supported_where_retrying_is_pointless() {
        for error in [
            HlsError::Encrypted,
            HlsError::Fmp4,
            HlsError::VideoOnly,
            HlsError::NoAudioVariant,
            HlsError::UnsupportedCodec("ac-3".to_string()),
        ] {
            assert!(error.to_string().contains("not supported"), "{error}");
            assert!(error.to_string().starts_with("HLS:"));
        }
        assert!(!HlsError::Malformed("x")
            .to_string()
            .contains("not supported"));
    }

    #[test]
    fn attribute_parser_keeps_commas_inside_quotes() {
        let attrs = parse_attributes(r#"BANDWIDTH=64000,CODECS="mp4a.40.2,avc1.4d401f",name=x"#);

        assert_eq!(attr(&attrs, "BANDWIDTH"), Some("64000"));
        assert_eq!(attr(&attrs, "CODECS"), Some("mp4a.40.2,avc1.4d401f"));
        assert_eq!(attr(&attrs, "NAME"), Some("x"));
        assert_eq!(attr(&attrs, "MISSING"), None);
    }

    const MASTER: &str = "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=128000,CODECS=\"mp4a.40.2\"\nhi.m3u8\n#EXT-X-STREAM-INF:BANDWIDTH=64000,CODECS=\"mp4a.40.2\"\nlo.m3u8\n";

    #[test]
    fn master_picks_the_lowest_bandwidth_audio_variant() {
        let uri = select_variant(&master(MASTER)).unwrap();
        assert_eq!(uri.as_str(), "https://radio.example/live/lo.m3u8");
    }

    #[test]
    fn master_prefers_aac_lc_over_he_aac_even_at_higher_bandwidth() {
        let text = "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=32000,CODECS=\"mp4a.40.5\"\nhe.m3u8\n#EXT-X-STREAM-INF:BANDWIDTH=96000,CODECS=\"mp4a.40.2\"\nlc.m3u8\n";
        let uri = select_variant(&master(text)).unwrap();
        assert!(uri.as_str().ends_with("lc.m3u8"));
    }

    #[test]
    fn master_falls_back_to_he_aac_when_it_is_all_there_is() {
        let text = "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=32000,CODECS=\"mp4a.40.5\"\nhe.m3u8\n";
        let uri = select_variant(&master(text)).unwrap();
        assert!(uri.as_str().ends_with("he.m3u8"));
    }

    #[test]
    fn master_prefers_an_audio_rendition_with_a_uri() {
        let text = "#EXTM3U\n#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"a\",NAME=\"en\",URI=\"en.m3u8\"\n#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"a\",NAME=\"fr\",DEFAULT=YES,URI=\"fr.m3u8\"\n#EXT-X-STREAM-INF:BANDWIDTH=200000,CODECS=\"avc1.4d401f,mp4a.40.2\",AUDIO=\"a\"\nvideo.m3u8\n";
        let uri = select_variant(&master(text)).unwrap();
        assert!(uri.as_str().ends_with("fr.m3u8"));
    }

    #[test]
    fn master_ignores_audio_renditions_without_a_uri() {
        let text = "#EXTM3U\n#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"a\",NAME=\"main\"\n#EXT-X-STREAM-INF:BANDWIDTH=64000,CODECS=\"mp4a.40.2\"\naudio.m3u8\n";
        let uri = select_variant(&master(text)).unwrap();
        assert!(uri.as_str().ends_with("audio.m3u8"));
    }

    #[test]
    fn master_with_only_video_variants_is_rejected() {
        let text = "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=900000,CODECS=\"avc1.4d401f,mp4a.40.2\"\nv.m3u8\n";
        assert_eq!(select_variant(&master(text)), Err(HlsError::VideoOnly));
    }

    #[test]
    fn master_with_only_undecodable_audio_names_the_codec() {
        let text = "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=64000,CODECS=\"ac-3\"\na.m3u8\n";
        assert_eq!(
            select_variant(&master(text)),
            Err(HlsError::UnsupportedCodec("ac-3".to_string()))
        );
    }

    #[test]
    fn master_variant_without_codecs_is_a_last_resort() {
        let text = "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=64000\nplain.m3u8\n#EXT-X-STREAM-INF:BANDWIDTH=900000,CODECS=\"avc1.4d401f\"\nvideo.m3u8\n";
        let uri = select_variant(&master(text)).unwrap();
        assert!(uri.as_str().ends_with("plain.m3u8"));
    }

    #[test]
    fn master_without_variants_has_no_audio() {
        let master = Master {
            variants: vec![],
            renditions: vec![],
        };
        assert_eq!(select_variant(&master), Err(HlsError::NoAudioVariant));
    }

    #[test]
    fn codec_classes_cover_common_radio_strings() {
        assert_eq!(classify_codecs("mp4a.40.2"), CodecClass::Good);
        assert_eq!(classify_codecs("mp3"), CodecClass::Good);
        assert_eq!(classify_codecs("mp4a.40.34"), CodecClass::Good);
        assert_eq!(classify_codecs("mp4a.40.5"), CodecClass::HeAac);
        assert_eq!(classify_codecs("mp4a.40.29"), CodecClass::HeAac);
        assert_eq!(classify_codecs("avc1.4d401f,mp4a.40.2"), CodecClass::Video);
        assert_eq!(classify_codecs("ec-3"), CodecClass::Other);
        assert_eq!(classify_codecs("opus"), CodecClass::Other);
    }

    mod property_tests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn parse_never_panics_on_arbitrary_text(text in ".*") {
                let _ = parse(&text, &base());
            }

            #[test]
            fn parse_never_panics_on_playlist_like_text(
                lines in proptest::collection::vec(
                    prop_oneof![
                        Just("#EXTM3U".to_string()),
                        Just("#EXT-X-ENDLIST".to_string()),
                        Just("#EXT-X-DISCONTINUITY".to_string()),
                        "#EXT-X-[A-Z-]{1,12}:[ -~]{0,30}",
                        "[ -~]{0,30}",
                    ],
                    0..20,
                )
            ) {
                let _ = parse(&lines.join("\n"), &base());
            }

            #[test]
            fn segment_numbers_count_up_from_media_sequence(
                start in 0u64..1_000_000,
                count in 1usize..40,
            ) {
                let mut text = format!(
                    "#EXTM3U\n#EXT-X-TARGETDURATION:6\n#EXT-X-MEDIA-SEQUENCE:{start}\n"
                );
                for index in 0..count {
                    text.push_str(&format!("#EXTINF:6,\nseg{index}.ts\n"));
                }

                let playlist = media(&text);

                prop_assert_eq!(playlist.segments.len(), count);
                for (index, segment) in playlist.segments.iter().enumerate() {
                    prop_assert_eq!(segment.seq, start + index as u64);
                }
            }
        }
    }
}
