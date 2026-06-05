use super::*;
use crate::favorites::resolve_parent_genre;

impl App {
    /// The currently visible list. In Normal mode: library. In Search mode: search results.
    pub fn visible_stations(&self) -> Vec<&Station> {
        if self.input_mode == InputMode::Search {
            return self.search_results.iter().collect();
        }

        if let Some(genre) = self.library.available_genres.get(self.selected_genre_idx) {
            if genre == "All" {
                self.library.stations.iter().collect()
            } else {
                self.library
                    .stations
                    .iter()
                    .filter(|s| resolve_parent_genre(&s.genre).eq_ignore_ascii_case(genre))
                    .collect()
            }
        } else {
            self.library.stations.iter().collect()
        }
    }

    /// Get the highlighted station from the currently visible list.
    pub fn selected_station(&self) -> Option<&Station> {
        self.visible_stations().get(self.selected).copied()
    }

    /// Get the currently playing station, if any.
    pub fn now_playing(&self) -> Option<&Station> {
        self.playing_url.as_ref().and_then(|url| {
            self.library
                .stations
                .iter()
                .find(|s| s.url == *url)
                .or_else(|| self.search_results.iter().find(|s| s.url == *url))
                .or_else(|| {
                    self.undo_history.iter().rev().find_map(|(station, _, _)| {
                        if station.url == *url {
                            Some(station)
                        } else {
                            None
                        }
                    })
                })
        })
    }

    /// Count visible stations without allocating a Vec.
    pub fn visible_count(&self) -> usize {
        if self.input_mode == InputMode::Search {
            return self.search_results.len();
        }

        if let Some(genre) = self.library.available_genres.get(self.selected_genre_idx) {
            if genre == "All" {
                self.library.stations.len()
            } else {
                self.library
                    .stations
                    .iter()
                    .filter(|s| resolve_parent_genre(&s.genre).eq_ignore_ascii_case(genre))
                    .count()
            }
        } else {
            self.library.stations.len()
        }
    }

    /// Try to select the currently playing station in the visible list.
    pub(super) fn select_playing(&mut self) {
        if let Some(ref url) = self.playing_url {
            if let Some(pos) = self.visible_stations().iter().position(|s| s.url == *url) {
                self.selected = pos;
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
        Station {
            name: name.to_string(),
            url: url.to_string(),
            genre: "Synthwave".to_string(),
            country: "US".to_string(),
            bitrate: 128,
        }
    }

    #[test]
    fn selected_station_returns_highlighted_visible_station() {
        let mut app = App::new(Library::in_memory(vec![
            station("A", "http://a"),
            station("B", "http://b"),
        ]));
        app.selected = 1;

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
}
