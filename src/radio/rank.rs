use std::collections::HashMap;

use super::query::{SearchField, StationSearchQuery};
use super::{Station, StationIdentity};

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
}
