use std::collections::HashMap;

use super::query::{SearchField, StationSearchQuery};
use super::{Station, StationIdentity};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RankExplanation {
    pub signals: Vec<RankSignal>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RankSignal {
    ExactName,
    ExactTag,
    CountryCode(String),
    Language(String),
    Codec(String),
    LastCheckOk,
    HighVotes,
    HighClicks,
    AlreadySaved,
    Https,
}

pub fn explain_station_match(
    query: &StationSearchQuery,
    station: &Station,
    is_saved: bool,
) -> RankExplanation {
    let mut signals = Vec::new();

    match query.field() {
        SearchField::Name if station.name.eq_ignore_ascii_case(query.value()) => {
            signals.push(RankSignal::ExactName);
        }
        SearchField::Tag
            if station
                .tags
                .iter()
                .any(|tag| tag.eq_ignore_ascii_case(query.value())) =>
        {
            signals.push(RankSignal::ExactTag);
        }
        SearchField::CountryCode if station.country_code.eq_ignore_ascii_case(query.value()) => {
            signals.push(RankSignal::CountryCode(station.country_code.clone()));
        }
        SearchField::Language if station.language.eq_ignore_ascii_case(query.value()) => {
            signals.push(RankSignal::Language(station.language.clone()));
        }
        SearchField::Codec if station.codec.eq_ignore_ascii_case(query.value()) => {
            signals.push(RankSignal::Codec(station.codec.clone()));
        }
        _ => {}
    }

    if station.last_check_ok == Some(true) {
        signals.push(RankSignal::LastCheckOk);
    }
    if station.votes.unwrap_or(0) >= 100 {
        signals.push(RankSignal::HighVotes);
    }
    if station.click_count.unwrap_or(0) >= 1_000 {
        signals.push(RankSignal::HighClicks);
    }
    if is_saved {
        signals.push(RankSignal::AlreadySaved);
    }
    if station.url.starts_with("https://") {
        signals.push(RankSignal::Https);
    }

    RankExplanation { signals }
}

pub fn rank_explanation_label(explanation: &RankExplanation) -> String {
    if explanation.signals.is_empty() {
        return "General relevance".to_string();
    }

    explanation
        .signals
        .iter()
        .map(rank_signal_label)
        .collect::<Vec<_>>()
        .join(" + ")
}

fn rank_signal_label(signal: &RankSignal) -> String {
    match signal {
        RankSignal::ExactName => "Exact name".to_string(),
        RankSignal::ExactTag => "Exact tag".to_string(),
        RankSignal::CountryCode(code) => format!("Country {}", code.to_ascii_uppercase()),
        RankSignal::Language(language) => format!("Language {}", language),
        RankSignal::Codec(codec) => codec.to_ascii_uppercase(),
        RankSignal::LastCheckOk => "Last check OK".to_string(),
        RankSignal::HighVotes => "High votes".to_string(),
        RankSignal::HighClicks => "High clicks".to_string(),
        RankSignal::AlreadySaved => "Saved".to_string(),
        RankSignal::Https => "HTTPS".to_string(),
    }
}

pub(super) fn rank_search_results(
    query: &StationSearchQuery,
    stations: Vec<Station>,
) -> Vec<Station> {
    let mut unique = dedupe_by_identity(query, stations);
    unique.sort_by(|a, b| {
        station_score(query, b)
            .cmp(&station_score(query, a))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.url.cmp(&b.url))
    });
    unique
}

fn dedupe_by_identity(query: &StationSearchQuery, stations: Vec<Station>) -> Vec<Station> {
    let mut by_key: HashMap<StationIdentity, Station> = HashMap::new();
    for station in stations {
        let key = station.identity();
        match by_key.get_mut(&key) {
            Some(existing) if station_score(query, &station) > station_score(query, existing) => {
                *existing = station;
            }
            Some(_) => {}
            None => {
                by_key.insert(key, station);
            }
        }
    }
    by_key.into_values().collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct SearchScore {
    name_exact: u8,
    name_prefix: u8,
    field_match: u8,
    checked_ok: u8,
    https: u8,
    known_codec: u8,
    known_bitrate: u8,
    click_count: u32,
    votes: u32,
}

fn station_score(query: &StationSearchQuery, station: &Station) -> SearchScore {
    let value = query.value().to_lowercase();
    let name = station.name.to_lowercase();

    SearchScore {
        name_exact: u8::from(name == value),
        name_prefix: u8::from(name.starts_with(&value)),
        field_match: u8::from(query_matches_field(query, station)),
        checked_ok: u8::from(station.last_check_ok == Some(true)),
        https: u8::from(station.url.starts_with("https://")),
        known_codec: u8::from(!station.codec.trim().is_empty()),
        known_bitrate: u8::from(station.bitrate > 0),
        click_count: station.click_count.unwrap_or(0),
        votes: station.votes.unwrap_or(0),
    }
}

fn query_matches_field(query: &StationSearchQuery, station: &Station) -> bool {
    let value = query.value().to_lowercase();
    match query.field() {
        SearchField::Name => station.name.to_lowercase().contains(&value),
        SearchField::Tag => {
            station
                .tags
                .iter()
                .any(|tag| tag.to_lowercase().contains(&value))
                || station.genre.to_lowercase().contains(&value)
        }
        SearchField::Country => station.country.to_lowercase().contains(&value),
        SearchField::CountryCode => station.country_code.eq_ignore_ascii_case(query.value()),
        SearchField::Language => station.language.to_lowercase().contains(&value),
        SearchField::Codec => station.codec.eq_ignore_ascii_case(query.value()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn station(name: &str, url: &str) -> Station {
        Station::basic(name, url, "Radio", "US", 128)
    }

    #[test]
    fn ranking_prefers_exact_name_match() {
        let query = StationSearchQuery::parse("lofi");
        let stations = vec![
            station("A Popular Thing", "http://b"),
            station("lofi", "http://a"),
        ];

        let ranked = rank_search_results(&query, stations);
        assert_eq!(ranked[0].name, "lofi");
    }

    #[test]
    fn ranking_prefers_checked_ok_when_relevance_equal() {
        let query = StationSearchQuery::parse("lofi");
        let mut unchecked = station("lofi one", "http://a");
        unchecked.last_check_ok = Some(false);
        let mut checked = station("lofi two", "http://b");
        checked.last_check_ok = Some(true);

        let ranked = rank_search_results(&query, vec![unchecked, checked]);
        assert_eq!(ranked[0].url, "http://b");
    }

    #[test]
    fn ranking_prefers_https_when_relevance_equal() {
        let query = StationSearchQuery::parse("lofi");
        let http = station("lofi one", "http://a");
        let https = station("lofi two", "https://b");

        let ranked = rank_search_results(&query, vec![http, https]);
        assert_eq!(ranked[0].url, "https://b");
    }

    #[test]
    fn ranking_uses_click_count_after_relevance() {
        let query = StationSearchQuery::parse("lofi");
        let mut low = station("lofi one", "http://a");
        low.click_count = Some(1);
        let mut high = station("lofi two", "http://b");
        high.click_count = Some(99);

        let ranked = rank_search_results(&query, vec![low, high]);
        assert_eq!(ranked[0].url, "http://b");
    }

    #[test]
    fn ranking_dedupes_by_uuid_and_keeps_stronger_candidate() {
        let query = StationSearchQuery::parse("lofi");
        let mut weak = station("lofi", "http://weak");
        weak.station_uuid = Some("uuid".to_string());
        let mut strong = station("lofi", "https://strong");
        strong.station_uuid = Some("UUID".to_string());
        strong.last_check_ok = Some(true);

        let ranked = rank_search_results(&query, vec![weak, strong]);
        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].url, "https://strong");
    }

    #[test]
    fn ranking_dedupes_by_url_when_uuid_missing() {
        let query = StationSearchQuery::parse("lofi");
        let mut low = station("lofi", " HTTP://A/ ");
        low.click_count = Some(1);
        let mut high = station("lofi", "http://a");
        high.click_count = Some(2);

        let ranked = rank_search_results(&query, vec![low, high]);
        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].click_count, Some(2));
    }

    #[test]
    fn ranking_order_is_deterministic_for_equal_scores() {
        let query = StationSearchQuery::parse("lofi");
        let ranked = rank_search_results(
            &query,
            vec![station("lofi b", "http://b"), station("lofi a", "http://a")],
        );

        assert_eq!(ranked[0].name, "lofi a");
    }

    #[test]
    fn explanation_includes_exact_tag_saved_and_health_signals() {
        let query = StationSearchQuery::parse("tag:jazz");
        let mut station = station("Jazz Shed", "https://jazz");
        station.tags = vec!["jazz".to_string()];
        station.last_check_ok = Some(true);

        let explanation = explain_station_match(&query, &station, true);

        assert_eq!(
            explanation.signals,
            vec![
                RankSignal::ExactTag,
                RankSignal::LastCheckOk,
                RankSignal::AlreadySaved,
                RankSignal::Https,
            ]
        );
        assert_eq!(
            rank_explanation_label(&explanation),
            "Exact tag + Last check OK + Saved + HTTPS"
        );
    }

    #[test]
    fn explanation_includes_country_code_and_codec_labels() {
        let country_query = StationSearchQuery::parse("country:ba");
        let mut country_station = station("Bosnia", "http://ba");
        country_station.country_code = "BA".to_string();

        let country_explanation = explain_station_match(&country_query, &country_station, false);
        assert_eq!(rank_explanation_label(&country_explanation), "Country BA");

        let codec_query = StationSearchQuery::parse("codec:mp3");
        let mut codec_station = station("MP3", "http://mp3");
        codec_station.codec = "MP3".to_string();

        let codec_explanation = explain_station_match(&codec_query, &codec_station, false);
        assert_eq!(rank_explanation_label(&codec_explanation), "MP3");
    }

    #[test]
    fn explanation_falls_back_when_no_signal_is_known() {
        let query = StationSearchQuery::parse("lofi");
        let station = station("Other", "http://other");

        let explanation = explain_station_match(&query, &station, false);

        assert!(explanation.signals.is_empty());
        assert_eq!(rank_explanation_label(&explanation), "General relevance");
    }
}
