use super::*;
use crate::action::Action;

impl App {
    /// Process an action and update state accordingly.
    pub fn update(&mut self, action: Action) {
        if self.ui.input_mode == InputMode::CommandPalette {
            self.handle_command_palette_action(action);
            return;
        }

        match self.ui.overlays.active {
            ActiveOverlay::Settings if self.show_settings() => {
                self.handle_settings_action(action);
                return;
            }
            ActiveOverlay::SleepTimer => {
                self.handle_sleep_timer_action(action);
                return;
            }
            _ => {}
        }

        match action {
            Action::NextStation => self.next_station(),
            Action::PrevStation => self.prev_station(),

            Action::PlaySelected => self.play_selected(),
            Action::TogglePause => self.toggle_pause(),
            Action::Stop => self.stop_playback(),
            Action::RetryStream => self.retry_stream(),

            Action::VolumeUp => self.volume_up(),
            Action::VolumeDown => self.volume_down(),
            Action::ToggleMute => self.toggle_mute(),

            Action::EnterSearch => self.enter_search(),
            Action::ExitSearch => self.exit_search(),
            Action::SearchInput(c) => self.search_input(c),
            Action::SearchBackspace => self.search_backspace(),
            Action::SearchConfirm => self.confirm_search(),
            Action::SearchAudition => self.audition_search_result(),

            Action::OpenCommandPalette => self.open_command_palette(),
            Action::CommandPaletteConfirm
            | Action::CommandPaletteClose
            | Action::CommandPaletteInput(_)
            | Action::CommandPaletteBackspace
            | Action::CommandPaletteNext
            | Action::CommandPalettePrev => {}

            Action::RemoveLibrarySelection => self.remove_library_selection(),
            Action::UndoRemoveLibrarySelection => self.undo_remove_library_selection(),
            Action::NextGenre => self.next_genre(),
            Action::PrevGenre => self.prev_genre(),

            Action::EnterLibraryFilter => self.enter_library_filter(),
            Action::ExitLibraryFilter => self.exit_library_filter(),
            Action::LibraryFilterInput(c) => self.library_filter_input(c),
            Action::LibraryFilterBackspace => self.library_filter_backspace(),
            Action::LibraryFilterConfirm => self.library_filter_confirm(),

            // Station preset slots
            Action::PlaySlot(n) => self.play_slot(n),
            Action::AssignSlot(n) => self.assign_slot(n),

            // Favorites (handler added in task 13)
            Action::ToggleFavorite => self.toggle_favorite(),

            // Number jump
            Action::NumberJumpDigit(c) => self.handle_number_jump_digit(c),
            Action::NumberJumpConfirm => self.handle_number_jump_confirm(),
            Action::NumberJumpCancel => self.handle_number_jump_cancel(),

            Action::ToggleHelp => self.toggle_help(),
            Action::ToggleStationDetails => self.toggle_station_details(),
            Action::ToggleRecentTracks => self.toggle_recent_tracks(),
            Action::TogglePlaybackDoctor => self.toggle_playback_doctor(),
            Action::StepSettingForward | Action::StepSettingBackward => {}
            Action::ToggleSettings => self.toggle_settings(),
            Action::CycleThemeSetting => self.cycle_theme_setting(),
            Action::ToggleStreamMetadata => self.toggle_stream_metadata_setting(),
            Action::RefreshLibraryMetadata => self.request_metadata_refresh(),
            Action::CycleLayout => self.cycle_layout(),
            Action::ToggleVisualizerMode => self.toggle_visualizer_mode(),
            Action::ToggleSleepTimer => self.toggle_sleep_timer(),
            Action::SleepTimerIncrease
            | Action::SleepTimerDecrease
            | Action::SleepTimerPreset(_)
            | Action::SleepTimerClear => {}
            Action::ExportLibrary => self.export_library(),

            Action::Tick => self.tick(),
            Action::Quit => self.quit(),
        }
    }

    pub(super) fn tick(&mut self) {
        let now = std::time::Instant::now();
        self.ui.tick_count += 1;
        self.tick_notice();
        self.poll_audio_status();
        self.update_visualizer();
        self.drive_reconnect(now);
        self.check_sleep_timer(now);
        self.check_number_jump_timeout(now);
        self.flush_persistence();
    }

    pub(super) fn quit(&mut self) {
        if self.close_any_overlay() {
            return;
        }

        self.stop_audio_before_quit();
        self.ui.should_quit = true;
    }

    fn next_station(&mut self) {
        if self.ui.input_mode == InputMode::LibraryFilter {
            self.library_filter_next();
            return;
        }
        let count = self.visible_count();
        if count > 0 {
            self.ui.nav.selected = (self.ui.nav.selected + 1) % count;
        }
    }

    fn prev_station(&mut self) {
        if self.ui.input_mode == InputMode::LibraryFilter {
            self.library_filter_prev();
            return;
        }
        let count = self.visible_count();
        if count > 0 {
            self.ui.nav.selected = if self.ui.nav.selected == 0 {
                count - 1
            } else {
                self.ui.nav.selected - 1
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Action;
    use crate::favorites::Library;
    use crate::radio::Station;

    fn station(name: &str, url: &str) -> Station {
        Station::basic(name, url, "Synthwave", "US", 128)
    }

    fn test_app() -> App {
        App::new(Library::in_memory(vec![
            station("A", "http://a"),
            station("B", "http://b"),
        ]))
    }

    // ---- Mode-gating: settings overlay swallows actions --------------------

    #[test]
    fn settings_overlay_swallows_play_and_navigation() {
        let mut app = test_app();
        app.ui.overlays.active = ActiveOverlay::Settings;
        app.ui.overlays.selected_setting_idx = 0;
        app.ui.nav.selected = 0;

        app.update(Action::NextStation);
        // NextStation in settings navigates settings rows, not the station list
        assert_eq!(app.ui.overlays.selected_setting_idx, 1);
        // Station nav unchanged
        assert_eq!(app.ui.nav.selected, 0);
    }

    #[test]
    fn settings_overlay_swallows_search_entry() {
        let mut app = test_app();
        app.ui.overlays.active = ActiveOverlay::Settings;

        app.update(Action::EnterSearch);

        assert_eq!(app.ui.input_mode, InputMode::Normal);
        assert_eq!(app.ui.overlays.active, ActiveOverlay::Settings);
    }

    // ---- Mode-gating: sleep timer overlay isolation ------------------------

    #[test]
    fn sleep_timer_overlay_swallows_non_timer_actions() {
        let mut app = test_app();
        app.ui.overlays.active = ActiveOverlay::SleepTimer;
        app.ui.input_mode = InputMode::SleepTimer;

        app.update(Action::NextStation);

        // Navigation should not change station index
        assert_eq!(app.ui.nav.selected, 0);
    }

    // ---- Action routing: basic dispatch ------------------------------------

    #[test]
    fn play_selected_action_triggers_connecting_state() {
        let mut app = test_app();

        app.update(Action::PlaySelected);

        assert_eq!(app.playback.view.state, PlaybackState::Connecting);
        assert_eq!(app.playback.view.playing_url.as_deref(), Some("http://a"));
    }

    #[test]
    fn stop_action_stops_playback() {
        let mut app = test_app();
        app.playback.view.playing_url = Some("http://a".to_string());
        app.playback.view.state = PlaybackState::Connecting;

        app.update(Action::Stop);

        assert_eq!(app.playback.view.state, PlaybackState::Stopped);
    }

    #[test]
    fn next_station_wraps_around() {
        let mut app = test_app();
        app.ui.nav.selected = 1;

        app.update(Action::NextStation);

        assert_eq!(app.ui.nav.selected, 0);
    }

    #[test]
    fn prev_station_wraps_around() {
        let mut app = test_app();
        app.ui.nav.selected = 0;

        app.update(Action::PrevStation);

        assert_eq!(app.ui.nav.selected, 1);
    }

    #[test]
    fn quit_action_quits_when_no_overlay_active() {
        let mut app = test_app();

        app.update(Action::Quit);

        assert!(app.ui.should_quit);
    }

    #[test]
    fn quit_action_closes_overlay_before_quitting() {
        let mut app = test_app();
        app.ui.overlays.active = ActiveOverlay::Help;

        app.update(Action::Quit);

        assert!(!app.ui.should_quit);
        assert_eq!(app.ui.overlays.active, ActiveOverlay::None);
    }

    // ---- Command palette mode routing ------------------------------------

    #[test]
    fn command_palette_mode_routes_to_palette_handler() {
        let mut app = test_app();
        app.ui.input_mode = InputMode::CommandPalette;

        // Actions should be handled by palette handler, not the main dispatch
        app.update(Action::CommandPaletteClose);

        assert_eq!(app.ui.input_mode, InputMode::Normal);
    }

    // ---- Tick action always runs ------------------------------------------

    #[test]
    fn tick_increments_tick_count() {
        let mut app = test_app();
        let before = app.ui.tick_count;

        app.update(Action::Tick);

        assert_eq!(app.ui.tick_count, before + 1);
    }
}
