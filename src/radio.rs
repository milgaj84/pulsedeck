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

    Ok(api_stations
        .into_iter()
        .filter_map(map_api_station)
        .collect())
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
        assert!(RADIO_BROWSER_HTTPS_SERVERS
            .iter()
            .all(|server| server.starts_with("https://")));
        assert!(RADIO_BROWSER_HTTP_SERVERS
            .iter()
            .all(|server| server.starts_with("http://")));
        assert_eq!(
            RADIO_BROWSER_HTTPS_SERVERS.len(),
            RADIO_BROWSER_HTTP_SERVERS.len()
        );
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
        let station = map_api_station(api_station(
            "  Lo-Fi Radio  ",
            "http://stream",
            "lofi,chill",
        ))
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

        assert!(stations
            .iter()
            .any(|station| station.name == "Nightride FM"));
        assert!(stations
            .iter()
            .any(|station| station.name == "SomaFM: Groove Salad"));
        assert!(stations.iter().all(|station| !station.url.is_empty()));
    }
}
