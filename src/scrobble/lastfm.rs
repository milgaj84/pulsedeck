// Last.fm API client — signature computation, request building, and response parsing.

use super::{ScrobbleClient, ScrobbleError, TrackMetadata};

const LASTFM_API_URL: &str = "https://ws.audioscrobbler.com/2.0/";
const AUTH_ERROR_CODES: &[u32] = &[4, 9, 10, 13];

/// Last.fm scrobble client (infrastructure layer).
pub struct LastFmClient {
    client: reqwest::blocking::Client,
    api_key: String,
    shared_secret: String,
    session_key: String,
}

impl LastFmClient {
    pub fn new(api_key: String, shared_secret: String, session_key: String) -> Self {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("failed to build HTTP client");
        Self { client, api_key, shared_secret, session_key }
    }
}

impl ScrobbleClient for LastFmClient {
    fn now_playing(&self, meta: &TrackMetadata) -> Result<(), ScrobbleError> {
        let params = build_now_playing_params(
            &meta.artist, &meta.title, &self.api_key, &self.session_key,
        );
        let api_sig = compute_api_sig(&params, &self.shared_secret);
        let mut form: Vec<(&str, &str)> = params;
        form.push(("api_sig", &api_sig));
        send_request(&self.client, &form)
    }

    fn scrobble(&self, meta: &TrackMetadata, timestamp: u64) -> Result<(), ScrobbleError> {
        let ts_str = timestamp.to_string();
        let params = build_scrobble_params(
            &meta.artist, &meta.title, &ts_str, &self.api_key, &self.session_key,
        );
        let api_sig = compute_api_sig(&params, &self.shared_secret);
        let mut form: Vec<(&str, &str)> = params;
        form.push(("api_sig", &api_sig));
        send_request(&self.client, &form)
    }
}

fn build_now_playing_params<'a>(
    artist: &'a str, track: &'a str, api_key: &'a str, sk: &'a str,
) -> Vec<(&'a str, &'a str)> {
    vec![
        ("method", "track.updateNowPlaying"),
        ("artist", artist),
        ("track", track),
        ("api_key", api_key),
        ("sk", sk),
    ]
}

fn build_scrobble_params<'a>(
    artist: &'a str, track: &'a str, timestamp: &'a str,
    api_key: &'a str, sk: &'a str,
) -> Vec<(&'a str, &'a str)> {
    vec![
        ("method", "track.scrobble"),
        ("artist", artist),
        ("track", track),
        ("timestamp", timestamp),
        ("api_key", api_key),
        ("sk", sk),
    ]
}

fn send_request(
    client: &reqwest::blocking::Client, form: &[(&str, &str)],
) -> Result<(), ScrobbleError> {
    let response = client
        .post(LASTFM_API_URL)
        .form(form)
        .send()
        .map_err(|e| ScrobbleError::Network(e.to_string()))?;

    let body = response
        .text()
        .map_err(|e| ScrobbleError::Network(e.to_string()))?;

    parse_lastfm_response(&body)
}

/// Parse a Last.fm XML response body into Ok or the appropriate ScrobbleError.
/// Extracted as a pure function for unit testing without HTTP.
pub fn parse_lastfm_response(body: &str) -> Result<(), ScrobbleError> {
    if body.contains("status=\"ok\"") {
        return Ok(());
    }
    let error_code = extract_error_code(body);
    match error_code {
        Some(code) if AUTH_ERROR_CODES.contains(&code) => {
            let msg = extract_error_message(body);
            Err(ScrobbleError::Auth(msg))
        }
        Some(_code) => {
            let msg = extract_error_message(body);
            Err(ScrobbleError::Network(msg))
        }
        None => Err(ScrobbleError::Network("unexpected response format".to_string())),
    }
}

fn extract_error_code(body: &str) -> Option<u32> {
    let start = body.find("code=\"")? + 6;
    let end = body[start..].find('"')? + start;
    body[start..end].parse().ok()
}

fn extract_error_message(body: &str) -> String {
    // Message is between <error ...> and </error>
    let tag_end = match body.find("<error") {
        Some(pos) => body[pos..].find('>').map(|i| pos + i + 1),
        None => None,
    };
    let close = body.find("</error>");
    match (tag_end, close) {
        (Some(start), Some(end)) if start < end => body[start..end].trim().to_string(),
        _ => "unknown error".to_string(),
    }
}

/// Compute Last.fm API method signature.
/// Pure function: md5(sorted params concatenated as "key1value1key2value2..." + secret).
pub fn compute_api_sig(params: &[(&str, &str)], secret: &str) -> String {
    let mut sorted: Vec<(&str, &str)> = params.to_vec();
    sorted.sort_by_key(|(key, _)| *key);

    let mut input = String::new();
    for (key, value) in &sorted {
        input.push_str(key);
        input.push_str(value);
    }
    input.push_str(secret);

    format!("{:x}", md5::compute(input.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- compute_api_sig tests ---

    #[test]
    fn test_compute_api_sig_empty_params_returns_md5_of_secret() {
        let result = compute_api_sig(&[], "mysecret");
        let expected = format!("{:x}", md5::compute(b"mysecret"));
        assert_eq!(result, expected);
        assert_eq!(result.len(), 32);
    }

    #[test]
    fn test_compute_api_sig_single_param() {
        let result = compute_api_sig(&[("method", "track.scrobble")], "secret");
        let expected = format!("{:x}", md5::compute(b"methodtrack.scrobblesecret"));
        assert_eq!(result, expected);
    }

    #[test]
    fn test_compute_api_sig_multiple_params_sorted_alphabetically() {
        let params = [
            ("method", "track.updateNowPlaying"),
            ("api_key", "abc123"),
        ];
        let result = compute_api_sig(&params, "mysecret");
        let expected = format!(
            "{:x}",
            md5::compute(b"api_keyabc123methodtrack.updateNowPlayingmysecret")
        );
        assert_eq!(result, expected);
    }

    #[test]
    fn test_compute_api_sig_known_test_vector() {
        let params = [
            ("method", "track.updateNowPlaying"),
            ("api_key", "abc123"),
        ];
        let result = compute_api_sig(&params, "mysecret");
        let expected = format!(
            "{:x}",
            md5::compute("api_keyabc123methodtrack.updateNowPlayingmysecret")
        );
        assert_eq!(result, expected);
        assert_eq!(result.len(), 32);
        assert!(result.chars().all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()));
    }

    #[test]
    fn test_compute_api_sig_returns_lowercase_hex_32_chars() {
        let result = compute_api_sig(&[("a", "1"), ("b", "2")], "s");
        assert_eq!(result.len(), 32);
        assert!(result.chars().all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()));
    }

    // --- parse_lastfm_response tests ---

    #[test]
    fn test_parse_response_ok_status() {
        let body = r#"<?xml version="1.0" encoding="utf-8"?>
<lfm status="ok">
<nowplaying><track corrected="0">Creep</track></nowplaying>
</lfm>"#;
        assert_eq!(parse_lastfm_response(body), Ok(()));
    }

    #[test]
    fn test_parse_response_auth_error_code_4() {
        let body = r#"<?xml version="1.0" encoding="utf-8"?>
<lfm status="failed">
<error code="4">Invalid authentication token</error>
</lfm>"#;
        let result = parse_lastfm_response(body);
        assert_eq!(result, Err(ScrobbleError::Auth("Invalid authentication token".to_string())));
    }

    #[test]
    fn test_parse_response_auth_error_code_9() {
        let body = r#"<lfm status="failed"><error code="9">Invalid session key</error></lfm>"#;
        let result = parse_lastfm_response(body);
        assert_eq!(result, Err(ScrobbleError::Auth("Invalid session key".to_string())));
    }

    #[test]
    fn test_parse_response_auth_error_code_10() {
        let body = r#"<lfm status="failed"><error code="10">Invalid API key</error></lfm>"#;
        let result = parse_lastfm_response(body);
        assert_eq!(result, Err(ScrobbleError::Auth("Invalid API key".to_string())));
    }

    #[test]
    fn test_parse_response_auth_error_code_13() {
        let body = r#"<lfm status="failed"><error code="13">Invalid method signature</error></lfm>"#;
        let result = parse_lastfm_response(body);
        assert_eq!(result, Err(ScrobbleError::Auth("Invalid method signature".to_string())));
    }

    #[test]
    fn test_parse_response_non_auth_error_returns_network() {
        let body = r#"<lfm status="failed"><error code="6">Artist not found</error></lfm>"#;
        let result = parse_lastfm_response(body);
        assert_eq!(result, Err(ScrobbleError::Network("Artist not found".to_string())));
    }

    #[test]
    fn test_parse_response_unexpected_format_returns_network() {
        let body = "some garbage response";
        let result = parse_lastfm_response(body);
        assert_eq!(
            result,
            Err(ScrobbleError::Network("unexpected response format".to_string()))
        );
    }

    #[test]
    fn test_parse_response_empty_body_returns_network() {
        let result = parse_lastfm_response("");
        assert_eq!(
            result,
            Err(ScrobbleError::Network("unexpected response format".to_string()))
        );
    }

    // --- Request parameter construction tests ---

    #[test]
    fn test_build_now_playing_params_contains_required_fields() {
        let params = build_now_playing_params("Radiohead", "Creep", "mykey", "mysession");
        assert!(params.contains(&("method", "track.updateNowPlaying")));
        assert!(params.contains(&("artist", "Radiohead")));
        assert!(params.contains(&("track", "Creep")));
        assert!(params.contains(&("api_key", "mykey")));
        assert!(params.contains(&("sk", "mysession")));
    }

    #[test]
    fn test_build_scrobble_params_contains_required_fields() {
        let params = build_scrobble_params("Daft Punk", "One More Time", "1234567890", "key", "sk");
        assert!(params.contains(&("method", "track.scrobble")));
        assert!(params.contains(&("artist", "Daft Punk")));
        assert!(params.contains(&("track", "One More Time")));
        assert!(params.contains(&("timestamp", "1234567890")));
        assert!(params.contains(&("api_key", "key")));
        assert!(params.contains(&("sk", "sk")));
    }

    #[test]
    fn test_now_playing_api_sig_uses_correct_params() {
        let params = build_now_playing_params("Artist", "Track", "key123", "session456");
        let sig = compute_api_sig(&params, "secret");
        // Manually compute expected sig with all params sorted
        let mut sorted = params.clone();
        sorted.sort_by_key(|(k, _)| *k);
        let mut input = String::new();
        for (k, v) in &sorted {
            input.push_str(k);
            input.push_str(v);
        }
        input.push_str("secret");
        let expected = format!("{:x}", md5::compute(input.as_bytes()));
        assert_eq!(sig, expected);
    }

    #[test]
    fn test_scrobble_api_sig_uses_correct_params() {
        let params = build_scrobble_params("Artist", "Track", "999", "key", "session");
        let sig = compute_api_sig(&params, "shared");
        let mut sorted = params.clone();
        sorted.sort_by_key(|(k, _)| *k);
        let mut input = String::new();
        for (k, v) in &sorted {
            input.push_str(k);
            input.push_str(v);
        }
        input.push_str("shared");
        let expected = format!("{:x}", md5::compute(input.as_bytes()));
        assert_eq!(sig, expected);
    }
}
