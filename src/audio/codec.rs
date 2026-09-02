//! Conservative codec classification for live radio streams.
//!
//! Header and URL hints are frequently wrong in public station directories, so
//! magic bytes take priority. ICY metadata is intentionally not considered here:
//! ICY describes metadata framing, not the underlying audio codec.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CodecHint {
    Mp3,
    Aac,
    OggVorbis,
    Opus,
    Ogg,
    Flac,
    Wav,
    Unknown,
}

impl CodecHint {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Mp3 => "MP3",
            Self::Aac => "AAC",
            Self::OggVorbis => "Ogg Vorbis",
            Self::Opus => "Opus",
            Self::Ogg => "Ogg",
            Self::Flac => "FLAC",
            Self::Wav => "WAV",
            Self::Unknown => "Auto-detected",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CodecSource {
    MagicBytes,
    ContentType,
    UrlExtension,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct CodecDetection {
    pub(super) hint: CodecHint,
    pub(super) source: CodecSource,
}

impl CodecDetection {
    pub(super) fn verified_mp3(self) -> bool {
        self.hint == CodecHint::Mp3 && self.source == CodecSource::MagicBytes
    }
}

pub(super) fn detect_codec(
    prebuffer: &[u8],
    content_type: &str,
    final_url: &str,
) -> CodecDetection {
    if let Some(hint) = codec_from_magic(prebuffer) {
        return CodecDetection {
            hint,
            source: CodecSource::MagicBytes,
        };
    }

    if let Some(hint) = codec_from_content_type(content_type) {
        return CodecDetection {
            hint,
            source: CodecSource::ContentType,
        };
    }

    if let Some(hint) = codec_from_url(final_url) {
        return CodecDetection {
            hint,
            source: CodecSource::UrlExtension,
        };
    }

    CodecDetection {
        hint: CodecHint::Unknown,
        source: CodecSource::Unknown,
    }
}

fn codec_from_magic(bytes: &[u8]) -> Option<CodecHint> {
    if bytes.starts_with(b"fLaC") {
        return Some(CodecHint::Flac);
    }

    if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WAVE" {
        return Some(CodecHint::Wav);
    }

    if bytes.starts_with(b"OggS") {
        let probe_len = bytes.len().min(128);
        let probe = &bytes[..probe_len];
        if contains_bytes(probe, b"OpusHead") {
            return Some(CodecHint::Opus);
        }
        if contains_bytes(probe, b"\x01vorbis") {
            return Some(CodecHint::OggVorbis);
        }
        return Some(CodecHint::Ogg);
    }

    if bytes.starts_with(b"ID3") {
        return Some(CodecHint::Mp3);
    }

    if bytes.len() >= 2 {
        let first = bytes[0];
        let second = bytes[1];

        // ADTS AAC syncword: 0xFFF, layer bits must be 00. Common second
        // bytes are 0xF1 and 0xF9.
        if first == 0xFF && (second & 0xF6) == 0xF0 {
            return Some(CodecHint::Aac);
        }

        // MPEG audio frame sync with non-reserved version and layer fields.
        let version = (second >> 3) & 0x03;
        let layer = (second >> 1) & 0x03;
        if first == 0xFF && (second & 0xE0) == 0xE0 && version != 0x01 && layer != 0x00 {
            return Some(CodecHint::Mp3);
        }
    }

    None
}

fn codec_from_content_type(content_type: &str) -> Option<CodecHint> {
    let media_type = content_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();

    match media_type.as_str() {
        "audio/mpeg" | "audio/mp3" | "audio/x-mpeg" => Some(CodecHint::Mp3),
        "audio/aac" | "audio/aacp" | "audio/x-aac" | "audio/vnd.dlna.adts" => Some(CodecHint::Aac),
        "audio/opus" => Some(CodecHint::Opus),
        "audio/ogg" | "application/ogg" => Some(CodecHint::Ogg),
        "audio/flac" | "audio/x-flac" => Some(CodecHint::Flac),
        "audio/wav" | "audio/wave" | "audio/x-wav" => Some(CodecHint::Wav),
        _ => None,
    }
}

fn codec_from_url(url: &str) -> Option<CodecHint> {
    let path = url
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .trim_end_matches('/')
        .to_ascii_lowercase();

    if path.ends_with(".mp3") {
        Some(CodecHint::Mp3)
    } else if path.ends_with(".aac") || path.ends_with(".aacp") || path.ends_with(".adts") {
        Some(CodecHint::Aac)
    } else if path.ends_with(".opus") {
        Some(CodecHint::Opus)
    } else if path.ends_with(".ogg") || path.ends_with(".oga") {
        Some(CodecHint::Ogg)
    } else if path.ends_with(".flac") {
        Some(CodecHint::Flac)
    } else if path.ends_with(".wav") || path.ends_with(".wave") {
        Some(CodecHint::Wav)
    } else {
        None
    }
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn magic_bytes_override_wrong_mp3_content_type_and_url() {
        let aac = [0xFF, 0xF1, 0x50, 0x80];
        let detection = detect_codec(&aac, "audio/mpeg", "https://radio.test/live.mp3");

        assert_eq!(detection.hint, CodecHint::Aac);
        assert_eq!(detection.source, CodecSource::MagicBytes);
        assert!(!detection.verified_mp3());
    }

    #[test]
    fn adts_aac_is_not_misclassified_as_mp3() {
        for second in [0xF1, 0xF9] {
            assert_eq!(
                codec_from_magic(&[0xFF, second, 0x50]),
                Some(CodecHint::Aac)
            );
        }
    }

    #[test]
    fn mp3_frame_and_id3_are_detected() {
        assert_eq!(codec_from_magic(b"ID3\x04\x00"), Some(CodecHint::Mp3));
        assert_eq!(codec_from_magic(&[0xFF, 0xFB, 0x90]), Some(CodecHint::Mp3));
    }

    #[test]
    fn verified_mp3_requires_magic_bytes() {
        let header = detect_codec(&[], "audio/mpeg", "https://radio.test/live");
        let url = detect_codec(&[], "", "https://radio.test/live.mp3");
        let magic = detect_codec(&[0xFF, 0xFB, 0x90], "", "https://radio.test/live");

        assert!(!header.verified_mp3());
        assert!(!url.verified_mp3());
        assert!(magic.verified_mp3());
    }

    #[test]
    fn opus_and_vorbis_are_distinguished_inside_ogg() {
        let mut opus = b"OggS".to_vec();
        opus.extend_from_slice(b"padding OpusHead padding");
        let mut vorbis = b"OggS".to_vec();
        vorbis.extend_from_slice(b"padding \x01vorbis padding");

        assert_eq!(codec_from_magic(&opus), Some(CodecHint::Opus));
        assert_eq!(codec_from_magic(&vorbis), Some(CodecHint::OggVorbis));
        assert_eq!(codec_from_magic(b"OggSunknown"), Some(CodecHint::Ogg));
    }

    #[test]
    fn flac_and_wav_magic_are_detected() {
        assert_eq!(codec_from_magic(b"fLaCmore"), Some(CodecHint::Flac));
        assert_eq!(codec_from_magic(b"RIFF1234WAVEfmt "), Some(CodecHint::Wav));
    }

    #[test]
    fn content_type_parameters_and_case_are_ignored() {
        assert_eq!(
            codec_from_content_type(" Audio/AAC; charset=binary "),
            Some(CodecHint::Aac)
        );
        assert_eq!(
            codec_from_content_type("audio/mpeg; bitrate=128"),
            Some(CodecHint::Mp3)
        );
    }

    #[test]
    fn ambiguous_ogg_content_type_stays_generic_ogg() {
        assert_eq!(codec_from_content_type("audio/ogg"), Some(CodecHint::Ogg));
    }

    #[test]
    fn url_detection_ignores_query_and_fragment() {
        assert_eq!(
            codec_from_url("https://radio.test/live.OPUS?token=abc#player"),
            Some(CodecHint::Opus)
        );
    }

    #[test]
    fn unknown_inputs_remain_unknown() {
        let detection = detect_codec(b"not audio", "application/octet-stream", "https://x/live");
        assert_eq!(detection.hint, CodecHint::Unknown);
        assert_eq!(detection.source, CodecSource::Unknown);
        assert_eq!(detection.hint.label(), "Auto-detected");
    }

    #[test]
    fn icy_metadata_does_not_participate_in_codec_detection() {
        // ICY headers are intentionally absent from this API. An AAC prebuffer
        // remains AAC regardless of whether the caller separately enables ICY.
        let detection = detect_codec(&[0xFF, 0xF1, 0x50], "", "https://radio.test/live");
        assert_eq!(detection.hint, CodecHint::Aac);
    }
}
