use crate::radio::Station;

/// Maximum allowed query length. Characters beyond this are ignored.
pub const LIBRARY_FILTER_MAX_QUERY: usize = 256;

/// Returns true if the station matches the query as a case-insensitive
/// substring against name, genre, or any tag.
pub fn station_matches_query(station: &Station, query: &str) -> bool {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return true;
    }

    let effective_query: &str = if trimmed.len() > LIBRARY_FILTER_MAX_QUERY {
        &trimmed[..LIBRARY_FILTER_MAX_QUERY]
    } else {
        trimmed
    };

    let lower_query = effective_query.to_lowercase();

    if station.name.to_lowercase().contains(&lower_query) {
        return true;
    }

    if station.genre.to_lowercase().contains(&lower_query) {
        return true;
    }

    station
        .tags
        .iter()
        .any(|tag| tag.to_lowercase().contains(&lower_query))
}

/// Filter stations by case-insensitive substring match on name, genre, or any tag.
/// Empty/whitespace-only query returns the full input unchanged.
/// Query is truncated to 256 characters before matching.
pub fn filter_stations<'a>(stations: &'a [Station], query: &str) -> Vec<&'a Station> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return stations.iter().collect();
    }

    stations
        .iter()
        .filter(|station| station_matches_query(station, query))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_station(name: &str, genre: &str, tags: Vec<&str>) -> Station {
        let mut station = Station::basic(name, "http://test", genre, "US", 128);
        station.tags = tags.into_iter().map(String::from).collect();
        station
    }

    #[test]
    fn matches_by_name_substring() {
        let station = make_station("Nightride FM", "Synthwave", vec![]);
        assert!(station_matches_query(&station, "night"));
        assert!(station_matches_query(&station, "ride"));
    }

    #[test]
    fn matches_by_genre_substring() {
        let station = make_station("Station A", "Ambient Chill", vec![]);
        assert!(station_matches_query(&station, "ambient"));
        assert!(station_matches_query(&station, "chill"));
    }

    #[test]
    fn matches_by_tag_substring() {
        let station = make_station("Station A", "Rock", vec!["indie", "alternative"]);
        assert!(station_matches_query(&station, "indie"));
        assert!(station_matches_query(&station, "alter"));
    }

    #[test]
    fn match_is_case_insensitive() {
        let station = make_station("SomaFM", "Synthwave", vec!["Electronic"]);
        assert!(station_matches_query(&station, "SOMAFM"));
        assert!(station_matches_query(&station, "synthwave"));
        assert!(station_matches_query(&station, "ELECTRONIC"));
        assert!(station_matches_query(&station, "sOmAfM"));
    }

    #[test]
    fn empty_query_matches_all() {
        let station = make_station("Any Station", "Any Genre", vec![]);
        assert!(station_matches_query(&station, ""));
    }

    #[test]
    fn whitespace_only_query_matches_all() {
        let station = make_station("Any Station", "Any Genre", vec![]);
        assert!(station_matches_query(&station, "   "));
        assert!(station_matches_query(&station, "\t\n"));
    }

    #[test]
    fn no_match_returns_false() {
        let station = make_station("Jazz FM", "Jazz", vec!["smooth"]);
        assert!(!station_matches_query(&station, "rock"));
        assert!(!station_matches_query(&station, "metal"));
    }

    #[test]
    fn query_truncated_to_max_length() {
        // Station name is 256 'a' chars — matches a query of 256 'a' chars
        let long_name = "a".repeat(256);
        let station = make_station(&long_name, "Genre", vec![]);
        // Query is 256 'a' chars + extra 'b' chars beyond the limit
        let long_query = "a".repeat(256) + &"b".repeat(100);
        // After truncation to 256 chars, query is all 'a' — matches the name
        assert!(station_matches_query(&station, &long_query));
    }

    #[test]
    fn query_beyond_max_length_ignored_chars_dont_affect_match() {
        let station = make_station("a]", "Genre", vec![]);
        // Build a query where the first 256 chars are all 'z' (no match)
        // and chars beyond 256 would match — those are ignored
        let query = "z".repeat(256) + "a]";
        assert!(!station_matches_query(&station, &query));
    }

    #[test]
    fn filter_stations_empty_query_returns_all() {
        let stations = vec![
            make_station("A", "Rock", vec![]),
            make_station("B", "Jazz", vec![]),
            make_station("C", "Pop", vec![]),
        ];
        let result = filter_stations(&stations, "");
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn filter_stations_whitespace_query_returns_all() {
        let stations = vec![
            make_station("A", "Rock", vec![]),
            make_station("B", "Jazz", vec![]),
        ];
        let result = filter_stations(&stations, "   ");
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn filter_stations_narrows_by_query() {
        let stations = vec![
            make_station("Jazz FM", "Jazz", vec![]),
            make_station("Rock Radio", "Rock", vec![]),
            make_station("Smooth Jazz", "Jazz", vec!["smooth"]),
        ];
        let result = filter_stations(&stations, "jazz");
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].name, "Jazz FM");
        assert_eq!(result[1].name, "Smooth Jazz");
    }

    #[test]
    fn filter_stations_no_matches_returns_empty() {
        let stations = vec![
            make_station("Station A", "Rock", vec![]),
            make_station("Station B", "Pop", vec![]),
        ];
        let result = filter_stations(&stations, "classical");
        assert!(result.is_empty());
    }

    #[test]
    fn filter_stations_empty_input_returns_empty() {
        let stations: Vec<Station> = vec![];
        let result = filter_stations(&stations, "anything");
        assert!(result.is_empty());
    }

    #[test]
    fn match_is_unicode_case_insensitive() {
        let station = make_station("Ñoño Radio", "Música", vec!["español"]);
        assert!(station_matches_query(&station, "ñoño"));
        assert!(station_matches_query(&station, "ÑOÑO"));
        assert!(station_matches_query(&station, "música"));
        assert!(station_matches_query(&station, "ESPAÑOL"));
    }

    #[test]
    fn filter_handles_regex_special_chars_safely() {
        let stations = vec![
            make_station("Rock (Live)", "Rock", vec![]),
            make_station("Jazz [Smooth]", "Jazz", vec![]),
            make_station("C++ Radio", "Electronic", vec!["c++"]),
        ];
        // These would break if we used regex — contains() handles them fine
        assert_eq!(filter_stations(&stations, "(Live)").len(), 1);
        assert_eq!(filter_stations(&stations, "[Smooth]").len(), 1);
        assert_eq!(filter_stations(&stations, "c++").len(), 1);
        assert_eq!(filter_stations(&stations, ".*").len(), 0); // no station matches literal ".*"
    }
}
