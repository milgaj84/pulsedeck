/// Parse the `StreamTitle` field from an ICY metadata string.
pub(super) fn parse_stream_title(meta: &str) -> Option<String> {
    let key = "StreamTitle='";
    if let Some(start_idx) = meta.find(key) {
        let value_start = start_idx + key.len();
        if let Some(end_idx) = meta[value_start..].find("';") {
            let title = &meta[value_start..value_start + end_idx];
            return Some(title.trim().to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

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
