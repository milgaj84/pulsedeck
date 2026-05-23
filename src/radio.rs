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

/// Search for stations by name via the Radio Browser API.
pub async fn search_stations(query: &str) -> anyhow::Result<Vec<Station>> {
    if query.len() < 2 {
        return Ok(Vec::new());
    }

    let client = reqwest::Client::builder()
        .user_agent(format!("DriftFM/{}", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(5))
        .build()?;

    let url = "https://de1.api.radio-browser.info/json/stations/search";

    let resp = client.get(url)
        .query(&[
            ("name", query),
            ("hidebroken", "true"),
            ("order", "clickcount"),
            ("reverse", "true"),
            ("limit", "20"),
        ])
        .send()
        .await?;
    let api_stations = resp.json::<Vec<ApiBrowseStation>>().await?;

    let stations = api_stations
        .into_iter()
        .filter(|s| !s.url_resolved.is_empty())
        .map(|s| Station {
            name: s.name.trim().to_string(),
            url: s.url_resolved,
            genre: s.tags
                .split(',')
                .next()
                .unwrap_or("Radio")
                .trim()
                .to_string(),
            country: s.country,
            bitrate: s.bitrate,
        })
        .collect();

    Ok(stations)
}
