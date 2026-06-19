use std::collections::HashSet;

use serde::{Deserialize, Serialize};

const MAX_REASONABLE_BITRATE: u32 = 1024;

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
    #[serde(default, skip_serializing_if = "StationHealth::is_empty")]
    pub health: StationHealth,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct StationHealth {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_success_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_failure_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_count: Option<u32>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub last_error_summary: String,
}

impl StationHealth {
    pub fn is_empty(&self) -> bool {
        self.last_success_at.is_none()
            && self.last_failure_at.is_none()
            && self.failure_count.unwrap_or(0) == 0
            && self.last_error_summary.is_empty()
    }
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
            health: StationHealth::default(),
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

    /// Fill missing metadata from a richer Radio Browser station while preserving
    /// the user's saved-facing station name, stream URL, and genre label.
    pub fn enrich_from(&mut self, incoming: &Station) -> bool {
        let mut changed = false;

        if self.station_uuid.is_none() && incoming.station_uuid.is_some() {
            self.station_uuid = incoming.station_uuid.clone();
            changed = true;
        }

        changed |= fill_string(&mut self.country_code, &incoming.country_code);
        changed |= fill_string(&mut self.language, &incoming.language);
        changed |= fill_string(&mut self.codec, &incoming.codec);
        changed |= fill_string(&mut self.homepage, &incoming.homepage);

        if self.tags.is_empty() && !incoming.tags.is_empty() {
            self.tags = incoming.tags.clone();
            changed = true;
        }

        if self.bitrate == 0 && incoming.bitrate > 0 {
            self.bitrate = incoming.bitrate;
            changed = true;
        }

        if self.country.trim().is_empty() && !incoming.country.trim().is_empty() {
            self.country = incoming.country.trim().to_string();
            changed = true;
        }

        changed |= set_option_if_some(&mut self.last_check_ok, incoming.last_check_ok);
        changed |= set_option_if_some(&mut self.votes, incoming.votes);
        changed |= set_option_if_some(&mut self.click_count, incoming.click_count);

        changed
    }
}

fn fill_string(target: &mut String, incoming: &str) -> bool {
    if target.trim().is_empty() && !incoming.trim().is_empty() {
        *target = incoming.trim().to_string();
        true
    } else {
        false
    }
}

fn set_option_if_some<T: Copy + PartialEq>(target: &mut Option<T>, incoming: Option<T>) -> bool {
    if incoming.is_some() && *target != incoming {
        *target = incoming;
        true
    } else {
        false
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
    let mut seen = HashSet::new();
    let mut tags = Vec::new();

    for value in values {
        let tag = value.trim();
        if tag.is_empty() {
            continue;
        }
        if seen.insert(tag.to_ascii_lowercase()) {
            tags.push(tag.to_string());
        }
    }

    tags
}

pub fn normalize_station_uuid(value: String) -> Option<String> {
    let trimmed = value.trim().to_string();
    (!trimmed.is_empty()).then_some(trimmed)
}

pub fn normalize_country_code(value: &str) -> String {
    value.trim().to_ascii_uppercase()
}

pub fn normalize_codec(value: &str) -> String {
    let normalized = value.trim().to_ascii_uppercase();
    match normalized.as_str() {
        "AUDIO/MPEG" | "MPEG" => "MP3".to_string(),
        "AAC+" | "HE-AAC" | "HEAAC" => "AAC".to_string(),
        "OGG VORBIS" | "VORBIS" => "OGG".to_string(),
        _ => normalized,
    }
}

pub fn sanitize_bitrate(value: u32) -> u32 {
    if value > MAX_REASONABLE_BITRATE {
        0
    } else {
        value
    }
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
