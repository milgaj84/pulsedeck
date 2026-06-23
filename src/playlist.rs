use crate::radio::Station;

pub fn to_m3u(stations: &[Station]) -> String {
    let mut out = String::from("#EXTM3U\n");
    for s in stations {
        out.push_str(&format!("#EXTINF:-1,{}\n", s.name));
        out.push_str(&format!("#EXTGENRE:{}\n", s.genre));
        out.push_str(&format!("{}\n", s.url));
    }
    out
}

pub fn from_m3u(text: &str) -> Vec<Station> {
    let mut stations = Vec::new();
    let mut name = String::new();
    let mut genre = String::new();
    for line in text.lines().map(str::trim) {
        if let Some(rest) = line.strip_prefix("#EXTINF:") {
            name = rest
                .split_once(',')
                .map(|x| x.1)
                .unwrap_or("")
                .trim()
                .to_string();
        } else if let Some(rest) = line.strip_prefix("#EXTGENRE:") {
            genre = rest.trim().to_string();
        } else if !line.is_empty() && !line.starts_with('#') {
            stations.push(Station::basic(
                if name.is_empty() {
                    line.to_string()
                } else {
                    name.clone()
                },
                line.to_string(),
                if genre.is_empty() {
                    "Unknown".to_string()
                } else {
                    genre.clone()
                },
                String::new(),
                0,
            ));
            name.clear();
            genre.clear();
        }
    }
    stations
}

pub fn to_json(stations: &[Station]) -> serde_json::Result<String> {
    serde_json::to_string_pretty(stations)
}

pub fn from_json(text: &str) -> serde_json::Result<Vec<Station>> {
    serde_json::from_str(text)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaylistFormat {
    M3u,
    Json,
}

pub fn format_for_path(path: &str) -> PlaylistFormat {
    if path
        .rsplit('.')
        .next()
        .is_some_and(|e| e.eq_ignore_ascii_case("json"))
    {
        PlaylistFormat::Json
    } else {
        PlaylistFormat::M3u
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_m3u_roundtrip() {
        let stations = vec![
            Station::basic("Station A", "http://a", "Synthwave", "US", 128),
            Station::basic("Station B", "http://b", "Ambient", "UK", 96),
        ];

        let m3u = to_m3u(&stations);
        let parsed = from_m3u(&m3u);
        assert_eq!(parsed.len(), 2);

        assert_eq!(parsed[0].name, "Station A");
        assert_eq!(parsed[0].url, "http://a");
        assert_eq!(parsed[0].genre, "Synthwave");

        assert_eq!(parsed[1].name, "Station B");
        assert_eq!(parsed[1].url, "http://b");
        assert_eq!(parsed[1].genre, "Ambient");
    }

    #[test]
    fn test_m3u_from_bare_urls() {
        let text = "http://a\nhttp://b\n";
        let parsed = from_m3u(text);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].name, "http://a");
        assert_eq!(parsed[0].url, "http://a");
        assert_eq!(parsed[0].genre, "Unknown");
    }

    #[test]
    fn test_format_for_path() {
        assert_eq!(format_for_path("foo.json"), PlaylistFormat::Json);
        assert_eq!(format_for_path("foo.JSON"), PlaylistFormat::Json);
        assert_eq!(format_for_path("foo.m3u"), PlaylistFormat::M3u);
        assert_eq!(format_for_path("foo.txt"), PlaylistFormat::M3u);
        assert_eq!(format_for_path("foo"), PlaylistFormat::M3u);
    }

    #[test]
    fn test_json_roundtrip() {
        let stations = vec![Station::basic(
            "Station A",
            "http://a",
            "Synthwave",
            "US",
            128,
        )];

        let json = to_json(&stations).unwrap();
        let parsed = from_json(&json).unwrap();
        assert_eq!(parsed, stations);
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    /// Strategy for generating valid Station instances for playlist property tests.
    ///
    /// Constraints for M3U round-trip compatibility:
    /// - Names: non-empty, no newlines, no leading/trailing whitespace (trim-stable)
    /// - URLs: non-empty, only safe ASCII chars, no '#' prefix, no whitespace
    /// - Genres: non-empty, no newlines, no leading/trailing whitespace (trim-stable)
    fn arb_station() -> impl Strategy<Value = Station> {
        (
            "[^\n]{1,200}",                          // name: non-empty, no newlines
            "[a-zA-Z0-9_./:~@!$&*+,;=%\\-]{1,2000}", // url: safe ASCII, no whitespace, no '#'
            "[^\n]{1,100}",                          // genre: non-empty, no newlines
            "[A-Z]{0,2}",                            // country: 0-2 uppercase ASCII chars
            0u32..=1024u32,                          // bitrate
        )
            .prop_map(|(name, url, genre, country, bitrate)| {
                // Trim name/genre to ensure round-trip stability (from_m3u trims parsed values)
                let name = name.trim().to_string();
                let genre = genre.trim().to_string();
                // Ensure non-empty after trim
                let name = if name.is_empty() {
                    "Station".to_string()
                } else {
                    name
                };
                let genre = if genre.is_empty() {
                    "Genre".to_string()
                } else {
                    genre
                };
                Station::basic(name, url, genre, country, bitrate)
            })
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Feature: test-coverage-improvement, Property 6: JSON playlist round-trip**
        ///
        /// For any valid Station list, from_json(to_json(stations)) produces an equal list.
        ///
        /// **Validates: Requirements 7.1**
        #[test]
        fn json_round_trip(stations in proptest::collection::vec(arb_station(), 0..=50)) {
            let json = to_json(&stations).unwrap();
            let parsed = from_json(&json).unwrap();
            prop_assert_eq!(parsed.len(), stations.len());
            for (original, restored) in stations.iter().zip(parsed.iter()) {
                prop_assert_eq!(&original.name, &restored.name);
                prop_assert_eq!(&original.url, &restored.url);
                prop_assert_eq!(&original.genre, &restored.genre);
                prop_assert_eq!(&original.country, &restored.country);
                prop_assert_eq!(original.bitrate, restored.bitrate);
            }
        }

        /// **Feature: test-coverage-improvement, Property 7: M3U playlist round-trip**
        ///
        /// For any valid Station list, from_m3u(to_m3u(stations)) produces same length
        /// list with identical name, URL, and genre per element.
        ///
        /// **Validates: Requirements 7.2**
        #[test]
        fn m3u_round_trip(stations in proptest::collection::vec(arb_station(), 0..=50)) {
            let m3u = to_m3u(&stations);
            let parsed = from_m3u(&m3u);
            prop_assert_eq!(parsed.len(), stations.len());
            for (original, restored) in stations.iter().zip(parsed.iter()) {
                prop_assert_eq!(&original.name, &restored.name);
                prop_assert_eq!(&original.url, &restored.url);
                prop_assert_eq!(&original.genre, &restored.genre);
            }
        }

        /// **Feature: test-coverage-improvement, Property 8: M3U structural invariant**
        ///
        /// For any valid Station list (1–50), to_m3u output starts with `#EXTM3U\n` and
        /// contains exactly one non-comment, non-empty line per station equal to that station's URL.
        ///
        /// **Validates: Requirements 7.3**
        #[test]
        fn m3u_structural_invariant(stations in proptest::collection::vec(arb_station(), 1..=50)) {
            let m3u = to_m3u(&stations);
            prop_assert!(m3u.starts_with("#EXTM3U\n"));

            let url_lines: Vec<&str> = m3u
                .lines()
                .filter(|line| !line.is_empty() && !line.starts_with('#'))
                .collect();
            prop_assert_eq!(url_lines.len(), stations.len());
            for (station, url_line) in stations.iter().zip(url_lines.iter()) {
                prop_assert_eq!(*url_line, station.url.as_str());
            }
        }

        /// **Feature: test-coverage-improvement, Property 9: JSON output validity**
        ///
        /// For any valid Station list, to_json output is parseable by serde_json without error.
        ///
        /// **Validates: Requirements 7.4**
        #[test]
        fn json_output_validity(stations in proptest::collection::vec(arb_station(), 0..=50)) {
            let json = to_json(&stations).unwrap();
            let result: Result<serde_json::Value, _> = serde_json::from_str(&json);
            prop_assert!(result.is_ok(), "to_json produced invalid JSON: {:?}", result.err());
        }
    }
}
