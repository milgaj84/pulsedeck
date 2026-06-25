use crate::favorites_set::FavoritesSet;
use crate::radio::Station;

/// Sort mode for the library view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortMode {
    FavoritesFirst,
    Alphabetical,
    RecentlyAdded,
    MostPlayed,
}

impl SortMode {
    pub const ALL: [Self; 4] = [
        Self::FavoritesFirst,
        Self::Alphabetical,
        Self::RecentlyAdded,
        Self::MostPlayed,
    ];

    /// Advance to the next mode, wrapping at the end.
    pub fn next(self) -> Self {
        match self {
            Self::FavoritesFirst => Self::Alphabetical,
            Self::Alphabetical => Self::RecentlyAdded,
            Self::RecentlyAdded => Self::MostPlayed,
            Self::MostPlayed => Self::FavoritesFirst,
        }
    }

    /// Stable string key for TOML serialization.
    pub fn to_key(self) -> &'static str {
        match self {
            Self::FavoritesFirst => "favorites_first",
            Self::Alphabetical => "alphabetical",
            Self::RecentlyAdded => "recently_added",
            Self::MostPlayed => "most_played",
        }
    }

    /// Parse from TOML key string. Returns None for unrecognized values.
    pub fn from_key(s: &str) -> Option<Self> {
        match s {
            "favorites_first" => Some(Self::FavoritesFirst),
            "alphabetical" => Some(Self::Alphabetical),
            "recently_added" => Some(Self::RecentlyAdded),
            "most_played" => Some(Self::MostPlayed),
            _ => None,
        }
    }

    /// Single-character abbreviation for footer chip display.
    pub fn chip(self) -> char {
        match self {
            Self::FavoritesFirst => 'F',
            Self::Alphabetical => 'A',
            Self::RecentlyAdded => 'R',
            Self::MostPlayed => 'M',
        }
    }

    /// Human-readable label for notice display.
    pub fn label(self) -> &'static str {
        match self {
            Self::FavoritesFirst => "Favorites First",
            Self::Alphabetical => "Alphabetical",
            Self::RecentlyAdded => "Recently Added",
            Self::MostPlayed => "Most Played",
        }
    }
}

/// Pure sort function. Returns a new Vec with the same stations, reordered.
/// Favorites are always pinned to the top regardless of mode.
pub fn sort_library<'a>(
    stations: Vec<&'a Station>,
    mode: SortMode,
    favorites: &FavoritesSet,
) -> Vec<&'a Station> {
    let (mut favs, mut rest) = partition_by_favorites(stations, favorites);
    apply_secondary_sort(&mut favs, &mut rest, mode);
    favs.extend(rest);
    favs
}

fn partition_by_favorites<'a>(
    stations: Vec<&'a Station>,
    favorites: &FavoritesSet,
) -> (Vec<&'a Station>, Vec<&'a Station>) {
    let mut favs = Vec::new();
    let mut rest = Vec::new();
    for station in stations {
        if favorites.contains(&station.url) {
            favs.push(station);
        } else {
            rest.push(station);
        }
    }
    (favs, rest)
}

fn apply_secondary_sort<'a>(favs: &mut [&'a Station], rest: &mut [&'a Station], mode: SortMode) {
    match mode {
        SortMode::FavoritesFirst => {} // preserve insertion order
        SortMode::Alphabetical => {
            sort_by_name(favs);
            sort_by_name(rest);
        }
        SortMode::RecentlyAdded => {
            favs.reverse();
            rest.reverse();
        }
        SortMode::MostPlayed => {
            sort_by_click_count(favs);
            sort_by_click_count(rest);
        }
    }
}

fn sort_by_name(stations: &mut [&Station]) {
    stations.sort_by_key(|s| s.name.to_lowercase());
}

fn sort_by_click_count(stations: &mut [&Station]) {
    stations.sort_by(|a, b| {
        let count_b = b.click_count.unwrap_or(0);
        let count_a = a.click_count.unwrap_or(0);
        count_b.cmp(&count_a)
    });
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    fn arb_station() -> impl Strategy<Value = Station> {
        ("[a-zA-Z ]{1,15}", 0u32..200u32).prop_map(|(name, clicks)| {
            let url = format!("http://{}.test", name.trim().replace(' ', "-"));
            let mut s = Station::basic(&name, &url, "Genre", "US", 128);
            s.click_count = Some(clicks);
            s
        })
    }

    fn arb_stations_unique_urls() -> impl Strategy<Value = Vec<Station>> {
        prop::collection::vec(arb_station(), 0..=20).prop_map(|mut stations| {
            let mut seen = std::collections::HashSet::new();
            stations.retain(|s| seen.insert(s.url.clone()));
            stations
        })
    }

    fn arb_sort_mode() -> impl Strategy<Value = SortMode> {
        prop_oneof![
            Just(SortMode::FavoritesFirst),
            Just(SortMode::Alphabetical),
            Just(SortMode::RecentlyAdded),
            Just(SortMode::MostPlayed),
        ]
    }

    // Feature: v100-features, Property 1: Sort permutation invariant
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Validates: Requirements 1.8**
        #[test]
        fn sort_permutation_invariant(
            stations in arb_stations_unique_urls(),
            mode in arb_sort_mode(),
        ) {
            let favorites = {
                let mut set = FavoritesSet::default();
                for (i, s) in stations.iter().enumerate() {
                    if i % 3 == 0 { set.toggle(&s.url); }
                }
                set
            };
            let refs: Vec<&Station> = stations.iter().collect();
            let sorted = sort_library(refs.clone(), mode, &favorites);

            prop_assert_eq!(sorted.len(), stations.len());
            let mut input_urls: Vec<&str> = refs.iter().map(|s| s.url.as_str()).collect();
            let mut output_urls: Vec<&str> = sorted.iter().map(|s| s.url.as_str()).collect();
            input_urls.sort();
            output_urls.sort();
            prop_assert_eq!(input_urls, output_urls);
        }
    }

    // Feature: v100-features, Property 2: FavoritesFirst partition preserves insertion order
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Validates: Requirements 1.2**
        #[test]
        fn favorites_first_preserves_insertion_order(
            stations in arb_stations_unique_urls(),
        ) {
            let favorites = {
                let mut set = FavoritesSet::default();
                for (i, s) in stations.iter().enumerate() {
                    if i % 2 == 0 { set.toggle(&s.url); }
                }
                set
            };
            let refs: Vec<&Station> = stations.iter().collect();
            let sorted = sort_library(refs, SortMode::FavoritesFirst, &favorites);

            // Check partition: all favorites before all non-favorites
            let mut seen_non_fav = false;
            for s in &sorted {
                if favorites.contains(&s.url) {
                    prop_assert!(!seen_non_fav, "favorite after non-favorite");
                } else {
                    seen_non_fav = true;
                }
            }

            // Check insertion order within each group
            let fav_group: Vec<usize> = sorted.iter()
                .filter(|s| favorites.contains(&s.url))
                .map(|s| stations.iter().position(|orig| orig.url == s.url).unwrap())
                .collect();
            let non_fav_group: Vec<usize> = sorted.iter()
                .filter(|s| !favorites.contains(&s.url))
                .map(|s| stations.iter().position(|orig| orig.url == s.url).unwrap())
                .collect();

            for w in fav_group.windows(2) {
                prop_assert!(w[0] < w[1], "favorites not in insertion order");
            }
            for w in non_fav_group.windows(2) {
                prop_assert!(w[0] < w[1], "non-favorites not in insertion order");
            }
        }
    }

    // Feature: v100-features, Property 3: Alphabetical sort ordering within groups
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Validates: Requirements 1.3**
        #[test]
        fn alphabetical_sort_ordering(
            stations in arb_stations_unique_urls(),
        ) {
            let favorites = {
                let mut set = FavoritesSet::default();
                for (i, s) in stations.iter().enumerate() {
                    if i % 2 == 0 { set.toggle(&s.url); }
                }
                set
            };
            let refs: Vec<&Station> = stations.iter().collect();
            let sorted = sort_library(refs, SortMode::Alphabetical, &favorites);

            // Check partition
            let mut seen_non_fav = false;
            for s in &sorted {
                if favorites.contains(&s.url) {
                    prop_assert!(!seen_non_fav, "favorite after non-favorite");
                } else {
                    seen_non_fav = true;
                }
            }

            // Check alphabetical within each group
            let fav_names: Vec<String> = sorted.iter()
                .filter(|s| favorites.contains(&s.url))
                .map(|s| s.name.to_lowercase())
                .collect();
            let non_fav_names: Vec<String> = sorted.iter()
                .filter(|s| !favorites.contains(&s.url))
                .map(|s| s.name.to_lowercase())
                .collect();

            for w in fav_names.windows(2) {
                prop_assert!(w[0] <= w[1], "favorites not alphabetical: {} > {}", w[0], w[1]);
            }
            for w in non_fav_names.windows(2) {
                prop_assert!(w[0] <= w[1], "non-favorites not alphabetical: {} > {}", w[0], w[1]);
            }
        }
    }

    // Feature: v100-features, Property 4: RecentlyAdded descending index within groups
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Validates: Requirements 1.4**
        #[test]
        fn recently_added_descending_index(
            stations in arb_stations_unique_urls(),
        ) {
            let favorites = {
                let mut set = FavoritesSet::default();
                for (i, s) in stations.iter().enumerate() {
                    if i % 2 == 0 { set.toggle(&s.url); }
                }
                set
            };
            let refs: Vec<&Station> = stations.iter().collect();
            let sorted = sort_library(refs, SortMode::RecentlyAdded, &favorites);

            // Check partition
            let mut seen_non_fav = false;
            for s in &sorted {
                if favorites.contains(&s.url) {
                    prop_assert!(!seen_non_fav, "favorite after non-favorite");
                } else {
                    seen_non_fav = true;
                }
            }

            // Check descending original index within each group
            let fav_indices: Vec<usize> = sorted.iter()
                .filter(|s| favorites.contains(&s.url))
                .map(|s| stations.iter().position(|orig| orig.url == s.url).unwrap())
                .collect();
            let non_fav_indices: Vec<usize> = sorted.iter()
                .filter(|s| !favorites.contains(&s.url))
                .map(|s| stations.iter().position(|orig| orig.url == s.url).unwrap())
                .collect();

            for w in fav_indices.windows(2) {
                prop_assert!(w[0] > w[1], "favorites not in descending index order");
            }
            for w in non_fav_indices.windows(2) {
                prop_assert!(w[0] > w[1], "non-favorites not in descending index order");
            }
        }
    }

    // Feature: v100-features, Property 5: MostPlayed descending count within groups
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Validates: Requirements 1.5**
        #[test]
        fn most_played_descending_count(
            stations in arb_stations_unique_urls(),
        ) {
            let favorites = {
                let mut set = FavoritesSet::default();
                for (i, s) in stations.iter().enumerate() {
                    if i % 2 == 0 { set.toggle(&s.url); }
                }
                set
            };
            let refs: Vec<&Station> = stations.iter().collect();
            let sorted = sort_library(refs, SortMode::MostPlayed, &favorites);

            // Check partition
            let mut seen_non_fav = false;
            for s in &sorted {
                if favorites.contains(&s.url) {
                    prop_assert!(!seen_non_fav, "favorite after non-favorite");
                } else {
                    seen_non_fav = true;
                }
            }

            // Check non-increasing click_count within each group
            let fav_counts: Vec<u32> = sorted.iter()
                .filter(|s| favorites.contains(&s.url))
                .map(|s| s.click_count.unwrap_or(0))
                .collect();
            let non_fav_counts: Vec<u32> = sorted.iter()
                .filter(|s| !favorites.contains(&s.url))
                .map(|s| s.click_count.unwrap_or(0))
                .collect();

            for w in fav_counts.windows(2) {
                prop_assert!(w[0] >= w[1], "favorites not in non-increasing count");
            }
            for w in non_fav_counts.windows(2) {
                prop_assert!(w[0] >= w[1], "non-favorites not in non-increasing count");
            }
        }
    }

    // Feature: v100-features, Property 6: SortMode cycling round-trip
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Validates: Requirements 1.6**
        #[test]
        fn sort_mode_cycling_round_trip(mode in arb_sort_mode()) {
            let cycled = mode.next().next().next().next();
            prop_assert_eq!(cycled, mode);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn station(name: &str, url: &str) -> Station {
        Station::basic(name, url, "Synthwave", "US", 128)
    }

    fn station_with_clicks(name: &str, url: &str, clicks: u32) -> Station {
        let mut s = station(name, url);
        s.click_count = Some(clicks);
        s
    }

    fn favorites_from(urls: &[&str]) -> FavoritesSet {
        let mut set = FavoritesSet::default();
        for url in urls {
            set.toggle(url);
        }
        set
    }

    #[test]
    fn test_favorites_first_partitions_correctly() {
        let stations = [
            station("Alpha", "http://a"),
            station("Beta", "http://b"),
            station("Gamma", "http://c"),
        ];
        let refs: Vec<&Station> = stations.iter().collect();
        let favorites = favorites_from(&["http://b"]);

        let sorted = sort_library(refs, SortMode::FavoritesFirst, &favorites);

        assert_eq!(sorted[0].name, "Beta");
        assert_eq!(sorted[1].name, "Alpha");
        assert_eq!(sorted[2].name, "Gamma");
    }

    #[test]
    fn test_alphabetical_sorts_by_name() {
        let stations = [
            station("Zeta", "http://z"),
            station("alpha", "http://a"),
            station("Beta", "http://b"),
        ];
        let refs: Vec<&Station> = stations.iter().collect();
        let favorites = favorites_from(&["http://z"]);

        let sorted = sort_library(refs, SortMode::Alphabetical, &favorites);

        // Favorites group (just Zeta)
        assert_eq!(sorted[0].name, "Zeta");
        // Non-favorites group sorted: alpha, Beta
        assert_eq!(sorted[1].name, "alpha");
        assert_eq!(sorted[2].name, "Beta");
    }

    #[test]
    fn test_recently_added_reverses_order() {
        let stations = [
            station("First", "http://1"),
            station("Second", "http://2"),
            station("Third", "http://3"),
        ];
        let refs: Vec<&Station> = stations.iter().collect();
        let favorites = favorites_from(&["http://1"]);

        let sorted = sort_library(refs, SortMode::RecentlyAdded, &favorites);

        // Favorites group reversed (only one, so same)
        assert_eq!(sorted[0].name, "First");
        // Non-favorites reversed: Third, Second
        assert_eq!(sorted[1].name, "Third");
        assert_eq!(sorted[2].name, "Second");
    }

    #[test]
    fn test_most_played_sorts_by_click_count() {
        let stations = [
            station_with_clicks("Low", "http://low", 5),
            station_with_clicks("High", "http://high", 100),
            station("None", "http://none"),
        ];
        let refs: Vec<&Station> = stations.iter().collect();
        let favorites = FavoritesSet::default();

        let sorted = sort_library(refs, SortMode::MostPlayed, &favorites);

        assert_eq!(sorted[0].name, "High");
        assert_eq!(sorted[1].name, "Low");
        assert_eq!(sorted[2].name, "None");
    }

    #[test]
    fn test_empty_list_returns_empty() {
        let refs: Vec<&Station> = Vec::new();
        let favorites = FavoritesSet::default();

        for &mode in &SortMode::ALL {
            let sorted = sort_library(refs.clone(), mode, &favorites);
            assert!(sorted.is_empty());
        }
    }

    #[test]
    fn test_all_favorites_no_crash() {
        let stations = [station("A", "http://a"), station("B", "http://b")];
        let refs: Vec<&Station> = stations.iter().collect();
        let favorites = favorites_from(&["http://a", "http://b"]);

        let sorted = sort_library(refs, SortMode::FavoritesFirst, &favorites);

        assert_eq!(sorted.len(), 2);
        assert_eq!(sorted[0].name, "A");
        assert_eq!(sorted[1].name, "B");
    }

    #[test]
    fn test_no_favorites_preserves_sort() {
        let stations = [
            station("A", "http://a"),
            station("B", "http://b"),
            station("C", "http://c"),
        ];
        let refs: Vec<&Station> = stations.iter().collect();
        let favorites = FavoritesSet::default();

        let sorted = sort_library(refs, SortMode::FavoritesFirst, &favorites);

        assert_eq!(sorted[0].name, "A");
        assert_eq!(sorted[1].name, "B");
        assert_eq!(sorted[2].name, "C");
    }

    #[test]
    fn test_sort_mode_next_cycles() {
        assert_eq!(SortMode::FavoritesFirst.next(), SortMode::Alphabetical);
        assert_eq!(SortMode::Alphabetical.next(), SortMode::RecentlyAdded);
        assert_eq!(SortMode::RecentlyAdded.next(), SortMode::MostPlayed);
        assert_eq!(SortMode::MostPlayed.next(), SortMode::FavoritesFirst);
    }

    #[test]
    fn test_sort_mode_label() {
        for &mode in &SortMode::ALL {
            assert!(!mode.label().is_empty());
        }
        assert_eq!(SortMode::FavoritesFirst.label(), "Favorites First");
        assert_eq!(SortMode::Alphabetical.label(), "Alphabetical");
        assert_eq!(SortMode::RecentlyAdded.label(), "Recently Added");
        assert_eq!(SortMode::MostPlayed.label(), "Most Played");
    }

    #[test]
    fn test_to_key_returns_expected_strings() {
        assert_eq!(SortMode::FavoritesFirst.to_key(), "favorites_first");
        assert_eq!(SortMode::Alphabetical.to_key(), "alphabetical");
        assert_eq!(SortMode::RecentlyAdded.to_key(), "recently_added");
        assert_eq!(SortMode::MostPlayed.to_key(), "most_played");
    }

    #[test]
    fn test_from_key_parses_valid_strings() {
        assert_eq!(SortMode::from_key("favorites_first"), Some(SortMode::FavoritesFirst));
        assert_eq!(SortMode::from_key("alphabetical"), Some(SortMode::Alphabetical));
        assert_eq!(SortMode::from_key("recently_added"), Some(SortMode::RecentlyAdded));
        assert_eq!(SortMode::from_key("most_played"), Some(SortMode::MostPlayed));
    }

    #[test]
    fn test_from_key_returns_none_for_invalid() {
        assert_eq!(SortMode::from_key(""), None);
        assert_eq!(SortMode::from_key("unknown"), None);
        assert_eq!(SortMode::from_key("Alphabetical"), None);
        assert_eq!(SortMode::from_key("FAVORITES_FIRST"), None);
    }

    #[test]
    fn test_to_key_from_key_round_trip() {
        for &mode in &SortMode::ALL {
            let key = mode.to_key();
            let parsed = SortMode::from_key(key);
            assert_eq!(parsed, Some(mode));
        }
    }

    #[test]
    fn test_chip_returns_expected_chars() {
        assert_eq!(SortMode::FavoritesFirst.chip(), 'F');
        assert_eq!(SortMode::Alphabetical.chip(), 'A');
        assert_eq!(SortMode::RecentlyAdded.chip(), 'R');
        assert_eq!(SortMode::MostPlayed.chip(), 'M');
    }
}
