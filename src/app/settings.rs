use super::*;
use crate::action::Action;
use crate::ui::theme::ThemeName;

impl App {
    pub(super) fn handle_settings_action(&mut self, action: Action) {
        match action {
            Action::NextStation => {
                self.selected_setting_idx = (self.selected_setting_idx + 1) % SettingRow::COUNT;
            }
            Action::PrevStation => {
                self.selected_setting_idx = if self.selected_setting_idx == 0 {
                    SettingRow::COUNT - 1
                } else {
                    self.selected_setting_idx - 1
                };
            }
            Action::PlaySelected | Action::TogglePause => {
                if self.apply_selected_setting() {
                    self.save_library_or_notice("settings");
                }
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

    pub(super) fn selected_setting_row(&self) -> Option<SettingRow> {
        SettingRow::from_index(self.selected_setting_idx)
    }

    pub(super) fn apply_selected_setting(&mut self) -> bool {
        match self.selected_setting_row() {
            Some(SettingRow::Notifications) => {
                self.library.settings.notifications_enabled =
                    !self.library.settings.notifications_enabled;
                true
            }
            Some(SettingRow::AutoplayLast) => {
                self.library.settings.autoplay_last = !self.library.settings.autoplay_last;
                true
            }
            Some(SettingRow::RecordingDir) => {
                self.library.settings.recording_dir =
                    match self.library.settings.recording_dir.as_str() {
                        "./recordings" => "./music".to_string(),
                        "./music" => "./driftfm-captures".to_string(),
                        _ => "./recordings".to_string(),
                    };
                true
            }
            Some(SettingRow::KeepSnippets) => {
                self.library.settings.keep_snippets = !self.library.settings.keep_snippets;
                true
            }
            Some(SettingRow::MinSongDuration) => {
                if self.library.settings.keep_snippets {
                    self.set_info_notice("Min duration is disabled while keeping all snippets");
                    return false;
                }

                // Cycle min duration: 30 -> 60 -> 90 -> 120 -> 180
                self.library.settings.min_song_duration_secs =
                    match self.library.settings.min_song_duration_secs {
                        30 => 60,
                        60 => 90,
                        90 => 120,
                        120 => 180,
                        _ => 30,
                    };
                true
            }
            Some(SettingRow::Theme) => {
                let current = ThemeName::from_key(&self.library.settings.theme);
                let next = current.next();
                self.library.settings.theme = next.key().to_string();
                crate::ui::theme::set_active(next);
                true
            }
            None => false,
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

    fn notice_text(app: &App) -> Option<&str> {
        match app.notice.as_ref() {
            Some(AppNotice::Info(message)) | Some(AppNotice::Error(message)) => Some(message),
            None => None,
        }
    }

    #[test]
    fn settings_blocks_play_selected() {
        let mut app = test_app();
        app.show_settings = true;
        app.selected_setting_idx = SettingRow::Notifications.index();
        let before = app.library.settings.notifications_enabled;

        app.update(Action::PlaySelected);

        assert_eq!(app.playing_url, None);
        assert_eq!(app.library.settings.notifications_enabled, !before);
    }

    #[test]
    fn settings_navigation_wraps_using_row_count() {
        let mut app = test_app();
        app.show_settings = true;
        app.selected_setting_idx = SettingRow::COUNT - 1;

        app.update(Action::NextStation);
        assert_eq!(app.selected_setting_idx, 0);

        app.update(Action::PrevStation);
        assert_eq!(app.selected_setting_idx, SettingRow::COUNT - 1);
    }

    #[test]
    fn each_setting_row_maps_from_its_index() {
        for row in SettingRow::ALL {
            assert_eq!(SettingRow::from_index(row.index()), Some(row));
        }
        assert_eq!(SettingRow::from_index(SettingRow::COUNT), None);
    }

    #[test]
    fn disabled_min_duration_row_does_not_cycle() {
        let mut app = test_app();
        app.show_settings = true;
        app.selected_setting_idx = SettingRow::MinSongDuration.index();
        app.library.settings.keep_snippets = true;
        app.library.settings.min_song_duration_secs = 90;

        app.update(Action::TogglePause);

        assert_eq!(app.library.settings.min_song_duration_secs, 90);
        assert_eq!(
            notice_text(&app),
            Some("Min duration is disabled while keeping all snippets")
        );
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
