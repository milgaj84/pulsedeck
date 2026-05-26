use super::*;

impl App {
    pub(super) fn remove_library_selection(&mut self) {
        if self.input_mode == InputMode::Normal {
            if let Some(station) = self.visible_stations().get(self.selected).cloned() {
                let removed_index = self.selected;
                let url = station.url.clone();
                match self.library.remove(&url) {
                    Ok(true) => {
                        self.undo_removed_station = Some((station, removed_index));
                        self.set_info_notice("Station removed. Press u to undo");
                    }
                    Ok(false) => {}
                    Err(err) => self.set_error_notice(format!(
                        "Station removed in memory, but could not save library: {err}"
                    )),
                }
                // Clamp selection.
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

        let Some((station, previous_index)) = self.undo_removed_station.take() else {
            self.set_info_notice("Nothing to undo");
            return;
        };

        if self.library.contains(&station.url) {
            self.set_info_notice("Station already restored");
            return;
        }

        let insert_at = previous_index.min(self.library.stations.len());
        self.library.stations.insert(insert_at, station.clone());
        self.library.rebuild_genres();
        self.selected = insert_at;
        match self.library.save() {
            Ok(()) => self.set_info_notice("Station restored"),
            Err(err) => self.set_error_notice(format!(
                "Station restored in memory, but could not save library: {err}"
            )),
        }
    }

    pub(super) fn next_genre(&mut self) {
        if self.input_mode == InputMode::Normal {
            let count = self.library.available_genres.len();
            if count > 0 {
                self.selected_genre_idx = (self.selected_genre_idx + 1) % count;
                self.selected = 0;
            }
        }
    }

    pub(super) fn prev_genre(&mut self) {
        if self.input_mode == InputMode::Normal {
            let count = self.library.available_genres.len();
            if count > 0 {
                self.selected_genre_idx = if self.selected_genre_idx == 0 {
                    count - 1
                } else {
                    self.selected_genre_idx - 1
                };
                self.selected = 0;
            }
        }
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
        assert_eq!(notice_text(&app), Some("Station restored"));
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
        assert_eq!(notice_text(&app), Some("Nothing to undo"));
    }

    #[test]
    fn new_removal_replaces_previous_undo_slot() {
        let mut app = App::new(Library::in_memory(vec![
            station("A", "http://a", "Synthwave"),
            station("B", "http://b", "Synthwave"),
            station("C", "http://c", "Synthwave"),
        ]));

        app.selected = 0;
        app.update(Action::RemoveLibrarySelection);
        app.selected = 0;
        app.update(Action::RemoveLibrarySelection);
        app.update(Action::UndoRemoveLibrarySelection);

        assert!(!app.library.contains("http://a"));
        assert!(app.library.contains("http://b"));
        assert!(app.library.contains("http://c"));
    }

    #[test]
    fn next_genre_resets_selection() {
        let mut app = App::new(Library::in_memory(vec![
            station("A", "http://a", "Synthwave"),
            station("B", "http://b", "Ambient"),
        ]));
        app.selected = 1;

        app.next_genre();

        assert_eq!(app.selected, 0);
    }
}
