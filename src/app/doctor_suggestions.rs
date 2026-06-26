// Context-aware recovery hints based on current playback diagnostics.

use super::types::{DecoderState, PlaybackDiagnostics};

/// Returns context-aware recovery suggestions based on diagnostic state.
/// Pure function: no side effects, no I/O.
pub fn suggest_actions(diagnostics: &PlaybackDiagnostics) -> Vec<&'static str> {
    let mut suggestions = Vec::new();

    if has_output_device_error(diagnostics) {
        suggestions.push("Try a different output device");
    }

    if is_stream_unreachable(diagnostics) {
        suggestions.push("Stream may be unreachable — try again later");
    }

    if is_station_offline(diagnostics) {
        suggestions.push("Station may be offline — check health dot");
    } else if has_high_reconnect_count(diagnostics) {
        suggestions.push("High reconnect count — consider r manual retry");
    }

    if has_metadata_error(diagnostics) {
        suggestions.push("Try disabling stream metadata in settings");
    }

    suggestions
}

fn has_output_device_error(diagnostics: &PlaybackDiagnostics) -> bool {
    error_contains_any(diagnostics, &["output", "device", "sink", "audio"])
}

fn is_stream_unreachable(diagnostics: &PlaybackDiagnostics) -> bool {
    diagnostics.buffer_percent == 0 && diagnostics.decoder_state != DecoderState::Idle
}

fn is_station_offline(diagnostics: &PlaybackDiagnostics) -> bool {
    diagnostics.reconnect_limit > 0 && diagnostics.reconnect_attempts >= diagnostics.reconnect_limit
}

fn has_high_reconnect_count(diagnostics: &PlaybackDiagnostics) -> bool {
    diagnostics.reconnect_attempts >= 2
}

fn has_metadata_error(diagnostics: &PlaybackDiagnostics) -> bool {
    error_contains_any(diagnostics, &["metadata", "icy", "parse"])
}

fn error_contains_any(diagnostics: &PlaybackDiagnostics, keywords: &[&str]) -> bool {
    let Some(ref error) = diagnostics.last_error else {
        return false;
    };
    let lower = error.to_ascii_lowercase();
    keywords.iter().any(|kw| lower.contains(kw))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_diagnostics() -> PlaybackDiagnostics {
        PlaybackDiagnostics::default()
    }

    #[test]
    fn test_no_error_returns_empty() {
        let diagnostics = default_diagnostics();
        assert!(suggest_actions(&diagnostics).is_empty());
    }

    #[test]
    fn test_output_device_error_detected() {
        let mut diagnostics = default_diagnostics();
        diagnostics.last_error = Some("Failed to open output device".to_string());

        let suggestions = suggest_actions(&diagnostics);
        assert!(suggestions.contains(&"Try a different output device"));
    }

    #[test]
    fn test_output_device_error_case_insensitive() {
        let mut diagnostics = default_diagnostics();
        diagnostics.last_error = Some("AUDIO SINK failed".to_string());

        let suggestions = suggest_actions(&diagnostics);
        assert!(suggestions.contains(&"Try a different output device"));
    }

    #[test]
    fn test_station_offline_when_retries_exhausted() {
        let mut diagnostics = default_diagnostics();
        diagnostics.reconnect_attempts = 3;
        diagnostics.reconnect_limit = 3;

        let suggestions = suggest_actions(&diagnostics);
        assert!(suggestions.contains(&"Station may be offline — check health dot"));
        assert!(!suggestions.contains(&"High reconnect count — consider r manual retry"));
    }

    #[test]
    fn test_high_reconnect_when_below_limit() {
        let mut diagnostics = default_diagnostics();
        diagnostics.reconnect_attempts = 2;
        diagnostics.reconnect_limit = 3;

        let suggestions = suggest_actions(&diagnostics);
        assert!(suggestions.contains(&"High reconnect count — consider r manual retry"));
        assert!(!suggestions.contains(&"Station may be offline — check health dot"));
    }

    #[test]
    fn test_metadata_error_detected() {
        let mut diagnostics = default_diagnostics();
        diagnostics.last_error = Some("ICY metadata parse failure".to_string());

        let suggestions = suggest_actions(&diagnostics);
        assert!(suggestions.contains(&"Try disabling stream metadata in settings"));
    }

    #[test]
    fn test_buffer_unreachable_with_non_idle_decoder() {
        let mut diagnostics = default_diagnostics();
        diagnostics.buffer_percent = 0;
        diagnostics.decoder_state = DecoderState::Connecting;

        let suggestions = suggest_actions(&diagnostics);
        assert!(suggestions.contains(&"Stream may be unreachable — try again later"));
    }

    #[test]
    fn test_buffer_zero_with_idle_decoder_no_suggestion() {
        let mut diagnostics = default_diagnostics();
        diagnostics.buffer_percent = 0;
        diagnostics.decoder_state = DecoderState::Idle;

        let suggestions = suggest_actions(&diagnostics);
        assert!(!suggestions.contains(&"Stream may be unreachable — try again later"));
    }

    #[test]
    fn test_multiple_conditions_produce_multiple_hints() {
        let mut diagnostics = default_diagnostics();
        diagnostics.last_error = Some("output device lost".to_string());
        diagnostics.reconnect_attempts = 3;
        diagnostics.reconnect_limit = 3;

        let suggestions = suggest_actions(&diagnostics);
        assert!(suggestions.len() >= 2);
        assert!(suggestions.contains(&"Try a different output device"));
        assert!(suggestions.contains(&"Station may be offline — check health dot"));
    }

    #[test]
    fn test_single_reconnect_no_hint() {
        let mut diagnostics = default_diagnostics();
        diagnostics.reconnect_attempts = 1;
        diagnostics.reconnect_limit = 3;

        let suggestions = suggest_actions(&diagnostics);
        assert!(!suggestions.contains(&"High reconnect count — consider r manual retry"));
        assert!(!suggestions.contains(&"Station may be offline — check health dot"));
    }

    #[test]
    fn test_zero_reconnect_limit_no_offline_hint() {
        let mut diagnostics = default_diagnostics();
        diagnostics.reconnect_attempts = 5;
        diagnostics.reconnect_limit = 0;

        let suggestions = suggest_actions(&diagnostics);
        assert!(!suggestions.contains(&"Station may be offline — check health dot"));
    }
}
