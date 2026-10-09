/// Codec capability policy.
///
/// This module answers one question: given a station codec string, what
/// should playback do? The active decode path uses Symphonia probe-based
/// decoding (via rodio's `Decoder::new`), which supports MP3, AAC, OGG/Vorbis,
/// Opus, FLAC, and WAV. HLS playlists are played by the HLS fetcher
/// (`audio::hls`) for audio-only AAC/MP3 streams; encrypted, fMP4 and
/// video-only variants are rejected at runtime with a clear error. Windows
/// Media (WMA/ASF) has no Symphonia decoder and is `Unsupported`. Missing or
/// unrecognized codec metadata is `Unknown`, which allows a playback attempt
/// because Radio Browser entries are often incomplete.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackCapability {
    Supported,
    Unknown,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodecCapability {
    pub normalized_codec: &'static str,
    pub capability: PlaybackCapability,
    pub reason: &'static str,
}

/// Return the capability policy for a given raw codec string.
///
/// MP3 and all Symphonia-supported formats (AAC, OGG/Vorbis, Opus, FLAC, WAV)
/// are `Supported`, and so is HLS/M3U8 (audio-only streams; see the reason
/// text for the limits). WMA/ASF is `Unsupported`. Everything else (including
/// empty) is `Unknown` and allowed to attempt playback.
pub fn codec_capability(codec: &str) -> CodecCapability {
    match normalize_playback_codec(codec).as_str() {
        "" => CodecCapability {
            normalized_codec: "",
            capability: PlaybackCapability::Unknown,
            reason: "codec metadata is missing",
        },
        "MP3" | "MPEG" | "AUDIO/MPEG" | "AUDIO/MP3" => CodecCapability {
            normalized_codec: "MP3",
            capability: PlaybackCapability::Supported,
            reason: "MP3 streams are supported via the MP3 fast-path decoder",
        },
        "AAC" | "AAC+" | "HE-AAC" | "AUDIO/AAC" | "AUDIO/AACPLUS" => CodecCapability {
            normalized_codec: "AAC",
            capability: PlaybackCapability::Supported,
            reason: "AAC streams are supported via Symphonia decoding",
        },
        "OGG" | "VORBIS" | "OGG/VORBIS" | "APPLICATION/OGG" => CodecCapability {
            normalized_codec: "OGG",
            capability: PlaybackCapability::Supported,
            reason: "OGG/Vorbis streams are supported via Symphonia decoding",
        },
        "OPUS" | "AUDIO/OPUS" => CodecCapability {
            normalized_codec: "OPUS",
            capability: PlaybackCapability::Supported,
            reason: "Opus streams are supported via Symphonia decoding",
        },
        "FLAC" | "AUDIO/FLAC" => CodecCapability {
            normalized_codec: "FLAC",
            capability: PlaybackCapability::Supported,
            reason: "FLAC streams are supported via Symphonia decoding",
        },
        "HLS" | "M3U8" | "APPLICATION/X-MPEGURL" | "APPLICATION/VND.APPLE.MPEGURL" => {
            CodecCapability {
                normalized_codec: "HLS",
                capability: PlaybackCapability::Supported,
                reason: "HLS audio (AAC/MP3) is played from live segments; encrypted, fMP4 and video-only streams are not supported",
            }
        }
        "WMA" | "ASF" | "AUDIO/X-MS-WMA" | "AUDIO/X-MS-ASF" => CodecCapability {
            normalized_codec: "WMA",
            capability: PlaybackCapability::Unsupported,
            reason: "Windows Media (WMA/ASF) streams cannot be decoded",
        },
        "WAV" | "AUDIO/WAV" | "AUDIO/X-WAV" => CodecCapability {
            normalized_codec: "WAV",
            capability: PlaybackCapability::Supported,
            reason: "WAV streams are supported via Symphonia decoding",
        },
        _ => CodecCapability {
            normalized_codec: "UNKNOWN",
            capability: PlaybackCapability::Unknown,
            reason: "codec metadata is not recognized",
        },
    }
}

/// Returns `true` when the codec is safe to attempt playback.
///
/// `Unknown` returns `true` because missing or stale Radio Browser metadata
/// is common and blocking unknowns would produce false negatives on working
/// MP3 streams that simply lack codec metadata.
#[cfg(test)]
pub fn is_codec_playback_supported(codec: &str) -> bool {
    !matches!(
        codec_capability(codec).capability,
        PlaybackCapability::Unsupported
    )
}

fn normalize_playback_codec(codec: &str) -> String {
    codec
        .trim()
        .to_ascii_uppercase()
        .replace('_', "-")
        .replace(' ', "")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mp3_aliases_are_supported() {
        for codec in ["MP3", "mp3", " audio/mpeg ", "mpeg", "AUDIO/MP3"] {
            assert_eq!(
                codec_capability(codec).capability,
                PlaybackCapability::Supported,
                "{codec} should be Supported"
            );
        }
    }

    #[test]
    fn missing_codec_is_unknown_and_allowed_to_try() {
        let capability = codec_capability("   ");

        assert_eq!(capability.capability, PlaybackCapability::Unknown);
        assert!(is_codec_playback_supported(""));
        assert!(is_codec_playback_supported("   "));
    }

    #[test]
    fn known_non_mp3_codecs_are_now_supported() {
        for codec in ["AAC", "aac+", "OGG", "Opus", "FLAC", "WAV"] {
            assert_eq!(
                codec_capability(codec).capability,
                PlaybackCapability::Supported,
                "{codec} should be Supported via Symphonia decoding"
            );
            assert!(is_codec_playback_supported(codec));
        }
    }

    #[test]
    fn hls_is_supported_with_a_caveat() {
        for codec in ["m3u8", "HLS", "APPLICATION/X-MPEGURL"] {
            let capability = codec_capability(codec);
            assert_eq!(
                capability.capability,
                PlaybackCapability::Supported,
                "{codec}"
            );
            assert_eq!(capability.normalized_codec, "HLS");
            assert!(capability.reason.contains("not supported"));
            assert!(is_codec_playback_supported(codec));
        }
    }

    #[test]
    fn wma_is_unsupported() {
        for codec in ["wma", "WMA", "asf", "audio/x-ms-wma"] {
            let capability = codec_capability(codec);
            assert_eq!(
                capability.capability,
                PlaybackCapability::Unsupported,
                "{codec}"
            );
            assert_eq!(capability.normalized_codec, "WMA");
            assert!(!is_codec_playback_supported(codec));
        }
    }

    #[test]
    fn unknown_codec_is_not_blocked() {
        let capability = codec_capability("weird-radio-format");

        assert_eq!(capability.capability, PlaybackCapability::Unknown);
        assert!(is_codec_playback_supported("weird-radio-format"));
    }

    #[test]
    fn he_aac_and_audio_aac_variants_are_supported() {
        for codec in ["HE-AAC", "AUDIO/AAC", "AUDIO/AACPLUS"] {
            assert_eq!(
                codec_capability(codec).capability,
                PlaybackCapability::Supported,
                "{codec} should be Supported"
            );
        }
    }

    #[test]
    fn hls_variants_are_supported() {
        for codec in [
            "HLS",
            "M3U8",
            "APPLICATION/X-MPEGURL",
            "application/vnd.apple.mpegurl",
            " hls ",
        ] {
            assert_eq!(
                codec_capability(codec).capability,
                PlaybackCapability::Supported,
                "{codec} should be Supported"
            );
        }
    }

    #[test]
    fn normalize_strips_whitespace_and_uppercases() {
        // mp3 with spaces should normalize to MP3
        assert_eq!(
            codec_capability("  mp3  ").capability,
            PlaybackCapability::Supported
        );
        // ogg/vorbis lowercase
        assert_eq!(
            codec_capability("ogg").capability,
            PlaybackCapability::Supported
        );
    }

    #[test]
    fn codec_capability_reason_is_not_empty() {
        for codec in [
            "MP3", "AAC", "OGG", "OPUS", "FLAC", "HLS", "WMA", "", "unknown",
        ] {
            assert!(!codec_capability(codec).reason.is_empty());
        }
    }
}
