use super::*;

impl App {
    pub(super) fn remove_library_selection(&mut self) {
        if self.input_mode == InputMode::Normal {
            if let Some(station) = self.visible_stations().get(self.selected) {
                let url = station.url.clone();
                match self.library.remove(&url) {
                    Ok(true) => self.set_info_notice("Station removed"),
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
