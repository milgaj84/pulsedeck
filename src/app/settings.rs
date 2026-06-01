use super::*;
use crate::action::Action;
use crate::ui::theme::ThemeName;

const RECORDING_DIR_CHOICES: [&str; 3] = ["./recordings", "./music", "./driftfm-captures"];
const MIN_SONG_DURATION_CHOICES: [u32; 5] = [30, 60, 90, 120, 180];

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
            Action::PlaySelected | Action::TogglePause | Action::StepSettingForward
                if self.apply_selected_setting(true) =>
            {
                self.save_library_or_notice("settings");
            }
            Action::StepSettingBackward | Action::ToggleHelp
                if self.apply_selected_setting(false) =>
            {
                self.save_library_or_notice("settings");
            }
            Action::PlaySelected
            | Action::TogglePause
            | Action::StepSettingForward
            | Action::StepSettingBackward
            | Action::ToggleHelp => {}
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

    pub(super) fn apply_selected_setting(&mut self, forward: bool) -> bool {
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
                self.library.settings.recording_dir = step_choice(
                    &RECORDING_DIR_CHOICES,
                    self.library.settings.recording_dir.as_str(),
                    forward,
                )
                .to_string();
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

                self.library.settings.min_song_duration_secs = step_choice(
                    &MIN_SONG_DURATION_CHOICES,
                    self.library.settings.min_song_duration_secs,
                    forward,
                );
                true
            }
            Some(SettingRow::Theme) => {
                let current = ThemeName::from_key(&self.library.settings.theme);
                let next = step_choice(ThemeName::ALL, current, forward);
                self.library.settings.theme = next.key().to_string();
                crate::ui::theme::set_active(next);
                true
            }
            None => false,
        }
    }
}

fn step_choice<T: Copy + PartialEq>(choices: &[T], current: T, forward: bool) -> T {
    if choices.is_empty() {
        return current;
    }

    let Some(index) = choices.iter().position(|choice| *choice == current) else {
        return if forward {
            choices[0]
        } else {
            choices[choices.len() - 1]
        };
    };

    if forward {
        choices[(index + 1) % choices.len()]
    } else if index == 0 {
        choices[choices.len() - 1]
    } else {
        choices[index - 1]
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
    fn settings_forward_and_backward_cycle_theme() {
        let mut app = test_app();
        app.show_settings = true;
        app.selected_setting_idx = SettingRow::Theme.index();
        app.library.settings.theme = "Retrowave".to_string();

        app.update(Action::StepSettingForward);
        assert_eq!(app.library.settings.theme, "CatppuccinMocha");

        app.update(Action::StepSettingBackward);
        assert_eq!(app.library.settings.theme, "Retrowave");
    }

    #[test]
    fn settings_backward_wraps_theme() {
        let mut app = test_app();
        app.show_settings = true;
        app.selected_setting_idx = SettingRow::Theme.index();
        app.library.settings.theme = "Retrowave".to_string();

        app.update(Action::StepSettingBackward);

        assert_eq!(app.library.settings.theme, "CatppuccinLatte");
    }

    #[test]
    fn settings_backward_wraps_min_song_duration() {
        let mut app = test_app();
        app.show_settings = true;
        app.selected_setting_idx = SettingRow::MinSongDuration.index();
        app.library.settings.keep_snippets = false;
        app.library.settings.min_song_duration_secs = 30;

        app.update(Action::StepSettingBackward);

        assert_eq!(app.library.settings.min_song_duration_secs, 180);
    }

    #[test]
    fn settings_backward_wraps_recording_dir() {
        let mut app = test_app();
        app.show_settings = true;
        app.selected_setting_idx = SettingRow::RecordingDir.index();
        app.library.settings.recording_dir = "./recordings".to_string();

        app.update(Action::StepSettingBackward);

        assert_eq!(app.library.settings.recording_dir, "./driftfm-captures");
    }

    #[test]
    fn settings_h_action_steps_backward_without_closing_popup() {
        let mut app = test_app();
        app.show_settings = true;
        app.selected_setting_idx = SettingRow::Theme.index();
        app.library.settings.theme = "CatppuccinMocha".to_string();

        app.update(Action::ToggleHelp);

        assert!(app.show_settings);
        assert_eq!(app.library.settings.theme, "Retrowave");
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
