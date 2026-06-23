use super::*;
use crate::action::Action;
use crate::theme_name::ThemeName;

impl App {
    pub(super) fn handle_settings_action(&mut self, action: Action) {
        match action {
            Action::NextStation => {
                self.ui.overlays.selected_setting_idx =
                    (self.ui.overlays.selected_setting_idx + 1) % SettingRow::COUNT;
            }
            Action::PrevStation => {
                self.ui.overlays.selected_setting_idx =
                    if self.ui.overlays.selected_setting_idx == 0 {
                        SettingRow::COUNT - 1
                    } else {
                        self.ui.overlays.selected_setting_idx - 1
                    };
            }
            Action::PlaySelected | Action::TogglePause if self.apply_selected_setting(true) => {
                self.mark_library_dirty();
            }
            Action::StepSettingForward if self.apply_selected_setting(true) => {
                self.mark_library_dirty();
            }
            Action::StepSettingBackward | Action::ToggleHelp
                if self.apply_selected_setting(false) =>
            {
                self.mark_library_dirty();
            }
            Action::PlaySelected
            | Action::TogglePause
            | Action::StepSettingForward
            | Action::StepSettingBackward
            | Action::ToggleHelp => {}
            Action::ToggleSettings => {
                self.ui.overlays.active = ActiveOverlay::None;
            }
            Action::Quit => {
                self.ui.overlays.active = ActiveOverlay::None;
            }
            Action::Tick => self.tick(),
            _ => {
                // Block all other actions while settings are open.
            }
        }
    }

    pub(super) fn selected_setting_row(&self) -> Option<SettingRow> {
        SettingRow::from_index(self.ui.overlays.selected_setting_idx)
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
                self.playback.diagnostics.output_device =
                    output_device_display_name(self.library.settings.output_device_name.as_deref());
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
            Some(SettingRow::StreamMetadata) => {
                self.library.settings.stream_metadata_enabled =
                    !self.library.settings.stream_metadata_enabled;
                self.playback.diagnostics.metadata_enabled =
                    self.library.settings.stream_metadata_enabled;
                self.sync_stream_metadata();
                self.set_info_notice(format!(
                    "Song info metadata: {}",
                    if self.library.settings.stream_metadata_enabled {
                        "on"
                    } else {
                        "off"
                    }
                ));
                true
            }
            Some(SettingRow::SaveHistory) => {
                self.library.settings.save_history = !self.library.settings.save_history;
                true
            }
            None => false,
        }
    }

    pub(super) fn cycle_theme_setting(&mut self) {
        self.ui.overlays.selected_setting_idx = SettingRow::Theme.index();
        if self.apply_selected_setting(true) {
            self.mark_library_dirty();
            self.set_info_notice(format!("Theme: {}", self.library.settings.theme));
        }
    }

    pub(super) fn toggle_stream_metadata_setting(&mut self) {
        self.ui.overlays.selected_setting_idx = SettingRow::StreamMetadata.index();
        if self.apply_selected_setting(true) {
            self.mark_library_dirty();
        }
    }

    pub(super) fn sync_output_device(&self) -> bool {
        self.playback
            .audio
            .send(crate::audio::AudioCommand::SetOutputDevice(
                self.library.settings.output_device_name.clone(),
            ))
    }

    pub(super) fn sync_stream_metadata(&self) -> bool {
        self.playback
            .audio
            .send(crate::audio::AudioCommand::SetStreamMetadata(
                self.library.settings.stream_metadata_enabled,
            ))
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

fn step_choice(choices: &[ThemeName], current: ThemeName, forward: bool) -> ThemeName {
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
        Station::basic(name, url, "Synthwave", "US", 128)
    }

    fn test_app() -> App {
        App::new(Library::in_memory(vec![station("A", "http://a")]))
    }

    #[test]
    fn settings_blocks_play_selected() {
        let mut app = test_app();
        app.ui.overlays.active = ActiveOverlay::Settings;
        app.ui.overlays.selected_setting_idx = SettingRow::Notifications.index();
        let before = app.library.settings.notifications_enabled;

        app.update(Action::PlaySelected);

        assert_eq!(app.playback.view.playing_url, None);
        assert_eq!(app.library.settings.notifications_enabled, !before);
    }

    #[test]
    fn settings_navigation_wraps_using_row_count() {
        let mut app = test_app();
        app.ui.overlays.active = ActiveOverlay::Settings;
        app.ui.overlays.selected_setting_idx = SettingRow::COUNT - 1;

        app.update(Action::NextStation);
        assert_eq!(app.ui.overlays.selected_setting_idx, 0);

        app.update(Action::PrevStation);
        assert_eq!(app.ui.overlays.selected_setting_idx, SettingRow::COUNT - 1);
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
        app.ui.overlays.active = ActiveOverlay::Settings;
        app.ui.overlays.selected_setting_idx = SettingRow::Theme.index();
        app.library.settings.theme = "Retrowave".to_string();

        app.update(Action::StepSettingForward);
        assert_eq!(app.library.settings.theme, "CatppuccinMocha");

        app.update(Action::StepSettingBackward);
        assert_eq!(app.library.settings.theme, "Retrowave");
    }

    #[test]
    fn settings_backward_wraps_theme() {
        let mut app = test_app();
        app.ui.overlays.active = ActiveOverlay::Settings;
        app.ui.overlays.selected_setting_idx = SettingRow::Theme.index();
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
    fn settings_toggle_stream_metadata_updates_setting() {
        let mut app = test_app();
        app.ui.overlays.active = ActiveOverlay::Settings;
        app.ui.overlays.selected_setting_idx = SettingRow::StreamMetadata.index();
        app.library.settings.stream_metadata_enabled = true;

        app.update(Action::TogglePause);

        assert!(!app.library.settings.stream_metadata_enabled);
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
        app.ui.overlays.active = ActiveOverlay::Settings;
        app.ui.overlays.selected_setting_idx = SettingRow::Theme.index();
        app.library.settings.theme = "CatppuccinMocha".to_string();

        app.update(Action::ToggleHelp);

        assert!(app.show_settings());
        assert_eq!(app.library.settings.theme, "Retrowave");
    }

    #[test]
    fn settings_blocks_search_entry() {
        let mut app = test_app();
        app.ui.overlays.active = ActiveOverlay::Settings;

        app.update(Action::EnterSearch);

        assert_eq!(app.ui.input_mode, InputMode::Normal);
        assert!(app.show_settings());
    }

    #[test]
    fn settings_quit_closes_settings_without_quitting_app() {
        let mut app = test_app();
        app.ui.overlays.active = ActiveOverlay::Settings;

        app.update(Action::Quit);

        assert!(!app.show_settings());
        assert!(!app.ui.should_quit);
    }

    #[test]
    fn settings_tick_still_polls_and_updates() {
        let mut app = test_app();
        app.ui.overlays.active = ActiveOverlay::Settings;
        app.set_info_notice("hello");

        app.update(Action::Tick);

        assert_eq!(app.ui.tick_count, 1);
        assert!(app.ui.notice.current.is_some());
    }

    #[test]
    fn settings_toggle_save_history() {
        let mut app = test_app();
        app.ui.overlays.active = ActiveOverlay::Settings;
        app.ui.overlays.selected_setting_idx = SettingRow::SaveHistory.index();
        assert!(!app.library.settings.save_history);

        app.update(Action::PlaySelected);
        assert!(app.library.settings.save_history);

        app.update(Action::PlaySelected);
        assert!(!app.library.settings.save_history);
    }
}
