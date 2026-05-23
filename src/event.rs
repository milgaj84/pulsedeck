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
        (KeyModifiers::CONTROL, KeyCode::Char('c')) => Some(Action::Quit),

        // Search
        (_, KeyCode::Char('/')) => Some(Action::EnterSearch),
        (KeyModifiers::CONTROL, KeyCode::Char('f')) => Some(Action::EnterSearch),

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

        // Favorites
        (_, KeyCode::Char('f')) => Some(Action::ToggleFavorite),
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

/// Key mapping for search mode — all printable chars go to search input.
fn map_search(key: KeyEvent) -> Option<Action> {
    match (key.modifiers, key.code) {
        // Exit search
        (_, KeyCode::Esc) => Some(Action::ExitSearch),

        // Confirm search (select highlighted + exit)
        (_, KeyCode::Enter) => Some(Action::SearchConfirm),

        // Navigate within filtered results
        (_, KeyCode::Up) => Some(Action::PrevStation),
        (_, KeyCode::Down) => Some(Action::NextStation),

        // Delete character
        (_, KeyCode::Backspace) => Some(Action::SearchBackspace),

        // Ctrl+C still quits
        (KeyModifiers::CONTROL, KeyCode::Char('c')) => Some(Action::Quit),

        // All other printable characters go to search input
        (_, KeyCode::Char(c)) => Some(Action::SearchInput(c)),

        _ => None,
    }
}
