// ListenBrainz scrobble client implementation.

use std::time::Duration;

use serde_json::{json, Value};

use super::{ScrobbleClient, ScrobbleError, TrackMetadata};

const API_URL: &str = "https://api.listenbrainz.org/1/submit-listens";
const TIMEOUT_SECS: u64 = 10;

pub struct ListenBrainzClient {
    client: reqwest::blocking::Client,
    token: String,
}

impl ListenBrainzClient {
    pub fn new(token: String) -> Self {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(TIMEOUT_SECS))
            .build()
            .expect("failed to build HTTP client");
        Self { client, token }
    }
}

impl ScrobbleClient for ListenBrainzClient {
    fn now_playing(&self, meta: &TrackMetadata) -> Result<(), ScrobbleError> {
        let body = build_now_playing_body(meta);
        let response = self
            .client
            .post(API_URL)
            .header("Authorization", format!("Token {}", self.token))
            .json(&body)
            .send()
            .map_err(|e| ScrobbleError::Network(e.to_string()))?;
        map_response_status(response.status().as_u16())
    }

    fn scrobble(&self, meta: &TrackMetadata, timestamp: u64) -> Result<(), ScrobbleError> {
        let body = build_scrobble_body(meta, timestamp);
        let response = self
            .client
            .post(API_URL)
            .header("Authorization", format!("Token {}", self.token))
            .json(&body)
            .send()
            .map_err(|e| ScrobbleError::Network(e.to_string()))?;
        map_response_status(response.status().as_u16())
    }
}

/// Build JSON body for a "playing_now" submission.
pub fn build_now_playing_body(meta: &TrackMetadata) -> Value {
    json!({
        "listen_type": "playing_now",
        "payload": [{
            "track_metadata": {
                "artist_name": meta.artist,
                "track_name": meta.title
            }
        }]
    })
}

/// Build JSON body for a "single" scrobble submission.
pub fn build_scrobble_body(meta: &TrackMetadata, timestamp: u64) -> Value {
    json!({
        "listen_type": "single",
        "payload": [{
            "track_metadata": {
                "artist_name": meta.artist,
                "track_name": meta.title
            },
            "listened_at": timestamp
        }]
    })
}

/// Map an HTTP status code to a ScrobbleError result.
pub fn map_response_status(status_code: u16) -> Result<(), ScrobbleError> {
    match status_code {
        200 => Ok(()),
        401 => Err(ScrobbleError::Auth("invalid token".to_string())),
        code => Err(ScrobbleError::Network(format!("HTTP {code}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_meta() -> TrackMetadata {
        TrackMetadata {
            artist: "Radiohead".to_string(),
            title: "Creep".to_string(),
        }
    }

    // --- Body construction tests ---

    #[test]
    fn test_build_now_playing_body_listen_type() {
        let body = build_now_playing_body(&sample_meta());
        assert_eq!(body["listen_type"], "playing_now");
    }

    #[test]
    fn test_build_now_playing_body_track_metadata() {
        let body = build_now_playing_body(&sample_meta());
        let track = &body["payload"][0]["track_metadata"];
        assert_eq!(track["artist_name"], "Radiohead");
        assert_eq!(track["track_name"], "Creep");
    }

    #[test]
    fn test_build_now_playing_body_no_listened_at() {
        let body = build_now_playing_body(&sample_meta());
        assert!(body["payload"][0].get("listened_at").is_none());
    }

    #[test]
    fn test_build_scrobble_body_listen_type() {
        let body = build_scrobble_body(&sample_meta(), 1700000000);
        assert_eq!(body["listen_type"], "single");
    }

    #[test]
    fn test_build_scrobble_body_track_metadata() {
        let body = build_scrobble_body(&sample_meta(), 1700000000);
        let track = &body["payload"][0]["track_metadata"];
        assert_eq!(track["artist_name"], "Radiohead");
        assert_eq!(track["track_name"], "Creep");
    }

    #[test]
    fn test_build_scrobble_body_listened_at() {
        let body = build_scrobble_body(&sample_meta(), 1700000000);
        assert_eq!(body["payload"][0]["listened_at"], 1700000000);
    }

    #[test]
    fn test_build_scrobble_body_different_timestamp() {
        let body = build_scrobble_body(&sample_meta(), 1234567890);
        assert_eq!(body["payload"][0]["listened_at"], 1234567890);
    }

    // --- Response mapping tests ---

    #[test]
    fn test_map_response_status_200_ok() {
        assert_eq!(map_response_status(200), Ok(()));
    }

    #[test]
    fn test_map_response_status_401_auth_error() {
        assert_eq!(
            map_response_status(401),
            Err(ScrobbleError::Auth("invalid token".to_string()))
        );
    }

    #[test]
    fn test_map_response_status_500_network_error() {
        assert_eq!(
            map_response_status(500),
            Err(ScrobbleError::Network("HTTP 500".to_string()))
        );
    }

    #[test]
    fn test_map_response_status_403_network_error() {
        assert_eq!(
            map_response_status(403),
            Err(ScrobbleError::Network("HTTP 403".to_string()))
        );
    }

    #[test]
    fn test_map_response_status_429_network_error() {
        assert_eq!(
            map_response_status(429),
            Err(ScrobbleError::Network("HTTP 429".to_string()))
        );
    }

    // --- Header format test ---

    #[test]
    fn test_authorization_header_format() {
        let token = "my-secret-token";
        let header = format!("Token {}", token);
        assert_eq!(header, "Token my-secret-token");
    }

    // --- Edge cases ---

    #[test]
    fn test_build_now_playing_body_empty_artist() {
        let meta = TrackMetadata {
            artist: String::new(),
            title: "Unknown".to_string(),
        };
        let body = build_now_playing_body(&meta);
        assert_eq!(body["payload"][0]["track_metadata"]["artist_name"], "");
        assert_eq!(body["payload"][0]["track_metadata"]["track_name"], "Unknown");
    }

    #[test]
    fn test_build_scrobble_body_special_characters() {
        let meta = TrackMetadata {
            artist: "AC/DC".to_string(),
            title: "It's a Long Way".to_string(),
        };
        let body = build_scrobble_body(&meta, 100);
        assert_eq!(body["payload"][0]["track_metadata"]["artist_name"], "AC/DC");
        assert_eq!(
            body["payload"][0]["track_metadata"]["track_name"],
            "It's a Long Way"
        );
    }
}
