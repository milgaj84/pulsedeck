#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlaybackErrorKind {
    Network,
    Http,
    Decode,
    Output,
    Timeout,
    Unknown,
}

pub(crate) fn classify_playback_error(error: &str) -> PlaybackErrorKind {
    let lower = error.to_ascii_lowercase();
    if lower.contains("soundcard")
        || lower.contains("hardware output")
        || lower.contains("sink error")
    {
        PlaybackErrorKind::Output
    } else if lower.contains("decode") || lower.contains("unsupported") {
        PlaybackErrorKind::Decode
    } else if lower.contains("http ") {
        PlaybackErrorKind::Http
    } else if lower.contains("timeout") || lower.contains("timed out") {
        PlaybackErrorKind::Timeout
    } else if lower.contains("connection") || lower.contains("network") {
        PlaybackErrorKind::Network
    } else {
        PlaybackErrorKind::Unknown
    }
}

pub fn playback_error_action_hint(error: &str) -> &'static str {
    match classify_playback_error(error) {
        PlaybackErrorKind::Output => "r retry output  , choose device  s stop",
        PlaybackErrorKind::Decode => "r retry  / search alternatives  s stop",
        PlaybackErrorKind::Http | PlaybackErrorKind::Network | PlaybackErrorKind::Timeout => {
            "r retry  / search alternatives  d inspect"
        }
        PlaybackErrorKind::Unknown => "r retry  d inspect  s stop",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_output_errors() {
        assert_eq!(
            classify_playback_error("Hardware output error: Sink error: no device"),
            PlaybackErrorKind::Output
        );
    }

    #[test]
    fn classifies_decode_errors() {
        assert_eq!(
            classify_playback_error("Decode error: unsupported format"),
            PlaybackErrorKind::Decode
        );
    }

    #[test]
    fn classifies_http_timeout_and_network_errors() {
        assert_eq!(classify_playback_error("HTTP 404"), PlaybackErrorKind::Http);
        assert_eq!(
            classify_playback_error("operation timed out"),
            PlaybackErrorKind::Timeout
        );
        assert_eq!(
            classify_playback_error("Connection failed: network down"),
            PlaybackErrorKind::Network
        );
    }
}
