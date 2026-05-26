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
        (mods, KeyCode::Char('f')) if mods.contains(KeyModifiers::CONTROL) => {
            Some(Action::EnterSearch)
        }

        // Navigation
        (_, KeyCode::Up) | (_, KeyCode::Char('k')) => Some(Action::PrevStation),
        (_, KeyCode::Down) | (_, KeyCode::Char('j')) => Some(Action::NextStation),

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

        // Tape recording
        (_, KeyCode::Char('r')) => Some(Action::ToggleRecording),

        _ => None,
    }
}

/// Key mapping for search mode.
/// Printable characters remain search input; Enter is the only add-from-search action.
fn map_search(key: KeyEvent) -> Option<Action> {
    match (key.modifiers, key.code) {
        // Exit search
        (_, KeyCode::Esc) => Some(Action::ExitSearch),

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

        // All other printable characters go to search input
        (_, KeyCode::Char(c)) => Some(Action::SearchInput(c)),

        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
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
    fn search_mode_enter_adds_and_plays_selected_result() {
        assert_eq!(
            map_key(key(KeyCode::Enter), &InputMode::Search),
            Some(Action::SearchConfirm),
        );
    }
}
