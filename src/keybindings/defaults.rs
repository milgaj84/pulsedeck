// Default keybinding table — extracted from hardcoded match tables in event.rs.

use super::{InputMode, KeyBinding, KeySpec, Modifier, NamedKey};
use crate::action::Action;

/// Build the complete default keybinding table.
/// Equivalent to the current hardcoded match tables in event.rs.
pub fn default_bindings() -> Vec<KeyBinding> {
    let mut bindings = Vec::new();
    bindings.extend(normal_mode_defaults());
    bindings.extend(search_mode_defaults());
    bindings.extend(command_palette_defaults());
    bindings.extend(sleep_timer_defaults());
    bindings.extend(library_filter_defaults());
    bindings
}

/// Normal mode defaults extracted from `map_normal` in event.rs.
pub fn normal_mode_defaults() -> Vec<KeyBinding> {
    let mode = InputMode::Normal;
    vec![
        // Quit
        bind(KeySpec::Char('q'), vec![], Action::Quit, mode.clone()),
        bind(
            KeySpec::Named(NamedKey::Esc),
            vec![],
            Action::Quit,
            mode.clone(),
        ),
        bind(
            KeySpec::Char('c'),
            vec![Modifier::Ctrl],
            Action::Quit,
            mode.clone(),
        ),
        // Search
        bind(
            KeySpec::Char('/'),
            vec![],
            Action::EnterSearch,
            mode.clone(),
        ),
        bind(
            KeySpec::Function(3),
            vec![],
            Action::EnterSearch,
            mode.clone(),
        ),
        bind(
            KeySpec::Char('f'),
            vec![Modifier::Ctrl],
            Action::EnterSearch,
            mode.clone(),
        ),
        // Command palette
        bind(
            KeySpec::Char(':'),
            vec![],
            Action::OpenCommandPalette,
            mode.clone(),
        ),
        bind(
            KeySpec::Char('p'),
            vec![Modifier::Ctrl],
            Action::OpenCommandPalette,
            mode.clone(),
        ),
        // Navigation
        bind(
            KeySpec::Named(NamedKey::Up),
            vec![],
            Action::PrevStation,
            mode.clone(),
        ),
        bind(
            KeySpec::Char('k'),
            vec![],
            Action::PrevStation,
            mode.clone(),
        ),
        bind(
            KeySpec::Named(NamedKey::Down),
            vec![],
            Action::NextStation,
            mode.clone(),
        ),
        bind(
            KeySpec::Char('j'),
            vec![],
            Action::NextStation,
            mode.clone(),
        ),
        bind(
            KeySpec::Char('l'),
            vec![],
            Action::StepSettingForward,
            mode.clone(),
        ),
        bind(
            KeySpec::Named(NamedKey::Right),
            vec![],
            Action::StepSettingForward,
            mode.clone(),
        ),
        bind(
            KeySpec::Named(NamedKey::Left),
            vec![],
            Action::StepSettingBackward,
            mode.clone(),
        ),
        bind(
            KeySpec::Char('a'),
            vec![],
            Action::StepSettingBackward,
            mode.clone(),
        ),
        // Playback
        bind(
            KeySpec::Named(NamedKey::Enter),
            vec![],
            Action::PlaySelected,
            mode.clone(),
        ),
        bind(
            KeySpec::Char(' '),
            vec![],
            Action::TogglePause,
            mode.clone(),
        ),
        bind(KeySpec::Char('s'), vec![], Action::Stop, mode.clone()),
        bind(
            KeySpec::Char('r'),
            vec![],
            Action::RetryStream,
            mode.clone(),
        ),
        // Volume
        bind(KeySpec::Char('+'), vec![], Action::VolumeUp, mode.clone()),
        bind(KeySpec::Char('='), vec![], Action::VolumeUp, mode.clone()),
        bind(KeySpec::Char('-'), vec![], Action::VolumeDown, mode.clone()),
        bind(KeySpec::Char('m'), vec![], Action::ToggleMute, mode.clone()),
        // Library management
        bind(
            KeySpec::Char('f'),
            vec![],
            Action::RemoveLibrarySelection,
            mode.clone(),
        ),
        bind(
            KeySpec::Char('u'),
            vec![],
            Action::UndoRemoveLibrarySelection,
            mode.clone(),
        ),
        bind(
            KeySpec::Named(NamedKey::Tab),
            vec![],
            Action::NextGenre,
            mode.clone(),
        ),
        bind(
            KeySpec::Named(NamedKey::Tab),
            vec![Modifier::Shift],
            Action::PrevGenre,
            mode.clone(),
        ),
        // Library filter
        bind(
            KeySpec::Char('l'),
            vec![Modifier::Ctrl],
            Action::EnterLibraryFilter,
            mode.clone(),
        ),
        // Favorites
        bind(
            KeySpec::Char('*'),
            vec![],
            Action::ToggleFavorite,
            mode.clone(),
        ),
        // Number jump confirm
        bind(
            KeySpec::Char('G'),
            vec![],
            Action::NumberJumpConfirm,
            mode.clone(),
        ),
        // Help and overlays
        bind(KeySpec::Char('?'), vec![], Action::ToggleHelp, mode.clone()),
        bind(KeySpec::Char('h'), vec![], Action::ToggleHelp, mode.clone()),
        bind(
            KeySpec::Char('i'),
            vec![],
            Action::ToggleStationDetails,
            mode.clone(),
        ),
        bind(
            KeySpec::Char('g'),
            vec![],
            Action::ToggleRecentTracks,
            mode.clone(),
        ),
        bind(
            KeySpec::Char('d'),
            vec![],
            Action::TogglePlaybackDoctor,
            mode.clone(),
        ),
        // Layout and display
        bind(
            KeySpec::Char('b'),
            vec![],
            Action::CycleLayout,
            mode.clone(),
        ),
        bind(
            KeySpec::Char('v'),
            vec![],
            Action::ToggleVisualizerMode,
            mode.clone(),
        ),
        bind(
            KeySpec::Char(','),
            vec![],
            Action::ToggleSettings,
            mode.clone(),
        ),
        bind(
            KeySpec::Char('t'),
            vec![],
            Action::ToggleSleepTimer,
            mode.clone(),
        ),
        bind(
            KeySpec::Char('e'),
            vec![],
            Action::ExportLibrary,
            mode.clone(),
        ),
        bind(
            KeySpec::Function(6),
            vec![],
            Action::ToggleMiniMode,
            mode.clone(),
        ),
    ]
}

/// Search mode defaults extracted from `map_search` in event.rs.
pub fn search_mode_defaults() -> Vec<KeyBinding> {
    let mode = InputMode::Search;
    vec![
        bind(
            KeySpec::Named(NamedKey::Esc),
            vec![],
            Action::ExitSearch,
            mode.clone(),
        ),
        bind(
            KeySpec::Named(NamedKey::Enter),
            vec![Modifier::Ctrl],
            Action::SearchAudition,
            mode.clone(),
        ),
        bind(
            KeySpec::Char(' '),
            vec![],
            Action::SearchAudition,
            mode.clone(),
        ),
        bind(
            KeySpec::Named(NamedKey::Enter),
            vec![],
            Action::SearchConfirm,
            mode.clone(),
        ),
        bind(
            KeySpec::Named(NamedKey::Up),
            vec![],
            Action::SearchHistoryUp,
            mode.clone(),
        ),
        bind(
            KeySpec::Named(NamedKey::Down),
            vec![],
            Action::SearchHistoryDown,
            mode.clone(),
        ),
        bind(
            KeySpec::Named(NamedKey::Backspace),
            vec![],
            Action::SearchBackspace,
            mode.clone(),
        ),
        bind(
            KeySpec::Char('c'),
            vec![Modifier::Ctrl],
            Action::Quit,
            mode.clone(),
        ),
        // Modifier escape rails for audio controls
        bind(
            KeySpec::Char('-'),
            vec![Modifier::Ctrl],
            Action::VolumeDown,
            mode.clone(),
        ),
        bind(
            KeySpec::Char('-'),
            vec![Modifier::Alt],
            Action::VolumeDown,
            mode.clone(),
        ),
        bind(
            KeySpec::Char('+'),
            vec![Modifier::Ctrl],
            Action::VolumeUp,
            mode.clone(),
        ),
        bind(
            KeySpec::Char('+'),
            vec![Modifier::Alt],
            Action::VolumeUp,
            mode.clone(),
        ),
        bind(
            KeySpec::Char('='),
            vec![Modifier::Ctrl],
            Action::VolumeUp,
            mode.clone(),
        ),
        bind(
            KeySpec::Char('='),
            vec![Modifier::Alt],
            Action::VolumeUp,
            mode.clone(),
        ),
        bind(
            KeySpec::Char('m'),
            vec![Modifier::Ctrl],
            Action::ToggleMute,
            mode.clone(),
        ),
        bind(
            KeySpec::Char('m'),
            vec![Modifier::Alt],
            Action::ToggleMute,
            mode.clone(),
        ),
    ]
}

/// Command palette defaults extracted from `map_command_palette` in event.rs.
pub fn command_palette_defaults() -> Vec<KeyBinding> {
    let mode = InputMode::CommandPalette;
    vec![
        bind(
            KeySpec::Char('c'),
            vec![Modifier::Ctrl],
            Action::Quit,
            mode.clone(),
        ),
        bind(
            KeySpec::Named(NamedKey::Esc),
            vec![],
            Action::CommandPaletteClose,
            mode.clone(),
        ),
        bind(
            KeySpec::Named(NamedKey::Enter),
            vec![],
            Action::CommandPaletteConfirm,
            mode.clone(),
        ),
        bind(
            KeySpec::Named(NamedKey::Backspace),
            vec![],
            Action::CommandPaletteBackspace,
            mode.clone(),
        ),
        bind(
            KeySpec::Named(NamedKey::Up),
            vec![],
            Action::CommandPalettePrev,
            mode.clone(),
        ),
        bind(
            KeySpec::Named(NamedKey::Down),
            vec![],
            Action::CommandPaletteNext,
            mode.clone(),
        ),
    ]
}

/// Sleep timer defaults extracted from `map_sleep_timer` in event.rs.
pub fn sleep_timer_defaults() -> Vec<KeyBinding> {
    let mode = InputMode::SleepTimer;
    vec![
        bind(
            KeySpec::Char('c'),
            vec![Modifier::Ctrl],
            Action::Quit,
            mode.clone(),
        ),
        // Increase
        bind(
            KeySpec::Named(NamedKey::Up),
            vec![],
            Action::SleepTimerIncrease,
            mode.clone(),
        ),
        bind(
            KeySpec::Char('+'),
            vec![],
            Action::SleepTimerIncrease,
            mode.clone(),
        ),
        bind(
            KeySpec::Char('='),
            vec![],
            Action::SleepTimerIncrease,
            mode.clone(),
        ),
        // Decrease
        bind(
            KeySpec::Named(NamedKey::Down),
            vec![],
            Action::SleepTimerDecrease,
            mode.clone(),
        ),
        bind(
            KeySpec::Char('-'),
            vec![],
            Action::SleepTimerDecrease,
            mode.clone(),
        ),
        // Presets
        bind(
            KeySpec::Char('1'),
            vec![],
            Action::SleepTimerPreset(15),
            mode.clone(),
        ),
        bind(
            KeySpec::Char('2'),
            vec![],
            Action::SleepTimerPreset(30),
            mode.clone(),
        ),
        bind(
            KeySpec::Char('3'),
            vec![],
            Action::SleepTimerPreset(45),
            mode.clone(),
        ),
        bind(
            KeySpec::Char('4'),
            vec![],
            Action::SleepTimerPreset(60),
            mode.clone(),
        ),
        bind(
            KeySpec::Char('5'),
            vec![],
            Action::SleepTimerPreset(90),
            mode.clone(),
        ),
        bind(
            KeySpec::Char('6'),
            vec![],
            Action::SleepTimerPreset(120),
            mode.clone(),
        ),
        // Clear
        bind(
            KeySpec::Char('0'),
            vec![],
            Action::SleepTimerClear,
            mode.clone(),
        ),
        bind(
            KeySpec::Char('c'),
            vec![],
            Action::SleepTimerClear,
            mode.clone(),
        ),
        // Close overlay
        bind(
            KeySpec::Named(NamedKey::Esc),
            vec![],
            Action::ToggleSleepTimer,
            mode.clone(),
        ),
        bind(
            KeySpec::Named(NamedKey::Enter),
            vec![],
            Action::ToggleSleepTimer,
            mode.clone(),
        ),
        bind(
            KeySpec::Char('t'),
            vec![],
            Action::ToggleSleepTimer,
            mode.clone(),
        ),
        bind(
            KeySpec::Char('q'),
            vec![],
            Action::ToggleSleepTimer,
            mode.clone(),
        ),
    ]
}

/// Library filter defaults extracted from `map_library_filter` in event.rs.
pub fn library_filter_defaults() -> Vec<KeyBinding> {
    let mode = InputMode::LibraryFilter;
    vec![
        bind(
            KeySpec::Char('c'),
            vec![Modifier::Ctrl],
            Action::Quit,
            mode.clone(),
        ),
        bind(
            KeySpec::Named(NamedKey::Esc),
            vec![],
            Action::ExitLibraryFilter,
            mode.clone(),
        ),
        bind(
            KeySpec::Function(6),
            vec![],
            Action::ToggleMiniMode,
            mode.clone(),
        ),
        bind(
            KeySpec::Named(NamedKey::Enter),
            vec![],
            Action::LibraryFilterConfirm,
            mode.clone(),
        ),
        bind(
            KeySpec::Named(NamedKey::Backspace),
            vec![],
            Action::LibraryFilterBackspace,
            mode.clone(),
        ),
        bind(
            KeySpec::Named(NamedKey::Up),
            vec![],
            Action::PrevStation,
            mode.clone(),
        ),
        bind(
            KeySpec::Named(NamedKey::Down),
            vec![],
            Action::NextStation,
            mode.clone(),
        ),
        bind(
            KeySpec::Char('k'),
            vec![],
            Action::PrevStation,
            mode.clone(),
        ),
        bind(
            KeySpec::Char('j'),
            vec![],
            Action::NextStation,
            mode.clone(),
        ),
    ]
}

/// Helper to construct a KeyBinding concisely.
fn bind(key: KeySpec, modifiers: Vec<Modifier>, action: Action, mode: InputMode) -> KeyBinding {
    KeyBinding {
        key,
        modifiers,
        action,
        mode,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_bindings_is_non_empty() {
        let bindings = default_bindings();
        assert!(!bindings.is_empty());
    }

    #[test]
    fn test_all_modes_have_bindings() {
        let bindings = default_bindings();
        let has_normal = bindings.iter().any(|b| b.mode == InputMode::Normal);
        let has_search = bindings.iter().any(|b| b.mode == InputMode::Search);
        let has_command = bindings.iter().any(|b| b.mode == InputMode::CommandPalette);
        let has_sleep = bindings.iter().any(|b| b.mode == InputMode::SleepTimer);
        let has_filter = bindings.iter().any(|b| b.mode == InputMode::LibraryFilter);
        assert!(has_normal, "missing Normal mode bindings");
        assert!(has_search, "missing Search mode bindings");
        assert!(has_command, "missing CommandPalette mode bindings");
        assert!(has_sleep, "missing SleepTimer mode bindings");
        assert!(has_filter, "missing LibraryFilter mode bindings");
    }

    #[test]
    fn test_known_binding_q_quits_in_normal() {
        let bindings = normal_mode_defaults();
        let quit_q = bindings.iter().find(|b| {
            b.key == KeySpec::Char('q') && b.modifiers.is_empty() && b.action == Action::Quit
        });
        assert!(quit_q.is_some(), "'q' should map to Quit in Normal mode");
    }

    #[test]
    fn test_known_binding_esc_exits_search() {
        let bindings = search_mode_defaults();
        let esc_exit = bindings.iter().find(|b| {
            b.key == KeySpec::Named(NamedKey::Esc)
                && b.modifiers.is_empty()
                && b.action == Action::ExitSearch
        });
        assert!(
            esc_exit.is_some(),
            "Esc should map to ExitSearch in Search mode"
        );
    }

    #[test]
    fn test_known_binding_enter_confirms_command_palette() {
        let bindings = command_palette_defaults();
        let enter = bindings.iter().find(|b| {
            b.key == KeySpec::Named(NamedKey::Enter)
                && b.modifiers.is_empty()
                && b.action == Action::CommandPaletteConfirm
        });
        assert!(
            enter.is_some(),
            "Enter should confirm in CommandPalette mode"
        );
    }

    #[test]
    fn test_known_binding_sleep_timer_preset() {
        let bindings = sleep_timer_defaults();
        let preset_2 = bindings
            .iter()
            .find(|b| b.key == KeySpec::Char('2') && b.action == Action::SleepTimerPreset(30));
        assert!(preset_2.is_some(), "'2' should map to SleepTimerPreset(30)");
    }

    #[test]
    fn test_known_binding_library_filter_esc_exits() {
        let bindings = library_filter_defaults();
        let esc = bindings.iter().find(|b| {
            b.key == KeySpec::Named(NamedKey::Esc)
                && b.modifiers.is_empty()
                && b.action == Action::ExitLibraryFilter
        });
        assert!(esc.is_some(), "Esc should exit LibraryFilter mode");
    }
}
