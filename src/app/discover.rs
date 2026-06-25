use super::*;
use crate::audio::AudioCommand;
use crate::recommend::{
    build_favorites_profile, recommend, select_fallback_tag, select_top_genres, select_top_tags,
    FavoritesProfile,
};
use std::collections::HashSet;

/// Request for discover fetch, supporting multi-query fallback.
#[derive(Debug, Clone, PartialEq)]
pub struct DiscoverFetchRequest {
    pub primary_tag: String,
    pub fallback_tag: Option<String>,
}

/// Build a tag query string from the profile's single most popular genre or tag.
/// Radio Browser's tag: parameter works best with a single value.
fn build_discover_tag_query(profile: &FavoritesProfile) -> String {
    let top_genres = select_top_genres(profile);
    let top_tags = select_top_tags(profile);

    // Pick the single highest-count genre, falling back to the highest-count tag
    top_genres
        .iter()
        .next()
        .or_else(|| top_tags.iter().next())
        .cloned()
        .unwrap_or_default()
}

impl App {
    /// Handle the Discover action: build a favorites profile and request async fetch.
    pub(super) fn handle_discover(&mut self) {
        let profile =
            build_favorites_profile(&self.library.stations, &self.library.settings.favorites);

        if profile.genres.is_empty() && profile.tags.is_empty() {
            self.set_info_notice("Discover: no genre or tag data in favorites");
            self.discover_results = Vec::new();
            self.discover_cursor = 0;
            return;
        }

        let primary_tag = build_discover_tag_query(&profile);
        let fallback_tag = select_fallback_tag(&profile, &primary_tag);
        self.discover_fetch_pending = Some(DiscoverFetchRequest {
            primary_tag,
            fallback_tag,
        });
        self.discover_cursor = 0;
        self.set_info_notice("Loading recommendations...");
    }

    /// Take the pending discover fetch request (consumed by the runtime driver).
    pub fn take_discover_fetch_request(&mut self) -> Option<DiscoverFetchRequest> {
        self.discover_fetch_pending.take()
    }

    /// Apply the async discover fetch response from the runtime driver.
    pub fn apply_discover_response(&mut self, result: Result<Vec<Station>, String>) {
        match result {
            Ok(candidates) => {
                let profile = build_favorites_profile(
                    &self.library.stations,
                    &self.library.settings.favorites,
                );
                let library_urls = self.library_url_set();
                let results =
                    recommend(&profile, &candidates, &library_urls, &self.config.discover);
                if results.is_empty() {
                    self.set_info_notice("No matches found — try starring more stations");
                }
                self.discover_results = results;
                self.discover_cursor = 0;
            }
            Err(message) => {
                self.discover_results = Vec::new();
                self.discover_cursor = 0;
                self.set_error_notice(format!("Discover fetch failed: {message}"));
            }
        }
    }

    pub(super) fn discover_next(&mut self) {
        if self.discover_results.is_empty() {
            return;
        }
        let max = self.discover_results.len().saturating_sub(1);
        self.discover_cursor = (self.discover_cursor + 1).min(max);
    }

    pub(super) fn discover_prev(&mut self) {
        if self.discover_results.is_empty() {
            return;
        }
        self.discover_cursor = self.discover_cursor.saturating_sub(1);
    }

    pub(super) fn discover_select(&mut self) {
        if self.discover_results.is_empty() {
            return;
        }
        let station = self.discover_results[self.discover_cursor].station.clone();
        self.library.stations.push(station.clone());
        self.library.rebuild_genres();
        self.mark_library_dirty();

        self.playback.reconnect.disarm();
        self.playback.view.playing_url = Some(station.url.clone());
        self.playback.view.state = PlaybackState::Connecting;
        self.playback.elapsed_timer.reset();
        self.playback.elapsed_timer.start();
        self.library.settings.last_played_url = Some(station.url.clone());
        if self.send_audio_command(AudioCommand::Play(station.url)) {
            self.sync_volume();
        }

        self.discover_results.clear();
        self.discover_cursor = 0;
    }

    pub(super) fn discover_dismiss(&mut self) {
        self.discover_results.clear();
        self.discover_cursor = 0;
    }

    fn library_url_set(&self) -> HashSet<String> {
        self.library
            .stations
            .iter()
            .map(|s| crate::radio::normalized_station_url(&s.url))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Action;
    use crate::app::command_palette::{filtered_commands, PaletteCommand};
    use crate::favorites::Library;
    use crate::radio::Station;

    fn station(name: &str, url: &str, genre: &str) -> Station {
        Station::basic(name, url, genre, "US", 128)
    }

    fn test_app_with_favorites() -> App {
        let mut library = Library::in_memory(vec![
            station("Jazz FM", "http://jazz", "Jazz"),
            station("Rock Radio", "http://rock", "Rock"),
            station("Jazz Cafe", "http://jazzcafe", "Jazz"),
        ]);
        library.settings.favorites.toggle("http://jazz");
        library.settings.favorites.toggle("http://jazzcafe");
        App::new(library)
    }

    fn test_app_empty_library() -> App {
        App::new(Library::in_memory(vec![]))
    }

    fn test_app_with_discover_results() -> App {
        let mut app = App::new(Library::in_memory(vec![station(
            "Existing",
            "http://existing",
            "Rock",
        )]));
        app.discover_results = vec![
            ScoredStation {
                station: station("Disco A", "http://disco-a", "Disco"),
                score: 3,
            },
            ScoredStation {
                station: station("Disco B", "http://disco-b", "Disco"),
                score: 2,
            },
            ScoredStation {
                station: station("Disco C", "http://disco-c", "Disco"),
                score: 1,
            },
        ];
        app.discover_cursor = 0;
        app
    }

    #[test]
    fn discover_command_listed_in_palette() {
        let app = test_app_with_favorites();
        let commands = filtered_commands("discover", &app);
        assert!(commands.contains(&PaletteCommand::Discover));
    }

    #[test]
    fn discover_empty_library_shows_info_notice() {
        let mut app = test_app_empty_library();

        app.update(Action::Discover);

        assert!(matches!(
            app.ui.notice.current,
            Some(AppNotice::Info(ref msg)) if msg.contains("no genre or tag data")
        ));
        assert!(app.discover_results.is_empty());
    }

    #[test]
    fn discover_no_favorites_shows_info_notice() {
        let library = Library::in_memory(vec![station("Jazz FM", "http://jazz", "Jazz")]);
        let mut app = App::new(library);

        app.update(Action::Discover);

        assert!(matches!(
            app.ui.notice.current,
            Some(AppNotice::Info(ref msg)) if msg.contains("no genre or tag data")
        ));
    }

    #[test]
    fn discover_with_favorites_sets_pending_fetch() {
        let mut app = test_app_with_favorites();

        app.update(Action::Discover);

        assert!(matches!(
            app.ui.notice.current,
            Some(AppNotice::Info(ref msg)) if msg.contains("Loading recommendations")
        ));
        assert!(app.discover_fetch_pending.is_some());
        assert_eq!(app.discover_cursor, 0);
    }

    #[test]
    fn discover_action_maps_from_palette_command() {
        use crate::app::command_palette::command_action;
        assert_eq!(command_action(PaletteCommand::Discover), Action::Discover);
    }

    // --- Discover cursor navigation tests ---

    #[test]
    fn discover_cursor_starts_at_zero() {
        let app = test_app_with_discover_results();
        assert_eq!(app.discover_cursor, 0);
    }

    #[test]
    fn discover_next_increments_cursor() {
        let mut app = test_app_with_discover_results();

        app.update(Action::DiscoverNext);

        assert_eq!(app.discover_cursor, 1);
    }

    #[test]
    fn discover_next_clamps_at_end() {
        let mut app = test_app_with_discover_results();
        app.discover_cursor = 2; // last index

        app.update(Action::DiscoverNext);

        assert_eq!(app.discover_cursor, 2);
    }

    #[test]
    fn discover_prev_decrements_cursor() {
        let mut app = test_app_with_discover_results();
        app.discover_cursor = 2;

        app.update(Action::DiscoverPrev);

        assert_eq!(app.discover_cursor, 1);
    }

    #[test]
    fn discover_prev_clamps_at_zero() {
        let mut app = test_app_with_discover_results();
        app.discover_cursor = 0;

        app.update(Action::DiscoverPrev);

        assert_eq!(app.discover_cursor, 0);
    }

    #[test]
    fn discover_select_adds_station_to_library_and_starts_playback() {
        let mut app = test_app_with_discover_results();
        app.discover_cursor = 1;

        app.update(Action::DiscoverSelect);

        assert!(app.library.contains("http://disco-b"));
        assert_eq!(
            app.playback.view.playing_url.as_deref(),
            Some("http://disco-b")
        );
        assert!(app.discover_results.is_empty());
        assert_eq!(app.discover_cursor, 0);
    }

    #[test]
    fn discover_dismiss_clears_results_and_resets_cursor() {
        let mut app = test_app_with_discover_results();
        app.discover_cursor = 2;

        app.update(Action::DiscoverDismiss);

        assert!(app.discover_results.is_empty());
        assert_eq!(app.discover_cursor, 0);
    }

    #[test]
    fn discover_next_empty_results_is_noop() {
        let mut app = test_app_empty_library();

        app.update(Action::DiscoverNext);

        assert_eq!(app.discover_cursor, 0);
    }

    #[test]
    fn discover_prev_empty_results_is_noop() {
        let mut app = test_app_empty_library();

        app.update(Action::DiscoverPrev);

        assert_eq!(app.discover_cursor, 0);
    }

    #[test]
    fn discover_select_empty_results_is_noop() {
        let mut app = test_app_empty_library();
        let library_len = app.library.stations.len();

        app.update(Action::DiscoverSelect);

        assert_eq!(app.library.stations.len(), library_len);
        assert_eq!(app.playback.view.playing_url, None);
    }

    #[test]
    fn discover_dismiss_empty_results_is_noop() {
        let mut app = test_app_empty_library();

        app.update(Action::DiscoverDismiss);

        assert!(app.discover_results.is_empty());
        assert_eq!(app.discover_cursor, 0);
    }

    #[test]
    fn discover_resets_cursor_on_new_results() {
        let mut app = test_app_with_favorites();
        app.discover_cursor = 5;

        app.update(Action::Discover);

        assert_eq!(app.discover_cursor, 0);
    }

    // --- Async discover fetch tests ---

    #[test]
    fn discover_empty_profile_skips_fetch_and_shows_info() {
        let mut app = test_app_empty_library();

        app.update(Action::Discover);

        assert!(app.discover_fetch_pending.is_none());
        assert!(matches!(
            app.ui.notice.current,
            Some(AppNotice::Info(ref msg)) if msg.contains("no genre or tag data")
        ));
    }

    #[test]
    fn discover_with_profile_sets_pending_tag_query() {
        let mut app = test_app_with_favorites();

        app.update(Action::Discover);

        let request = app.discover_fetch_pending.as_ref().unwrap();
        // Profile has jazz genre favored twice — should appear in primary tag
        assert!(request.primary_tag.contains("jazz"));
    }

    #[test]
    fn take_discover_fetch_request_consumes_pending() {
        let mut app = test_app_with_favorites();
        app.update(Action::Discover);
        assert!(app.discover_fetch_pending.is_some());

        let taken = app.take_discover_fetch_request();

        assert!(taken.is_some());
        assert!(app.discover_fetch_pending.is_none());
    }

    #[test]
    fn discover_sets_fallback_tag_when_profile_has_multiple_entries() {
        let mut library = Library::in_memory(vec![
            station("Jazz FM", "http://jazz", "Jazz"),
            station("Rock Radio", "http://rock", "Rock"),
            station("Jazz Cafe", "http://jazzcafe", "Jazz"),
            station("Rock Live", "http://rocklive", "Rock"),
        ]);
        library.settings.favorites.toggle("http://jazz");
        library.settings.favorites.toggle("http://jazzcafe");
        library.settings.favorites.toggle("http://rock");
        let mut app = App::new(library);

        app.update(Action::Discover);

        let request = app.discover_fetch_pending.as_ref().unwrap();
        // Profile has jazz(2) and rock(1); primary is one, fallback is the other
        assert!(!request.primary_tag.is_empty());
        assert!(request.fallback_tag.is_some());
        let fallback = request.fallback_tag.as_ref().unwrap();
        assert_ne!(&request.primary_tag, fallback);
    }

    #[test]
    fn discover_skips_fallback_when_single_entry_profile() {
        // Only one genre/tag in the profile
        let mut library = Library::in_memory(vec![
            station("Jazz FM", "http://jazz", "Jazz"),
            station("Jazz Cafe", "http://jazzcafe", "Jazz"),
        ]);
        library.settings.favorites.toggle("http://jazz");
        library.settings.favorites.toggle("http://jazzcafe");
        let mut app = App::new(library);

        app.update(Action::Discover);

        let request = app.discover_fetch_pending.as_ref().unwrap();
        assert_eq!(request.primary_tag, "jazz");
        assert_eq!(request.fallback_tag, None);
    }

    #[test]
    fn apply_discover_response_success_populates_results() {
        let mut app = test_app_with_favorites();
        let mut candidate = station("Jazz Station", "http://new-jazz", "Jazz");
        candidate.tags = vec!["jazz".to_string()];

        app.apply_discover_response(Ok(vec![candidate]));

        assert!(!app.discover_results.is_empty());
        assert_eq!(app.discover_cursor, 0);
    }

    #[test]
    fn apply_discover_response_error_shows_error_notice() {
        let mut app = test_app_with_favorites();

        app.apply_discover_response(Err("network timeout".to_string()));

        assert!(app.discover_results.is_empty());
        assert_eq!(app.discover_cursor, 0);
        assert!(matches!(
            app.ui.notice.current,
            Some(AppNotice::Error(ref msg)) if msg.contains("network timeout")
        ));
    }

    #[test]
    fn apply_discover_response_empty_results_sets_info_notice() {
        let mut app = test_app_with_favorites();
        // Candidates with no matching genre/tags → all score zero → empty results
        let candidate = station("Unrelated FM", "http://unrelated", "Polka");

        app.apply_discover_response(Ok(vec![candidate]));

        assert!(app.discover_results.is_empty());
        assert!(matches!(
            app.ui.notice.current,
            Some(AppNotice::Info(ref msg)) if msg.contains("No matches found")
        ));
    }

    #[test]
    fn apply_discover_response_excludes_library_stations() {
        let mut app = test_app_with_favorites();
        // "http://jazz" is already in library
        let candidate_in_library = station("Jazz FM", "http://jazz", "Jazz");
        let mut candidate_new = station("New Jazz", "http://new-jazz", "Jazz");
        candidate_new.tags = vec!["jazz".to_string()];

        app.apply_discover_response(Ok(vec![candidate_in_library, candidate_new]));

        // Only the new station should appear (library URL excluded)
        for result in &app.discover_results {
            assert_ne!(result.station.url, "http://jazz");
        }
    }

    #[test]
    fn test_discover_uses_app_config_for_exclusions() {
        let mut app = test_app_with_favorites();
        app.config.discover.exclude_countries = vec!["US".to_string()];

        let mut candidate_us = station("US Jazz", "http://us-jazz", "Jazz");
        candidate_us.tags = vec!["jazz".to_string()];
        candidate_us.country_code = "US".to_string();

        let mut candidate_de = station("DE Jazz", "http://de-jazz", "Jazz");
        candidate_de.tags = vec!["jazz".to_string()];
        candidate_de.country_code = "DE".to_string();

        app.apply_discover_response(Ok(vec![candidate_us, candidate_de]));

        // US stations should be excluded by app config
        assert!(!app.discover_results.is_empty());
        for result in &app.discover_results {
            assert_ne!(
                result.station.country_code, "US",
                "US stations should be excluded by config"
            );
        }
    }
}
