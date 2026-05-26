use super::*;
use crate::favorites::resolve_parent_genre;

impl App {
    /// The currently visible list. In Normal mode: library. In Search mode: search results.
    pub fn visible_stations(&self) -> Vec<&Station> {
        match self.input_mode {
            InputMode::Normal => {
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
            InputMode::Search => self.search_results.iter().collect(),
        }
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
                    self.undo_removed_station.as_ref().and_then(|(station, _, _)| {
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
        match self.input_mode {
            InputMode::Normal => {
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
            InputMode::Search => self.search_results.len(),
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
