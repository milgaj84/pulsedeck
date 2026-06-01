use super::*;
use crate::action::Action;

impl App {
    /// Process an action and update state accordingly.
    pub fn update(&mut self, action: Action) {
        if self.show_settings {
            self.handle_settings_action(action);
            return;
        }

        match action {
            Action::NextStation => self.next_station(),
            Action::PrevStation => self.prev_station(),

            Action::PlaySelected => self.play_selected(),
            Action::TogglePause => self.toggle_pause(),
            Action::Stop => self.stop_playback(),

            Action::VolumeUp => self.volume_up(),
            Action::VolumeDown => self.volume_down(),
            Action::ToggleMute => self.toggle_mute(),

            Action::EnterSearch => self.enter_search(),
            Action::ExitSearch => self.exit_search(),
            Action::SearchInput(c) => self.search_input(c),
            Action::SearchBackspace => self.search_backspace(),
            Action::SearchConfirm => self.confirm_search(),
            Action::SearchAudition => self.audition_search_result(),

            Action::RemoveLibrarySelection => self.remove_library_selection(),
            Action::UndoRemoveLibrarySelection => self.undo_remove_library_selection(),
            Action::NextGenre => self.next_genre(),
            Action::PrevGenre => self.prev_genre(),

            Action::ToggleHelp => self.toggle_help(),
            Action::ToggleSettings => self.toggle_settings(),
            Action::ToggleRecording => self.toggle_recording(),
            Action::CycleLayout => self.cycle_layout(),
            Action::NextDeckPage => self.next_deck_page(),
            Action::ToggleVisualizerMode => self.toggle_visualizer_mode(),

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
