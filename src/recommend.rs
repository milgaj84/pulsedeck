// Station recommendation engine — scores candidates against a favorites profile.

use std::collections::{HashMap, HashSet};

use crate::config_toml::DiscoverConfig;
use crate::favorites_set::FavoritesSet;
use crate::radio::{normalized_station_url, Station};

/// A station paired with its recommendation score.
#[derive(Debug, Clone)]
pub struct ScoredStation {
    pub station: Station,
    pub score: u32,
}

/// Configurable weights for the discover scoring formula.
#[derive(Debug, Clone, PartialEq)]
pub struct ScoringWeights {
    pub genre_weight: u32,
    pub tag_weight: u32,
    pub country_weight: u32,
}

impl Default for ScoringWeights {
    fn default() -> Self {
        Self {
            genre_weight: 3,
            tag_weight: 1,
            country_weight: 1,
        }
    }
}

/// A profile built from the user's favorited stations.
#[derive(Debug)]
pub struct FavoritesProfile {
    pub genres: HashMap<String, u32>,
    pub tags: HashMap<String, u32>,
    pub country_codes: HashMap<String, u32>,
}

/// Score a single candidate station against a favorites profile.
/// Returns 0 when profile is empty.
pub fn score_station(
    profile: &FavoritesProfile,
    candidate: &Station,
    weights: &ScoringWeights,
) -> u32 {
    if profile.genres.is_empty() && profile.tags.is_empty() && profile.country_codes.is_empty() {
        return 0;
    }

    let genre_match: u32 = if profile
        .genres
        .contains_key(&candidate.genre.trim().to_ascii_lowercase())
    {
        1
    } else {
        0
    };

    let matching_tag_count: u32 = candidate
        .tags
        .iter()
        .filter(|t| profile.tags.contains_key(&t.trim().to_ascii_lowercase()))
        .count() as u32;

    let country_match: u32 = if profile
        .country_codes
        .contains_key(&candidate.country_code.trim().to_ascii_uppercase())
    {
        1
    } else {
        0
    };

    (weights.genre_weight * genre_match)
        + (weights.tag_weight * matching_tag_count)
        + (weights.country_weight * country_match)
}

/// Explanation of which factors contributed to a station's score.
#[derive(Debug, PartialEq)]
pub struct ScoreExplanation {
    pub genres: Vec<String>,
    pub tags: Vec<String>,
    pub countries: Vec<String>,
}

/// Explain which factors contributed to a station's score.
/// Returns lists of matching genres, tags, and countries.
pub fn explain_score(profile: &FavoritesProfile, station: &Station) -> ScoreExplanation {
    let genre = station.genre.trim().to_ascii_lowercase();
    let genres = if profile.genres.contains_key(&genre) {
        vec![genre]
    } else {
        vec![]
    };

    let tags: Vec<String> = station
        .tags
        .iter()
        .map(|t| t.trim().to_ascii_lowercase())
        .filter(|t| profile.tags.contains_key(t))
        .collect();

    let country = station.country_code.trim().to_ascii_uppercase();
    let countries = if profile.country_codes.contains_key(&country) {
        vec![country]
    } else {
        vec![]
    };

    ScoreExplanation {
        genres,
        tags,
        countries,
    }
}

/// Compute a favorites profile from a slice of stations whose URLs are in the favorites set.
pub fn build_favorites_profile(stations: &[Station], favorites: &FavoritesSet) -> FavoritesProfile {
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
const PREFILTER_THRESHOLD: usize = 1000;
const MAX_TOP_GENRES: usize = 5;
const MAX_TOP_TAGS: usize = 10;

/// Select top genres from profile by occurrence count (up to 5, ties at boundary included).
pub fn select_top_genres(profile: &FavoritesProfile) -> HashSet<String> {
    select_top_n(&profile.genres, MAX_TOP_GENRES)
}

/// Select top tags from profile by occurrence count (up to 10, ties at boundary included).
pub fn select_top_tags(profile: &FavoritesProfile) -> HashSet<String> {
    select_top_n(&profile.tags, MAX_TOP_TAGS)
}

/// Generic top-N selection with tie inclusion at the boundary.
fn select_top_n(counts: &HashMap<String, u32>, limit: usize) -> HashSet<String> {
    if counts.is_empty() {
        return HashSet::new();
    }
    let mut entries: Vec<(&String, &u32)> = counts.iter().collect();
    entries.sort_by(|a, b| b.1.cmp(a.1));

    let boundary_count = if entries.len() > limit {
        *entries[limit - 1].1
    } else {
        0 // include all when fewer entries than limit
    };

    entries
        .into_iter()
        .filter(|(_, count)| **count >= boundary_count)
        .map(|(key, _)| key.clone())
        .collect()
}

/// Retain candidates whose genre or tags overlap with the top genres/tags.
fn prefilter_candidates<'a>(
    candidates: &'a [Station],
    top_genres: &HashSet<String>,
    top_tags: &HashSet<String>,
) -> Vec<&'a Station> {
    candidates
        .iter()
        .filter(|station| {
            let genre = station.genre.trim().to_ascii_lowercase();
            if top_genres.contains(&genre) {
                return true;
            }
            station
                .tags
                .iter()
                .any(|t| top_tags.contains(&t.trim().to_ascii_lowercase()))
        })
        .collect()
}

/// Produce a ranked recommendation list (max 25 items, descending score).
/// Excludes stations whose URL is already in `library_urls`.
/// Excludes stations matching `config.exclude_tags` (by genre or tag) or `config.exclude_countries`.
/// Applies pre-filter when candidates exceed 1000 and profile has genres or tags.
pub fn recommend(
    profile: &FavoritesProfile,
    candidates: &[Station],
    library_urls: &HashSet<String>,
    config: &DiscoverConfig,
) -> Vec<ScoredStation> {
    let weights = ScoringWeights {
        genre_weight: config.genre_weight,
        tag_weight: config.tag_weight,
        country_weight: config.country_weight,
    };

    let effective_candidates = maybe_prefilter(profile, candidates);
    let iter: Box<dyn Iterator<Item = &Station>> = match &effective_candidates {
        Some(filtered) => Box::new(filtered.iter().copied()),
        None => Box::new(candidates.iter()),
    };

    let mut scored: Vec<(u32, &Station)> = iter
        .filter(|s| !library_urls.contains(&normalized_station_url(&s.url)))
        .filter(|s| !is_excluded(s, config))
        .map(|s| (score_station(profile, s, &weights), s))
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
        .map(|(score, s)| ScoredStation {
            station: s.clone(),
            score,
        })
        .collect()
}

/// Check if a station should be excluded based on config exclusion lists.
fn is_excluded(station: &Station, config: &DiscoverConfig) -> bool {
    if is_excluded_by_tags(station, &config.exclude_tags) {
        return true;
    }
    is_excluded_by_country(station, &config.exclude_countries)
}

/// Check if a station's genre or any of its tags match the exclude_tags list.
fn is_excluded_by_tags(station: &Station, exclude_tags: &[String]) -> bool {
    if exclude_tags.is_empty() {
        return false;
    }
    let genre = station.genre.trim().to_ascii_lowercase();
    if exclude_tags.iter().any(|t| t == &genre) {
        return true;
    }
    station.tags.iter().any(|tag| {
        let normalized = tag.trim().to_ascii_lowercase();
        exclude_tags.iter().any(|t| t == &normalized)
    })
}

/// Check if a station's country_code matches the exclude_countries list.
fn is_excluded_by_country(station: &Station, exclude_countries: &[String]) -> bool {
    if exclude_countries.is_empty() {
        return false;
    }
    let country = station.country_code.trim().to_ascii_uppercase();
    exclude_countries.iter().any(|c| c == &country)
}

/// Apply pre-filter if candidates exceed threshold and profile has genres or tags.
fn maybe_prefilter<'a>(
    profile: &FavoritesProfile,
    candidates: &'a [Station],
) -> Option<Vec<&'a Station>> {
    if candidates.len() <= PREFILTER_THRESHOLD {
        return None;
    }
    if profile.genres.is_empty() && profile.tags.is_empty() {
        return None;
    }
    let top_genres = select_top_genres(profile);
    let top_tags = select_top_tags(profile);
    Some(prefilter_candidates(candidates, &top_genres, &top_tags))
}

/// Select the next-best genre or tag from a profile, different from the primary.
/// Combines genres and tags, sorts by count descending, returns the first entry
/// that differs from `primary`. Returns None if no alternative exists.
pub fn select_fallback_tag(profile: &FavoritesProfile, primary: &str) -> Option<String> {
    let mut entries: Vec<(&String, &u32)> =
        profile.genres.iter().chain(profile.tags.iter()).collect();
    entries.sort_by(|a, b| b.1.cmp(a.1));
    entries
        .into_iter()
        .find(|(name, _)| name.as_str() != primary)
        .map(|(name, _)| name.clone())
}

/// Deduplicate stations by normalized URL, keeping first occurrence.
pub fn deduplicate_stations(stations: &[Station]) -> Vec<Station> {
    let mut seen = HashSet::new();
    stations
        .iter()
        .filter(|s| seen.insert(normalized_station_url(&s.url)))
        .cloned()
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

    fn profile_with(genres: &[&str], tags: &[&str], countries: &[&str]) -> FavoritesProfile {
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

        assert_eq!(
            score_station(&profile, &station, &ScoringWeights::default()),
            0
        );
    }

    #[test]
    fn score_station_exact_genre_match() {
        let profile = profile_with(&["rock"], &[], &[]);
        let station = make_station("http://a", "Rock", vec![], "");

        assert_eq!(
            score_station(&profile, &station, &ScoringWeights::default()),
            3
        );
    }

    #[test]
    fn score_station_partial_tag_overlap() {
        let profile = profile_with(&[], &["guitar", "chill", "ambient"], &[]);
        let station = make_station("http://a", "", vec!["Guitar", "Live"], "");

        // Only "guitar" overlaps (case-insensitive) → 1 tag × 1 = 1
        assert_eq!(
            score_station(&profile, &station, &ScoringWeights::default()),
            1
        );
    }

    #[test]
    fn score_station_country_match() {
        let profile = profile_with(&[], &[], &["US", "DE"]);
        let station = make_station("http://a", "", vec![], "de");

        assert_eq!(
            score_station(&profile, &station, &ScoringWeights::default()),
            1
        );
    }

    #[test]
    fn score_station_combined_scoring() {
        let profile = profile_with(&["jazz"], &["smooth", "chill"], &["DE"]);
        let station = make_station("http://a", "Jazz", vec!["smooth", "chill", "live"], "DE");

        // genre: 3 + tags: 2 (smooth, chill) + country: 1 = 6
        assert_eq!(
            score_station(&profile, &station, &ScoringWeights::default()),
            6
        );
    }

    #[test]
    fn score_station_custom_weights_change_scores() {
        let profile = profile_with(&["rock"], &["guitar"], &["US"]);
        let station = make_station("http://a", "Rock", vec!["guitar"], "US");
        let weights = ScoringWeights {
            genre_weight: 5,
            tag_weight: 1,
            country_weight: 1,
        };

        // genre: 5×1 + tag: 1×1 + country: 1×1 = 7
        assert_eq!(score_station(&profile, &station, &weights), 7);
    }

    #[test]
    fn score_station_all_zero_weights_returns_zero() {
        let profile = profile_with(&["rock"], &["guitar", "chill"], &["US"]);
        let station = make_station("http://a", "Rock", vec!["guitar", "chill"], "US");
        let weights = ScoringWeights {
            genre_weight: 0,
            tag_weight: 0,
            country_weight: 0,
        };

        assert_eq!(score_station(&profile, &station, &weights), 0);
    }

    #[test]
    fn score_station_tag_weight_multiplies_matching_count() {
        let profile = profile_with(&[], &["guitar", "chill", "smooth"], &[]);
        let station = make_station("http://a", "", vec!["guitar", "chill", "smooth"], "");
        let weights = ScoringWeights {
            genre_weight: 0,
            tag_weight: 2,
            country_weight: 0,
        };

        // tag: 2×3 = 6
        assert_eq!(score_station(&profile, &station, &weights), 6);
    }

    #[test]
    fn score_station_default_weights_match_original_behavior() {
        let profile = profile_with(&["jazz"], &["smooth", "chill"], &["DE"]);
        let station = make_station("http://a", "Jazz", vec!["smooth", "chill", "live"], "DE");

        // Original hardcoded: genre=3, tag=1 per match, country=1
        // Default weights: genre_weight=3, tag_weight=1, country_weight=1
        // Expected: 3×1 + 1×2 + 1×1 = 6
        assert_eq!(
            score_station(&profile, &station, &ScoringWeights::default()),
            6
        );
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
        let config = DiscoverConfig::default();

        let result = recommend(&profile, &[], &library, &config);

        assert!(result.is_empty());
    }

    #[test]
    fn recommend_excludes_library_urls() {
        let profile = profile_with(&["rock"], &[], &[]);
        let candidates = vec![
            make_station("http://a", "Rock", vec![], ""),
            make_station("http://b", "Rock", vec![], ""),
        ];
        let library: HashSet<String> = vec!["http://a".to_string()].into_iter().collect();
        let config = DiscoverConfig::default();

        let result = recommend(&profile, &candidates, &library, &config);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].station.url, "http://b");
    }

    #[test]
    fn recommend_max_25_results() {
        let profile = profile_with(&["rock"], &[], &[]);
        let candidates: Vec<Station> = (0..50)
            .map(|i| make_station(&format!("http://s{i}"), "Rock", vec![], ""))
            .collect();
        let library = HashSet::new();
        let config = DiscoverConfig::default();

        let result = recommend(&profile, &candidates, &library, &config);

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
        let config = DiscoverConfig::default();

        let result = recommend(&profile, &candidates, &library, &config);

        // All have same score (3), so tie-break by votes desc, then clicks desc
        assert_eq!(result[0].station.url, "http://c"); // votes=50
        assert_eq!(result[1].station.url, "http://b"); // votes=10, clicks=20
        assert_eq!(result[2].station.url, "http://a"); // votes=10, clicks=5
    }

    #[test]
    fn recommend_excludes_zero_score_candidates() {
        let profile = profile_with(&["jazz"], &[], &[]);
        let candidates = vec![
            make_station("http://a", "Rock", vec![], ""),
            make_station("http://b", "Jazz", vec![], ""),
        ];
        let library = HashSet::new();
        let config = DiscoverConfig::default();

        let result = recommend(&profile, &candidates, &library, &config);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].station.url, "http://b");
    }

    #[test]
    fn recommend_library_url_normalization() {
        let profile = profile_with(&["rock"], &[], &[]);
        let candidates = vec![make_station("HTTP://A/", "Rock", vec![], "")];
        // Library contains normalized form
        let library: HashSet<String> = vec!["http://a".to_string()].into_iter().collect();
        let config = DiscoverConfig::default();

        let result = recommend(&profile, &candidates, &library, &config);

        assert!(result.is_empty());
    }

    // --- recommend exclusion tests ---

    #[test]
    fn test_recommend_excludes_by_tag() {
        let profile = profile_with(&["rock"], &["guitar", "metal"], &[]);
        let candidates = vec![
            make_station("http://a", "Rock", vec!["guitar"], "US"),
            make_station("http://b", "Rock", vec!["metal"], "US"),
            make_station("http://c", "Rock", vec!["live"], "US"),
        ];
        let library = HashSet::new();
        let config = DiscoverConfig {
            exclude_tags: vec!["metal".to_string()],
            ..DiscoverConfig::default()
        };

        let result = recommend(&profile, &candidates, &library, &config);

        // Station "http://b" has tag "metal" which is excluded
        assert!(result.iter().all(|s| s.station.url != "http://b"));
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_recommend_excludes_by_genre_matching_exclude_tags() {
        let profile = profile_with(&["jazz", "rock"], &[], &[]);
        let candidates = vec![
            make_station("http://a", "Jazz", vec![], "US"),
            make_station("http://b", "Rock", vec![], "US"),
        ];
        let library = HashSet::new();
        let config = DiscoverConfig {
            exclude_tags: vec!["jazz".to_string()],
            ..DiscoverConfig::default()
        };

        let result = recommend(&profile, &candidates, &library, &config);

        // Station "http://a" has genre "Jazz" which matches exclude_tags "jazz"
        assert!(result.iter().all(|s| s.station.url != "http://a"));
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].station.url, "http://b");
    }

    #[test]
    fn test_recommend_excludes_by_country() {
        let profile = profile_with(&["rock"], &[], &["US", "DE"]);
        let candidates = vec![
            make_station("http://a", "Rock", vec![], "US"),
            make_station("http://b", "Rock", vec![], "DE"),
            make_station("http://c", "Rock", vec![], "GB"),
        ];
        let library = HashSet::new();
        let config = DiscoverConfig {
            exclude_countries: vec!["DE".to_string()],
            ..DiscoverConfig::default()
        };

        let result = recommend(&profile, &candidates, &library, &config);

        // Station "http://b" has country "DE" which is excluded
        assert!(result.iter().all(|s| s.station.url != "http://b"));
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_recommend_all_excluded_returns_empty() {
        let profile = profile_with(&["rock"], &[], &[]);
        let candidates = vec![
            make_station("http://a", "Rock", vec![], "US"),
            make_station("http://b", "Rock", vec![], "DE"),
        ];
        let library = HashSet::new();
        let config = DiscoverConfig {
            exclude_countries: vec!["US".to_string(), "DE".to_string()],
            ..DiscoverConfig::default()
        };

        let result = recommend(&profile, &candidates, &library, &config);

        assert!(result.is_empty());
    }

    #[test]
    fn test_recommend_uses_configured_weights() {
        let profile = profile_with(&["rock"], &["guitar"], &["US"]);
        let candidates = vec![
            make_station("http://a", "Rock", vec!["guitar"], "US"),
            make_station("http://b", "Rock", vec![], "US"),
        ];
        let library = HashSet::new();
        // Give tags very high weight, genre zero weight
        let config = DiscoverConfig {
            genre_weight: 0,
            tag_weight: 10,
            country_weight: 0,
            ..DiscoverConfig::default()
        };

        let result = recommend(&profile, &candidates, &library, &config);

        // Station "http://a" has tag "guitar" → score = 0*1 + 10*1 + 0*1 = 10
        // Station "http://b" has no matching tags → score = 0*1 + 10*0 + 0*1 = 0 (excluded)
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].station.url, "http://a");
        assert_eq!(result[0].score, 10);
    }

    // --- explain_score tests ---

    #[test]
    fn explain_score_all_matches() {
        let profile = profile_with(&["jazz"], &["smooth", "chill"], &["DE"]);
        let station = make_station("http://a", "Jazz", vec!["smooth", "chill"], "DE");

        let explanation = explain_score(&profile, &station);

        assert_eq!(explanation.genres, vec!["jazz"]);
        assert_eq!(explanation.tags, vec!["smooth", "chill"]);
        assert_eq!(explanation.countries, vec!["DE"]);
    }

    #[test]
    fn explain_score_partial_matches() {
        let profile = profile_with(&["rock"], &["guitar", "chill"], &["US"]);
        let station = make_station("http://a", "Jazz", vec!["guitar", "live"], "US");

        let explanation = explain_score(&profile, &station);

        assert!(explanation.genres.is_empty());
        assert_eq!(explanation.tags, vec!["guitar"]);
        assert_eq!(explanation.countries, vec!["US"]);
    }

    #[test]
    fn explain_score_no_matches() {
        let profile = profile_with(&["rock"], &["guitar"], &["US"]);
        let station = make_station("http://a", "Jazz", vec!["smooth"], "DE");

        let explanation = explain_score(&profile, &station);

        assert!(explanation.genres.is_empty());
        assert!(explanation.tags.is_empty());
        assert!(explanation.countries.is_empty());
    }

    // --- select_top_genres tests ---

    #[test]
    fn select_top_genres_returns_up_to_5() {
        let profile = FavoritesProfile {
            genres: [
                ("rock", 10),
                ("jazz", 8),
                ("pop", 6),
                ("metal", 4),
                ("blues", 2),
            ]
            .iter()
            .map(|(k, v)| (k.to_string(), *v))
            .collect(),
            tags: HashMap::new(),
            country_codes: HashMap::new(),
        };

        let result = select_top_genres(&profile);

        assert_eq!(result.len(), 5);
    }

    #[test]
    fn select_top_genres_includes_ties_at_boundary() {
        let profile = FavoritesProfile {
            genres: [
                ("rock", 10),
                ("jazz", 8),
                ("pop", 6),
                ("metal", 4),
                ("blues", 4),     // tied at boundary
                ("classical", 4), // tied at boundary
                ("ambient", 1),
            ]
            .iter()
            .map(|(k, v)| (k.to_string(), *v))
            .collect(),
            tags: HashMap::new(),
            country_codes: HashMap::new(),
        };

        let result = select_top_genres(&profile);

        // Top 5 by count: rock(10), jazz(8), pop(6), metal(4), blues(4), classical(4)
        // The 5th position has count=4, so all with count>=4 are included (6 total)
        assert!(result.contains("rock"));
        assert!(result.contains("jazz"));
        assert!(result.contains("pop"));
        assert!(result.contains("metal"));
        assert!(result.contains("blues"));
        assert!(result.contains("classical"));
        assert!(!result.contains("ambient"));
        assert_eq!(result.len(), 6);
    }

    #[test]
    fn select_top_genres_empty_profile() {
        let profile = empty_profile();
        let result = select_top_genres(&profile);
        assert!(result.is_empty());
    }

    // --- select_top_tags tests ---

    #[test]
    fn select_top_tags_returns_up_to_10() {
        let profile = FavoritesProfile {
            genres: HashMap::new(),
            tags: (0..10)
                .map(|i| (format!("tag{i}"), 10 - i as u32))
                .collect(),
            country_codes: HashMap::new(),
        };

        let result = select_top_tags(&profile);

        assert_eq!(result.len(), 10);
    }

    #[test]
    fn select_top_tags_includes_ties_at_boundary() {
        let profile = FavoritesProfile {
            genres: HashMap::new(),
            tags: {
                let mut tags: HashMap<String, u32> = (0..10)
                    .map(|i| (format!("tag{i}"), 20 - i as u32))
                    .collect();
                // tag9 has count 11, add tag10 and tag11 also with count 11
                tags.insert("tag10".to_string(), 11);
                tags.insert("tag11".to_string(), 11);
                tags.insert("tag_low".to_string(), 1);
                tags
            },
            country_codes: HashMap::new(),
        };

        let result = select_top_tags(&profile);

        // The 10th position has count=11, so all with count>=11 are included
        assert!(result.contains("tag10"));
        assert!(result.contains("tag11"));
        assert!(!result.contains("tag_low"));
    }

    // --- prefilter_candidates tests ---

    #[test]
    fn prefilter_candidates_retains_genre_match() {
        let candidates = vec![
            make_station("http://a", "Rock", vec![], "US"),
            make_station("http://b", "Jazz", vec![], "US"),
        ];
        let top_genres: HashSet<String> = ["rock".to_string()].into();
        let top_tags: HashSet<String> = HashSet::new();

        let result = prefilter_candidates(&candidates, &top_genres, &top_tags);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].url, "http://a");
    }

    #[test]
    fn prefilter_candidates_retains_tag_match() {
        let candidates = vec![
            make_station("http://a", "Classical", vec!["guitar"], "US"),
            make_station("http://b", "Classical", vec!["piano"], "US"),
        ];
        let top_genres: HashSet<String> = HashSet::new();
        let top_tags: HashSet<String> = ["guitar".to_string()].into();

        let result = prefilter_candidates(&candidates, &top_genres, &top_tags);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].url, "http://a");
    }

    #[test]
    fn prefilter_candidates_case_insensitive() {
        let candidates = vec![make_station("http://a", "ROCK", vec!["Guitar"], "US")];
        let top_genres: HashSet<String> = ["rock".to_string()].into();
        let top_tags: HashSet<String> = ["guitar".to_string()].into();

        let result = prefilter_candidates(&candidates, &top_genres, &top_tags);

        assert_eq!(result.len(), 1);
    }

    // --- select_fallback_tag tests ---

    #[test]
    fn select_fallback_tag_returns_next_best_genre() {
        let profile = FavoritesProfile {
            genres: [("rock", 5), ("jazz", 3), ("pop", 1)]
                .iter()
                .map(|(k, v)| (k.to_string(), *v))
                .collect(),
            tags: HashMap::new(),
            country_codes: HashMap::new(),
        };

        let result = select_fallback_tag(&profile, "rock");
        assert_eq!(result, Some("jazz".to_string()));
    }

    #[test]
    fn select_fallback_tag_returns_none_when_only_primary_exists() {
        let profile = FavoritesProfile {
            genres: [("rock", 5)]
                .iter()
                .map(|(k, v)| (k.to_string(), *v))
                .collect(),
            tags: HashMap::new(),
            country_codes: HashMap::new(),
        };

        let result = select_fallback_tag(&profile, "rock");
        assert_eq!(result, None);
    }

    #[test]
    fn select_fallback_tag_considers_tags_too() {
        let profile = FavoritesProfile {
            genres: [("rock", 3)]
                .iter()
                .map(|(k, v)| (k.to_string(), *v))
                .collect(),
            tags: [("guitar", 7), ("chill", 2)]
                .iter()
                .map(|(k, v)| (k.to_string(), *v))
                .collect(),
            country_codes: HashMap::new(),
        };

        // primary is "guitar" (tag with highest count), fallback should be "rock" (next best)
        let result = select_fallback_tag(&profile, "guitar");
        assert_eq!(result, Some("rock".to_string()));
    }

    #[test]
    fn select_fallback_tag_empty_profile_returns_none() {
        let profile = empty_profile();
        let result = select_fallback_tag(&profile, "rock");
        assert_eq!(result, None);
    }

    // --- deduplicate_stations tests ---

    #[test]
    fn deduplicate_stations_removes_duplicates_by_normalized_url() {
        let stations = vec![
            make_station("http://a", "Rock", vec![], "US"),
            make_station("HTTP://A/", "Jazz", vec![], "DE"),
            make_station("http://b", "Pop", vec![], "GB"),
        ];

        let result = deduplicate_stations(&stations);

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].url, "http://a");
        assert_eq!(result[1].url, "http://b");
    }

    #[test]
    fn deduplicate_stations_preserves_order_keeps_first() {
        let stations = vec![
            make_station("http://x", "Rock", vec![], "US"),
            make_station("http://y", "Jazz", vec![], "DE"),
            make_station("http://x/", "Pop", vec![], "GB"),
            make_station("http://z", "Metal", vec![], "FR"),
        ];

        let result = deduplicate_stations(&stations);

        assert_eq!(result.len(), 3);
        assert_eq!(result[0].url, "http://x");
        assert_eq!(result[0].genre, "Rock"); // first occurrence kept
        assert_eq!(result[1].url, "http://y");
        assert_eq!(result[2].url, "http://z");
    }

    #[test]
    fn deduplicate_stations_empty_input_returns_empty() {
        let result = deduplicate_stations(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn deduplicate_stations_no_duplicates_preserves_all() {
        let stations = vec![
            make_station("http://a", "Rock", vec![], "US"),
            make_station("http://b", "Jazz", vec![], "DE"),
        ];

        let result = deduplicate_stations(&stations);
        assert_eq!(result.len(), 2);
    }

    // --- recommend pre-filter integration tests ---

    #[test]
    fn recommend_no_prefilter_at_or_below_threshold() {
        let profile = profile_with(&["rock"], &[], &[]);
        // Exactly 1000 candidates — no pre-filter, all scored
        let mut candidates: Vec<Station> = (0..1000)
            .map(|i| make_station(&format!("http://s{i}"), "Jazz", vec![], ""))
            .collect();
        // Add one rock station that would match
        candidates[999] = make_station("http://rock", "Rock", vec![], "");
        let library = HashSet::new();
        let config = DiscoverConfig::default();

        let result = recommend(&profile, &candidates, &library, &config);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].station.url, "http://rock");
    }

    #[test]
    fn recommend_prefilter_applied_above_threshold() {
        let profile = FavoritesProfile {
            genres: [("rock".to_string(), 5)].into(),
            tags: HashMap::new(),
            country_codes: [("US".to_string(), 1)].into(),
        };
        // 1001 candidates: only 2 are "rock" genre, 999 are "classical"
        let mut candidates: Vec<Station> = (0..1001)
            .map(|i| make_station(&format!("http://s{i}"), "Classical", vec![], "US"))
            .collect();
        candidates[0] = make_station("http://rock1", "Rock", vec![], "US");
        candidates[1] = make_station("http://rock2", "Rock", vec![], "US");
        let library = HashSet::new();
        let config = DiscoverConfig::default();

        let result = recommend(&profile, &candidates, &library, &config);

        // Pre-filter keeps only "rock" genre stations (2), then scores them
        // Country match alone doesn't pass pre-filter (country is not in pre-filter logic)
        assert_eq!(result.len(), 2);
        assert!(result.iter().all(|s| s.station.genre == "Rock"));
    }

    #[test]
    fn recommend_empty_profile_skips_prefilter_above_threshold() {
        // Profile has country_codes but no genres and no tags
        let profile = FavoritesProfile {
            genres: HashMap::new(),
            tags: HashMap::new(),
            country_codes: [("US".to_string(), 3)].into(),
        };
        // 1001 candidates, all with country US → should all score 1 (country match)
        let candidates: Vec<Station> = (0..1001)
            .map(|i| make_station(&format!("http://s{i}"), "Rock", vec![], "US"))
            .collect();
        let library = HashSet::new();
        let config = DiscoverConfig::default();

        let result = recommend(&profile, &candidates, &library, &config);

        // No pre-filter because genres and tags are empty
        // All 1001 score > 0 (country match), capped at 25
        assert_eq!(result.len(), 25);
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
            let actual = score_station(&profile, &candidate, &ScoringWeights::default());

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
            let config = DiscoverConfig::default();
            let result = recommend(&profile, &candidates, &library_urls, &config);

            // (a) length ≤ 25
            prop_assert!(
                result.len() <= 25,
                "result length {} exceeds 25", result.len()
            );

            // (b) no result URL is in library_urls (after normalization)
            for scored in &result {
                let normalized = normalized_station_url(&scored.station.url);
                prop_assert!(
                    !library_urls.contains(&normalized),
                    "result contains library URL: {}", scored.station.url
                );
            }

            // (c) consecutive pairs sorted descending by score, then votes, then clicks
            for pair in result.windows(2) {
                let score_a = pair[0].score;
                let score_b = pair[1].score;
                let votes_a = pair[0].station.votes.unwrap_or(0);
                let votes_b = pair[1].station.votes.unwrap_or(0);
                let clicks_a = pair[0].station.click_count.unwrap_or(0);
                let clicks_b = pair[1].station.click_count.unwrap_or(0);

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

    // Feature: v090-features, Property 13: Discover scoring formula correctness
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Validates: Requirements 5.4, 5.5**
        #[test]
        fn discover_scoring_formula_correctness(
            genre_weight in 0..=10u32,
            tag_weight in 0..=10u32,
            country_weight in 0..=10u32,
            profile_genres in proptest::collection::hash_map(arb_genre(), 1..5u32, 1..=5),
            profile_tags in proptest::collection::hash_map(arb_tag(), 1..5u32, 0..=5),
            profile_countries in proptest::collection::hash_map(arb_country_code(), 1..5u32, 0..=3),
            candidate in arb_station_scored(),
        ) {
            let profile = FavoritesProfile {
                genres: profile_genres,
                tags: profile_tags,
                country_codes: profile_countries,
            };

            let weights = ScoringWeights {
                genre_weight,
                tag_weight,
                country_weight,
            };

            let actual = score_station(&profile, &candidate, &weights);

            let genre_match: u32 = if profile
                .genres
                .contains_key(&candidate.genre.trim().to_ascii_lowercase())
            {
                1
            } else {
                0
            };

            let matching_tag_count: u32 = candidate
                .tags
                .iter()
                .filter(|t| profile.tags.contains_key(&t.trim().to_ascii_lowercase()))
                .count() as u32;

            let country_match: u32 = if profile
                .country_codes
                .contains_key(&candidate.country_code.trim().to_ascii_uppercase())
            {
                1
            } else {
                0
            };

            let expected = (genre_weight * genre_match)
                + (tag_weight * matching_tag_count)
                + (country_weight * country_match);

            prop_assert_eq!(
                actual, expected,
                "score mismatch: weights=({},{},{}), genre_match={}, tag_count={}, country_match={}",
                genre_weight, tag_weight, country_weight,
                genre_match, matching_tag_count, country_match
            );
        }
    }

    // --- Generators for exclusion filtering test ---

    fn arb_lowercase_string() -> impl Strategy<Value = String> {
        "[a-z]{1,8}".prop_map(|s| s.to_string())
    }

    fn arb_uppercase_country() -> impl Strategy<Value = String> {
        "[A-Z]{2}".prop_map(|s| s.to_string())
    }

    fn arb_non_empty_profile() -> impl Strategy<Value = FavoritesProfile> {
        (
            proptest::collection::hash_map(arb_genre(), 1..5u32, 1..=5),
            proptest::collection::hash_map(arb_tag(), 1..5u32, 0..=5),
            proptest::collection::hash_map(arb_country_code(), 1..5u32, 0..=3),
        )
            .prop_map(|(genres, tags, country_codes)| FavoritesProfile {
                genres,
                tags,
                country_codes,
            })
    }

    // Feature: v090-features, Property 14: Discover exclusion filtering invariant
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Validates: Requirements 6.6, 6.7, 6.8, 6.9**
        #[test]
        fn discover_exclusion_filtering_invariant(
            exclude_tags in proptest::collection::vec(arb_lowercase_string(), 0..=5),
            exclude_countries in proptest::collection::vec(arb_uppercase_country(), 0..=3),
            candidates in proptest::collection::vec(arb_station_scored(), 3..=10),
            profile in arb_non_empty_profile(),
        ) {
            let config = DiscoverConfig {
                genre_weight: 3,
                tag_weight: 1,
                country_weight: 1,
                exclude_tags: exclude_tags.clone(),
                exclude_countries: exclude_countries.clone(),
            };

            let library_urls = HashSet::new();
            let result = recommend(&profile, &candidates, &library_urls, &config);

            for scored in &result {
                let genre_lower = scored.station.genre.trim().to_ascii_lowercase();
                prop_assert!(
                    !exclude_tags.contains(&genre_lower),
                    "result contains station with excluded genre '{}' (station url: {})",
                    genre_lower, scored.station.url
                );

                for tag in &scored.station.tags {
                    let tag_lower = tag.trim().to_ascii_lowercase();
                    prop_assert!(
                        !exclude_tags.contains(&tag_lower),
                        "result contains station with excluded tag '{}' (station url: {})",
                        tag_lower, scored.station.url
                    );
                }

                let country_upper = scored.station.country_code.trim().to_ascii_uppercase();
                prop_assert!(
                    !exclude_countries.contains(&country_upper),
                    "result contains station with excluded country '{}' (station url: {})",
                    country_upper, scored.station.url
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
