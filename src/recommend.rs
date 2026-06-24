// Station recommendation engine — scores candidates against a favorites profile.

use std::collections::{HashMap, HashSet};

use crate::favorites_set::FavoritesSet;
use crate::radio::{normalized_station_url, Station};

/// A profile built from the user's favorited stations.
#[derive(Debug)]
pub struct FavoritesProfile {
    pub genres: HashMap<String, u32>,
    pub tags: HashMap<String, u32>,
    pub country_codes: HashMap<String, u32>,
}

/// Score a single candidate station against a favorites profile.
/// Returns 0 when profile is empty.
pub fn score_station(profile: &FavoritesProfile, candidate: &Station) -> u32 {
    if profile.genres.is_empty() && profile.tags.is_empty() && profile.country_codes.is_empty() {
        return 0;
    }

    let genre_score = if profile
        .genres
        .contains_key(&candidate.genre.trim().to_ascii_lowercase())
    {
        3
    } else {
        0
    };

    let tag_score = candidate
        .tags
        .iter()
        .filter(|t| profile.tags.contains_key(&t.trim().to_ascii_lowercase()))
        .count() as u32;

    let country_score = if profile
        .country_codes
        .contains_key(&candidate.country_code.trim().to_ascii_uppercase())
    {
        1
    } else {
        0
    };

    genre_score + tag_score + country_score
}

/// Compute a favorites profile from a slice of stations whose URLs are in the favorites set.
pub fn build_favorites_profile(
    stations: &[Station],
    favorites: &FavoritesSet,
) -> FavoritesProfile {
    let mut genres: HashMap<String, u32> = HashMap::new();
    let mut tags: HashMap<String, u32> = HashMap::new();
    let mut country_codes: HashMap<String, u32> = HashMap::new();

    for station in stations.iter().filter(|s| favorites.contains(&s.url)) {
        let genre = station.genre.trim().to_ascii_lowercase();
        if !genre.is_empty() {
            *genres.entry(genre).or_insert(0) += 1;
        }

        for tag in &station.tags {
            let normalized = tag.trim().to_ascii_lowercase();
            if !normalized.is_empty() {
                *tags.entry(normalized).or_insert(0) += 1;
            }
        }

        let code = station.country_code.trim().to_ascii_uppercase();
        if !code.is_empty() {
            *country_codes.entry(code).or_insert(0) += 1;
        }
    }

    FavoritesProfile {
        genres,
        tags,
        country_codes,
    }
}

const MAX_RECOMMENDATIONS: usize = 25;

/// Produce a ranked recommendation list (max 25 items, descending score).
/// Excludes stations whose URL is already in `library_urls`.
pub fn recommend(
    profile: &FavoritesProfile,
    candidates: &[Station],
    library_urls: &HashSet<String>,
) -> Vec<Station> {
    let mut scored: Vec<(u32, &Station)> = candidates
        .iter()
        .filter(|s| !library_urls.contains(&normalized_station_url(&s.url)))
        .map(|s| (score_station(profile, s), s))
        .filter(|(score, _)| *score > 0)
        .collect();

    scored.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| votes_of(b.1).cmp(&votes_of(a.1)))
            .then_with(|| clicks_of(b.1).cmp(&clicks_of(a.1)))
    });

    scored
        .into_iter()
        .take(MAX_RECOMMENDATIONS)
        .map(|(_, s)| s.clone())
        .collect()
}

fn votes_of(station: &Station) -> u32 {
    station.votes.unwrap_or(0)
}

fn clicks_of(station: &Station) -> u32 {
    station.click_count.unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_station(url: &str, genre: &str, tags: Vec<&str>, country_code: &str) -> Station {
        let mut station = Station::basic(url, url, genre, "", 128);
        station.tags = tags.into_iter().map(String::from).collect();
        station.country_code = country_code.to_string();
        station
    }

    fn favorites_with(urls: &[&str]) -> FavoritesSet {
        let mut set = FavoritesSet::default();
        for url in urls {
            set.toggle(url);
        }
        set
    }

    #[test]
    fn build_favorites_profile_empty_favorites_returns_empty_profile() {
        let stations = vec![make_station("http://a", "Rock", vec!["rock"], "US")];
        let favorites = FavoritesSet::default();

        let profile = build_favorites_profile(&stations, &favorites);

        assert!(profile.genres.is_empty());
        assert!(profile.tags.is_empty());
        assert!(profile.country_codes.is_empty());
    }

    #[test]
    fn build_favorites_profile_single_favorite() {
        let stations = vec![make_station(
            "http://a",
            "Jazz",
            vec!["smooth", "chill"],
            "DE",
        )];
        let favorites = favorites_with(&["http://a"]);

        let profile = build_favorites_profile(&stations, &favorites);

        assert_eq!(profile.genres.get("jazz"), Some(&1));
        assert_eq!(profile.tags.get("smooth"), Some(&1));
        assert_eq!(profile.tags.get("chill"), Some(&1));
        assert_eq!(profile.country_codes.get("DE"), Some(&1));
    }

    #[test]
    fn build_favorites_profile_multiple_favorites_with_overlapping_genres() {
        let stations = vec![
            make_station("http://a", "Rock", vec!["guitar", "loud"], "US"),
            make_station("http://b", "rock", vec!["guitar", "live"], "GB"),
            make_station("http://c", "Jazz", vec!["smooth"], "US"),
        ];
        let favorites = favorites_with(&["http://a", "http://b", "http://c"]);

        let profile = build_favorites_profile(&stations, &favorites);

        // "Rock" and "rock" should merge case-insensitively
        assert_eq!(profile.genres.get("rock"), Some(&2));
        assert_eq!(profile.genres.get("jazz"), Some(&1));
        // Tags
        assert_eq!(profile.tags.get("guitar"), Some(&2));
        assert_eq!(profile.tags.get("loud"), Some(&1));
        assert_eq!(profile.tags.get("live"), Some(&1));
        assert_eq!(profile.tags.get("smooth"), Some(&1));
        // Country codes
        assert_eq!(profile.country_codes.get("US"), Some(&2));
        assert_eq!(profile.country_codes.get("GB"), Some(&1));
    }

    #[test]
    fn build_favorites_profile_ignores_non_favorited_stations() {
        let stations = vec![
            make_station("http://a", "Rock", vec!["guitar"], "US"),
            make_station("http://b", "Jazz", vec!["smooth"], "DE"),
        ];
        let favorites = favorites_with(&["http://a"]);

        let profile = build_favorites_profile(&stations, &favorites);

        assert_eq!(profile.genres.len(), 1);
        assert_eq!(profile.genres.get("rock"), Some(&1));
        assert!(!profile.genres.contains_key("jazz"));
    }

    #[test]
    fn build_favorites_profile_country_codes_are_uppercase() {
        let stations = vec![make_station("http://a", "Pop", vec![], "de")];
        let favorites = favorites_with(&["http://a"]);

        let profile = build_favorites_profile(&stations, &favorites);

        assert_eq!(profile.country_codes.get("DE"), Some(&1));
        assert!(!profile.country_codes.contains_key("de"));
    }

    // --- score_station tests ---

    fn empty_profile() -> FavoritesProfile {
        FavoritesProfile {
            genres: HashMap::new(),
            tags: HashMap::new(),
            country_codes: HashMap::new(),
        }
    }

    fn profile_with(
        genres: &[&str],
        tags: &[&str],
        countries: &[&str],
    ) -> FavoritesProfile {
        FavoritesProfile {
            genres: genres.iter().map(|g| (g.to_string(), 1)).collect(),
            tags: tags.iter().map(|t| (t.to_string(), 1)).collect(),
            country_codes: countries.iter().map(|c| (c.to_string(), 1)).collect(),
        }
    }

    #[test]
    fn score_station_empty_profile_returns_zero() {
        let profile = empty_profile();
        let station = make_station("http://a", "Rock", vec!["guitar"], "US");

        assert_eq!(score_station(&profile, &station), 0);
    }

    #[test]
    fn score_station_exact_genre_match() {
        let profile = profile_with(&["rock"], &[], &[]);
        let station = make_station("http://a", "Rock", vec![], "");

        assert_eq!(score_station(&profile, &station), 3);
    }

    #[test]
    fn score_station_partial_tag_overlap() {
        let profile = profile_with(&[], &["guitar", "chill", "ambient"], &[]);
        let station = make_station("http://a", "", vec!["Guitar", "Live"], "");

        // Only "guitar" overlaps (case-insensitive) → 1 tag × 1 = 1
        assert_eq!(score_station(&profile, &station), 1);
    }

    #[test]
    fn score_station_country_match() {
        let profile = profile_with(&[], &[], &["US", "DE"]);
        let station = make_station("http://a", "", vec![], "de");

        assert_eq!(score_station(&profile, &station), 1);
    }

    #[test]
    fn score_station_combined_scoring() {
        let profile = profile_with(&["jazz"], &["smooth", "chill"], &["DE"]);
        let station = make_station("http://a", "Jazz", vec!["smooth", "chill", "live"], "DE");

        // genre: 3 + tags: 2 (smooth, chill) + country: 1 = 6
        assert_eq!(score_station(&profile, &station), 6);
    }

    // --- build_favorites_profile tests ---

    #[test]
    fn build_favorites_profile_skips_empty_genre_and_tags() {
        let mut station = Station::basic("http://a", "http://a", "", "", 128);
        station.tags = vec!["".to_string(), "  ".to_string()];
        station.country_code = String::new();
        let stations = vec![station];
        let favorites = favorites_with(&["http://a"]);

        let profile = build_favorites_profile(&stations, &favorites);

        assert!(profile.genres.is_empty());
        assert!(profile.tags.is_empty());
        assert!(profile.country_codes.is_empty());
    }

    // --- recommend tests ---

    #[test]
    fn recommend_empty_candidates_returns_empty() {
        let profile = profile_with(&["rock"], &["guitar"], &["US"]);
        let library = HashSet::new();

        let result = recommend(&profile, &[], &library);

        assert!(result.is_empty());
    }

    #[test]
    fn recommend_excludes_library_urls() {
        let profile = profile_with(&["rock"], &[], &[]);
        let candidates = vec![
            make_station("http://a", "Rock", vec![], ""),
            make_station("http://b", "Rock", vec![], ""),
        ];
        let library: HashSet<String> =
            vec!["http://a".to_string()].into_iter().collect();

        let result = recommend(&profile, &candidates, &library);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].url, "http://b");
    }

    #[test]
    fn recommend_max_25_results() {
        let profile = profile_with(&["rock"], &[], &[]);
        let candidates: Vec<Station> = (0..50)
            .map(|i| make_station(&format!("http://s{i}"), "Rock", vec![], ""))
            .collect();
        let library = HashSet::new();

        let result = recommend(&profile, &candidates, &library);

        assert_eq!(result.len(), 25);
    }

    #[test]
    fn recommend_tie_breaking_by_votes_then_clicks() {
        let profile = profile_with(&["rock"], &[], &[]);
        let mut s1 = make_station("http://a", "Rock", vec![], "");
        s1.votes = Some(10);
        s1.click_count = Some(5);
        let mut s2 = make_station("http://b", "Rock", vec![], "");
        s2.votes = Some(10);
        s2.click_count = Some(20);
        let mut s3 = make_station("http://c", "Rock", vec![], "");
        s3.votes = Some(50);
        s3.click_count = Some(1);
        let candidates = vec![s1, s2, s3];
        let library = HashSet::new();

        let result = recommend(&profile, &candidates, &library);

        // All have same score (3), so tie-break by votes desc, then clicks desc
        assert_eq!(result[0].url, "http://c"); // votes=50
        assert_eq!(result[1].url, "http://b"); // votes=10, clicks=20
        assert_eq!(result[2].url, "http://a"); // votes=10, clicks=5
    }

    #[test]
    fn recommend_excludes_zero_score_candidates() {
        let profile = profile_with(&["jazz"], &[], &[]);
        let candidates = vec![
            make_station("http://a", "Rock", vec![], ""),
            make_station("http://b", "Jazz", vec![], ""),
        ];
        let library = HashSet::new();

        let result = recommend(&profile, &candidates, &library);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].url, "http://b");
    }

    #[test]
    fn recommend_library_url_normalization() {
        let profile = profile_with(&["rock"], &[], &[]);
        let candidates = vec![make_station("HTTP://A/", "Rock", vec![], "")];
        // Library contains normalized form
        let library: HashSet<String> = vec!["http://a".to_string()].into_iter().collect();

        let result = recommend(&profile, &candidates, &library);

        assert!(result.is_empty());
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::collection::vec as prop_vec;
    use proptest::prelude::*;

    // --- Shared generators for scoring formula test ---

    fn arb_genre() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("rock".to_string()),
            Just("jazz".to_string()),
            Just("pop".to_string()),
            Just("electronic".to_string()),
            Just("classical".to_string()),
            Just("ambient".to_string()),
            Just("metal".to_string()),
            Just("blues".to_string()),
        ]
    }

    fn arb_tag() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("guitar".to_string()),
            Just("chill".to_string()),
            Just("smooth".to_string()),
            Just("live".to_string()),
            Just("instrumental".to_string()),
            Just("vocal".to_string()),
            Just("bass".to_string()),
            Just("synth".to_string()),
            Just("piano".to_string()),
            Just("drums".to_string()),
        ]
    }

    fn arb_country_code() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("US".to_string()),
            Just("DE".to_string()),
            Just("GB".to_string()),
            Just("FR".to_string()),
            Just("JP".to_string()),
            Just("BR".to_string()),
        ]
    }

    fn arb_favorites_profile() -> impl Strategy<Value = FavoritesProfile> {
        (
            proptest::collection::hash_map(arb_genre(), 1..5u32, 0..5),
            proptest::collection::hash_map(arb_tag(), 1..5u32, 0..6),
            proptest::collection::hash_map(arb_country_code(), 1..5u32, 0..4),
        )
            .prop_map(|(genres, tags, country_codes)| FavoritesProfile {
                genres,
                tags,
                country_codes,
            })
    }

    fn arb_station_scored() -> impl Strategy<Value = Station> {
        (
            arb_genre(),
            proptest::collection::vec(arb_tag(), 0..5),
            arb_country_code(),
        )
            .prop_map(|(genre, tags, country_code)| {
                let mut station = Station::basic("http://test", "test", &genre, "", 128);
                station.tags = tags;
                station.country_code = country_code;
                station
            })
    }

    // --- Generators for recommendation output invariants ---

    fn arb_profile_regex() -> impl Strategy<Value = FavoritesProfile> {
        let genres = prop::collection::hash_map("[a-z]{1,8}", 1..5u32, 0..5);
        let tags = prop::collection::hash_map("[a-z]{1,8}", 1..5u32, 0..5);
        let country_codes = prop::collection::hash_map("[A-Z]{2}", 1..5u32, 0..3);
        (genres, tags, country_codes).prop_map(|(genres, tags, country_codes)| FavoritesProfile {
            genres,
            tags,
            country_codes,
        })
    }

    fn arb_station_full() -> impl Strategy<Value = Station> {
        let url = "[a-z]{3,10}://[a-z]{2,8}(\\.[a-z]{2,5}){1,2}/[a-z]{1,6}";
        let genre = "[a-z]{1,8}";
        let tags = prop::collection::vec("[a-z]{1,8}", 0..4);
        let country_code = "[A-Z]{2}";
        let votes = prop::option::of(0..1000u32);
        let clicks = prop::option::of(0..1000u32);
        (url, genre, tags, country_code, votes, clicks).prop_map(
            |(url, genre, tags, country_code, votes, clicks)| {
                let mut station = Station::basic(&url, &url, &genre, "", 128);
                station.tags = tags;
                station.country_code = country_code;
                station.votes = votes;
                station.click_count = clicks;
                station
            },
        )
    }

    fn arb_library_urls() -> impl Strategy<Value = HashSet<String>> {
        prop::collection::hash_set("[a-z]{3,10}://[a-z]{2,8}/[a-z]{1,6}", 0..5)
    }

    // --- Generators for profile aggregation ---

    fn arb_genre_aggregation() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("".to_string()),
            "[a-zA-Z ]{1,15}".prop_map(|s| s.to_string()),
        ]
    }

    fn arb_tag_aggregation() -> impl Strategy<Value = String> {
        "[a-zA-Z ]{1,10}".prop_map(|s| s.to_string())
    }

    fn arb_country_code_aggregation() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("".to_string()),
            "[a-zA-Z]{2}".prop_map(|s| s.to_string()),
        ]
    }

    fn arb_station_aggregation(index: usize) -> impl Strategy<Value = Station> {
        (
            arb_genre_aggregation(),
            prop_vec(arb_tag_aggregation(), 0..4),
            arb_country_code_aggregation(),
        )
            .prop_map(move |(genre, tags, country_code)| {
                let url = format!("http://station-{index}");
                let mut station = Station::basic(&url, &url, &genre, "", 128);
                station.tags = tags;
                station.country_code = country_code;
                station
            })
    }

    fn arb_stations_aggregation() -> impl Strategy<Value = Vec<Station>> {
        (1..=10usize).prop_flat_map(|count| {
            let strategies: Vec<_> = (0..count).map(arb_station_aggregation).collect();
            strategies
        })
    }

    // Feature: v080-features, Property 1: Scoring formula correctness
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Validates: Requirements 1.2**
        #[test]
        fn scoring_formula_correctness(
            profile in arb_favorites_profile(),
            candidate in arb_station_scored(),
        ) {
            let actual = score_station(&profile, &candidate);

            let genre_score: u32 = if profile
                .genres
                .contains_key(&candidate.genre.trim().to_ascii_lowercase())
            {
                3
            } else {
                0
            };

            let tag_score: u32 = candidate
                .tags
                .iter()
                .filter(|t| profile.tags.contains_key(&t.trim().to_ascii_lowercase()))
                .count() as u32;

            let country_score: u32 = if profile
                .country_codes
                .contains_key(&candidate.country_code.trim().to_ascii_uppercase())
            {
                1
            } else {
                0
            };

            let expected = genre_score + tag_score + country_score;

            let profile_empty = profile.genres.is_empty()
                && profile.tags.is_empty()
                && profile.country_codes.is_empty();

            if profile_empty {
                prop_assert_eq!(actual, 0, "empty profile should yield 0");
            } else {
                prop_assert_eq!(
                    actual, expected,
                    "score mismatch: genre_score={}, tag_score={}, country_score={}, \
                     candidate genre='{}', tags={:?}, country='{}'",
                    genre_score, tag_score, country_score,
                    candidate.genre, candidate.tags, candidate.country_code
                );
            }
        }
    }

    // Feature: v080-features, Property 2: Recommendation output invariants
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Validates: Requirements 1.4, 1.5, 1.6**
        #[test]
        fn recommend_output_invariants(
            profile in arb_profile_regex(),
            candidates in prop::collection::vec(arb_station_full(), 0..60),
            library_urls in arb_library_urls(),
        ) {
            let result = recommend(&profile, &candidates, &library_urls);

            // (a) length ≤ 25
            prop_assert!(
                result.len() <= 25,
                "result length {} exceeds 25", result.len()
            );

            // (b) no result URL is in library_urls (after normalization)
            for station in &result {
                let normalized = normalized_station_url(&station.url);
                prop_assert!(
                    !library_urls.contains(&normalized),
                    "result contains library URL: {}", station.url
                );
            }

            // (c) consecutive pairs sorted descending by score, then votes, then clicks
            for pair in result.windows(2) {
                let score_a = score_station(&profile, &pair[0]);
                let score_b = score_station(&profile, &pair[1]);
                let votes_a = pair[0].votes.unwrap_or(0);
                let votes_b = pair[1].votes.unwrap_or(0);
                let clicks_a = pair[0].click_count.unwrap_or(0);
                let clicks_b = pair[1].click_count.unwrap_or(0);

                let ordering = score_a.cmp(&score_b).reverse()
                    .then(votes_a.cmp(&votes_b).reverse())
                    .then(clicks_a.cmp(&clicks_b).reverse());

                prop_assert!(
                    ordering != std::cmp::Ordering::Greater,
                    "ordering violated: ({}, v={}, c={}) should come before ({}, v={}, c={})",
                    score_a, votes_a, clicks_a,
                    score_b, votes_b, clicks_b
                );
            }
        }
    }

    // Feature: v080-features, Property 3: Profile aggregation correctness
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Validates: Requirements 1.1**
        #[test]
        fn profile_aggregation_correctness(
            stations in arb_stations_aggregation(),
            seed in any::<u64>(),
        ) {
            let mut favorites = FavoritesSet::default();
            for (i, station) in stations.iter().enumerate() {
                if (seed.wrapping_mul(i as u64 + 1)) % 3 != 0 {
                    favorites.toggle(&station.url);
                }
            }

            let profile = build_favorites_profile(&stations, &favorites);

            let mut expected_genres: HashMap<String, u32> = HashMap::new();
            let mut expected_tags: HashMap<String, u32> = HashMap::new();
            let mut expected_country_codes: HashMap<String, u32> = HashMap::new();

            for station in stations.iter().filter(|s| favorites.contains(&s.url)) {
                let genre = station.genre.trim().to_ascii_lowercase();
                if !genre.is_empty() {
                    *expected_genres.entry(genre).or_insert(0) += 1;
                }

                for tag in &station.tags {
                    let normalized = tag.trim().to_ascii_lowercase();
                    if !normalized.is_empty() {
                        *expected_tags.entry(normalized).or_insert(0) += 1;
                    }
                }

                let code = station.country_code.trim().to_ascii_uppercase();
                if !code.is_empty() {
                    *expected_country_codes.entry(code).or_insert(0) += 1;
                }
            }

            prop_assert_eq!(&profile.genres, &expected_genres, "genres mismatch");
            prop_assert_eq!(&profile.tags, &expected_tags, "tags mismatch");
            prop_assert_eq!(
                &profile.country_codes, &expected_country_codes,
                "country_codes mismatch"
            );
        }
    }
}
