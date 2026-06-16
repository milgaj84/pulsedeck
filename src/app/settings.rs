use super::*;
use crate::action::Action;
use crate::ui::theme::ThemeName;

impl App {
    pub(super) fn handle_settings_action(&mut self, action: Action) {
        match action {
            Action::NextStation => {
                self.overlays.selected_setting_idx =
                    (self.overlays.selected_setting_idx + 1) % SettingRow::COUNT;
            }
            Action::PrevStation => {
                self.overlays.selected_setting_idx = if self.overlays.selected_setting_idx == 0 {
                    SettingRow::COUNT - 1
                } else {
                    self.overlays.selected_setting_idx - 1
                };
            }
            Action::PlaySelected | Action::TogglePause if self.apply_selected_setting(true) => {
                self.mark_library_dirty();
            }
            Action::StepSettingForward if self.apply_directional_setting(true) => {
                self.mark_library_dirty();
            }
            Action::StepSettingBackward | Action::ToggleHelp
                if self.apply_directional_setting(false) =>
            {
                self.mark_library_dirty();
            }
            Action::PlaySelected
            | Action::TogglePause
            | Action::StepSettingForward
            | Action::StepSettingBackward
            | Action::ToggleHelp => {}
            Action::ToggleSettings => {
                self.overlays.active = ActiveOverlay::None;
            }
            Action::Quit => {
                self.overlays.active = ActiveOverlay::None;
            }
            Action::Tick => self.tick(),
            _ => {
                // Block all other actions while settings are open.
            }
        }
    }

    pub(super) fn selected_setting_row(&self) -> Option<SettingRow> {
        SettingRow::from_index(self.overlays.selected_setting_idx)
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
            Some(SettingRow::OutputDevice) => {
                self.library.settings.output_device_name = step_output_device_preference(
                    self.library.settings.output_device_name.as_deref(),
                    &available_output_device_choices(),
                    forward,
                );
                self.sync_output_device();
                self.set_info_notice(format!(
                    "Audio output: {}",
                    output_device_display_name(self.library.settings.output_device_name.as_deref())
                ));
                true
            }
            Some(SettingRow::Theme) => {
                let current = ThemeName::from_key(&self.library.settings.theme);
                let next = step_choice(ThemeName::ALL, current, forward);
                self.library.settings.theme = next.key().to_string();
                crate::ui::theme::set_active(next);
                true
            }
            Some(SettingRow::SaveHistory) => {
                self.library.settings.save_history = !self.library.settings.save_history;
                true
            }
            None => false,
        }
    }

    pub(super) fn sync_output_device(&self) {
        self.audio.send(crate::audio::AudioCommand::SetOutputDevice(
            self.library.settings.output_device_name.clone(),
        ));
    }

    fn apply_directional_setting(&mut self, forward: bool) -> bool {
        self.apply_selected_setting(forward)
    }
}

fn available_output_device_choices() -> Vec<String> {
    let mut choices = vec![crate::audio::DEFAULT_OUTPUT_DEVICE_LABEL.to_string()];
    choices.extend(crate::audio::list_output_device_names());
    choices
}

fn output_device_display_name(value: Option<&str>) -> String {
    crate::audio::output_device_display_name(value)
}

fn step_output_device_preference(
    current: Option<&str>,
    choices: &[String],
    forward: bool,
) -> Option<String> {
    if choices.is_empty() {
        return None;
    }

    let current_label = output_device_display_name(current);
    let current_index = choices
        .iter()
        .position(|choice| choice.eq_ignore_ascii_case(&current_label));

    let next_index = match (current_index, forward) {
        (Some(index), true) => (index + 1) % choices.len(),
        (Some(0), false) => choices.len() - 1,
        (Some(index), false) => index - 1,
        (None, true) => 0,
        (None, false) => choices.len() - 1,
    };

    let next = choices[next_index].trim();
    if next.eq_ignore_ascii_case(crate::audio::DEFAULT_OUTPUT_DEVICE_LABEL) {
        None
    } else {
        Some(next.to_string())
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

    #[test]
    fn settings_blocks_play_selected() {
        let mut app = test_app();
        app.overlays.active = ActiveOverlay::Settings;
        app.overlays.selected_setting_idx = SettingRow::Notifications.index();
        let before = app.library.settings.notifications_enabled;

        app.update(Action::PlaySelected);

        assert_eq!(app.player.playing_url, None);
        assert_eq!(app.library.settings.notifications_enabled, !before);
    }

    #[test]
    fn settings_navigation_wraps_using_row_count() {
        let mut app = test_app();
        app.overlays.active = ActiveOverlay::Settings;
        app.overlays.selected_setting_idx = SettingRow::COUNT - 1;

        app.update(Action::NextStation);
        assert_eq!(app.overlays.selected_setting_idx, 0);

        app.update(Action::PrevStation);
        assert_eq!(app.overlays.selected_setting_idx, SettingRow::COUNT - 1);
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
        app.overlays.active = ActiveOverlay::Settings;
        app.overlays.selected_setting_idx = SettingRow::Theme.index();
        app.library.settings.theme = "Retrowave".to_string();

        app.update(Action::StepSettingForward);
        assert_eq!(app.library.settings.theme, "CatppuccinMocha");

        app.update(Action::StepSettingBackward);
        assert_eq!(app.library.settings.theme, "Retrowave");
    }

    #[test]
    fn settings_backward_wraps_theme() {
        let mut app = test_app();
        app.overlays.active = ActiveOverlay::Settings;
        app.overlays.selected_setting_idx = SettingRow::Theme.index();
        app.library.settings.theme = "Retrowave".to_string();

        app.update(Action::StepSettingBackward);

        assert_eq!(app.library.settings.theme, "Terminal");
    }

    #[test]
    fn output_device_preference_cycles_default_and_devices() {
        let choices = vec![
            crate::audio::DEFAULT_OUTPUT_DEVICE_LABEL.to_string(),
            "Built-in Speakers".to_string(),
            "BlueZ Headphones".to_string(),
        ];

        assert_eq!(
            step_output_device_preference(None, &choices, true).as_deref(),
            Some("Built-in Speakers")
        );
        assert_eq!(
            step_output_device_preference(Some("Built-in Speakers"), &choices, true).as_deref(),
            Some("BlueZ Headphones")
        );
        assert_eq!(
            step_output_device_preference(Some("BlueZ Headphones"), &choices, true),
            None
        );
        assert_eq!(
            step_output_device_preference(None, &choices, false).as_deref(),
            Some("BlueZ Headphones")
        );
    }

    #[test]
    fn output_device_preference_handles_missing_saved_device() {
        let choices = vec![
            crate::audio::DEFAULT_OUTPUT_DEVICE_LABEL.to_string(),
            "Built-in Speakers".to_string(),
        ];

        assert_eq!(
            step_output_device_preference(Some("Missing Bluetooth"), &choices, true),
            None
        );
        assert_eq!(
            step_output_device_preference(Some("Missing Bluetooth"), &choices, false).as_deref(),
            Some("Built-in Speakers")
        );
    }

    #[test]
    fn output_device_display_name_uses_default_label_for_none() {
        assert_eq!(
            output_device_display_name(None),
            crate::audio::DEFAULT_OUTPUT_DEVICE_LABEL
        );
        assert_eq!(
            output_device_display_name(Some("BlueZ Headphones")),
            "BlueZ Headphones"
        );
    }

    #[test]
    fn settings_h_action_steps_backward_without_closing_popup() {
        let mut app = test_app();
        app.overlays.active = ActiveOverlay::Settings;
        app.overlays.selected_setting_idx = SettingRow::Theme.index();
        app.library.settings.theme = "CatppuccinMocha".to_string();

        app.update(Action::ToggleHelp);

        assert!(app.show_settings());
        assert_eq!(app.library.settings.theme, "Retrowave");
    }

    #[test]
    fn settings_blocks_search_entry() {
        let mut app = test_app();
        app.overlays.active = ActiveOverlay::Settings;

        app.update(Action::EnterSearch);

        assert_eq!(app.input_mode, InputMode::Normal);
        assert!(app.show_settings());
    }

    #[test]
    fn settings_quit_closes_settings_without_quitting_app() {
        let mut app = test_app();
        app.overlays.active = ActiveOverlay::Settings;

        app.update(Action::Quit);

        assert!(!app.show_settings());
        assert!(!app.should_quit);
    }

    #[test]
    fn settings_tick_still_polls_and_updates() {
        let mut app = test_app();
        app.overlays.active = ActiveOverlay::Settings;
        app.set_info_notice("hello");

        app.update(Action::Tick);

        assert_eq!(app.tick_count, 1);
        assert!(app.notice.current.is_some());
    }

    #[test]
    fn settings_toggle_save_history() {
        let mut app = test_app();
        app.overlays.active = ActiveOverlay::Settings;
        app.overlays.selected_setting_idx = SettingRow::SaveHistory.index();
        assert!(!app.library.settings.save_history);

        app.update(Action::PlaySelected);
        assert!(app.library.settings.save_history);

        app.update(Action::PlaySelected);
        assert!(!app.library.settings.save_history);
    }
}
