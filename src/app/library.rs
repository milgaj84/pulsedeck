use super::*;

const UNDO_HISTORY_LIMIT: usize = 10;

impl App {
    pub(super) fn remove_library_selection(&mut self) {
        if self.input_mode == InputMode::Normal {
            if let Some(station) = self.visible_stations().get(self.selected).copied().cloned() {
                let removed_index = self
                    .library
                    .stations
                    .iter()
                    .position(|saved| saved.url == station.url)
                    .unwrap_or(self.selected);
                let removed_genre = self
                    .library
                    .available_genres
                    .get(self.selected_genre_idx)
                    .cloned()
                    .unwrap_or_else(|| "All".to_string());
                let url = station.url.clone();
                match self.library.remove(&url) {
                    Ok(true) => {
                        self.remember_removed_station(station, removed_index, removed_genre);
                        self.set_info_notice("Station removed. Press u to undo");
                    }
                    Ok(false) => {}
                    Err(err) => {
                        self.remember_removed_station(station, removed_index, removed_genre);
                        self.set_error_notice(format!(
                            "Station removed in memory, but could not save library: {err}"
                        ));
                    }
                }
                let count = self.visible_count();
                if self.selected >= count && self.selected > 0 {
                    self.selected = count - 1;
                }
            }
        }
    }

    pub(super) fn undo_remove_library_selection(&mut self) {
        if self.input_mode != InputMode::Normal {
            return;
        }

        let Some((station, previous_index, previous_genre)) = self.undo_history.pop_back() else {
            self.set_info_notice("Nothing left to undo");
            return;
        };

        if self.library.contains(&station.url) {
            self.set_info_notice("Station already restored");
            return;
        }

        let station_name = station.name.clone();
        let insert_at = previous_index.min(self.library.stations.len());
        self.library.stations.insert(insert_at, station.clone());
        self.library.rebuild_genres();
        if let Some(genre_index) = self
            .library
            .available_genres
            .iter()
            .position(|genre| genre == &previous_genre)
        {
            self.selected_genre_idx = genre_index;
        }
        self.selected = self
            .visible_stations()
            .iter()
            .position(|visible| visible.url == station.url)
            .unwrap_or(0);
        match self.library.save() {
            Ok(()) => self.set_info_notice(format!("Restored station: {station_name}")),
            Err(err) => self.set_error_notice(format!(
                "Station restored in memory, but could not save library: {err}"
            )),
        }
    }

    fn remember_removed_station(&mut self, station: Station, index: usize, genre: String) {
        self.undo_history.push_back((station, index, genre));
        while self.undo_history.len() > UNDO_HISTORY_LIMIT {
            self.undo_history.pop_front();
        }
    }

    pub(super) fn next_genre(&mut self) {
        if self.input_mode == InputMode::Normal {
            let count = self.library.available_genres.len();
            if count > 0 {
                self.remember_current_genre_selection();
                self.selected_genre_idx = (self.selected_genre_idx + 1) % count;
                self.select_remembered_genre_station_or_default();
            }
        }
    }

    pub(super) fn prev_genre(&mut self) {
        if self.input_mode == InputMode::Normal {
            let count = self.library.available_genres.len();
            if count > 0 {
                self.remember_current_genre_selection();
                self.selected_genre_idx = if self.selected_genre_idx == 0 {
                    count - 1
                } else {
                    self.selected_genre_idx - 1
                };
                self.select_remembered_genre_station_or_default();
            }
        }
    }

    pub(super) fn remember_current_genre_selection(&mut self) {
        if self.input_mode != InputMode::Normal {
            return;
        }

        if let Some(genre) = self.current_genre_key() {
            self.genre_selection_memory.insert(genre, self.selected);
        }
    }

    fn select_remembered_genre_station_or_default(&mut self) {
        let count = self.visible_count();
        if count == 0 {
            self.selected = 0;
            return;
        }

        if let Some(remembered) = self
            .current_genre_key()
            .and_then(|genre| self.genre_selection_memory.get(&genre).copied())
        {
            self.selected = remembered.min(count - 1);
            return;
        }

        self.select_playing_station_or_first_visible();
    }

    fn current_genre_key(&self) -> Option<String> {
        self.library
            .available_genres
            .get(self.selected_genre_idx)
            .cloned()
    }

    fn select_playing_station_or_first_visible(&mut self) {
        let next_selected = self
            .playing_url
            .as_deref()
            .and_then(|playing_url| {
                self.visible_stations()
                    .iter()
                    .position(|station| station.url == playing_url)
            })
            .unwrap_or(0);

        self.selected = next_selected;
    }

    pub(super) fn save_library_or_notice(&mut self, context: &str) {
        if let Err(err) = self.library.save() {
            self.set_error_notice(format!("Could not save {context}: {err}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Action;
    use crate::favorites::Library;
    use crate::radio::Station;

    fn station(name: &str, url: &str, genre: &str) -> Station {
        Station {
            name: name.to_string(),
            url: url.to_string(),
            genre: genre.to_string(),
            country: "US".to_string(),
            bitrate: 128,
        }
    }

    fn notice_text(app: &App) -> Option<&str> {
        match app.notice.as_ref() {
            Some(AppNotice::Info(message)) | Some(AppNotice::Error(message)) => Some(message),
            None => None,
        }
    }

    fn genre_index(app: &App, genre: &str) -> usize {
        app.library
            .available_genres
            .iter()
            .position(|candidate| candidate == genre)
            .unwrap()
    }

    #[test]
    fn remove_library_selection_removes_selected_station() {
        let mut app = App::new(Library::in_memory(vec![
            station("A", "http://a", "Synthwave"),
            station("B", "http://b", "Synthwave"),
        ]));
        app.selected = 0;

        app.remove_library_selection();

        assert!(!app.library.contains("http://a"));
        assert!(app.library.contains("http://b"));
        assert_eq!(notice_text(&app), Some("Station removed. Press u to undo"));
    }

    #[test]
    fn undo_remove_library_selection_restores_last_removed_station() {
        let mut app = App::new(Library::in_memory(vec![
            station("A", "http://a", "Synthwave"),
            station("B", "http://b", "Synthwave"),
        ]));
        app.selected = 0;

        app.update(Action::RemoveLibrarySelection);
        app.update(Action::UndoRemoveLibrarySelection);

        assert_eq!(app.library.stations.len(), 2);
        assert_eq!(app.library.stations[0].name, "A");
        assert_eq!(app.library.stations[1].name, "B");
        assert_eq!(app.selected, 0);
        assert_eq!(notice_text(&app), Some("Restored station: A"));
    }

    #[test]
    fn undo_remove_restores_selection_inside_genre_filter() {
        let mut app = App::new(Library::in_memory(vec![
            station("Synth A", "http://synth-a", "Synthwave"),
            station("Ambient A", "http://ambient-a", "Ambient"),
            station("Synth B", "http://synth-b", "Synthwave"),
        ]));
        app.selected_genre_idx = app
            .library
            .available_genres
            .iter()
            .position(|genre| genre == "Synthwave")
            .unwrap();
        app.selected = 1;

        app.update(Action::RemoveLibrarySelection);
        app.update(Action::UndoRemoveLibrarySelection);

        assert_eq!(app.library.stations[2].name, "Synth B");
        assert_eq!(app.visible_stations()[app.selected].name, "Synth B");
    }

    #[test]
    fn now_playing_uses_undo_snapshot_after_playing_station_removal() {
        let mut app = App::new(Library::in_memory(vec![station(
            "A",
            "http://a",
            "Synthwave",
        )]));
        app.playing_url = Some("http://a".to_string());

        app.update(Action::RemoveLibrarySelection);

        assert_eq!(
            app.now_playing().map(|station| station.name.as_str()),
            Some("A")
        );
    }

    #[test]
    fn undo_without_removed_station_shows_notice() {
        let mut app = App::new(Library::in_memory(vec![station(
            "A",
            "http://a",
            "Synthwave",
        )]));

        app.update(Action::UndoRemoveLibrarySelection);

        assert!(app.library.contains("http://a"));
        assert_eq!(notice_text(&app), Some("Nothing left to undo"));
    }

    #[test]
    fn repeated_removals_restore_in_reverse_order() {
        let mut app = App::new(Library::in_memory(vec![
            station("A", "http://a", "Synthwave"),
            station("B", "http://b", "Synthwave"),
            station("C", "http://c", "Synthwave"),
        ]));

        app.selected = 0;
        app.update(Action::RemoveLibrarySelection);
        app.selected = 0;
        app.update(Action::RemoveLibrarySelection);

        assert!(!app.library.contains("http://a"));
        assert!(!app.library.contains("http://b"));
        assert_eq!(app.undo_history.len(), 2);

        app.update(Action::UndoRemoveLibrarySelection);
        assert!(app.library.contains("http://b"));
        assert!(!app.library.contains("http://a"));
        assert_eq!(notice_text(&app), Some("Restored station: B"));

        app.update(Action::UndoRemoveLibrarySelection);
        assert!(app.library.contains("http://a"));
        assert_eq!(notice_text(&app), Some("Restored station: A"));
        assert_eq!(
            app.library
                .stations
                .iter()
                .map(|station| station.name.as_str())
                .collect::<Vec<_>>(),
            vec!["A", "B", "C"]
        );
    }

    #[test]
    fn undo_history_keeps_only_ten_most_recent_removals() {
        let stations = (0..11)
            .map(|idx| {
                station(
                    &format!("Station {idx}"),
                    &format!("http://{idx}"),
                    "Synthwave",
                )
            })
            .collect::<Vec<_>>();
        let mut app = App::new(Library::in_memory(stations));

        for _ in 0..11 {
            app.selected = 0;
            app.update(Action::RemoveLibrarySelection);
        }

        assert_eq!(app.undo_history.len(), 10);

        for _ in 0..10 {
            app.update(Action::UndoRemoveLibrarySelection);
        }

        assert!(!app.library.contains("http://0"));
        assert!(app.library.contains("http://1"));
        assert!(app.library.contains("http://10"));
        assert_eq!(app.undo_history.len(), 0);
    }

    #[test]
    fn next_genre_selects_playing_station_when_visible() {
        let mut app = App::new(Library::in_memory(vec![
            station("Ambient A", "http://ambient-a", "Ambient"),
            station("Synth A", "http://synth-a", "Synthwave"),
            station("Ambient B", "http://ambient-b", "Ambient"),
        ]));
        app.selected_genre_idx = genre_index(&app, "All");
        app.selected = 1;
        app.playing_url = Some("http://ambient-b".to_string());

        app.update(Action::NextGenre);

        assert_eq!(
            app.library.available_genres[app.selected_genre_idx],
            "Ambient"
        );
        assert_eq!(app.visible_stations()[app.selected].url, "http://ambient-b");
    }

    #[test]
    fn prev_genre_selects_playing_station_when_visible() {
        let mut app = App::new(Library::in_memory(vec![
            station("Ambient A", "http://ambient-a", "Ambient"),
            station("Synth A", "http://synth-a", "Synthwave"),
            station("Ambient B", "http://ambient-b", "Ambient"),
        ]));
        app.selected_genre_idx = genre_index(&app, "Synthwave");
        app.selected = 0;
        app.playing_url = Some("http://ambient-b".to_string());

        app.update(Action::PrevGenre);

        assert_eq!(
            app.library.available_genres[app.selected_genre_idx],
            "Ambient"
        );
        assert_eq!(app.visible_stations()[app.selected].url, "http://ambient-b");
    }

    #[test]
    fn genre_switch_falls_back_to_first_station_when_playing_is_absent() {
        let mut app = App::new(Library::in_memory(vec![
            station("Ambient A", "http://ambient-a", "Ambient"),
            station("Synth A", "http://synth-a", "Synthwave"),
            station("Ambient B", "http://ambient-b", "Ambient"),
        ]));
        app.selected_genre_idx = genre_index(&app, "All");
        app.selected = 2;
        app.playing_url = Some("http://synth-a".to_string());

        app.update(Action::NextGenre);

        assert_eq!(
            app.library.available_genres[app.selected_genre_idx],
            "Ambient"
        );
        assert_eq!(app.selected, 0);
        assert_eq!(app.visible_stations()[app.selected].url, "http://ambient-a");
    }

    #[test]
    fn next_genre_restores_remembered_selection_when_returning() {
        let mut app = App::new(Library::in_memory(vec![
            station("Ambient A", "http://ambient-a", "Ambient"),
            station("Ambient B", "http://ambient-b", "Ambient"),
            station("Synth A", "http://synth-a", "Synthwave"),
            station("Synth B", "http://synth-b", "Synthwave"),
        ]));
        app.selected_genre_idx = genre_index(&app, "Ambient");
        app.selected = 1;

        app.update(Action::NextGenre);
        app.selected = 1;
        app.update(Action::PrevGenre);

        assert_eq!(
            app.library.available_genres[app.selected_genre_idx],
            "Ambient"
        );
        assert_eq!(app.visible_stations()[app.selected].url, "http://ambient-b");
    }

    #[test]
    fn remembered_genre_selection_clamps_after_station_removal() {
        let mut app = App::new(Library::in_memory(vec![
            station("Ambient A", "http://ambient-a", "Ambient"),
            station("Ambient B", "http://ambient-b", "Ambient"),
            station("Synth A", "http://synth-a", "Synthwave"),
        ]));
        app.selected_genre_idx = genre_index(&app, "Ambient");
        app.selected = 1;
        app.remember_current_genre_selection();
        app.library.remove("http://ambient-b").unwrap();
        app.library.rebuild_genres();

        app.update(Action::NextGenre);
        app.update(Action::PrevGenre);

        assert_eq!(
            app.library.available_genres[app.selected_genre_idx],
            "Ambient"
        );
        assert_eq!(app.selected, 0);
    }
}
