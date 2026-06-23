use super::*;
use crate::favorites::resolve_parent_genre;
use crate::library_filter::station_matches_query;
use crate::radio::{find_station_by_url, station_url_matches};

/// Shared genre filter: returns stations visible for the given genre (or all if genre is "All").
pub(super) fn filter_stations_by_genre<'a>(
    stations: &'a [Station],
    genre: Option<&str>,
) -> Vec<&'a Station> {
    match genre {
        Some("All") | None => stations.iter().collect(),
        Some(genre) => stations
            .iter()
            .filter(|s| resolve_parent_genre(&s.genre).eq_ignore_ascii_case(genre))
            .collect(),
    }
}

/// Shared genre count: returns the number of stations visible for the given genre.
pub(super) fn count_stations_by_genre(stations: &[Station], genre: Option<&str>) -> usize {
    match genre {
        Some("All") | None => stations.len(),
        Some(genre) => stations
            .iter()
            .filter(|s| resolve_parent_genre(&s.genre).eq_ignore_ascii_case(genre))
            .count(),
    }
}

impl App {
    /// The currently visible list. In Normal mode: library. In Search mode: search results.
    /// In LibraryFilter mode with a non-empty query: genre-filtered then substring-filtered.
    pub fn visible_stations(&self) -> Vec<&Station> {
        if self.ui.input_mode == InputMode::Search {
            return self.search.results.iter().collect();
        }

        let genre = self
            .library
            .available_genres
            .get(self.ui.nav.selected_genre_idx)
            .map(|s| s.as_str());
        let genre_filtered = filter_stations_by_genre(&self.library.stations, genre);
        let sorted = super::favorites_actions::sort_with_favorites(
            genre_filtered,
            &self.library.settings.favorites,
        );

        if self.ui.input_mode == InputMode::LibraryFilter
            && !self.library_filter_query.trim().is_empty()
        {
            sorted
                .into_iter()
                .filter(|s| station_matches_query(s, &self.library_filter_query))
                .collect()
        } else {
            sorted
        }
    }

    /// Get the highlighted station from the currently visible list.
    #[cfg(test)]
    pub fn selected_station(&self) -> Option<&Station> {
        self.visible_stations().get(self.ui.nav.selected).copied()
    }

    /// Get the currently playing station, if any.
    pub fn now_playing(&self) -> Option<&Station> {
        self.playback.view.playing_url.as_ref().and_then(|url| {
            find_station_by_url(&self.library.stations, url)
                .or_else(|| {
                    self.search
                        .results
                        .iter()
                        .find(|station| station_url_matches(&station.url, url))
                })
                .or_else(|| {
                    self.undo_history.iter().rev().find_map(|(station, _, _)| {
                        station_url_matches(&station.url, url).then_some(station)
                    })
                })
        })
    }

    /// Count visible stations without allocating a Vec.
    pub fn visible_count(&self) -> usize {
        if self.ui.input_mode == InputMode::Search {
            return self.search.results.len();
        }

        let genre = self
            .library
            .available_genres
            .get(self.ui.nav.selected_genre_idx)
            .map(|s| s.as_str());

        if self.ui.input_mode == InputMode::LibraryFilter
            && !self.library_filter_query.trim().is_empty()
        {
            filter_stations_by_genre(&self.library.stations, genre)
                .into_iter()
                .filter(|s| station_matches_query(s, &self.library_filter_query))
                .count()
        } else {
            count_stations_by_genre(&self.library.stations, genre)
        }
    }

    /// Try to select the currently playing station in the visible list.
    pub(super) fn select_playing(&mut self) {
        if let Some(ref url) = self.playback.view.playing_url {
            if let Some(pos) = self
                .visible_stations()
                .iter()
                .position(|station| station_url_matches(&station.url, url))
            {
                self.ui.nav.selected = pos;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::favorites::Library;
    use crate::radio::Station;

    fn station(name: &str, url: &str) -> Station {
        Station::basic(name, url, "Synthwave", "US", 128)
    }

    fn station_with_genre(name: &str, url: &str, genre: &str) -> Station {
        Station::basic(name, url, genre, "US", 128)
    }

    #[test]
    fn selected_station_returns_highlighted_visible_station() {
        let mut app = App::new(Library::in_memory(vec![
            station("A", "http://a"),
            station("B", "http://b"),
        ]));
        app.ui.nav.selected = 1;

        assert_eq!(
            app.selected_station().map(|s| s.url.as_str()),
            Some("http://b")
        );
    }

    #[test]
    fn selected_station_returns_none_for_empty_visible_list() {
        let app = App::new(Library::in_memory(vec![]));

        assert!(app.selected_station().is_none());
    }

    #[test]
    fn now_playing_matches_normalized_library_url() {
        let mut app = App::new(Library::in_memory(vec![station("A", " HTTP://STREAM/ ")]));
        app.playback.view.playing_url = Some("http://stream".to_string());

        assert_eq!(
            app.now_playing().map(|station| station.name.as_str()),
            Some("A")
        );
    }

    #[test]
    fn select_playing_matches_normalized_visible_url() {
        let mut app = App::new(Library::in_memory(vec![
            station("A", "http://a"),
            station("B", " HTTP://STREAM/ "),
        ]));
        app.playback.view.playing_url = Some("http://stream".to_string());

        app.select_playing();

        assert_eq!(app.ui.nav.selected, 1);
    }

    #[test]
    fn library_filter_mode_applies_query_within_active_genre() {
        let mut app = App::new(Library::in_memory(vec![
            station_with_genre("Jazz FM", "http://jazz", "Jazz"),
            station_with_genre("Nightride FM", "http://night", "Synthwave"),
            station_with_genre("SomaFM", "http://soma", "Synthwave"),
        ]));

        // Set genre to "All" so all stations are genre-visible
        app.library.available_genres = vec!["All".to_string()];
        app.ui.nav.selected_genre_idx = 0;

        // Without filter: all 3 visible
        assert_eq!(app.visible_stations().len(), 3);
        assert_eq!(app.visible_count(), 3);

        // Enter LibraryFilter mode with query "jazz"
        app.ui.input_mode = InputMode::LibraryFilter;
        app.library_filter_query = "jazz".to_string();

        let visible = app.visible_stations();
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].name, "Jazz FM");
        assert_eq!(app.visible_count(), 1);
    }

    #[test]
    fn library_filter_empty_query_returns_full_genre_set() {
        let mut app = App::new(Library::in_memory(vec![
            station("A", "http://a"),
            station("B", "http://b"),
        ]));

        app.ui.input_mode = InputMode::LibraryFilter;
        app.library_filter_query = "   ".to_string();

        assert_eq!(app.visible_stations().len(), 2);
        assert_eq!(app.visible_count(), 2);
    }

    #[test]
    fn library_filter_respects_genre_scoping() {
        let mut app = App::new(Library::in_memory(vec![
            station_with_genre("Jazz FM", "http://jazz", "Jazz"),
            station_with_genre("Nightride FM", "http://night", "Synthwave"),
            station_with_genre("SomaFM", "http://soma", "Synthwave"),
        ]));

        // Set genre filter to Synthwave only
        app.library.available_genres = vec![
            "All".to_string(),
            "Synthwave".to_string(),
            "Jazz".to_string(),
        ];
        app.ui.nav.selected_genre_idx = 1; // "Synthwave"

        // Enter LibraryFilter mode with query "fm" — should only match
        // within the Synthwave genre (Nightride FM, SomaFM), not Jazz FM
        app.ui.input_mode = InputMode::LibraryFilter;
        app.library_filter_query = "fm".to_string();

        let visible = app.visible_stations();
        assert_eq!(visible.len(), 2);
        assert_eq!(visible[0].name, "Nightride FM");
        assert_eq!(visible[1].name, "SomaFM");
    }

    #[test]
    fn library_filter_not_applied_in_normal_mode() {
        let mut app = App::new(Library::in_memory(vec![
            station("Jazz FM", "http://jazz"),
            station("Nightride FM", "http://night"),
        ]));

        // Query is set but mode is Normal — filter should NOT be applied
        app.ui.input_mode = InputMode::Normal;
        app.library_filter_query = "jazz".to_string();

        assert_eq!(app.visible_stations().len(), 2);
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use crate::radio::Station;
    use proptest::prelude::*;

    /// Strategy generating genre strings including known parent genres and random strings.
    fn arb_genre() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("Synthwave".to_string()),
            Just("Ambient".to_string()),
            Just("Rock".to_string()),
            Just("Vaporwave".to_string()),
            Just("Other".to_string()),
            "[a-zA-Z ]{0,50}",
        ]
    }

    /// Strategy generating a Vec<Station> of 0–50 elements with varied genre strings.
    fn arb_station_list() -> impl Strategy<Value = Vec<Station>> {
        prop::collection::vec(
            (
                "[a-zA-Z0-9 ]{1,30}", // name
                "[a-z]{1,30}",        // url
                arb_genre(),          // genre
            )
                .prop_map(|(name, url, genre)| Station::basic(&name, &url, &genre, "US", 128)),
            0..=50,
        )
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        /// **Feature: test-coverage-improvement, Property 11: Genre filter count-length consistency**
        ///
        /// For any station list and any genre string, count_stations_by_genre equals
        /// filter_stations_by_genre(...).len().
        ///
        /// **Validates: Requirements 16.1**
        #[test]
        fn genre_filter_count_length_consistency(
            stations in arb_station_list(),
            genre in arb_genre(),
        ) {
            let count = count_stations_by_genre(&stations, Some(&genre));
            let filtered_len = filter_stations_by_genre(&stations, Some(&genre)).len();
            prop_assert_eq!(count, filtered_len);
        }

        /// **Feature: test-coverage-improvement, Property 12: Genre filter "All"/None identity**
        ///
        /// For any station list, filtering by "All" or None returns all stations.
        ///
        /// **Validates: Requirements 16.2**
        #[test]
        fn genre_filter_all_none_identity(
            stations in arb_station_list(),
        ) {
            let all_len = filter_stations_by_genre(&stations, Some("All")).len();
            let none_len = filter_stations_by_genre(&stations, None).len();
            prop_assert_eq!(all_len, stations.len());
            prop_assert_eq!(none_len, stations.len());
        }
    }
}
