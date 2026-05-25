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
}
