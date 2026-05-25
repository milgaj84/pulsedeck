use super::*;
use crate::action::Action;
use crate::ui::theme::ThemeName;

impl App {
    pub(super) fn handle_settings_action(&mut self, action: Action) {
        match action {
            Action::NextStation => {
                self.selected_setting_idx = (self.selected_setting_idx + 1) % 6;
            }
            Action::PrevStation => {
                self.selected_setting_idx = if self.selected_setting_idx == 0 {
                    5
                } else {
                    self.selected_setting_idx - 1
                };
            }
            Action::PlaySelected | Action::TogglePause => {
                self.apply_selected_setting();
                self.save_library_or_notice("settings");
            }
            Action::ToggleSettings => {
                self.show_settings = false;
            }
            Action::Quit => {
                self.show_settings = false;
            }
            Action::Tick => {
                self.tick_count += 1;
                self.tick_notice();
                self.poll_audio_status();
                self.update_visualizer();
            }
            _ => {
                // Block all other actions while settings are open.
            }
        }
    }

    pub(super) fn apply_selected_setting(&mut self) {
        match self.selected_setting_idx {
            0 => {
                self.library.settings.notifications_enabled =
                    !self.library.settings.notifications_enabled;
            }
            1 => {
                self.library.settings.autoplay_last = !self.library.settings.autoplay_last;
            }
            2 => {
                self.library.settings.recording_dir =
                    match self.library.settings.recording_dir.as_str() {
                        "./recordings" => "./music".to_string(),
                        "./music" => "./driftfm-captures".to_string(),
                        _ => "./recordings".to_string(),
                    };
            }
            3 => {
                self.library.settings.keep_snippets = !self.library.settings.keep_snippets;
            }
            4 => {
                // Cycle min duration: 30 -> 60 -> 90 -> 120 -> 180
                self.library.settings.min_song_duration_secs =
                    match self.library.settings.min_song_duration_secs {
                        30 => 60,
                        60 => 90,
                        90 => 120,
                        120 => 180,
                        _ => 30,
                    };
            }
            5 => {
                let current = ThemeName::from_key(&self.library.settings.theme);
                let next = current.next();
                self.library.settings.theme = next.key().to_string();
                crate::ui::theme::set_active(next);
            }
            _ => {}
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

    fn test_app() -> App {
        App::new(Library::in_memory(vec![station("A", "http://a")]))
    }

    #[test]
    fn settings_blocks_play_selected() {
        let mut app = test_app();
        app.show_settings = true;
        app.selected_setting_idx = 0;
        let before = app.library.settings.notifications_enabled;

        app.update(Action::PlaySelected);

        assert_eq!(app.playing_url, None);
        assert_eq!(app.library.settings.notifications_enabled, !before);
    }

    #[test]
    fn settings_blocks_search_entry() {
        let mut app = test_app();
        app.show_settings = true;

        app.update(Action::EnterSearch);

        assert_eq!(app.input_mode, InputMode::Normal);
        assert!(app.show_settings);
    }

    #[test]
    fn settings_quit_closes_settings_without_quitting_app() {
        let mut app = test_app();
        app.show_settings = true;

        app.update(Action::Quit);

        assert!(!app.show_settings);
        assert!(!app.should_quit);
    }

    #[test]
    fn settings_tick_still_polls_and_updates() {
        let mut app = test_app();
        app.show_settings = true;
        app.set_info_notice("hello");

        app.update(Action::Tick);

        assert_eq!(app.tick_count, 1);
        assert!(app.notice.is_some());
    }
}
