use super::*;
use crate::action::Action;
use crate::app::settings_undo::SettingSnapshot;
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
            Action::PlaySelected | Action::TogglePause if self.capture_and_apply(true) => {
                self.persist_config_change();
            }
            Action::StepSettingForward if self.capture_and_apply(true) => {
                self.persist_config_change();
            }
            Action::StepSettingBackward | Action::ToggleHelp if self.capture_and_apply(false) => {
                self.persist_config_change();
            }
            Action::PlaySelected
            | Action::TogglePause
            | Action::StepSettingForward
            | Action::StepSettingBackward
            | Action::ToggleHelp => {}
            Action::UndoSetting | Action::UndoRemoveLibrarySelection => {
                self.undo_selected_setting();
            }
            Action::ToggleSettings => {
                self.settings_undo.clear();
                self.ui.overlays.active = ActiveOverlay::None;
            }
            Action::Quit => {
                self.settings_undo.clear();
                self.ui.overlays.active = ActiveOverlay::None;
            }
            Action::Tick => self.tick(),
            _ => {
                // Block all other actions while settings are open.
            }
        }
    }

    /// Capture the current value before applying a setting change.
    /// Returns true if the setting was successfully applied.
    fn capture_and_apply(&mut self, forward: bool) -> bool {
        let row_index = self.ui.overlays.selected_setting_idx;
        if let Some(snapshot) = self.snapshot_current_setting() {
            self.settings_undo.capture(row_index, snapshot);
        }
        self.apply_selected_setting(forward)
    }

    /// Take a snapshot of the currently selected setting's value.
    fn snapshot_current_setting(&self) -> Option<SettingSnapshot> {
        match self.selected_setting_row()? {
            SettingRow::Notifications => {
                Some(SettingSnapshot::Bool(self.config.ui.notifications_enabled))
            }
            SettingRow::AutoplayLast => {
                Some(SettingSnapshot::Bool(self.config.playback.autoplay_last))
            }
            SettingRow::OutputDevice => Some(SettingSnapshot::OptionalString(
                self.config.audio.output_device.clone(),
            )),
            SettingRow::Theme => Some(SettingSnapshot::String(self.config.ui.theme.clone())),
            SettingRow::StreamMetadata => Some(SettingSnapshot::Bool(
                self.config.ui.stream_metadata_enabled,
            )),
            SettingRow::SaveHistory => {
                Some(SettingSnapshot::Bool(self.config.playback.save_history))
            }
        }
    }

    /// Undo the last change for the currently selected setting row.
    fn undo_selected_setting(&mut self) {
        let row_index = self.ui.overlays.selected_setting_idx;
        match self.settings_undo.take(row_index) {
            Some(snapshot) => {
                self.restore_setting_snapshot(row_index, snapshot);
                self.persist_config_change();
            }
            None => {
                self.set_info_notice("Nothing to undo");
            }
        }
    }

    /// Restore a setting from a snapshot.
    fn restore_setting_snapshot(&mut self, row_index: usize, snapshot: SettingSnapshot) {
        let Some(row) = SettingRow::from_index(row_index) else {
            return;
        };
        match (row, snapshot) {
            (SettingRow::Notifications, SettingSnapshot::Bool(value)) => {
                self.config.ui.notifications_enabled = value;
                self.library.settings.notifications_enabled = value;
            }
            (SettingRow::AutoplayLast, SettingSnapshot::Bool(value)) => {
                self.config.playback.autoplay_last = value;
                self.library.settings.autoplay_last = value;
            }
            (SettingRow::OutputDevice, SettingSnapshot::OptionalString(value)) => {
                self.config.audio.output_device = value.clone();
                self.library.settings.output_device_name = value.clone();
                self.playback.diagnostics.output_device =
                    crate::audio::output_device_display_name(value.as_deref());
                self.sync_output_device();
            }
            (SettingRow::Theme, SettingSnapshot::String(value)) => {
                self.config.ui.theme = value.clone();
                self.library.settings.theme = value.clone();
                let theme = ThemeName::from_key(&value);
                crate::ui::theme::set_active(theme);
            }
            (SettingRow::StreamMetadata, SettingSnapshot::Bool(value)) => {
                self.config.ui.stream_metadata_enabled = value;
                self.library.settings.stream_metadata_enabled = value;
                self.playback.diagnostics.metadata_enabled = value;
                self.sync_stream_metadata();
            }
            (SettingRow::SaveHistory, SettingSnapshot::Bool(value)) => {
                self.config.playback.save_history = value;
                self.library.settings.save_history = value;
            }
            _ => {}
        }
    }

    pub(super) fn selected_setting_row(&self) -> Option<SettingRow> {
        SettingRow::from_index(self.ui.overlays.selected_setting_idx)
    }

    pub(super) fn apply_selected_setting(&mut self, forward: bool) -> bool {
        match self.selected_setting_row() {
            Some(SettingRow::Notifications) => {
                let value = !self.config.ui.notifications_enabled;
                self.config.ui.notifications_enabled = value;
                self.library.settings.notifications_enabled = value;
                true
            }
            Some(SettingRow::AutoplayLast) => {
                let value = !self.config.playback.autoplay_last;
                self.config.playback.autoplay_last = value;
                self.library.settings.autoplay_last = value;
                true
            }
            Some(SettingRow::OutputDevice) => self.apply_output_device_setting(forward),
            Some(SettingRow::Theme) => self.apply_theme_setting(forward),
            Some(SettingRow::StreamMetadata) => self.apply_stream_metadata_setting(),
            Some(SettingRow::SaveHistory) => {
                let value = !self.config.playback.save_history;
                self.config.playback.save_history = value;
                self.library.settings.save_history = value;
                true
            }
            None => false,
        }
    }

    fn apply_output_device_setting(&mut self, forward: bool) -> bool {
        let new_device = step_output_device_preference(
            self.config.audio.output_device.as_deref(),
            &available_output_device_choices(),
            forward,
        );
        self.config.audio.output_device = new_device.clone();
        self.library.settings.output_device_name = new_device.clone();
        self.playback.diagnostics.output_device = output_device_display_name(new_device.as_deref());
        self.sync_output_device();
        self.set_info_notice(format!(
            "Audio output: {}",
            output_device_display_name(new_device.as_deref())
        ));
        true
    }

    fn apply_theme_setting(&mut self, forward: bool) -> bool {
        let current = ThemeName::from_key(&self.config.ui.theme);
        let next = step_choice(ThemeName::ALL, current, forward);
        self.config.ui.theme = next.label().to_string();
        self.library.settings.theme = next.label().to_string();
        self.mark_library_dirty();
        crate::ui::theme::set_active(next);
        true
    }

    fn apply_stream_metadata_setting(&mut self) -> bool {
        let value = !self.config.ui.stream_metadata_enabled;
        self.config.ui.stream_metadata_enabled = value;
        self.library.settings.stream_metadata_enabled = value;
        self.playback.diagnostics.metadata_enabled = value;
        self.sync_stream_metadata();
        self.set_info_notice(format!(
            "Song info metadata: {}",
            if value { "on" } else { "off" }
        ));
        true
    }

    pub(super) fn cycle_theme_setting(&mut self) {
        self.ui.overlays.selected_setting_idx = SettingRow::Theme.index();
        if self.apply_selected_setting(true) {
            self.persist_config_change();
            self.set_info_notice(format!("Theme: {}", self.library.settings.theme));
        }
    }

    pub(super) fn toggle_stream_metadata_setting(&mut self) {
        self.ui.overlays.selected_setting_idx = SettingRow::StreamMetadata.index();
        if self.apply_selected_setting(true) {
            self.persist_config_change();
        }
    }

    pub(super) fn sync_output_device(&self) -> bool {
        self.playback
            .audio
            .send(crate::audio::AudioCommand::SetOutputDevice(
                self.config.audio.output_device.clone(),
            ))
    }

    pub(super) fn sync_stream_metadata(&self) -> bool {
        self.playback
            .audio
            .send(crate::audio::AudioCommand::SetStreamMetadata(
                self.config.ui.stream_metadata_enabled,
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
        app.config.ui.theme = "Retrowave".to_string();
        app.library.settings.theme = "Retrowave".to_string();

        app.update(Action::StepSettingForward);
        assert_eq!(app.config.ui.theme, "Catppuccin Mocha");
        assert_eq!(app.library.settings.theme, "Catppuccin Mocha");

        app.update(Action::StepSettingBackward);
        assert_eq!(app.config.ui.theme, "Retrowave");
        assert_eq!(app.library.settings.theme, "Retrowave");
    }

    #[test]
    fn settings_backward_wraps_theme() {
        let mut app = test_app();
        app.ui.overlays.active = ActiveOverlay::Settings;
        app.ui.overlays.selected_setting_idx = SettingRow::Theme.index();
        app.config.ui.theme = "Retrowave".to_string();
        app.library.settings.theme = "Retrowave".to_string();

        app.update(Action::StepSettingBackward);

        assert_eq!(app.config.ui.theme, "Terminal");
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
        app.config.ui.stream_metadata_enabled = true;
        app.library.settings.stream_metadata_enabled = true;

        app.update(Action::TogglePause);

        assert!(!app.library.settings.stream_metadata_enabled);
        assert!(!app.config.ui.stream_metadata_enabled);
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
        app.config.ui.theme = "CatppuccinMocha".to_string();
        app.library.settings.theme = "CatppuccinMocha".to_string();

        app.update(Action::ToggleHelp);

        assert!(app.show_settings());
        assert_eq!(app.config.ui.theme, "Retrowave");
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
        app.library.settings.save_history = false;
        app.config.playback.save_history = false;
        app.ui.overlays.active = ActiveOverlay::Settings;
        app.ui.overlays.selected_setting_idx = SettingRow::SaveHistory.index();
        assert!(!app.library.settings.save_history);

        app.update(Action::PlaySelected);
        assert!(app.library.settings.save_history);
        assert!(app.config.playback.save_history);

        app.update(Action::PlaySelected);
        assert!(!app.library.settings.save_history);
        assert!(!app.config.playback.save_history);
    }

    #[test]
    fn settings_change_persists_config_to_toml_file() {
        use std::fs;
        let dir = std::env::temp_dir().join(format!(
            "pulsedeck-settings-persist-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();

        let mut app = test_app();
        app.config_dir = Some(dir.clone());
        app.config.ui.notifications_enabled = true;
        app.library.settings.notifications_enabled = true;
        app.ui.overlays.active = ActiveOverlay::Settings;
        app.ui.overlays.selected_setting_idx = SettingRow::Notifications.index();

        app.update(Action::PlaySelected);

        let toml_path = dir.join("pulsedeck.toml");
        assert!(toml_path.exists(), "TOML config file should be written");
        let contents = fs::read_to_string(&toml_path).unwrap();
        assert!(
            contents.contains("notifications_enabled = false"),
            "Config should contain the updated setting, got:\n{contents}"
        );
    }

    #[test]
    fn settings_change_does_not_mark_library_dirty() {
        let dir = std::env::temp_dir().join(format!(
            "pulsedeck-settings-no-lib-dirty-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let library_path = dir.join("library.json");

        let mut app = test_app();
        app.library.path = Some(library_path.clone());
        app.ui.overlays.active = ActiveOverlay::Settings;
        app.ui.overlays.selected_setting_idx = SettingRow::Notifications.index();

        app.update(Action::PlaySelected);

        // Library dirty flag should not be set — flushing should not write library.json
        app.force_flush_persistence();
        assert!(
            !library_path.exists(),
            "library.json should not be written for settings changes"
        );
    }

    // --- Settings undo tests ---

    #[test]
    fn test_undo_restores_previous_value() {
        let mut app = test_app();
        app.ui.overlays.active = ActiveOverlay::Settings;
        app.ui.overlays.selected_setting_idx = SettingRow::Notifications.index();
        app.config.ui.notifications_enabled = true;
        app.library.settings.notifications_enabled = true;

        // Toggle the setting (true → false)
        app.update(Action::PlaySelected);
        assert!(!app.config.ui.notifications_enabled);

        // Undo → should restore to true
        app.update(Action::UndoSetting);
        assert!(app.config.ui.notifications_enabled);
        assert!(app.library.settings.notifications_enabled);
    }

    #[test]
    fn test_undo_with_no_entry_shows_notice() {
        let mut app = test_app();
        app.ui.overlays.active = ActiveOverlay::Settings;
        app.ui.overlays.selected_setting_idx = SettingRow::Notifications.index();

        // Undo without any prior change
        app.update(Action::UndoSetting);

        assert!(matches!(
            app.ui.notice.current,
            Some(AppNotice::Info(ref msg)) if msg == "Nothing to undo"
        ));
    }

    #[test]
    fn test_undo_stack_cleared_on_close() {
        let mut app = test_app();
        app.ui.overlays.active = ActiveOverlay::Settings;
        app.ui.overlays.selected_setting_idx = SettingRow::Notifications.index();
        app.config.ui.notifications_enabled = true;
        app.library.settings.notifications_enabled = true;

        // Make a change
        app.update(Action::PlaySelected);
        assert!(app
            .settings_undo
            .has_entry(SettingRow::Notifications.index()));

        // Close settings overlay
        app.update(Action::ToggleSettings);
        assert!(!app.show_settings());
        assert!(!app
            .settings_undo
            .has_entry(SettingRow::Notifications.index()));
    }

    #[test]
    fn test_undo_persists_config() {
        use std::fs;
        let dir = std::env::temp_dir().join(format!(
            "pulsedeck-settings-undo-persist-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();

        let mut app = test_app();
        app.config_dir = Some(dir.clone());
        app.ui.overlays.active = ActiveOverlay::Settings;
        app.ui.overlays.selected_setting_idx = SettingRow::Notifications.index();
        app.config.ui.notifications_enabled = true;
        app.library.settings.notifications_enabled = true;

        // Toggle setting (true → false)
        app.update(Action::PlaySelected);
        // Undo (false → true)
        app.update(Action::UndoSetting);

        let toml_path = dir.join("pulsedeck.toml");
        assert!(
            toml_path.exists(),
            "TOML config file should be written after undo"
        );
        let contents = fs::read_to_string(&toml_path).unwrap();
        assert!(
            contents.contains("notifications_enabled = true"),
            "Config should contain the restored setting after undo, got:\n{contents}"
        );
    }
}
