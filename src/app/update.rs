use super::*;
use crate::action::Action;

impl App {
    /// Process an action and update state accordingly.
    pub fn update(&mut self, action: Action) {
        if self.pending_tape_delete.is_some() {
            match action {
                Action::ConfirmDeleteTape => self.confirm_tape_delete(),
                Action::CancelDeleteTape | Action::Quit => self.cancel_tape_delete(),
                _ => {}
            }
            return;
        }

        if self.input_mode == InputMode::TapeFilter {
            self.handle_tape_filter_action(action);
            return;
        }

        if self.show_settings {
            self.handle_settings_action(action);
            return;
        }

        match action {
            Action::NextStation => {
                if self.is_tape_archive_focused() {
                    self.next_tape_archive_row();
                } else {
                    self.next_station();
                }
            }
            Action::PrevStation => {
                if self.is_tape_archive_focused() {
                    self.prev_tape_archive_row();
                } else {
                    self.prev_station();
                }
            }

            Action::PlaySelected => {
                if self.is_tape_archive_focused() {
                    self.play_selected_tape_or_toggle();
                } else {
                    self.play_selected();
                }
            }
            Action::TogglePause => {
                if self.is_tape_archive_focused() {
                    self.toggle_tape_archive_folder_or_pause();
                } else {
                    self.toggle_pause();
                }
            }
            Action::Stop => self.stop_playback(),

            Action::VolumeUp => self.volume_up(),
            Action::VolumeDown => self.volume_down(),
            Action::ToggleMute => self.toggle_mute(),

            Action::EnterSearch => {
                if self.is_tape_archive_focused() {
                    self.enter_tape_filter();
                } else {
                    self.enter_search();
                }
            }
            Action::EnterTapeFilter => self.enter_tape_filter(),
            Action::ExitTapeFilter => self.exit_tape_filter(),
            Action::TapeFilterInput(ch) => self.tape_filter_input(ch),
            Action::TapeFilterBackspace => self.tape_filter_backspace(),
            Action::ExitSearch => self.exit_search(),
            Action::SearchInput(c) => self.search_input(c),
            Action::SearchBackspace => self.search_backspace(),
            Action::SearchConfirm => self.confirm_search(),
            Action::SearchAudition => self.audition_search_result(),

            Action::RemoveLibrarySelection | Action::DeleteSelectedTape => {
                if self.is_tape_archive_focused() {
                    self.request_delete_selected_tape();
                } else {
                    self.remove_library_selection();
                }
            }
            Action::UndoRemoveLibrarySelection => self.undo_remove_library_selection(),
            Action::NextGenre => self.next_genre(),
            Action::PrevGenre => self.prev_genre(),

            Action::ToggleHelp => self.toggle_help(),
            Action::StepSettingForward | Action::StepSettingBackward => {}
            Action::ToggleSettings => self.toggle_settings(),
            Action::ToggleRecording => self.toggle_recording(),
            Action::KeepRecordingRecovery => self.keep_recording_recovery(),
            Action::TrashRecordingRecovery => self.trash_recording_recovery(),
            Action::DismissRecordingRecovery => self.dismiss_recording_recovery(),
            Action::CycleLayout => self.cycle_layout(),
            Action::NextDeckPage => self.next_deck_page(),
            Action::ToggleVisualizerMode => self.toggle_visualizer_mode(),
            Action::RefreshTapeArchive => self.refresh_tape_archive(),
            Action::OpenSelectedTapeFolder => self.open_selected_tape_folder(),
            Action::ConfirmDeleteTape => self.confirm_tape_delete(),
            Action::CancelDeleteTape => self.cancel_tape_delete(),

            Action::Tick => self.tick(),
            Action::Quit => self.quit(),
        }
    }

    pub(super) fn tick(&mut self) {
        self.tick_count += 1;
        self.poll_audio_status();
        self.update_visualizer();
    }

    pub(super) fn quit(&mut self) {
        if self.show_help {
            self.show_help = false;
        } else {
            self.stop_audio_before_quit();
            self.should_quit = true;
        }
    }

    fn next_station(&mut self) {
        let count = self.visible_count();
        if count > 0 {
            self.selected = (self.selected + 1) % count;
        }
    }

    fn prev_station(&mut self) {
        let count = self.visible_count();
        if count > 0 {
            self.selected = if self.selected == 0 {
                count - 1
            } else {
                self.selected - 1
            };
        }
    }
}
