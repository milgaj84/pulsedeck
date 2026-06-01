use crate::action::Action;
use crate::app::InputMode;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use std::time::Duration;

/// Poll for terminal events and map them to Actions.
/// Key mapping depends on the current input mode.
pub fn poll_action(timeout: Duration, mode: &InputMode) -> Option<Action> {
    if event::poll(timeout).ok()? {
        if let Event::Key(key) = event::read().ok()? {
            return map_key(key, mode);
        }
    }
    None
}

/// Map a key event to an Action based on current input mode.
fn map_key(key: KeyEvent, mode: &InputMode) -> Option<Action> {
    // Ignore key release events (crossterm sends both press and release)
    if key.kind != crossterm::event::KeyEventKind::Press {
        return None;
    }

    match mode {
        InputMode::Normal => map_normal(key),
        InputMode::Search => map_search(key),
        InputMode::TapeFilter => map_tape_filter(key),
    }
}

/// Key mapping for normal mode.
fn map_normal(key: KeyEvent) -> Option<Action> {
    match (key.modifiers, key.code) {
        // Quit
        (_, KeyCode::Char('q')) => Some(Action::Quit),
        (_, KeyCode::Esc) => Some(Action::Quit),
        (mods, KeyCode::Char('c')) if mods.contains(KeyModifiers::CONTROL) => Some(Action::Quit),

        // Search
        (_, KeyCode::Char('/')) => Some(Action::EnterSearch),
        (_, KeyCode::Char('t')) => Some(Action::EnterTapeFilter),
        (mods, KeyCode::Char('f')) if mods.contains(KeyModifiers::CONTROL) => {
            Some(Action::EnterSearch)
        }

        // Navigation
        (_, KeyCode::Up) | (_, KeyCode::Char('k')) => Some(Action::PrevStation),
        (_, KeyCode::Down) | (_, KeyCode::Char('j')) => Some(Action::NextStation),
        (_, KeyCode::Right) | (_, KeyCode::Char('l')) | (_, KeyCode::Char('d')) => {
            Some(Action::StepSettingForward)
        }
        (_, KeyCode::Left) | (_, KeyCode::Char('a')) => Some(Action::StepSettingBackward),

        // Playback
        (_, KeyCode::Enter) => Some(Action::PlaySelected),
        (_, KeyCode::Char(' ')) => Some(Action::TogglePause),
        (_, KeyCode::Char('s')) => Some(Action::Stop),

        // Volume
        (_, KeyCode::Char('+')) | (_, KeyCode::Char('=')) => Some(Action::VolumeUp),
        (_, KeyCode::Char('-')) => Some(Action::VolumeDown),
        (_, KeyCode::Char('m')) => Some(Action::ToggleMute),

        // Library management
        (_, KeyCode::Char('f')) => Some(Action::RemoveLibrarySelection),
        (_, KeyCode::Char('u')) => Some(Action::UndoRemoveLibrarySelection),
        (_, KeyCode::Tab) => Some(Action::NextGenre),
        (_, KeyCode::BackTab) => Some(Action::PrevGenre),

        // Help overlay
        (_, KeyCode::Char('?')) | (_, KeyCode::Char('h')) => Some(Action::ToggleHelp),

        // Bento layout cycle
        (_, KeyCode::Char('b')) => Some(Action::CycleLayout),

        // Deck page cycle
        (_, KeyCode::Char('p')) => Some(Action::NextDeckPage),

        // Visualizer mode toggle
        (_, KeyCode::Char('v')) => Some(Action::ToggleVisualizerMode),

        // Settings overlay
        (_, KeyCode::Char(',')) => Some(Action::ToggleSettings),

        // Local tape archive
        (mods, KeyCode::Char('r')) if mods.contains(KeyModifiers::CONTROL) => {
            Some(Action::RefreshTapeArchive)
        }
        (_, KeyCode::Delete) => Some(Action::DeleteSelectedTape),
        (_, KeyCode::Char('y')) => Some(Action::ConfirmDeleteTape),
        (_, KeyCode::Char('n')) => Some(Action::CancelDeleteTape),

        // Tape recording
        (_, KeyCode::Char('r')) => Some(Action::ToggleRecording),

        _ => None,
    }
}

/// Key mapping for search mode.
/// Printable characters remain search input, except Space auditions highlighted results.
fn map_search(key: KeyEvent) -> Option<Action> {
    match (key.modifiers, key.code) {
        // Exit search
        (_, KeyCode::Esc) => Some(Action::ExitSearch),

        // Audition search: play highlighted result without saving it.
        (mods, KeyCode::Enter) if mods.contains(KeyModifiers::CONTROL) => {
            Some(Action::SearchAudition)
        }
        (_, KeyCode::Char(' ')) => Some(Action::SearchAudition),

        // Confirm search: add highlighted result, play it, and leave search
        (_, KeyCode::Enter) => Some(Action::SearchConfirm),

        // Navigate within filtered results
        (_, KeyCode::Up) => Some(Action::PrevStation),
        (_, KeyCode::Down) => Some(Action::NextStation),

        // Delete character
        (_, KeyCode::Backspace) => Some(Action::SearchBackspace),

        // Ctrl+C still quits
        (mods, KeyCode::Char('c')) if mods.contains(KeyModifiers::CONTROL) => Some(Action::Quit),
        (_, KeyCode::Char('\u{3}')) => Some(Action::Quit),

        // Modifier escape rails: keep core audio controls reachable while typing.
        (mods, KeyCode::Char('-')) if has_search_escape_modifier(mods) => Some(Action::VolumeDown),
        (mods, KeyCode::Char('+') | KeyCode::Char('=')) if has_search_escape_modifier(mods) => {
            Some(Action::VolumeUp)
        }
        (mods, KeyCode::Char('m')) if has_search_escape_modifier(mods) => Some(Action::ToggleMute),

        // All other printable characters go to search input
        (_, KeyCode::Char(c)) => Some(Action::SearchInput(c)),

        _ => None,
    }
}

fn map_tape_filter(key: KeyEvent) -> Option<Action> {
    match (key.modifiers, key.code) {
        (_, KeyCode::Esc) => Some(Action::ExitTapeFilter),
        (_, KeyCode::Enter) => Some(Action::PlaySelected),
        (_, KeyCode::Up) | (_, KeyCode::Char('k')) => Some(Action::PrevStation),
        (_, KeyCode::Down) | (_, KeyCode::Char('j')) => Some(Action::NextStation),
        (_, KeyCode::Backspace) => Some(Action::TapeFilterBackspace),
        (mods, KeyCode::Char('r')) if mods.contains(KeyModifiers::CONTROL) => {
            Some(Action::RefreshTapeArchive)
        }
        (mods, KeyCode::Char('c')) if mods.contains(KeyModifiers::CONTROL) => Some(Action::Quit),
        (_, KeyCode::Char(c)) if !c.is_control() => Some(Action::TapeFilterInput(c)),
        _ => None,
    }
}

fn has_search_escape_modifier(modifiers: KeyModifiers) -> bool {
    modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn modified_key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    #[test]
    fn search_mode_treats_plain_f_as_text_input() {
        assert_eq!(
            map_key(key(KeyCode::Char('f')), &InputMode::Search),
            Some(Action::SearchInput('f')),
        );
    }

    #[test]
    fn search_mode_treats_plain_a_as_text_input() {
        assert_eq!(
            map_key(key(KeyCode::Char('a')), &InputMode::Search),
            Some(Action::SearchInput('a')),
        );
    }

    #[test]
    fn search_mode_treats_plain_u_as_text_input() {
        assert_eq!(
            map_key(key(KeyCode::Char('u')), &InputMode::Search),
            Some(Action::SearchInput('u')),
        );
    }

    #[test]
    fn search_mode_f2_does_not_add_selected_result() {
        assert_eq!(map_key(key(KeyCode::F(2)), &InputMode::Search), None,);
    }

    #[test]
    fn search_mode_insert_does_not_add_selected_result() {
        assert_eq!(map_key(key(KeyCode::Insert), &InputMode::Search), None,);
    }

    #[test]
    fn search_mode_plain_audio_keys_remain_text_input() {
        assert_eq!(
            map_key(key(KeyCode::Char('m')), &InputMode::Search),
            Some(Action::SearchInput('m')),
        );
        assert_eq!(
            map_key(key(KeyCode::Char('-')), &InputMode::Search),
            Some(Action::SearchInput('-')),
        );
        assert_eq!(
            map_key(key(KeyCode::Char('=')), &InputMode::Search),
            Some(Action::SearchInput('=')),
        );
        assert_eq!(
            map_key(key(KeyCode::Char('+')), &InputMode::Search),
            Some(Action::SearchInput('+')),
        );
    }

    #[test]
    fn search_mode_ctrl_audio_keys_bypass_text_input() {
        assert_eq!(
            map_key(
                modified_key(KeyCode::Char('-'), KeyModifiers::CONTROL),
                &InputMode::Search
            ),
            Some(Action::VolumeDown),
        );
        assert_eq!(
            map_key(
                modified_key(KeyCode::Char('='), KeyModifiers::CONTROL),
                &InputMode::Search
            ),
            Some(Action::VolumeUp),
        );
        assert_eq!(
            map_key(
                modified_key(KeyCode::Char('+'), KeyModifiers::CONTROL),
                &InputMode::Search
            ),
            Some(Action::VolumeUp),
        );
        assert_eq!(
            map_key(
                modified_key(KeyCode::Char('m'), KeyModifiers::CONTROL),
                &InputMode::Search
            ),
            Some(Action::ToggleMute),
        );
    }

    #[test]
    fn search_mode_alt_audio_keys_bypass_text_input() {
        assert_eq!(
            map_key(
                modified_key(KeyCode::Char('-'), KeyModifiers::ALT),
                &InputMode::Search
            ),
            Some(Action::VolumeDown),
        );
        assert_eq!(
            map_key(
                modified_key(KeyCode::Char('='), KeyModifiers::ALT),
                &InputMode::Search
            ),
            Some(Action::VolumeUp),
        );
        assert_eq!(
            map_key(
                modified_key(KeyCode::Char('m'), KeyModifiers::ALT),
                &InputMode::Search
            ),
            Some(Action::ToggleMute),
        );
    }

    #[test]
    fn normal_mode_right_steps_setting_forward() {
        assert_eq!(
            map_key(key(KeyCode::Right), &InputMode::Normal),
            Some(Action::StepSettingForward),
        );
    }

    #[test]
    fn normal_mode_left_steps_setting_backward() {
        assert_eq!(
            map_key(key(KeyCode::Left), &InputMode::Normal),
            Some(Action::StepSettingBackward),
        );
    }

    #[test]
    fn normal_mode_l_and_d_step_setting_forward() {
        assert_eq!(
            map_key(key(KeyCode::Char('l')), &InputMode::Normal),
            Some(Action::StepSettingForward),
        );
        assert_eq!(
            map_key(key(KeyCode::Char('d')), &InputMode::Normal),
            Some(Action::StepSettingForward),
        );
    }

    #[test]
    fn normal_mode_a_steps_setting_backward() {
        assert_eq!(
            map_key(key(KeyCode::Char('a')), &InputMode::Normal),
            Some(Action::StepSettingBackward),
        );
    }

    #[test]
    fn normal_mode_f_removes_library_selection() {
        assert_eq!(
            map_key(key(KeyCode::Char('f')), &InputMode::Normal),
            Some(Action::RemoveLibrarySelection),
        );
    }

    #[test]
    fn normal_mode_u_undoes_library_removal() {
        assert_eq!(
            map_key(key(KeyCode::Char('u')), &InputMode::Normal),
            Some(Action::UndoRemoveLibrarySelection),
        );
    }

    #[test]
    fn normal_mode_ctrl_r_refreshes_tape_archive() {
        assert_eq!(
            map_key(
                modified_key(KeyCode::Char('r'), KeyModifiers::CONTROL),
                &InputMode::Normal
            ),
            Some(Action::RefreshTapeArchive),
        );
    }

    #[test]
    fn normal_mode_t_enters_tape_filter_action() {
        assert_eq!(
            map_key(key(KeyCode::Char('t')), &InputMode::Normal),
            Some(Action::EnterTapeFilter),
        );
    }

    #[test]
    fn tape_filter_mode_collects_text() {
        assert_eq!(
            map_key(key(KeyCode::Char('s')), &InputMode::TapeFilter),
            Some(Action::TapeFilterInput('s')),
        );
    }

    #[test]
    fn tape_filter_mode_escape_exits_filter() {
        assert_eq!(
            map_key(key(KeyCode::Esc), &InputMode::TapeFilter),
            Some(Action::ExitTapeFilter),
        );
    }

    #[test]
    fn tape_filter_mode_backspace_edits_filter() {
        assert_eq!(
            map_key(key(KeyCode::Backspace), &InputMode::TapeFilter),
            Some(Action::TapeFilterBackspace),
        );
    }

    #[test]
    fn normal_mode_delete_requests_tape_delete() {
        assert_eq!(
            map_key(key(KeyCode::Delete), &InputMode::Normal),
            Some(Action::DeleteSelectedTape),
        );
    }

    #[test]
    fn search_mode_space_auditions_selected_result() {
        assert_eq!(
            map_key(key(KeyCode::Char(' ')), &InputMode::Search),
            Some(Action::SearchAudition),
        );
    }

    #[test]
    fn search_mode_ctrl_enter_auditions_selected_result_when_supported() {
        assert_eq!(
            map_key(
                modified_key(KeyCode::Enter, KeyModifiers::CONTROL),
                &InputMode::Search
            ),
            Some(Action::SearchAudition),
        );
    }

    #[test]
    fn search_mode_enter_adds_and_plays_selected_result() {
        assert_eq!(
            map_key(key(KeyCode::Enter), &InputMode::Search),
            Some(Action::SearchConfirm),
        );
    }
}
