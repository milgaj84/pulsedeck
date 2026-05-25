use super::*;
use crate::audio::AudioCommand;

impl App {
    pub(super) fn enter_search(&mut self) {
        self.input_mode = InputMode::Search;
        self.search_query.clear();
        self.search_results.clear();
        self.last_api_query.clear();
        self.search_status = SearchStatus::WaitingForInput;
        self.searching_api = false;
        self.pending_api_search = None;
        self.selected = 0;
    }

    pub(super) fn exit_search(&mut self) {
        self.input_mode = InputMode::Normal;
        self.search_query.clear();
        self.search_results.clear();
        self.last_api_query.clear();
        self.search_status = SearchStatus::WaitingForInput;
        self.searching_api = false;
        self.pending_api_search = None;
        self.selected = 0;
        self.select_playing();
    }

    pub(super) fn search_input(&mut self, c: char) {
        self.search_query.push(c);
        self.refresh_search_state();
    }

    pub(super) fn search_backspace(&mut self) {
        self.search_query.pop();
        self.refresh_search_state();
    }

    pub(super) fn confirm_search(&mut self) {
        // Add the selected search result to library and play it.
        if let Some(station) = self.search_results.get(self.selected).cloned() {
            match self.library.add(station.clone()) {
                Ok(true) => self.set_info_notice("Station saved to library"),
                Ok(false) => {}
                Err(err) => self.set_error_notice(format!(
                    "Station added in memory, but could not save library: {err}"
                )),
            }
            self.playing_url = Some(station.url.clone());

            // Persist last played station URL.
            self.library.settings.last_played_url = Some(station.url.clone());
            self.save_library_or_notice("last played station");

            self.audio.send(AudioCommand::Play(station.url));
            self.sync_volume();
        }

        self.exit_search();
    }

    /// Return the query currently waiting for debounce, if any.
    pub fn current_debounce_query(&self) -> Option<&str> {
        match &self.search_status {
            SearchStatus::Debouncing { query } => Some(query.as_str()),
            _ => None,
        }
    }

    /// Mark a debounced query as actively searching.
    pub fn mark_search_started(&mut self, query: &str) -> bool {
        let current_query = self.search_query.trim();
        if self.input_mode != InputMode::Search || current_query != query {
            return false;
        }

        if matches!(&self.search_status, SearchStatus::Debouncing { query: q } if q == query) {
            self.search_status = SearchStatus::Searching {
                query: query.to_string(),
            };
            self.searching_api = true;
            self.last_api_query = query.to_string();
            self.pending_api_search = None;
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
        let current_query = self.search_query.trim();
        let is_current_search = self.input_mode == InputMode::Search
            && current_query == query
            && matches!(&self.search_status, SearchStatus::Searching { query: q } if q == &query);

        if !is_current_search {
            return false;
        }

        self.searching_api = false;
        self.selected = 0;

        match result {
            Ok(results) => {
                self.search_results = results;
                if self.search_results.is_empty() {
                    self.search_status = SearchStatus::Empty { query };
                } else {
                    self.search_status = SearchStatus::Ready { query };
                }
            }
            Err(message) => {
                self.search_results.clear();
                self.search_status = SearchStatus::Error { query, message };
            }
        }

        true
    }

    pub(super) fn refresh_search_state(&mut self) {
        let query = self.search_query.trim().to_string();

        if query.chars().count() < types::SEARCH_MIN_CHARS {
            self.search_results.clear();
            self.selected = 0;
            self.searching_api = false;
            self.pending_api_search = None;
            self.search_status = SearchStatus::WaitingForInput;
            return;
        }

        let is_already_current = matches!(
            &self.search_status,
            SearchStatus::Debouncing { query: q }
                | SearchStatus::Searching { query: q }
                | SearchStatus::Ready { query: q }
                | SearchStatus::Empty { query: q }
                | SearchStatus::Error { query: q, .. } if q == &query
        );

        if is_already_current {
            return;
        }

        self.search_results.clear();
        self.selected = 0;
        self.searching_api = false;
        self.pending_api_search = None;
        self.search_status = SearchStatus::Debouncing { query };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Action;
    use crate::favorites::Library;
    use crate::radio::Station;

    fn station(name: &str, url: &str) -> Station {
        Station {
            name: name.to_string(),
            url: url.to_string(),
            genre: "Synthwave".to_string(),
            country: "US".to_string(),
            bitrate: 128,
        }
    }

    fn test_app() -> App {
        App::new(Library::in_memory(vec![]))
    }

    #[test]
    fn short_search_query_clears_results_and_waits_for_input() {
        let mut app = test_app();
        app.update(Action::EnterSearch);
        app.search_results = vec![station("Old", "http://old")];

        app.update(Action::SearchInput('l'));

        assert!(app.search_results.is_empty());
        assert_eq!(app.search_status, SearchStatus::WaitingForInput);
        assert!(app.current_debounce_query().is_none());
        assert!(!app.searching_api);
    }

    #[test]
    fn valid_search_query_enters_debounce_state() {
        let mut app = test_app();
        app.update(Action::EnterSearch);

        app.update(Action::SearchInput('l'));
        app.update(Action::SearchInput('o'));

        assert_eq!(
            app.search_status,
            SearchStatus::Debouncing {
                query: "lo".to_string()
            }
        );
        assert_eq!(app.current_debounce_query(), Some("lo"));
        assert!(!app.searching_api);
    }

    #[test]
    fn mark_search_started_moves_debounced_query_to_searching() {
        let mut app = test_app();
        app.update(Action::EnterSearch);
        app.update(Action::SearchInput('l'));
        app.update(Action::SearchInput('o'));

        assert!(app.mark_search_started("lo"));
        assert_eq!(
            app.search_status,
            SearchStatus::Searching {
                query: "lo".to_string()
            }
        );
        assert!(app.searching_api);
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
        assert_eq!(app.search_results.len(), 1);
        assert_eq!(
            app.search_status,
            SearchStatus::Ready {
                query: "lo".to_string()
            }
        );
        assert!(!app.searching_api);
        assert_eq!(app.selected, 0);
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
        assert!(app.search_results.is_empty());
        assert_eq!(
            app.search_status,
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
        assert!(app.search_results.is_empty());
        assert_eq!(
            app.search_status,
            SearchStatus::Error {
                query: "lo".to_string(),
                message: "network down".to_string()
            }
        );
        assert!(!app.searching_api);
    }

    #[test]
    fn stale_search_response_is_ignored() {
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
        assert!(app.search_results.is_empty());
        assert_eq!(
            app.search_status,
            SearchStatus::Searching {
                query: "lofi".to_string()
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
        assert!(app.search_results.is_empty());
        assert_eq!(app.search_status, SearchStatus::WaitingForInput);
    }

    #[test]
    fn search_confirm_adds_result_exits_search_and_selects_playing() {
        let mut app = test_app();
        app.update(Action::EnterSearch);
        app.search_results = vec![station("Lo-Fi Radio", "http://lofi")];
        app.selected = 0;

        app.update(Action::SearchConfirm);

        assert_eq!(app.input_mode, InputMode::Normal);
        assert_eq!(app.playing_url.as_deref(), Some("http://lofi"));
        assert!(app.library.contains("http://lofi"));
        assert_eq!(app.search_status, SearchStatus::WaitingForInput);
        assert!(app.search_results.is_empty());
    }

    #[test]
    fn search_confirm_without_result_exits_search_without_playing() {
        let mut app = test_app();
        app.update(Action::EnterSearch);

        app.update(Action::SearchConfirm);

        assert_eq!(app.input_mode, InputMode::Normal);
        assert_eq!(app.playing_url, None);
        assert!(app.search_results.is_empty());
    }
}
