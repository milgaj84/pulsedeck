use super::*;
use crate::library_filter::{filter_stations, LIBRARY_FILTER_MAX_QUERY};

impl App {
    pub(super) fn enter_library_filter(&mut self) {
        if self.library.stations.is_empty() {
            return;
        }

        self.number_jump.clear();
        self.ui.nav.library_filter_selected_snapshot = self.ui.nav.selected;
        self.ui.nav.library_filter_genre_snapshot = self.ui.nav.selected_genre_idx;
        self.library_filter_query.clear();
        self.ui.input_mode = InputMode::LibraryFilter;
    }

    pub(super) fn exit_library_filter(&mut self) {
        self.ui.input_mode = InputMode::Normal;
        self.ui.nav.selected = self.ui.nav.library_filter_selected_snapshot;
        self.ui.nav.selected_genre_idx = self.ui.nav.library_filter_genre_snapshot;
    }

    pub(super) fn library_filter_input(&mut self, c: char) {
        if self.library_filter_query.len() >= LIBRARY_FILTER_MAX_QUERY {
            return;
        }
        self.library_filter_query.push(c);
        self.clamp_filter_selection();
    }

    pub(super) fn library_filter_backspace(&mut self) {
        self.library_filter_query.pop();
        self.clamp_filter_selection();
    }

    pub(super) fn library_filter_confirm(&mut self) {
        let filtered = self.filtered_station_count();
        if filtered == 0 {
            return;
        }
        self.play_selected();
        self.ui.input_mode = InputMode::Normal;
    }

    pub(super) fn library_filter_next(&mut self) {
        let count = self.filtered_station_count();
        if count > 0 && self.ui.nav.selected < count - 1 {
            self.ui.nav.selected += 1;
        }
    }

    pub(super) fn library_filter_prev(&mut self) {
        if self.ui.nav.selected > 0 {
            self.ui.nav.selected -= 1;
        }
    }

    fn filtered_station_count(&self) -> usize {
        let stations = &self.library.stations;
        filter_stations(stations, &self.library_filter_query).len()
    }

    fn clamp_filter_selection(&mut self) {
        let count = self.filtered_station_count();
        if count == 0 {
            self.ui.nav.selected = 0;
        } else if self.ui.nav.selected >= count {
            self.ui.nav.selected = count - 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Action;
    use crate::favorites::Library;
    use crate::radio::Station;

    fn station(name: &str, url: &str) -> Station {
        Station::basic(name, url, "Synthwave", "US", 128)
    }

    fn test_app() -> App {
        App::new(Library::in_memory(vec![
            station("Alpha Stream", "http://alpha"),
            station("Beta FM", "http://beta"),
            station("Gamma Radio", "http://gamma"),
        ]))
    }

    #[test]
    fn enter_library_filter_from_normal_mode() {
        let mut app = test_app();
        app.ui.nav.selected = 1;
        app.ui.nav.selected_genre_idx = 0;

        app.update(Action::EnterLibraryFilter);

        assert_eq!(app.ui.input_mode, InputMode::LibraryFilter);
        assert_eq!(app.ui.nav.library_filter_selected_snapshot, 1);
        assert_eq!(app.ui.nav.library_filter_genre_snapshot, 0);
        assert!(app.library_filter_query.is_empty());
    }

    #[test]
    fn enter_library_filter_noop_on_empty_library() {
        let mut app = App::new(Library::in_memory(vec![]));

        app.update(Action::EnterLibraryFilter);

        assert_eq!(app.ui.input_mode, InputMode::Normal);
    }

    #[test]
    fn exit_library_filter_restores_state() {
        let mut app = test_app();
        app.ui.nav.selected = 2;
        app.ui.nav.selected_genre_idx = 0;

        app.update(Action::EnterLibraryFilter);
        app.update(Action::LibraryFilterInput('a'));
        app.ui.nav.selected = 0;
        app.update(Action::ExitLibraryFilter);

        assert_eq!(app.ui.input_mode, InputMode::Normal);
        assert_eq!(app.ui.nav.selected, 2);
        assert_eq!(app.ui.nav.selected_genre_idx, 0);
    }

    #[test]
    fn library_filter_input_appends_and_clamps() {
        let mut app = test_app();
        app.update(Action::EnterLibraryFilter);
        app.ui.nav.selected = 2;

        // Typing "alpha" should match only "Alpha Stream" — clamp selection to 0
        app.update(Action::LibraryFilterInput('a'));
        app.update(Action::LibraryFilterInput('l'));
        app.update(Action::LibraryFilterInput('p'));
        app.update(Action::LibraryFilterInput('h'));
        app.update(Action::LibraryFilterInput('a'));

        assert_eq!(app.library_filter_query, "alpha");
        assert_eq!(app.ui.nav.selected, 0);
    }

    #[test]
    fn library_filter_input_respects_max_length() {
        let mut app = test_app();
        app.update(Action::EnterLibraryFilter);
        app.library_filter_query = "x".repeat(LIBRARY_FILTER_MAX_QUERY);

        app.update(Action::LibraryFilterInput('z'));

        assert_eq!(app.library_filter_query.len(), LIBRARY_FILTER_MAX_QUERY);
    }

    #[test]
    fn library_filter_backspace_removes_char_and_reclamps() {
        let mut app = test_app();
        app.update(Action::EnterLibraryFilter);
        app.update(Action::LibraryFilterInput('a'));
        app.update(Action::LibraryFilterInput('l'));
        app.update(Action::LibraryFilterInput('p'));
        app.update(Action::LibraryFilterInput('h'));
        app.update(Action::LibraryFilterInput('a'));
        assert_eq!(app.library_filter_query, "alpha");

        app.update(Action::LibraryFilterBackspace);
        app.update(Action::LibraryFilterBackspace);
        app.update(Action::LibraryFilterBackspace);
        app.update(Action::LibraryFilterBackspace);
        app.update(Action::LibraryFilterBackspace);

        assert!(app.library_filter_query.is_empty());
        // Empty query matches all 3 stations
        assert!(app.ui.nav.selected < 3);
    }

    #[test]
    fn library_filter_confirm_plays_selected_station() {
        let mut app = test_app();
        app.update(Action::EnterLibraryFilter);
        app.ui.nav.selected = 1;

        app.update(Action::LibraryFilterConfirm);

        assert_eq!(app.ui.input_mode, InputMode::Normal);
        assert_eq!(
            app.playback.view.playing_url.as_deref(),
            Some("http://beta")
        );
    }

    #[test]
    fn library_filter_confirm_noop_on_empty_filtered_list() {
        let mut app = test_app();
        app.update(Action::EnterLibraryFilter);
        // Type something that matches nothing
        app.library_filter_query = "zzzzzzz".to_string();

        app.update(Action::LibraryFilterConfirm);

        assert_eq!(app.ui.input_mode, InputMode::LibraryFilter);
        assert_eq!(app.playback.view.playing_url, None);
    }

    #[test]
    fn library_filter_navigation_clamps_at_bounds() {
        let mut app = test_app();
        app.update(Action::EnterLibraryFilter);
        // All 3 stations visible with empty query
        app.ui.nav.selected = 2;

        app.library_filter_next();
        assert_eq!(app.ui.nav.selected, 2); // clamped at last

        app.ui.nav.selected = 0;
        app.library_filter_prev();
        assert_eq!(app.ui.nav.selected, 0); // clamped at first
    }

    #[test]
    fn entering_library_filter_while_number_jump_active() {
        let mut app = test_app();
        app.number_jump.push_digit('3');
        assert!(app.number_jump.is_active());

        app.update(Action::EnterLibraryFilter);

        // Number jump should be cleared when entering a different mode
        assert!(!app.number_jump.is_active());
        assert_eq!(app.ui.input_mode, InputMode::LibraryFilter);
    }

    #[test]
    fn f6_exits_library_filter_and_toggles_mini_mode() {
        let mut app = test_app();
        app.ui.nav.selected = 1;
        app.update(Action::EnterLibraryFilter);
        assert_eq!(app.ui.input_mode, InputMode::LibraryFilter);
        assert_eq!(app.ui.display_mode, DisplayMode::Normal);

        app.update(Action::ToggleMiniMode);

        assert_eq!(app.ui.input_mode, InputMode::Normal);
        assert_eq!(app.ui.nav.selected, 1); // restored from snapshot
        assert_eq!(app.ui.display_mode, DisplayMode::Mini);
    }
}
