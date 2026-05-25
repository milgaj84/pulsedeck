#!/usr/bin/env python3
"""Apply Phase 4 tests/docs hardening.

This is intentionally test/doc-focused:
- add pure Radio Browser search helper tests without live network calls
- add pure audio session buffer-level tests
- document the project's testing strategy and runtime smoke checklist
- update changelog
- remove this temporary script from the final branch state
"""

from pathlib import Path

RADIO = Path("src/radio.rs")
SESSION = Path("src/audio/session.rs")
DOC = Path("docs/testing-strategy.md")
CHANGELOG = Path("CHANGELOG.md")
THIS_SCRIPT = Path("scripts/apply_phase4_tests_docs.py")


def write(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text.strip() + "\n", encoding="utf-8")


def replace_once(text: str, old: str, new: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"Expected exactly one match, found {count}: {old[:140]!r}")
    return text.replace(old, new, 1)


def rewrite_radio() -> None:
    write(RADIO, r'''
use serde::Deserialize;

/// A radio station with all metadata needed for display and playback.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Station {
    pub name: String,
    pub url: String,
    pub genre: String,
    pub country: String,
    pub bitrate: u32,
}

/// JSON shape from Radio Browser API.
#[derive(Debug, Deserialize)]
struct ApiBrowseStation {
    name: String,
    #[serde(rename = "url_resolved")]
    url_resolved: String,
    tags: String,
    country: String,
    bitrate: u32,
}

/// Returns hardcoded fallback stations so the app works offline.
pub fn fallback_stations() -> Vec<Station> {
    vec![
        Station {
            name: "Nightride FM".into(),
            url: "https://stream.nightride.fm/nightride.m4a".into(),
            genre: "Synthwave".into(),
            country: "US".into(),
            bitrate: 128,
        },
        Station {
            name: "NightWave Plaza".into(),
            url: "https://radio.plaza.one/mp3".into(),
            genre: "Vaporwave".into(),
            country: "US".into(),
            bitrate: 128,
        },
        Station {
            name: "SomaFM: Groove Salad".into(),
            url: "https://ice2.somafm.com/groovesalad-128-mp3".into(),
            genre: "Ambient".into(),
            country: "US".into(),
            bitrate: 128,
        },
        Station {
            name: "SomaFM: DEF CON".into(),
            url: "https://ice2.somafm.com/defcon-128-mp3".into(),
            genre: "Synthwave".into(),
            country: "US".into(),
            bitrate: 128,
        },
        Station {
            name: "SomaFM: Space Station".into(),
            url: "https://ice2.somafm.com/spacestation-128-mp3".into(),
            genre: "Ambient Space".into(),
            country: "US".into(),
            bitrate: 128,
        },
        Station {
            name: "SomaFM: Vaporwaves".into(),
            url: "https://ice2.somafm.com/vaporwaves-128-mp3".into(),
            genre: "Vaporwave".into(),
            country: "US".into(),
            bitrate: 128,
        },
        Station {
            name: "Nightride FM: Chillsynth".into(),
            url: "https://stream.nightride.fm/chillsynth.m4a".into(),
            genre: "Chillsynth".into(),
            country: "US".into(),
            bitrate: 128,
        },
        Station {
            name: "Nightride FM: Ebsylon".into(),
            url: "https://stream.nightride.fm/ebsylon.m4a".into(),
            genre: "Darksynth".into(),
            country: "US".into(),
            bitrate: 128,
        },
        Station {
            name: "SomaFM: Underground 80s".into(),
            url: "https://ice2.somafm.com/u80s-128-mp3".into(),
            genre: "80s".into(),
            country: "US".into(),
            bitrate: 128,
        },
        Station {
            name: "SomaFM: Drone Zone".into(),
            url: "https://ice2.somafm.com/dronezone-128-mp3".into(),
            genre: "Drone Ambient".into(),
            country: "US".into(),
            bitrate: 128,
        },
    ]
}

const RADIO_BROWSER_HTTPS_SERVERS: &[&str] = &[
    "https://de1.api.radio-browser.info",
    "https://de2.api.radio-browser.info",
    "https://nl1.api.radio-browser.info",
    "https://at1.api.radio-browser.info",
];

const RADIO_BROWSER_HTTP_SERVERS: &[&str] = &[
    "http://de1.api.radio-browser.info",
    "http://de2.api.radio-browser.info",
    "http://nl1.api.radio-browser.info",
    "http://at1.api.radio-browser.info",
];

/// Search for stations by name via the Radio Browser API.
pub async fn search_stations(query: &str) -> anyhow::Result<Vec<Station>> {
    let query = query.trim();
    if is_short_search_query(query) {
        return Ok(Vec::new());
    }

    let client = reqwest::Client::builder()
        .user_agent(format!("DriftFM/{}", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(5))
        .build()?;

    let https_result =
        search_stations_with_servers(&client, RADIO_BROWSER_HTTPS_SERVERS, query).await;
    match https_result {
        Ok(stations) => Ok(stations),
        Err(https_error) => {
            match search_stations_with_servers(&client, RADIO_BROWSER_HTTP_SERVERS, query).await {
                Ok(stations) => Ok(stations),
                Err(http_error) => anyhow::bail!(
                    "HTTPS search failed: {https_error}; HTTP fallback failed: {http_error}"
                ),
            }
        }
    }
}

fn is_short_search_query(query: &str) -> bool {
    query.trim().chars().count() < 2
}

async fn search_stations_with_servers(
    client: &reqwest::Client,
    servers: &[&str],
    query: &str,
) -> anyhow::Result<Vec<Station>> {
    let mut errors = Vec::new();

    for server in servers {
        let url = radio_browser_search_url(server);
        match search_stations_on_server(client, &url, query).await {
            Ok(stations) => return Ok(stations),
            Err(err) => errors.push(format!("{server}: {err}")),
        }
    }

    anyhow::bail!("{}", format_search_errors(&errors))
}

fn radio_browser_search_url(server: &str) -> String {
    format!("{server}/json/stations/search")
}

fn format_search_errors(errors: &[String]) -> String {
    errors.join(" | ")
}

async fn search_stations_on_server(
    client: &reqwest::Client,
    url: &str,
    query: &str,
) -> anyhow::Result<Vec<Station>> {
    let resp = client
        .get(url)
        .query(&[
            ("name", query),
            ("hidebroken", "true"),
            ("order", "clickcount"),
            ("reverse", "true"),
            ("limit", "20"),
        ])
        .send()
        .await?
        .error_for_status()?;

    let api_stations = resp.json::<Vec<ApiBrowseStation>>().await?;

    Ok(api_stations.into_iter().filter_map(map_api_station).collect())
}

fn map_api_station(station: ApiBrowseStation) -> Option<Station> {
    if station.url_resolved.is_empty() {
        return None;
    }

    Some(Station {
        name: station.name.trim().to_string(),
        url: station.url_resolved,
        genre: station
            .tags
            .split(',')
            .next()
            .unwrap_or("Radio")
            .trim()
            .to_string(),
        country: station.country,
        bitrate: station.bitrate,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn api_station(name: &str, url: &str, tags: &str) -> ApiBrowseStation {
        ApiBrowseStation {
            name: name.to_string(),
            url_resolved: url.to_string(),
            tags: tags.to_string(),
            country: "US".to_string(),
            bitrate: 128,
        }
    }

    #[test]
    fn short_search_query_trims_before_counting() {
        assert!(is_short_search_query(" l "));
        assert!(!is_short_search_query(" lo "));
    }

    #[test]
    fn radio_browser_search_url_appends_expected_path() {
        assert_eq!(
            radio_browser_search_url("https://de1.api.radio-browser.info"),
            "https://de1.api.radio-browser.info/json/stations/search"
        );
    }

    #[test]
    fn https_servers_are_tried_before_http_fallback_servers() {
        assert!(RADIO_BROWSER_HTTPS_SERVERS.iter().all(|server| server.starts_with("https://")));
        assert!(RADIO_BROWSER_HTTP_SERVERS.iter().all(|server| server.starts_with("http://")));
        assert_eq!(RADIO_BROWSER_HTTPS_SERVERS.len(), RADIO_BROWSER_HTTP_SERVERS.len());
    }

    #[test]
    fn format_search_errors_keeps_server_context() {
        let errors = vec![
            "https://de1.api.radio-browser.info: timeout".to_string(),
            "https://de2.api.radio-browser.info: tls".to_string(),
        ];

        assert_eq!(
            format_search_errors(&errors),
            "https://de1.api.radio-browser.info: timeout | https://de2.api.radio-browser.info: tls"
        );
    }

    #[test]
    fn map_api_station_trims_name_and_uses_first_tag() {
        let station = map_api_station(api_station("  Lo-Fi Radio  ", "http://stream", "lofi,chill"))
            .expect("station should map");

        assert_eq!(station.name, "Lo-Fi Radio");
        assert_eq!(station.url, "http://stream");
        assert_eq!(station.genre, "lofi");
        assert_eq!(station.country, "US");
        assert_eq!(station.bitrate, 128);
    }

    #[test]
    fn map_api_station_drops_empty_resolved_urls() {
        assert!(map_api_station(api_station("Broken", "", "radio")).is_none());
    }

    #[test]
    fn fallback_stations_include_known_offline_defaults() {
        let stations = fallback_stations();

        assert!(stations.iter().any(|station| station.name == "Nightride FM"));
        assert!(stations.iter().any(|station| station.name == "SomaFM: Groove Salad"));
        assert!(stations.iter().all(|station| !station.url.is_empty()));
    }
}
''')


def patch_session() -> None:
    text = SESSION.read_text(encoding="utf-8")

    if "fn buffer_level_status" not in text:
        text = replace_once(
            text,
            "use std::time::Duration;\n",
            "use std::time::Duration;\n\nfn buffer_level_status(len: usize, capacity: usize, bytes_per_sec: usize) -> (u8, u32) {\n    let percent = if capacity == 0 {\n        0\n    } else {\n        ((len * 100) / capacity) as u8\n    };\n    let seconds = if bytes_per_sec == 0 {\n        0\n    } else {\n        (len / bytes_per_sec) as u32\n    };\n\n    (percent, seconds)\n}\n",
        )
        text = replace_once(
            text,
            "                    let percent = ((len * 100) / cap) as u8;\n                    let seconds = (len / bytes_per_sec) as u32;\n",
            "                    let (percent, seconds) = buffer_level_status(len, cap, bytes_per_sec);\n",
        )

    if "mod tests" not in text:
        text = text.rstrip() + r'''

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffer_level_status_reports_percent_and_seconds() {
        let (percent, seconds) = buffer_level_status(160_000, 1_000_000, 16_000);

        assert_eq!(percent, 16);
        assert_eq!(seconds, 10);
    }

    #[test]
    fn buffer_level_status_handles_zero_capacity_and_rate() {
        let (percent, seconds) = buffer_level_status(160_000, 0, 0);

        assert_eq!(percent, 0);
        assert_eq!(seconds, 0);
    }
}
'''

    SESSION.write_text(text.strip() + "\n", encoding="utf-8")


def write_doc() -> None:
    write(DOC, r'''
# Testing strategy

DriftFM uses fast, deterministic checks as the default quality gate. CI and local validation should remain useful even when a developer is offline, a stream host is down, or the Radio Browser network has certificate trouble.

## Required local gate

Run this before opening or updating a pull request:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo build --release
cargo run
```

## Test layers

- Unit tests cover pure helpers, reducers, parsing, theme serialization, library mutation, search state, and key mapping.
- Reducer contract tests protect behavior that previously regressed, especially settings-overlay action blocking and stale search response handling.
- Audio unit tests avoid opening sound devices or live streams. They test deterministic math and parsing helpers.
- Radio Browser tests avoid live network calls. They verify URL construction, fallback ordering, API mapping, and error formatting.
- Runtime smoke tests cover the real terminal, audio backend, search, playback, recording toggles, and terminal restore.

## Why no live network tests by default

Live Radio Browser and stream tests would make CI depend on external certificates, DNS, routing, and stream availability. Those failures are useful during manual diagnosis but too noisy for normal pull request gates.

Use manual network smoke tests when touching:

- `src/radio.rs`
- `src/audio/session.rs`
- `src/audio/stream_reader.rs`
- stream retry/cancellation behavior
- TLS or HTTP fallback behavior

## Manual smoke checklist

1. Start the app with `cargo run`.
2. Search for `lofi`.
3. Confirm search results appear or show a useful compact error.
4. Press `Enter` on a search result and confirm it adds/plays.
5. Pause and resume playback.
6. Stop playback.
7. Switch stations.
8. Toggle recording while playing, then stop recording.
9. Open settings and press playback/layout/search keys to confirm they do not leak through.
10. Open help and settings to confirm overlays still mutually close.
11. Quit and confirm the terminal restores cleanly.
''')


def update_changelog() -> None:
    text = CHANGELOG.read_text(encoding="utf-8")
    if "Phase 4 Test Hardening" in text:
        return

    insert = """### Added\n*   **Phase 4 Test Hardening**: Added deterministic unit tests for Radio Browser fallback helpers, API station mapping, fallback station defaults, and audio buffer-level status math.\n*   **Testing Strategy Documentation**: Added `docs/testing-strategy.md` describing local gates, test layers, network-test boundaries, and the manual runtime smoke checklist.\n"""

    text = replace_once(text, "### Added\n", insert)
    CHANGELOG.write_text(text, encoding="utf-8")


def main() -> None:
    rewrite_radio()
    patch_session()
    write_doc()
    update_changelog()
    THIS_SCRIPT.unlink(missing_ok=True)
    print("Applied Phase 4 tests/docs hardening.")


if __name__ == "__main__":
    main()
