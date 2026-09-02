use super::*;
use crate::audio::AudioCommand;

pub struct SearchState {
    pub query: String,
    pub status: SearchStatus,
    pub results: Vec<Station>,
    pub pending_api_search: Option<String>,
    pub searching_api: bool,
    pub last_api_query: String,
    /// `None` when not cycling, `Some(index)` when cycling through history ring.
    pub history_cycling: Option<usize>,
}

impl Default for SearchState {
    fn default() -> Self {
        Self {
            query: String::new(),
            status: SearchStatus::WaitingForInput,
            results: Vec::new(),
            pending_api_search: None,
            searching_api: false,
            last_api_query: String::new(),
            history_cycling: None,
        }
    }
}

impl App {
    pub(super) fn enter_search(&mut self) {
        self.remember_current_genre_selection();
        self.ui.nav.normal_selected_snapshot = self.ui.nav.selected;
        self.ui.input_mode = InputMode::Search;
        self.search.query.clear();
        self.search.results.clear();
        self.search.last_api_query.clear();
        self.search.status = SearchStatus::WaitingForInput;
        self.search.searching_api = false;
        self.search.pending_api_search = None;
        self.search.history_cycling = None;
        self.ui.nav.selected = clamped_index(
            self.ui.nav.search_selected_snapshot,
            self.search.results.len(),
        );
    }

    pub(super) fn exit_search(&mut self) {
        self.ui.nav.search_selected_snapshot = self.ui.nav.selected;
        self.ui.input_mode = InputMode::Normal;
        self.search.query.clear();
        self.search.results.clear();
        self.search.last_api_query.clear();
        self.search.status = SearchStatus::WaitingForInput;
        self.search.searching_api = false;
        self.search.pending_api_search = None;
        self.search.history_cycling = None;
        self.restore_normal_selection_snapshot();
    }

    pub(super) fn search_input(&mut self, c: char) {
        self.search.history_cycling = None;
        self.search.query.push(c);
        self.refresh_search_state();
    }

    pub(super) fn search_backspace(&mut self) {
        self.search.query.pop();
        self.refresh_search_state();
    }

    pub(super) fn search_history_up(&mut self) {
        if self.search.query.is_empty() || self.search.history_cycling.is_some() {
            self.cycle_history_backward();
        } else {
            self.prev_search_result();
        }
    }

    pub(super) fn search_history_down(&mut self) {
        if self.search.history_cycling.is_some() {
            self.cycle_history_forward();
        } else {
            self.next_search_result();
        }
    }

    fn cycle_history_backward(&mut self) {
        let ring_len = self.search_history.len();
        if ring_len == 0 {
            return;
        }

        let index = match self.search.history_cycling {
            None => ring_len - 1,
            Some(i) => {
                if i == 0 {
                    ring_len - 1
                } else {
                    i - 1
                }
            }
        };

        self.search.history_cycling = Some(index);
        if let Some(entry) = self.search_history.get(index) {
            self.search.query = entry.to_string();
        }
    }

    fn cycle_history_forward(&mut self) {
        let ring_len = self.search_history.len();
        if ring_len == 0 {
            return;
        }

        let index = match self.search.history_cycling {
            None => return,
            Some(i) => (i + 1) % ring_len,
        };

        self.search.history_cycling = Some(index);
        if let Some(entry) = self.search_history.get(index) {
            self.search.query = entry.to_string();
        }
    }

    fn push_search_query_to_history(&mut self) -> bool {
        let trimmed = self.search.query.trim();
        if !trimmed.is_empty() {
            self.search_history.push(trimmed)
        } else {
            false
        }
    }

    fn prev_search_result(&mut self) {
        let count = self.search.results.len();
        if count > 0 {
            self.ui.nav.selected = if self.ui.nav.selected == 0 {
                count - 1
            } else {
                self.ui.nav.selected - 1
            };
        }
    }

    fn next_search_result(&mut self) {
        let count = self.search.results.len();
        if count > 0 {
            self.ui.nav.selected = (self.ui.nav.selected + 1) % count;
        }
    }

    pub(super) fn confirm_search(&mut self) {
        let pushed = self.push_search_query_to_history();
        if pushed {
            if let Some(dir) = &self.config_dir {
                let _ = self
                    .search_history
                    .save(&dir.join(startup::SEARCH_HISTORY_FILE));
            }
        }

        // Add the selected search result to library and play it.
        let played = if let Some(station) = self.search.results.get(self.ui.nav.selected).cloned() {
            self.playback.reconnect.disarm();
            match self.library.add(station.clone()) {
                Ok(true) => {
                    self.mark_library_dirty();
                    self.set_info_notice("Station saved to library");
                }
                Ok(false) => {
                    if self.library.enrich_matching_station(&station) {
                        self.mark_library_dirty();
                        self.set_info_notice("Saved station metadata refreshed");
                    }
                }
                Err(err) => self.set_operation_error_notice("Could not add station", &err),
            }
            self.playback.view.playing_url = Some(station.url.clone());

            // Persist last played station URL.
            self.library.settings.last_played_url = Some(station.url.clone());
            self.mark_library_dirty();

            self.playback.elapsed_timer.reset();
            self.playback.elapsed_timer.start();

            if self.send_audio_command(AudioCommand::Play(station.url)) {
                self.sync_volume();
                true
            } else {
                false
            }
        } else {
            false
        };

        self.exit_search();
        if played {
            self.select_playing();
        }
    }

    pub(super) fn audition_search_result(&mut self) {
        // Push to in-memory ring only; audition does not persist to disk.
        let _ = self.push_search_query_to_history();

        if let Some(station) = self.search.results.get(self.ui.nav.selected).cloned() {
            self.playback.reconnect.disarm();
            let next_playback = if matches!(
                &self.playback.view.state,
                PlaybackState::Playing | PlaybackState::Paused | PlaybackState::FadingOut { .. }
            ) {
                PlaybackState::FadingOut {
                    current_volume: if self.playback.muted {
                        0.0
                    } else {
                        self.playback.volume as f32 / 100.0
                    },
                }
            } else {
                PlaybackState::Connecting
            };

            self.playback.view.playing_url = Some(station.url.clone());
            self.playback.view.state = next_playback;
            self.playback.elapsed_timer.reset();
            self.playback.elapsed_timer.start();
            if self.send_audio_command(AudioCommand::Play(station.url)) {
                self.sync_volume();
                self.set_info_notice("Auditioning stream (not saved to library)");
            }
        }
    }

    /// Return the query currently waiting for debounce, if any.
    pub fn current_debounce_query(&self) -> Option<&str> {
        match &self.search.status {
            SearchStatus::Debouncing { query } => Some(query.as_str()),
            _ => None,
        }
    }

    /// Mark a debounced query as actively searching.
    pub fn mark_search_started(&mut self, query: &str) -> bool {
        let current_query = self.search.query.trim();
        if self.ui.input_mode != InputMode::Search || current_query != query {
            return false;
        }

        if matches!(&self.search.status, SearchStatus::Debouncing { query: q } if q == query) {
            self.search.status = SearchStatus::Searching {
                query: query.to_string(),
            };
            self.search.searching_api = true;
            self.search.last_api_query = query.to_string();
            self.search.pending_api_search = None;
            true
        } else {
            false
        }
    }

    /// Apply a query-tagged search response. Returns false when the response was stale.
    pub fn apply_search_response(
        &mut self,
        query: String,
        result: Result<Vec<Station>, String>,
    ) -> bool {
        let current_query = self.search.query.trim().to_string();
        let is_current_search = self.ui.input_mode == InputMode::Search
            && current_query == query
            && matches!(
                &self.search.status,
                SearchStatus::Searching { query: q }
                    | SearchStatus::StaleResponseDiscarded { query: q, .. } if q == &query
            );

        if !is_current_search {
            self.note_stale_search_response(&current_query, query);
            return false;
        }

        self.search.searching_api = false;
        self.ui.nav.search_selected_snapshot = 0;
        self.ui.nav.selected = 0;

        match result {
            Ok(results) => {
                // Radio Browser responded successfully — mark as available if previously down.
                if self.radio_browser_status.is_unavailable() {
                    self.radio_browser_status.mark_available();
                }
                self.search.results = results;
                if self.search.results.is_empty() {
                    self.search.status = SearchStatus::Empty { query };
                } else {
                    self.search.status = SearchStatus::Ready { query };
                }
            }
            Err(message) => {
                // Radio Browser failed — show notice only on first failure.
                if self.radio_browser_status.mark_unavailable() {
                    self.set_info_notice(
                        "Radio Browser is unavailable — showing saved library only",
                    );
                }
                self.search.results.clear();
                self.search.status = SearchStatus::Error { query, message };
            }
        }

        true
    }

    fn note_stale_search_response(&mut self, current_query: &str, received_stale: String) {
        if self.ui.input_mode != InputMode::Search
            || current_query.chars().count() < types::SEARCH_MIN_CHARS
            || current_query == received_stale
        {
            return;
        }

        self.search.searching_api = matches!(
            &self.search.status,
            SearchStatus::Searching { query }
                | SearchStatus::StaleResponseDiscarded { query, .. } if query == current_query
        );
        self.search.status = SearchStatus::StaleResponseDiscarded {
            query: current_query.to_string(),
            received_stale,
        };
    }

    pub(super) fn refresh_search_state(&mut self) {
        let query = self.search.query.trim().to_string();

        if query.chars().count() < types::SEARCH_MIN_CHARS {
            self.search.results.clear();
            self.ui.nav.search_selected_snapshot = 0;
            self.ui.nav.selected = 0;
            self.search.searching_api = false;
            self.search.pending_api_search = None;
            self.search.status = SearchStatus::WaitingForInput;
            return;
        }

        let is_already_current = matches!(
            &self.search.status,
            SearchStatus::Debouncing { query: q }
                | SearchStatus::Searching { query: q }
                | SearchStatus::Ready { query: q }
                | SearchStatus::Empty { query: q }
                | SearchStatus::Error { query: q, .. }
                | SearchStatus::StaleResponseDiscarded { query: q, .. } if q == &query
        );

        if is_already_current {
            return;
        }

        self.search.results.clear();
        self.ui.nav.search_selected_snapshot = 0;
        self.ui.nav.selected = 0;
        self.search.searching_api = false;
        self.search.pending_api_search = None;
        self.search.status = SearchStatus::Debouncing { query };
    }

    fn restore_normal_selection_snapshot(&mut self) {
        let count = self.visible_count();
        if count == 0 {
            self.ui.nav.selected = 0;
        } else {
            self.ui.nav.selected = self.ui.nav.normal_selected_snapshot.min(count - 1);
        }
    }
}

fn clamped_index(index: usize, len: usize) -> usize {
    if len == 0 {
        0
    } else {
        index.min(len - 1)
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
        let mut app = App::new(Library::in_memory(vec![]));
        app.search_history = crate::search_history::SearchHistoryRing::new();
        app
    }

    fn notice_text(app: &App) -> Option<&str> {
        match app.ui.notice.current.as_ref() {
            Some(AppNotice::Info(message)) | Some(AppNotice::Error(message)) => {
                Some(message.as_str())
            }
            None => None,
        }
    }

    #[test]
    fn short_search_query_clears_results_and_waits_for_input() {
        let mut app = test_app();
        app.update(Action::EnterSearch);
        app.search.results = vec![station("Old", "http://old")];

        app.update(Action::SearchInput('l'));

        assert!(app.search.results.is_empty());
        assert_eq!(app.search.status, SearchStatus::WaitingForInput);
        assert!(app.current_debounce_query().is_none());
        assert!(!app.search.searching_api);
    }

    #[test]
    fn valid_search_query_enters_debounce_state() {
        let mut app = test_app();
        app.update(Action::EnterSearch);

        app.update(Action::SearchInput('l'));
        app.update(Action::SearchInput('o'));

        assert_eq!(
            app.search.status,
            SearchStatus::Debouncing {
                query: "lo".to_string()
            }
        );
        assert_eq!(app.current_debounce_query(), Some("lo"));
        assert!(!app.search.searching_api);
    }

    #[test]
    fn mark_search_started_moves_debounced_query_to_searching() {
        let mut app = test_app();
        app.update(Action::EnterSearch);
        app.update(Action::SearchInput('l'));
        app.update(Action::SearchInput('o'));

        assert!(app.mark_search_started("lo"));
        assert_eq!(
            app.search.status,
            SearchStatus::Searching {
                query: "lo".to_string()
            }
        );
        assert!(app.search.searching_api);
    }

    #[test]
    fn current_query_success_response_is_accepted() {
        let mut app = test_app();
        app.update(Action::EnterSearch);
        app.update(Action::SearchInput('l'));
        app.update(Action::SearchInput('o'));
        app.mark_search_started("lo");

        let accepted = app.apply_search_response(
            "lo".to_string(),
            Ok(vec![station("Lo-Fi Radio", "http://lofi")]),
        );

        assert!(accepted);
        assert_eq!(app.search.results.len(), 1);
        assert_eq!(
            app.search.status,
            SearchStatus::Ready {
                query: "lo".to_string()
            }
        );
        assert!(!app.search.searching_api);
        assert_eq!(app.ui.nav.selected, 0);
    }

    #[test]
    fn current_query_empty_response_sets_empty_status() {
        let mut app = test_app();
        app.update(Action::EnterSearch);
        app.update(Action::SearchInput('z'));
        app.update(Action::SearchInput('z'));
        app.mark_search_started("zz");

        let accepted = app.apply_search_response("zz".to_string(), Ok(vec![]));

        assert!(accepted);
        assert!(app.search.results.is_empty());
        assert_eq!(
            app.search.status,
            SearchStatus::Empty {
                query: "zz".to_string()
            }
        );
    }

    #[test]
    fn current_query_error_response_sets_error_status() {
        let mut app = test_app();
        app.update(Action::EnterSearch);
        app.update(Action::SearchInput('l'));
        app.update(Action::SearchInput('o'));
        app.mark_search_started("lo");

        let accepted = app.apply_search_response("lo".to_string(), Err("network down".to_string()));

        assert!(accepted);
        assert!(app.search.results.is_empty());
        assert_eq!(
            app.search.status,
            SearchStatus::Error {
                query: "lo".to_string(),
                message: "network down".to_string()
            }
        );
        assert!(!app.search.searching_api);
    }

    #[test]
    fn stale_search_response_is_reported_without_overwriting_results() {
        let mut app = test_app();
        app.update(Action::EnterSearch);
        app.update(Action::SearchInput('l'));
        app.update(Action::SearchInput('o'));
        app.update(Action::SearchInput('f'));
        app.update(Action::SearchInput('i'));
        app.mark_search_started("lofi");

        let accepted = app.apply_search_response(
            "lo".to_string(),
            Ok(vec![station("Old Result", "http://old")]),
        );

        assert!(!accepted);
        assert!(app.search.results.is_empty());
        assert!(app.search.searching_api);
        assert_eq!(
            app.search.status,
            SearchStatus::StaleResponseDiscarded {
                query: "lofi".to_string(),
                received_stale: "lo".to_string()
            }
        );
    }

    #[test]
    fn current_response_is_accepted_after_stale_response_notice() {
        let mut app = test_app();
        app.update(Action::EnterSearch);
        app.update(Action::SearchInput('j'));
        app.update(Action::SearchInput('a'));
        app.update(Action::SearchInput('z'));
        app.update(Action::SearchInput('z'));
        app.mark_search_started("jazz");

        assert!(!app.apply_search_response("synth".to_string(), Ok(vec![])));

        let accepted = app.apply_search_response(
            "jazz".to_string(),
            Ok(vec![station("Jazz Radio", "http://jazz")]),
        );

        assert!(accepted);
        assert_eq!(app.search.results.len(), 1);
        assert_eq!(
            app.search.status,
            SearchStatus::Ready {
                query: "jazz".to_string()
            }
        );
        assert!(!app.search.searching_api);
    }

    #[test]
    fn late_stale_response_after_ready_keeps_results_and_reports_discard() {
        let mut app = test_app();
        app.update(Action::EnterSearch);
        app.update(Action::SearchInput('j'));
        app.update(Action::SearchInput('a'));
        app.update(Action::SearchInput('z'));
        app.update(Action::SearchInput('z'));
        app.mark_search_started("jazz");
        assert!(app.apply_search_response(
            "jazz".to_string(),
            Ok(vec![station("Jazz Radio", "http://jazz")]),
        ));

        let accepted = app.apply_search_response(
            "synth".to_string(),
            Ok(vec![station("Synth Radio", "http://synth")]),
        );

        assert!(!accepted);
        assert_eq!(app.search.results.len(), 1);
        assert_eq!(app.search.results[0].url, "http://jazz");
        assert!(!app.search.searching_api);
        assert_eq!(
            app.search.status,
            SearchStatus::StaleResponseDiscarded {
                query: "jazz".to_string(),
                received_stale: "synth".to_string()
            }
        );
    }

    #[test]
    fn normal_mode_search_response_is_ignored() {
        let mut app = test_app();

        let accepted = app.apply_search_response(
            "lo".to_string(),
            Ok(vec![station("Ignored", "http://ignored")]),
        );

        assert!(!accepted);
        assert!(app.search.results.is_empty());
        assert_eq!(app.search.status, SearchStatus::WaitingForInput);
    }

    #[test]
    fn search_audition_plays_result_without_saving_or_exiting_search() {
        let mut app = test_app();
        app.update(Action::EnterSearch);
        app.search.results = vec![station("Lo-Fi Radio", "http://lofi")];
        app.ui.nav.selected = 0;
        app.library.settings.last_played_url = Some("http://previous".to_string());

        app.update(Action::SearchAudition);

        assert_eq!(app.ui.input_mode, InputMode::Search);
        assert_eq!(
            app.playback.view.playing_url.as_deref(),
            Some("http://lofi")
        );
        assert_eq!(app.playback.view.state, PlaybackState::Connecting);
        assert!(!app.library.contains("http://lofi"));
        assert_eq!(
            app.library.settings.last_played_url.as_deref(),
            Some("http://previous")
        );
        assert_eq!(
            notice_text(&app),
            Some("Auditioning stream (not saved to library)")
        );
        assert_eq!(app.search.results.len(), 1);
    }

    #[test]
    fn search_audition_without_result_keeps_search_state_unchanged() {
        let mut app = test_app();
        app.update(Action::EnterSearch);

        app.update(Action::SearchAudition);

        assert_eq!(app.ui.input_mode, InputMode::Search);
        assert_eq!(app.playback.view.playing_url, None);
        assert_eq!(app.playback.view.state, PlaybackState::Stopped);
        assert!(app.search.results.is_empty());
    }

    #[test]
    fn search_confirm_adds_result_exits_search_and_selects_playing() {
        let mut app = test_app();
        app.update(Action::EnterSearch);
        app.search.results = vec![station("Lo-Fi Radio", "http://lofi")];
        app.ui.nav.selected = 0;

        app.update(Action::SearchConfirm);

        assert_eq!(app.ui.input_mode, InputMode::Normal);
        assert_eq!(
            app.playback.view.playing_url.as_deref(),
            Some("http://lofi")
        );
        assert!(app.library.contains("http://lofi"));
        assert_eq!(app.search.status, SearchStatus::WaitingForInput);
        assert!(app.search.results.is_empty());
    }

    #[test]
    fn search_confirm_without_result_exits_search_without_playing() {
        let mut app = test_app();
        app.update(Action::EnterSearch);

        app.update(Action::SearchConfirm);

        assert_eq!(app.ui.input_mode, InputMode::Normal);
        assert_eq!(app.playback.view.playing_url, None);
        assert!(app.search.results.is_empty());
    }

    #[test]
    fn exit_search_restores_library_selection_snapshot() {
        let mut app = App::new(Library::in_memory(vec![
            station("Library A", "http://library-a"),
            station("Library B", "http://library-b"),
        ]));
        app.ui.nav.selected = 1;

        app.update(Action::EnterSearch);
        app.search.results = vec![station("Search A", "http://search-a")];
        app.ui.nav.selected = 0;
        app.update(Action::ExitSearch);

        assert_eq!(app.ui.input_mode, InputMode::Normal);
        assert_eq!(app.ui.nav.selected, 1);
        assert_eq!(
            app.visible_stations()[app.ui.nav.selected].url,
            "http://library-b"
        );
    }

    #[test]
    fn search_response_resets_search_selection_snapshot() {
        let mut app = test_app();
        app.update(Action::EnterSearch);
        app.ui.nav.search_selected_snapshot = 4;
        app.update(Action::SearchInput('l'));
        app.update(Action::SearchInput('o'));
        app.mark_search_started("lo");

        assert!(app.apply_search_response(
            "lo".to_string(),
            Ok(vec![station("Lo-Fi Radio", "http://lofi")]),
        ));

        assert_eq!(app.ui.nav.selected, 0);
        assert_eq!(app.ui.nav.search_selected_snapshot, 0);
    }

    #[test]
    fn clamped_index_handles_empty_and_short_lists() {
        assert_eq!(clamped_index(5, 0), 0);
        assert_eq!(clamped_index(5, 2), 1);
        assert_eq!(clamped_index(1, 2), 1);
    }

    // ---- History cycling tests ----

    fn test_app_with_history(entries: Vec<&str>) -> App {
        let mut app = test_app();
        for entry in entries {
            app.search_history.push(entry);
        }
        app
    }

    #[test]
    fn test_up_arrow_empty_input_enters_cycling() {
        let mut app = test_app_with_history(vec!["jazz", "lofi", "synthwave"]);
        app.update(Action::EnterSearch);

        app.update(Action::SearchHistoryUp);

        assert_eq!(app.search.history_cycling, Some(2));
        assert_eq!(app.search.query, "synthwave");
    }

    #[test]
    fn test_up_arrow_cycles_backward() {
        let mut app = test_app_with_history(vec!["jazz", "lofi", "synthwave"]);
        app.update(Action::EnterSearch);

        app.update(Action::SearchHistoryUp); // index 2 → "synthwave"
        app.update(Action::SearchHistoryUp); // index 1 → "lofi"
        app.update(Action::SearchHistoryUp); // index 0 → "jazz"
        app.update(Action::SearchHistoryUp); // wraps to index 2 → "synthwave"

        assert_eq!(app.search.history_cycling, Some(2));
        assert_eq!(app.search.query, "synthwave");
    }

    #[test]
    fn test_down_arrow_cycles_forward() {
        let mut app = test_app_with_history(vec!["jazz", "lofi", "synthwave"]);
        app.update(Action::EnterSearch);

        app.update(Action::SearchHistoryUp); // index 2 → "synthwave"
        app.update(Action::SearchHistoryDown); // index 0 → "jazz"

        assert_eq!(app.search.history_cycling, Some(0));
        assert_eq!(app.search.query, "jazz");
    }

    #[test]
    fn test_character_input_exits_cycling() {
        let mut app = test_app_with_history(vec!["jazz", "lofi"]);
        app.update(Action::EnterSearch);

        app.update(Action::SearchHistoryUp); // enter cycling
        assert!(app.search.history_cycling.is_some());

        app.update(Action::SearchInput('x'));

        assert_eq!(app.search.history_cycling, None);
        assert_eq!(app.search.query, "lofix");
    }

    #[test]
    fn test_up_arrow_empty_ring_no_op() {
        let mut app = test_app();
        app.update(Action::EnterSearch);

        app.update(Action::SearchHistoryUp);

        assert_eq!(app.search.history_cycling, None);
        assert_eq!(app.search.query, "");
    }

    // ---- Search history push tests ----

    #[test]
    fn test_confirm_search_pushes_query_to_history() {
        let mut app = test_app();
        app.update(Action::EnterSearch);
        app.search.query = "jazz".to_string();
        app.search.results = vec![station("Jazz FM", "http://jazz")];
        app.ui.nav.selected = 0;

        app.update(Action::SearchConfirm);

        assert_eq!(app.search_history.len(), 1);
        assert_eq!(app.search_history.get(0), Some("jazz"));
    }

    #[test]
    fn test_audition_pushes_query_to_history() {
        let mut app = test_app();
        app.update(Action::EnterSearch);
        app.search.query = "lofi".to_string();
        app.search.results = vec![station("Lofi Radio", "http://lofi")];
        app.ui.nav.selected = 0;

        app.update(Action::SearchAudition);

        assert_eq!(app.search_history.len(), 1);
        assert_eq!(app.search_history.get(0), Some("lofi"));
    }

    #[test]
    fn test_confirm_short_query_not_pushed() {
        let mut app = test_app();
        app.update(Action::EnterSearch);
        app.search.query = "a".to_string();
        app.search.results = vec![station("Station", "http://station")];
        app.ui.nav.selected = 0;

        app.update(Action::SearchConfirm);

        assert!(app.search_history.is_empty());
    }

    #[test]
    fn test_confirm_empty_query_not_pushed() {
        let mut app = test_app();
        app.update(Action::EnterSearch);
        app.search.query = String::new();

        app.update(Action::SearchConfirm);

        assert!(app.search_history.is_empty());
    }

    #[test]
    fn test_confirm_search_persists_history_to_disk() {
        let dir = std::env::temp_dir().join("pulsedeck_test_confirm_persists");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let mut app = test_app();
        app.config_dir = Some(dir.clone());
        app.update(Action::EnterSearch);
        app.search.query = "jazz".to_string();
        app.search.results = vec![station("Jazz FM", "http://jazz")];
        app.ui.nav.selected = 0;

        app.update(Action::SearchConfirm);

        let path = dir.join("search_history.json");
        assert!(
            path.exists(),
            "search_history.json should be written after confirm"
        );
        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(
            contents.contains("jazz"),
            "file should contain the pushed query"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_confirm_rejected_query_does_not_persist() {
        let dir = std::env::temp_dir().join("pulsedeck_test_confirm_rejected");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let mut app = test_app();
        app.config_dir = Some(dir.clone());
        app.update(Action::EnterSearch);
        app.search.query = "a".to_string(); // too short, push will reject
        app.search.results = vec![station("Station", "http://station")];
        app.ui.nav.selected = 0;

        app.update(Action::SearchConfirm);

        let path = dir.join("search_history.json");
        assert!(
            !path.exists(),
            "search_history.json should NOT be written for rejected query"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_audition_pushes_to_ring_but_does_not_persist() {
        let dir = std::env::temp_dir().join("pulsedeck_test_audition_no_persist");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let mut app = test_app();
        app.config_dir = Some(dir.clone());
        app.search_history = crate::search_history::SearchHistoryRing::new();
        app.update(Action::EnterSearch);
        app.search.query = "lofi".to_string();
        app.search.results = vec![station("Lofi Radio", "http://lofi")];
        app.ui.nav.selected = 0;

        app.update(Action::SearchAudition);

        // Ring should contain the query (pushed to memory)
        assert_eq!(app.search_history.len(), 1);
        assert_eq!(app.search_history.get(0), Some("lofi"));

        // No file should have been written (audition does NOT persist)
        let path = dir.join("search_history.json");
        assert!(
            !path.exists(),
            "search_history.json should NOT be written after audition"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use crate::action::Action;
    use crate::favorites::Library;
    use proptest::prelude::*;

    fn test_app() -> App {
        let mut app = App::new(Library::in_memory(vec![]));
        app.search_history = crate::search_history::SearchHistoryRing::new();
        app
    }

    // Feature: v090-features, Property 10: Search history Up-arrow cycling
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Validates: Requirements 4.4**
        #[test]
        fn up_arrow_cycling(
            n in 1usize..=10,
            k in 1usize..=30,
        ) {
            let entries = (0..n)
                .map(|i| format!("entry_{:02}", i))
                .collect::<Vec<_>>();

            let mut app = test_app();
            for entry in &entries {
                app.search_history.push(entry);
            }
            app.update(Action::EnterSearch);

            for _ in 0..k {
                app.update(Action::SearchHistoryUp);
            }

            let expected_index = n - 1 - ((k - 1) % n);
            let expected_query = &entries[expected_index];
            prop_assert_eq!(
                &app.search.query,
                expected_query,
                "After {} ups in ring of size {}, expected index {} = {:?}, got {:?}",
                k, n, expected_index, expected_query, app.search.query
            );
        }
    }

    // Feature: v090-features, Property 11: Search history Down-arrow cycling
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Validates: Requirements 4.5**
        #[test]
        fn down_arrow_cycling(
            n in 1usize..=10,
            initial_ups in 1usize..=10,
            d in 1usize..=30,
        ) {
            let entries = (0..n)
                .map(|i| format!("entry_{:02}", i))
                .collect::<Vec<_>>();

            let mut app = test_app();
            for entry in &entries {
                app.search_history.push(entry);
            }
            app.update(Action::EnterSearch);

            for _ in 0..initial_ups {
                app.update(Action::SearchHistoryUp);
            }

            for _ in 0..d {
                app.update(Action::SearchHistoryDown);
            }

            let pos_after_ups = n - 1 - ((initial_ups - 1) % n);
            let expected_index = (pos_after_ups + d) % n;
            let expected_query = &entries[expected_index];
            prop_assert_eq!(
                &app.search.query,
                expected_query,
                "After {} ups then {} downs in ring of size {}, expected index {} = {:?}, got {:?}",
                initial_ups, d, n, expected_index, expected_query, app.search.query
            );
        }
    }
}
