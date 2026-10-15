//! Conservative codec classification for live radio streams.
//!
//! Header and URL hints are frequently wrong in public station directories, so
//! magic bytes take priority. ICY metadata is intentionally not considered here:
//! ICY describes metadata framing, not the underlying audio codec.

use super::ac3::Dolby;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CodecHint {
    Mp3,
    Aac,
    OggVorbis,
    Opus,
    Ogg,
    Flac,
    Wav,
    Ac3,
    Eac3,
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
            Self::Ac3 => "AC-3",
            Self::Eac3 => "E-AC-3",
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
    /// The stream is ADTS AAC in a form Symphonia cannot read (the MPEG-2 header
    /// or CRC-protected frames) and its frames must be rewritten first.
    pub(super) adts_rewrite: bool,
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
    let (hint, source) = if let Some(hint) = codec_from_magic(prebuffer) {
        (hint, CodecSource::MagicBytes)
    } else if let Some(hint) = codec_from_content_type(content_type) {
        (hint, CodecSource::ContentType)
    } else if let Some(hint) = codec_from_url(final_url) {
        (hint, CodecSource::UrlExtension)
    } else {
        (CodecHint::Unknown, CodecSource::Unknown)
    };
    // Whatever called the stream AAC, what matters is the frames in it.
    let adts_rewrite = matches!(hint, CodecHint::Aac | CodecHint::Unknown)
        && super::adts_stream::needs_rewrite(prebuffer);
    CodecDetection {
        hint,
        source,
        adts_rewrite,
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

    // AC-3 and E-AC-3 have no fixed header, and a live stream can start in the
    // middle of a frame, so look for a chain of valid syncframes.
    match super::ac3::detect(bytes) {
        Some(Dolby::Ac3) => Some(CodecHint::Ac3),
        Some(Dolby::Eac3) => Some(CodecHint::Eac3),
        None => None,
    }
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
        "audio/ac3" | "audio/x-ac3" | "audio/vnd.dolby.dd-raw" => Some(CodecHint::Ac3),
        "audio/eac3" | "audio/x-eac3" => Some(CodecHint::Eac3),
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
    } else if path.ends_with(".ac3") {
        Some(CodecHint::Ac3)
    } else if path.ends_with(".eac3") || path.ends_with(".ec3") {
        Some(CodecHint::Eac3)
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
    fn aac_streams_in_the_forms_symphonia_rejects_are_flagged_for_rewriting() {
        let canonical = include_bytes!("hls/testdata/tone.aac");
        let mut mpeg2 = canonical.to_vec();
        // Every frame header, not just the first, carries the ID bit.
        let mut pos = 0;
        while pos + 7 <= mpeg2.len() {
            mpeg2[pos + 1] |= 0x08;
            pos += (usize::from(mpeg2[pos + 3] & 3) << 11)
                | (usize::from(mpeg2[pos + 4]) << 3)
                | usize::from(mpeg2[pos + 5] >> 5);
        }

        let plain = detect_codec(canonical, "audio/aac", "https://r.test/x");
        assert_eq!(plain.hint, CodecHint::Aac);
        assert!(!plain.adts_rewrite);

        let old = detect_codec(&mpeg2, "audio/aacp", "https://r.test/x");
        assert_eq!(old.hint, CodecHint::Aac);
        assert!(old.adts_rewrite);

        // Joined mid-frame, with a content type saying AAC: found by its frames.
        let joined = detect_codec(&mpeg2[40..], "audio/aac", "https://r.test/x");
        assert_eq!(joined.hint, CodecHint::Aac);
        assert!(joined.adts_rewrite);

        // Other codecs are never rewritten.
        let mp3 = include_bytes!("hls/testdata/tone.mp3");
        assert!(!detect_codec(mp3, "audio/mpeg", "https://r.test/x").adts_rewrite);
    }

    #[test]
    fn ac3_and_eac3_streams_are_detected_by_their_frames() {
        let ac3 = include_bytes!("testdata/dolby/tone.ac3");
        let eac3 = include_bytes!("testdata/dolby/tone.eac3");

        assert_eq!(codec_from_magic(ac3), Some(CodecHint::Ac3));
        assert_eq!(codec_from_magic(eac3), Some(CodecHint::Eac3));
        // A live stream joined in the middle of a frame.
        assert_eq!(codec_from_magic(&ac3[100..]), Some(CodecHint::Ac3));
        let detection = detect_codec(eac3, "application/octet-stream", "https://r.test/x");
        assert_eq!(detection.hint, CodecHint::Eac3);
        assert_eq!(detection.source, CodecSource::MagicBytes);
    }

    #[test]
    fn dolby_content_types_and_extensions_are_recognised() {
        for content_type in ["audio/ac3", "audio/x-ac3", "audio/vnd.dolby.dd-raw"] {
            assert_eq!(codec_from_content_type(content_type), Some(CodecHint::Ac3));
        }
        for content_type in ["audio/eac3", "AUDIO/X-EAC3; charset=x"] {
            assert_eq!(codec_from_content_type(content_type), Some(CodecHint::Eac3));
        }
        assert_eq!(
            codec_from_url("https://r.test/live.ac3"),
            Some(CodecHint::Ac3)
        );
        for url in ["https://r.test/live.eac3?x=1", "https://r.test/LIVE.EC3"] {
            assert_eq!(codec_from_url(url), Some(CodecHint::Eac3));
        }
    }

    #[test]
    fn other_audio_is_not_mistaken_for_dolby() {
        for bytes in [
            &include_bytes!("hls/testdata/tone.mp3")[..],
            &include_bytes!("hls/testdata/tone.aac")[..],
            &include_bytes!("testdata/tone.flac")[..],
            &[0u8; 2048][..],
        ] {
            assert_ne!(codec_from_magic(bytes), Some(CodecHint::Ac3));
            assert_ne!(codec_from_magic(bytes), Some(CodecHint::Eac3));
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
