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
        let count = self.visible_count();
        if count > 0 {
            self.ui.nav.selected = (self.ui.nav.selected + 1) % count;
        }
    }

    fn prev_station(&mut self) {
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
