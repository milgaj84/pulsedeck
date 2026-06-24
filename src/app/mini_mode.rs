use crate::action::Action;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Map a key event to an Action when in Mini display mode.
/// Only a restricted set of playback controls are available.
pub fn map_mini_mode_key(key: KeyEvent) -> Option<Action> {
    match (key.modifiers, key.code) {
        (_, KeyCode::Char(' ')) => Some(Action::TogglePause),
        (_, KeyCode::Char('+')) | (_, KeyCode::Char('=')) => Some(Action::VolumeUp),
        (_, KeyCode::Char('-')) => Some(Action::VolumeDown),
        (_, KeyCode::Char('s')) => Some(Action::Stop),
        (_, KeyCode::Char('q')) => Some(Action::Quit),
        (mods, KeyCode::Char('c')) if mods.contains(KeyModifiers::CONTROL) => Some(Action::Quit),
        (_, KeyCode::Char('m')) => Some(Action::ToggleMute),
        (_, KeyCode::F(6)) => Some(Action::ToggleMiniMode),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyEventKind;

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        }
    }

    fn press_modified(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        }
    }

    #[test]
    fn space_toggles_pause() {
        assert_eq!(
            map_mini_mode_key(press(KeyCode::Char(' '))),
            Some(Action::TogglePause),
        );
    }

    #[test]
    fn plus_and_equals_volume_up() {
        assert_eq!(
            map_mini_mode_key(press(KeyCode::Char('+'))),
            Some(Action::VolumeUp),
        );
        assert_eq!(
            map_mini_mode_key(press(KeyCode::Char('='))),
            Some(Action::VolumeUp),
        );
    }

    #[test]
    fn minus_volume_down() {
        assert_eq!(
            map_mini_mode_key(press(KeyCode::Char('-'))),
            Some(Action::VolumeDown),
        );
    }

    #[test]
    fn s_stops_playback() {
        assert_eq!(
            map_mini_mode_key(press(KeyCode::Char('s'))),
            Some(Action::Stop),
        );
    }

    #[test]
    fn q_quits() {
        assert_eq!(
            map_mini_mode_key(press(KeyCode::Char('q'))),
            Some(Action::Quit),
        );
    }

    #[test]
    fn ctrl_c_quits() {
        assert_eq!(
            map_mini_mode_key(press_modified(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            Some(Action::Quit),
        );
    }

    #[test]
    fn m_toggles_mute() {
        assert_eq!(
            map_mini_mode_key(press(KeyCode::Char('m'))),
            Some(Action::ToggleMute),
        );
    }

    #[test]
    fn f6_toggles_mini_mode() {
        assert_eq!(
            map_mini_mode_key(press(KeyCode::F(6))),
            Some(Action::ToggleMiniMode),
        );
    }

    #[test]
    fn disallowed_keys_return_none() {
        let disallowed = [
            KeyCode::Char('j'),
            KeyCode::Char('k'),
            KeyCode::Up,
            KeyCode::Down,
            KeyCode::Char('/'),
            KeyCode::Char(':'),
            KeyCode::Char('?'),
            KeyCode::Char('h'),
            KeyCode::Char('i'),
            KeyCode::Char('g'),
            KeyCode::Char('d'),
            KeyCode::Char('b'),
            KeyCode::Char(','),
            KeyCode::Tab,
            KeyCode::BackTab,
            KeyCode::Enter,
            KeyCode::Char('t'),
            KeyCode::Char('e'),
        ];

        for code in disallowed {
            assert_eq!(
                map_mini_mode_key(press(code)),
                None,
                "Expected None for {:?}",
                code,
            );
        }
    }
}
