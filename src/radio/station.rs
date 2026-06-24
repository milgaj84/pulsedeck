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
            left.trim().eq_ignore_ascii_case(right.trim())
        }
        _ => station_url_matches(&a.url, &b.url),
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

pub fn normalized_station_url(value: &str) -> String {
    value.trim().trim_end_matches('/').to_ascii_lowercase()
}

pub fn station_url_matches(left: &str, right: &str) -> bool {
    normalized_station_url(left) == normalized_station_url(right)
}

/// Find the first station in `stations` whose URL matches `target`.
pub fn find_station_by_url<'a>(stations: &'a [Station], target: &str) -> Option<&'a Station> {
    stations
        .iter()
        .find(|station| station_url_matches(&station.url, target))
}

/// Find the index of the first station in `stations` whose URL matches `target`.
pub fn find_station_index_by_url(stations: &[Station], target: &str) -> Option<usize> {
    stations
        .iter()
        .position(|station| station_url_matches(&station.url, target))
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
    fn station_url_matches_ignores_case_whitespace_and_trailing_slash() {
        assert!(station_url_matches(
            " HTTP://Example.COM/stream/ ",
            "http://example.com/stream"
        ));
    }

    #[test]
    fn station_identity_matches_trims_uuid_before_comparing() {
        let mut saved = Station::basic("A", "http://a", "Radio", "US", 128);
        saved.station_uuid = Some(" UUID-1 ".to_string());
        let mut result = Station::basic("B", "http://b", "Radio", "US", 128);
        result.station_uuid = Some("uuid-1".to_string());
        assert!(station_identity_matches(&saved, &result));
    }

    #[test]
    fn station_identity_prefers_uuid_mismatch_over_url_match() {
        let mut saved = Station::basic("A", "http://same", "Radio", "US", 128);
        saved.station_uuid = Some("uuid-a".to_string());
        let mut result = Station::basic("B", "http://same/", "Radio", "US", 128);
        result.station_uuid = Some("uuid-b".to_string());
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

    #[test]
    fn clean_tag_values_deduplicates_case_insensitively_keeping_first_occurrence() {
        let input = vec!["Jazz".to_string(), "jazz".to_string(), "JAZZ".to_string()];
        assert_eq!(clean_tag_values(input), vec!["Jazz".to_string()]);
    }

    #[test]
    fn clean_tag_values_preserves_all_unique_tags_in_order() {
        let input = vec![
            "Rock".to_string(),
            "Jazz".to_string(),
            "Blues".to_string(),
            "Ambient".to_string(),
        ];
        assert_eq!(
            clean_tag_values(input),
            vec![
                "Rock".to_string(),
                "Jazz".to_string(),
                "Blues".to_string(),
                "Ambient".to_string()
            ]
        );
    }

    #[test]
    fn clean_tag_values_three_plus_case_variants_produce_one_entry() {
        let input = vec![
            "Synthwave".to_string(),
            "synthwave".to_string(),
            "SYNTHWAVE".to_string(),
            "SynthWave".to_string(),
            "sYnThWaVe".to_string(),
        ];
        assert_eq!(clean_tag_values(input), vec!["Synthwave".to_string()]);
    }

    #[test]
    fn station_health_default_is_empty() {
        assert!(StationHealth::default().is_empty());
    }
    #[test]
    fn station_health_not_empty_when_last_success_at_set() {
        assert!(!StationHealth {
            last_success_at: Some("2024-01-01T00:00:00Z".to_string()),
            ..Default::default()
        }
        .is_empty());
    }
    #[test]
    fn station_health_not_empty_when_failure_count_nonzero() {
        assert!(!StationHealth {
            failure_count: Some(3),
            ..Default::default()
        }
        .is_empty());
    }
    #[test]
    fn station_health_not_empty_when_last_error_summary_nonempty() {
        assert!(!StationHealth {
            last_error_summary: "connection refused".to_string(),
            ..Default::default()
        }
        .is_empty());
    }
    #[test]
    fn station_health_not_empty_when_last_failure_at_set() {
        assert!(!StationHealth {
            last_failure_at: Some("2024-01-01T00:00:00Z".to_string()),
            ..Default::default()
        }
        .is_empty());
    }

    #[test]
    fn enrich_from_copies_trimmed_values_into_empty_string_fields() {
        let mut target = Station::basic("A", "http://a", "Rock", "US", 128);
        let mut incoming = Station::basic("B", "http://b", "Pop", "UK", 256);
        incoming.country_code = " DE ".to_string();
        incoming.language = " German ".to_string();
        incoming.codec = " MP3 ".to_string();
        incoming.homepage = " http://example.com ".to_string();
        let changed = target.enrich_from(&incoming);
        assert!(changed);
        assert_eq!(target.country_code, "DE");
        assert_eq!(target.language, "German");
        assert_eq!(target.codec, "MP3");
        assert_eq!(target.homepage, "http://example.com");
    }

    #[test]
    fn enrich_from_preserves_non_empty_existing_string_fields() {
        let mut target = Station::basic("A", "http://a", "Rock", "US", 128);
        target.country_code = "US".to_string();
        target.language = "English".to_string();
        target.codec = "AAC".to_string();
        target.homepage = "http://original.com".to_string();
        let mut incoming = Station::basic("B", "http://b", "Pop", "UK", 256);
        incoming.country_code = "DE".to_string();
        incoming.language = "German".to_string();
        incoming.codec = "MP3".to_string();
        incoming.homepage = "http://other.com".to_string();
        target.enrich_from(&incoming);
        assert_eq!(target.country_code, "US");
        assert_eq!(target.language, "English");
        assert_eq!(target.codec, "AAC");
        assert_eq!(target.homepage, "http://original.com");
    }

    #[test]
    fn enrich_from_copies_uuid_when_target_has_none() {
        let mut target = Station::basic("A", "http://a", "Rock", "US", 128);
        let mut incoming = Station::basic("B", "http://b", "Pop", "UK", 256);
        incoming.station_uuid = Some("uuid-123".to_string());
        assert!(target.enrich_from(&incoming));
        assert_eq!(target.station_uuid, Some("uuid-123".to_string()));
    }

    #[test]
    fn enrich_from_does_not_overwrite_existing_uuid() {
        let mut target = Station::basic("A", "http://a", "Rock", "US", 128);
        target.station_uuid = Some("original-uuid".to_string());
        let mut incoming = Station::basic("B", "http://b", "Pop", "UK", 256);
        incoming.station_uuid = Some("new-uuid".to_string());
        target.enrich_from(&incoming);
        assert_eq!(target.station_uuid, Some("original-uuid".to_string()));
    }

    #[test]
    fn enrich_from_sets_bitrate_when_target_is_zero() {
        let mut target = Station::basic("A", "http://a", "Rock", "US", 0);
        let incoming = Station::basic("B", "http://b", "Pop", "UK", 192);
        assert!(target.enrich_from(&incoming));
        assert_eq!(target.bitrate, 192);
    }

    #[test]
    fn enrich_from_copies_tags_when_target_tags_are_empty() {
        let mut target = Station::basic("A", "http://a", "Rock", "US", 128);
        let mut incoming = Station::basic("B", "http://b", "Pop", "UK", 256);
        incoming.tags = vec!["jazz".to_string(), "blues".to_string()];
        assert!(target.enrich_from(&incoming));
        assert_eq!(target.tags, vec!["jazz".to_string(), "blues".to_string()]);
    }

    #[test]
    fn enrich_from_preserves_existing_non_empty_tags() {
        let mut target = Station::basic("A", "http://a", "Rock", "US", 128);
        target.tags = vec!["rock".to_string(), "metal".to_string()];
        let mut incoming = Station::basic("B", "http://b", "Pop", "UK", 256);
        incoming.tags = vec!["jazz".to_string(), "blues".to_string()];
        target.enrich_from(&incoming);
        assert_eq!(target.tags, vec!["rock".to_string(), "metal".to_string()]);
    }

    #[test]
    fn enrich_from_overwrites_option_metadata_fields_when_incoming_differs() {
        let mut target = Station::basic("A", "http://a", "Rock", "US", 128);
        target.last_check_ok = Some(false);
        target.votes = Some(10);
        target.click_count = Some(5);
        let mut incoming = Station::basic("B", "http://b", "Pop", "UK", 256);
        incoming.last_check_ok = Some(true);
        incoming.votes = Some(42);
        incoming.click_count = Some(100);
        assert!(target.enrich_from(&incoming));
        assert_eq!(target.last_check_ok, Some(true));
        assert_eq!(target.votes, Some(42));
        assert_eq!(target.click_count, Some(100));
    }

    #[test]
    fn enrich_from_returns_false_when_no_fields_eligible() {
        let mut target = Station::basic("A", "http://a", "Rock", "US", 128);
        target.station_uuid = Some("uuid-1".to_string());
        target.country_code = "US".to_string();
        target.language = "English".to_string();
        target.codec = "MP3".to_string();
        target.homepage = "http://example.com".to_string();
        target.tags = vec!["rock".to_string()];
        target.last_check_ok = Some(true);
        target.votes = Some(10);
        target.click_count = Some(5);
        let mut incoming = Station::basic("B", "http://b", "Pop", "UK", 256);
        incoming.station_uuid = Some("uuid-2".to_string());
        incoming.country_code = "DE".to_string();
        incoming.language = "German".to_string();
        incoming.codec = "AAC".to_string();
        incoming.homepage = "http://other.com".to_string();
        incoming.tags = vec!["pop".to_string()];
        incoming.last_check_ok = Some(true);
        incoming.votes = Some(10);
        incoming.click_count = Some(5);
        assert!(!target.enrich_from(&incoming));
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn normalize_country_code_invariant(input in ".*") {
            let result = normalize_country_code(&input);
            prop_assert!(result == result.trim());
            for ch in result.chars() {
                if ch.is_ascii_alphabetic() { prop_assert!(ch.is_ascii_uppercase()); }
            }
        }
    }

    proptest! {
        #[test]
        fn normalize_codec_invariant(input in ".*") {
            let result = normalize_codec(&input);
            prop_assert_eq!(result.clone(), result.trim());
            for ch in result.chars() {
                if ch.is_ascii_alphabetic() { prop_assert!(ch.is_ascii_uppercase()); }
            }
        }

        #[test]
        fn normalize_codec_known_aliases(
            input in prop_oneof![
                Just("AUDIO/MPEG".to_string()), Just("MPEG".to_string()), Just("audio/mpeg".to_string()), Just("mpeg".to_string()), Just(" audio/mpeg ".to_string()),
                Just("AAC+".to_string()), Just("HE-AAC".to_string()), Just("HEAAC".to_string()), Just("aac+".to_string()), Just("he-aac".to_string()), Just("heaac".to_string()),
                Just("OGG VORBIS".to_string()), Just("VORBIS".to_string()), Just("ogg vorbis".to_string()), Just("vorbis".to_string()),
            ]
        ) {
            let result = normalize_codec(&input);
            let upper_trimmed = input.trim().to_ascii_uppercase();
            match upper_trimmed.as_str() {
                "AUDIO/MPEG" | "MPEG" => prop_assert_eq!(result, "MP3"),
                "AAC+" | "HE-AAC" | "HEAAC" => prop_assert_eq!(result, "AAC"),
                "OGG VORBIS" | "VORBIS" => prop_assert_eq!(result, "OGG"),
                _ => {}
            }
        }
    }

    proptest! {
        #[test]
        fn sanitize_bitrate_invariant(value in any::<u32>()) {
            let result = sanitize_bitrate(value);
            if value <= 1024 { prop_assert_eq!(result, value); } else { prop_assert_eq!(result, 0); }
        }
    }

    proptest! {
        #[test]
        fn normalized_station_url_invariant(input in ".*") {
            let result = normalized_station_url(&input);
            for ch in result.chars() {
                if ch.is_ascii_alphabetic() { prop_assert!(ch.is_ascii_lowercase()); }
            }
            if !result.is_empty() { prop_assert!(!result.ends_with('/')); }
        }
    }

    proptest! {
        #[test]
        fn station_url_matches_reflexivity(s in ".*") {
            prop_assert!(station_url_matches(&s, &s));
        }
    }
}
