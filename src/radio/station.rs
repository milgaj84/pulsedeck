use serde::{Deserialize, Serialize};

/// A radio station with display, playback, and optional Radio Browser trust metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Station {
    pub name: String,
    pub url: String,
    pub genre: String,
    pub country: String,
    pub bitrate: u32,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub station_uuid: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub country_code: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub language: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub codec: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub homepage: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_check_ok: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub votes: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub click_count: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum StationIdentity {
    Uuid(String),
    Url(String),
}

impl Station {
    pub fn basic(
        name: impl Into<String>,
        url: impl Into<String>,
        genre: impl Into<String>,
        country: impl Into<String>,
        bitrate: u32,
    ) -> Self {
        Self {
            name: name.into(),
            url: url.into(),
            genre: genre.into(),
            country: country.into(),
            bitrate,
            station_uuid: None,
            country_code: String::new(),
            tags: Vec::new(),
            language: String::new(),
            codec: String::new(),
            homepage: String::new(),
            last_check_ok: None,
            votes: None,
            click_count: None,
        }
    }

    pub fn identity(&self) -> StationIdentity {
        self.station_uuid
            .as_deref()
            .map(str::trim)
            .filter(|uuid| !uuid.is_empty())
            .map(|uuid| StationIdentity::Uuid(uuid.to_ascii_lowercase()))
            .unwrap_or_else(|| StationIdentity::Url(normalized_station_url(&self.url)))
    }
}

pub fn station_identity_matches(a: &Station, b: &Station) -> bool {
    match (a.station_uuid.as_deref(), b.station_uuid.as_deref()) {
        (Some(left), Some(right)) if !left.trim().is_empty() && !right.trim().is_empty() => {
            left.eq_ignore_ascii_case(right)
        }
        _ => normalized_station_url(&a.url) == normalized_station_url(&b.url),
    }
}

pub fn clean_tag_values(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect()
}

fn normalized_station_url(value: &str) -> String {
    value.trim().trim_end_matches('/').to_ascii_lowercase()
}

/// Returns hardcoded fallback stations so the app works offline.
pub fn fallback_stations() -> Vec<Station> {
    vec![
        Station::basic(
            "Nightride FM",
            "https://stream.nightride.fm/nightride.m4a",
            "Synthwave",
            "US",
            128,
        ),
        Station::basic(
            "NightWave Plaza",
            "https://radio.plaza.one/mp3",
            "Vaporwave",
            "US",
            128,
        ),
        Station::basic(
            "SomaFM: Groove Salad",
            "https://ice2.somafm.com/groovesalad-128-mp3",
            "Ambient",
            "US",
            128,
        ),
        Station::basic(
            "SomaFM: DEF CON",
            "https://ice2.somafm.com/defcon-128-mp3",
            "Synthwave",
            "US",
            128,
        ),
        Station::basic(
            "SomaFM: Space Station",
            "https://ice2.somafm.com/spacestation-128-mp3",
            "Ambient Space",
            "US",
            128,
        ),
        Station::basic(
            "SomaFM: Vaporwaves",
            "https://ice2.somafm.com/vaporwaves-128-mp3",
            "Vaporwave",
            "US",
            128,
        ),
        Station::basic(
            "Nightride FM: Chillsynth",
            "https://stream.nightride.fm/chillsynth.m4a",
            "Chillsynth",
            "US",
            128,
        ),
        Station::basic(
            "Nightride FM: Ebsylon",
            "https://stream.nightride.fm/ebsylon.m4a",
            "Darksynth",
            "US",
            128,
        ),
        Station::basic(
            "SomaFM: Underground 80s",
            "https://ice2.somafm.com/u80s-128-mp3",
            "80s",
            "US",
            128,
        ),
        Station::basic(
            "SomaFM: Drone Zone",
            "https://ice2.somafm.com/dronezone-128-mp3",
            "Drone Ambient",
            "US",
            128,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn station_identity_prefers_uuid_when_both_present() {
        let mut saved = Station::basic("A", "http://old", "Radio", "US", 128);
        saved.station_uuid = Some("ABC".to_string());
        let mut result = Station::basic("A", "http://new", "Radio", "US", 128);
        result.station_uuid = Some("abc".to_string());

        assert!(station_identity_matches(&saved, &result));
    }

    #[test]
    fn station_identity_falls_back_to_normalized_url() {
        let saved = Station::basic("A", " HTTP://STREAM/ ", "Radio", "US", 128);
        let result = Station::basic("A", "http://stream", "Radio", "US", 128);

        assert!(station_identity_matches(&saved, &result));
    }

    #[test]
    fn station_identity_does_not_match_by_name_only() {
        let saved = Station::basic("Same", "http://a", "Radio", "US", 128);
        let result = Station::basic("Same", "http://b", "Radio", "US", 128);

        assert!(!station_identity_matches(&saved, &result));
    }

    #[test]
    fn clean_tag_values_trims_and_drops_empty_tags() {
        assert_eq!(
            clean_tag_values(vec![
                " jazz ".to_string(),
                "".to_string(),
                "rock".to_string()
            ]),
            vec!["jazz".to_string(), "rock".to_string()]
        );
    }
}
