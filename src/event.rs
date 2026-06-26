use crate::action::Action;
use crate::app::{DisplayMode, InputMode};
use crate::keybindings::{KeySpec, KeybindingRegistry, Modifier, NamedKey};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use std::time::Duration;

/// Poll for terminal events, resolving via the keybinding registry.
/// All specific bindings live in the registry; only mode-specific "any char"
/// catch-alls (SearchInput, CommandPaletteInput, etc.) use a fallback path.
pub fn poll_action_with_registry(
    timeout: Duration,
    mode: &InputMode,
    display_mode: &DisplayMode,
    registry: &KeybindingRegistry,
) -> Option<Action> {
    if event::poll(timeout).ok()? {
        if let Event::Key(key) = event::read().ok()? {
            return map_key_with_registry(key, mode, display_mode, registry);
        }
    }
    None
}

/// Map a key event: registry first, then mode-specific char input fallback.
fn map_key_with_registry(
    key: KeyEvent,
    mode: &InputMode,
    display_mode: &DisplayMode,
    registry: &KeybindingRegistry,
) -> Option<Action> {
    if key.kind != crossterm::event::KeyEventKind::Press {
        return None;
    }

    // Mini display mode has its own restricted table (not registry-based).
    if *display_mode == DisplayMode::Mini && *mode == InputMode::Normal {
        return map_mini_mode_key(key);
    }

    if let Some(action) = resolve_from_registry(key, mode, registry) {
        return Some(action);
    }

    char_input_fallback(key, mode)
}

/// Fallback for mode-specific "any char" catch-alls that cannot be expressed
/// as individual KeyBindings (they match ANY printable character).
fn char_input_fallback(key: KeyEvent, mode: &InputMode) -> Option<Action> {
    let KeyCode::Char(c) = key.code else {
        return None;
    };

    match mode {
        InputMode::Search => {
            // Only plain chars become search input; modified chars are not text.
            if !has_search_escape_modifier(key.modifiers) {
                Some(Action::SearchInput(c))
            } else {
                None
            }
        }
        InputMode::CommandPalette => Some(Action::CommandPaletteInput(c)),
        InputMode::LibraryFilter => Some(Action::LibraryFilterInput(c)),
        InputMode::Normal => {
            // NumberJumpDigit for plain 0-9 (no Ctrl/Alt).
            if c.is_ascii_digit()
                && !key.modifiers.contains(KeyModifiers::CONTROL)
                && !key.modifiers.contains(KeyModifiers::ALT)
            {
                Some(Action::NumberJumpDigit(c))
            } else {
                None
            }
        }
        InputMode::SleepTimer => None,
    }
}

/// Map a key event to an Action based on current input mode and display mode.
/// Uses the old hardcoded tables — kept for property test comparison (task 3.3).
#[cfg(test)]
fn map_key(key: KeyEvent, mode: &InputMode, display_mode: &DisplayMode) -> Option<Action> {
    if key.kind != crossterm::event::KeyEventKind::Press {
        return None;
    }

    map_key_inner(key, mode, display_mode)
}

/// Core key mapping via hardcoded tables — retained as private test helper
/// for behavioral-equivalence property tests.
#[cfg(test)]
fn map_key_inner(key: KeyEvent, mode: &InputMode, display_mode: &DisplayMode) -> Option<Action> {
    if *display_mode == DisplayMode::Mini && *mode == InputMode::Normal {
        return map_mini_mode_key(key);
    }

    match mode {
        InputMode::Normal => map_normal(key),
        InputMode::Search => map_search(key),
        InputMode::CommandPalette => map_command_palette(key),
        InputMode::SleepTimer => map_sleep_timer(key),
        InputMode::LibraryFilter => map_library_filter(key),
    }
}

/// Attempt to resolve a key event via the keybinding registry.
fn resolve_from_registry(
    key: KeyEvent,
    mode: &InputMode,
    registry: &KeybindingRegistry,
) -> Option<Action> {
    let key_spec = key_code_to_spec(key.code)?;
    let modifiers = key_modifiers_to_vec(key.modifiers);
    registry.resolve(&key_spec, &modifiers, mode)
}

/// Convert crossterm KeyCode to keybinding KeySpec.
fn key_code_to_spec(code: KeyCode) -> Option<KeySpec> {
    match code {
        KeyCode::Char(c) => Some(KeySpec::Char(c)),
        KeyCode::F(n) => Some(KeySpec::Function(n)),
        KeyCode::Enter => Some(KeySpec::Named(NamedKey::Enter)),
        KeyCode::Esc => Some(KeySpec::Named(NamedKey::Esc)),
        KeyCode::Up => Some(KeySpec::Named(NamedKey::Up)),
        KeyCode::Down => Some(KeySpec::Named(NamedKey::Down)),
        KeyCode::Left => Some(KeySpec::Named(NamedKey::Left)),
        KeyCode::Right => Some(KeySpec::Named(NamedKey::Right)),
        KeyCode::Tab => Some(KeySpec::Named(NamedKey::Tab)),
        KeyCode::Backspace => Some(KeySpec::Named(NamedKey::Backspace)),
        KeyCode::Home => Some(KeySpec::Named(NamedKey::Home)),
        KeyCode::End => Some(KeySpec::Named(NamedKey::End)),
        KeyCode::PageUp => Some(KeySpec::Named(NamedKey::PageUp)),
        KeyCode::PageDown => Some(KeySpec::Named(NamedKey::PageDown)),
        KeyCode::Delete => Some(KeySpec::Named(NamedKey::Delete)),
        KeyCode::Insert => Some(KeySpec::Named(NamedKey::Insert)),
        KeyCode::BackTab => Some(KeySpec::Named(NamedKey::Tab)),
        _ => None,
    }
}

/// Convert crossterm KeyModifiers bitflags to a Vec<Modifier>.
fn key_modifiers_to_vec(modifiers: KeyModifiers) -> Vec<Modifier> {
    let mut result = Vec::new();
    if modifiers.contains(KeyModifiers::CONTROL) {
        result.push(Modifier::Ctrl);
    }
    if modifiers.contains(KeyModifiers::ALT) {
        result.push(Modifier::Alt);
    }
    if modifiers.contains(KeyModifiers::SHIFT) {
        result.push(Modifier::Shift);
    }
    result
}

/// Map a key event to an Action when in Mini display mode.
/// Only a restricted set of playback controls are available.
fn map_mini_mode_key(key: KeyEvent) -> Option<Action> {
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

/// Key mapping for normal mode.
#[cfg(test)]
fn map_normal(key: KeyEvent) -> Option<Action> {
    match (key.modifiers, key.code) {
        // Quit
        (_, KeyCode::Char('q')) => Some(Action::Quit),
        (_, KeyCode::Esc) => Some(Action::Quit),
        (mods, KeyCode::Char('c')) if mods.contains(KeyModifiers::CONTROL) => Some(Action::Quit),

        // Search
        (_, KeyCode::Char('/')) => Some(Action::EnterSearch),
        (_, KeyCode::F(3)) => Some(Action::EnterSearch),
        (mods, KeyCode::Char('f')) if mods.contains(KeyModifiers::CONTROL) => {
            Some(Action::EnterSearch)
        }

        // Command palette
        (_, KeyCode::Char(':')) => Some(Action::OpenCommandPalette),
        (mods, KeyCode::Char('p')) if mods.contains(KeyModifiers::CONTROL) => {
            Some(Action::OpenCommandPalette)
        }

        // Navigation
        (_, KeyCode::Up) | (_, KeyCode::Char('k')) => Some(Action::PrevStation),
        (_, KeyCode::Down) | (_, KeyCode::Char('j')) => Some(Action::NextStation),
        (mods, KeyCode::Char('l')) if !mods.contains(KeyModifiers::CONTROL) => {
            Some(Action::StepSettingForward)
        }
        (_, KeyCode::Right) => Some(Action::StepSettingForward),
        (_, KeyCode::Left) | (_, KeyCode::Char('a')) => Some(Action::StepSettingBackward),

        // Playback
        (_, KeyCode::Enter) => Some(Action::PlaySelected),
        (_, KeyCode::Char(' ')) => Some(Action::TogglePause),
        (_, KeyCode::Char('s')) => Some(Action::Stop),
        (mods, KeyCode::Char('r')) if allows_normal_shortcut_modifier(mods) => {
            Some(Action::RetryStream)
        }

        // Volume
        (_, KeyCode::Char('+')) | (_, KeyCode::Char('=')) => Some(Action::VolumeUp),
        (_, KeyCode::Char('-')) => Some(Action::VolumeDown),
        (_, KeyCode::Char('m')) => Some(Action::ToggleMute),

        // Library management
        (_, KeyCode::Char('f')) => Some(Action::RemoveLibrarySelection),
        (_, KeyCode::Char('u')) => Some(Action::UndoRemoveLibrarySelection),
        (_, KeyCode::Tab) => Some(Action::NextGenre),
        (_, KeyCode::BackTab) => Some(Action::PrevGenre),

        // Library filter
        (mods, KeyCode::Char('l')) if mods.contains(KeyModifiers::CONTROL) => {
            Some(Action::EnterLibraryFilter)
        }

        // Station preset slots: Alt+1–5 plays, Ctrl+1–5 assigns
        (mods, KeyCode::Char(c @ '1'..='5')) if mods.contains(KeyModifiers::ALT) => {
            Some(Action::PlaySlot(c as u8 - b'0'))
        }
        (mods, KeyCode::Char(c @ '1'..='5'))
            if mods.contains(KeyModifiers::CONTROL) && !mods.contains(KeyModifiers::ALT) =>
        {
            Some(Action::AssignSlot(c as u8 - b'0'))
        }

        // Favorites toggle
        (_, KeyCode::Char('*')) => Some(Action::ToggleFavorite),

        // Number jump: digit keys 0-9
        (mods, KeyCode::Char(c @ '0'..='9'))
            if !mods.contains(KeyModifiers::CONTROL) && !mods.contains(KeyModifiers::ALT) =>
        {
            Some(Action::NumberJumpDigit(c))
        }

        // Number jump confirm (uppercase G)
        (mods, KeyCode::Char('G')) if !mods.contains(KeyModifiers::CONTROL) => {
            Some(Action::NumberJumpConfirm)
        }

        // Help and context overlays
        (_, KeyCode::Char('?')) | (_, KeyCode::Char('h')) => Some(Action::ToggleHelp),
        (mods, KeyCode::Char('i') | KeyCode::Char('I'))
            if allows_normal_shortcut_modifier(mods) =>
        {
            Some(Action::ToggleStationDetails)
        }
        (mods, KeyCode::Char('g')) if allows_normal_shortcut_modifier(mods) => {
            Some(Action::ToggleRecentTracks)
        }
        (mods, KeyCode::Char('d') | KeyCode::Char('D'))
            if allows_normal_shortcut_modifier(mods) =>
        {
            Some(Action::TogglePlaybackDoctor)
        }

        // Bento layout cycle
        (_, KeyCode::Char('b')) => Some(Action::CycleLayout),

        // Visualizer mode toggle
        (_, KeyCode::Char('v')) => Some(Action::ToggleVisualizerMode),

        // Settings overlay
        (_, KeyCode::Char(',')) => Some(Action::ToggleSettings),

        // Sleep timer
        (_, KeyCode::Char('t')) => Some(Action::ToggleSleepTimer),

        // Export library
        (_, KeyCode::Char('e')) => Some(Action::ExportLibrary),

        // Mini mode toggle
        (_, KeyCode::F(6)) => Some(Action::ToggleMiniMode),

        _ => None,
    }
}

/// Some terminals report harmless modifier bits for printable shortcut keys.
/// Keep Ctrl reserved so legacy Ctrl+r stays unmapped and Ctrl+c still quits.
#[cfg(test)]
fn allows_normal_shortcut_modifier(modifiers: KeyModifiers) -> bool {
    !modifiers.contains(KeyModifiers::CONTROL)
}

/// Key mapping for search mode.
/// Printable characters remain search input, except Space auditions highlighted results.
#[cfg(test)]
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

        // Navigate within filtered results / cycle history
        (_, KeyCode::Up) => Some(Action::SearchHistoryUp),
        (_, KeyCode::Down) => Some(Action::SearchHistoryDown),

        // Delete character
        (_, KeyCode::Backspace) => Some(Action::SearchBackspace),

        // Ctrl+C still quits
        (mods, KeyCode::Char('c')) if mods.contains(KeyModifiers::CONTROL) => Some(Action::Quit),

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

fn has_search_escape_modifier(modifiers: KeyModifiers) -> bool {
    modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
}

#[cfg(test)]
fn map_command_palette(key: KeyEvent) -> Option<Action> {
    match (key.modifiers, key.code) {
        (mods, KeyCode::Char('c')) if mods.contains(KeyModifiers::CONTROL) => Some(Action::Quit),
        (_, KeyCode::Esc) => Some(Action::CommandPaletteClose),
        (_, KeyCode::Enter) => Some(Action::CommandPaletteConfirm),
        (_, KeyCode::Backspace) => Some(Action::CommandPaletteBackspace),
        (_, KeyCode::Up) => Some(Action::CommandPalettePrev),
        (_, KeyCode::Down) => Some(Action::CommandPaletteNext),
        (_, KeyCode::Char(c)) => Some(Action::CommandPaletteInput(c)),
        _ => None,
    }
}

/// Key mapping for library filter mode.
///
/// Fully isolated table: only reached when `InputMode::LibraryFilter` is active.
/// Supports text input for filtering, navigation within filtered results, and
/// confirm/exit actions.
#[cfg(test)]
fn map_library_filter(key: KeyEvent) -> Option<Action> {
    match (key.modifiers, key.code) {
        // Global quit still works from filter mode.
        (mods, KeyCode::Char('c')) if mods.contains(KeyModifiers::CONTROL) => Some(Action::Quit),

        // Exit filter mode, restore previous state.
        (_, KeyCode::Esc) => Some(Action::ExitLibraryFilter),

        // F6: exit filter and toggle mini mode.
        (_, KeyCode::F(6)) => Some(Action::ToggleMiniMode),

        // Confirm: play selected station and exit filter mode.
        (_, KeyCode::Enter) => Some(Action::LibraryFilterConfirm),

        // Delete last character from query.
        (_, KeyCode::Backspace) => Some(Action::LibraryFilterBackspace),

        // Navigate within filtered results (clamped, no wrap).
        (_, KeyCode::Up) => Some(Action::PrevStation),
        (_, KeyCode::Down) => Some(Action::NextStation),

        // Printable characters: j/k also navigate, all others are filter input.
        (_, KeyCode::Char('k')) => Some(Action::PrevStation),
        (_, KeyCode::Char('j')) => Some(Action::NextStation),
        (_, KeyCode::Char(c)) => Some(Action::LibraryFilterInput(c)),

        _ => None,
    }
}

/// Key mapping for the sleep-timer overlay.
///
/// This table is fully isolated: it is only reached when `InputMode::SleepTimer`
/// is active, so none of these keys can shadow or conflict with Normal/Search
/// bindings. Only `Ctrl+C` escapes to a global quit; no other Ctrl/Alt combos
/// are used, avoiding terminal-reserved sequences (XON/XOFF, SIGTSTP, ...).
#[cfg(test)]
fn map_sleep_timer(key: KeyEvent) -> Option<Action> {
    match (key.modifiers, key.code) {
        // Global quit still works from the overlay.
        (mods, KeyCode::Char('c')) if mods.contains(KeyModifiers::CONTROL) => Some(Action::Quit),

        // Fine adjust by a fixed 5-minute step.
        (_, KeyCode::Up) | (_, KeyCode::Char('+')) | (_, KeyCode::Char('=')) => {
            Some(Action::SleepTimerIncrease)
        }
        (_, KeyCode::Down) | (_, KeyCode::Char('-')) => Some(Action::SleepTimerDecrease),

        // Quick presets (minutes).
        (_, KeyCode::Char('1')) => Some(Action::SleepTimerPreset(15)),
        (_, KeyCode::Char('2')) => Some(Action::SleepTimerPreset(30)),
        (_, KeyCode::Char('3')) => Some(Action::SleepTimerPreset(45)),
        (_, KeyCode::Char('4')) => Some(Action::SleepTimerPreset(60)),
        (_, KeyCode::Char('5')) => Some(Action::SleepTimerPreset(90)),
        (_, KeyCode::Char('6')) => Some(Action::SleepTimerPreset(120)),

        // Turn the timer off without leaving the overlay.
        (_, KeyCode::Char('0')) | (_, KeyCode::Char('c')) => Some(Action::SleepTimerClear),

        // Close the overlay (changes already applied live).
        (_, KeyCode::Esc)
        | (_, KeyCode::Enter)
        | (_, KeyCode::Char('t'))
        | (_, KeyCode::Char('q')) => Some(Action::ToggleSleepTimer),

        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NORMAL_DISPLAY: DisplayMode = DisplayMode::Normal;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn modified_key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    #[test]
    fn sleep_timer_mode_maps_adjust_presets_and_close() {
        assert_eq!(
            map_key(key(KeyCode::Up), &InputMode::SleepTimer, &NORMAL_DISPLAY),
            Some(Action::SleepTimerIncrease)
        );
        assert_eq!(
            map_key(
                key(KeyCode::Char('+')),
                &InputMode::SleepTimer,
                &NORMAL_DISPLAY
            ),
            Some(Action::SleepTimerIncrease)
        );
        assert_eq!(
            map_key(key(KeyCode::Down), &InputMode::SleepTimer, &NORMAL_DISPLAY),
            Some(Action::SleepTimerDecrease)
        );
        assert_eq!(
            map_key(
                key(KeyCode::Char('-')),
                &InputMode::SleepTimer,
                &NORMAL_DISPLAY
            ),
            Some(Action::SleepTimerDecrease)
        );
        assert_eq!(
            map_key(
                key(KeyCode::Char('2')),
                &InputMode::SleepTimer,
                &NORMAL_DISPLAY
            ),
            Some(Action::SleepTimerPreset(30))
        );
        assert_eq!(
            map_key(
                key(KeyCode::Char('0')),
                &InputMode::SleepTimer,
                &NORMAL_DISPLAY
            ),
            Some(Action::SleepTimerClear)
        );
        assert_eq!(
            map_key(
                key(KeyCode::Char('c')),
                &InputMode::SleepTimer,
                &NORMAL_DISPLAY
            ),
            Some(Action::SleepTimerClear)
        );
        assert_eq!(
            map_key(key(KeyCode::Esc), &InputMode::SleepTimer, &NORMAL_DISPLAY),
            Some(Action::ToggleSleepTimer)
        );
        assert_eq!(
            map_key(
                key(KeyCode::Char('t')),
                &InputMode::SleepTimer,
                &NORMAL_DISPLAY
            ),
            Some(Action::ToggleSleepTimer)
        );
    }

    #[test]
    fn sleep_timer_mode_ctrl_c_quits() {
        assert_eq!(
            map_key(
                modified_key(KeyCode::Char('c'), KeyModifiers::CONTROL),
                &InputMode::SleepTimer,
                &NORMAL_DISPLAY
            ),
            Some(Action::Quit)
        );
    }

    #[test]
    fn search_mode_treats_plain_f_as_text_input() {
        assert_eq!(
            map_key(key(KeyCode::Char('f')), &InputMode::Search, &NORMAL_DISPLAY),
            Some(Action::SearchInput('f'))
        );
    }

    #[test]
    fn search_mode_treats_plain_a_as_text_input() {
        assert_eq!(
            map_key(key(KeyCode::Char('a')), &InputMode::Search, &NORMAL_DISPLAY),
            Some(Action::SearchInput('a'))
        );
    }

    #[test]
    fn search_mode_treats_plain_u_as_text_input() {
        assert_eq!(
            map_key(key(KeyCode::Char('u')), &InputMode::Search, &NORMAL_DISPLAY),
            Some(Action::SearchInput('u'))
        );
    }

    #[test]
    fn search_mode_treats_context_overlay_keys_as_text_input() {
        assert_eq!(
            map_key(key(KeyCode::Char('i')), &InputMode::Search, &NORMAL_DISPLAY),
            Some(Action::SearchInput('i'))
        );
        assert_eq!(
            map_key(key(KeyCode::Char('g')), &InputMode::Search, &NORMAL_DISPLAY),
            Some(Action::SearchInput('g'))
        );
        assert_eq!(
            map_key(key(KeyCode::Char('r')), &InputMode::Search, &NORMAL_DISPLAY),
            Some(Action::SearchInput('r'))
        );
    }

    #[test]
    fn search_mode_f2_does_not_add_selected_result() {
        assert_eq!(
            map_key(key(KeyCode::F(2)), &InputMode::Search, &NORMAL_DISPLAY),
            None
        );
    }

    #[test]
    fn search_mode_insert_does_not_add_selected_result() {
        assert_eq!(
            map_key(key(KeyCode::Insert), &InputMode::Search, &NORMAL_DISPLAY),
            None
        );
    }

    #[test]
    fn search_mode_plain_audio_keys_remain_text_input() {
        assert_eq!(
            map_key(key(KeyCode::Char('m')), &InputMode::Search, &NORMAL_DISPLAY),
            Some(Action::SearchInput('m'))
        );
        assert_eq!(
            map_key(key(KeyCode::Char('-')), &InputMode::Search, &NORMAL_DISPLAY),
            Some(Action::SearchInput('-'))
        );
        assert_eq!(
            map_key(key(KeyCode::Char('=')), &InputMode::Search, &NORMAL_DISPLAY),
            Some(Action::SearchInput('='))
        );
        assert_eq!(
            map_key(key(KeyCode::Char('+')), &InputMode::Search, &NORMAL_DISPLAY),
            Some(Action::SearchInput('+'))
        );
    }

    #[test]
    fn search_mode_ctrl_audio_keys_bypass_text_input() {
        assert_eq!(
            map_key(
                modified_key(KeyCode::Char('-'), KeyModifiers::CONTROL),
                &InputMode::Search,
                &NORMAL_DISPLAY
            ),
            Some(Action::VolumeDown)
        );
        assert_eq!(
            map_key(
                modified_key(KeyCode::Char('='), KeyModifiers::CONTROL),
                &InputMode::Search,
                &NORMAL_DISPLAY
            ),
            Some(Action::VolumeUp)
        );
        assert_eq!(
            map_key(
                modified_key(KeyCode::Char('+'), KeyModifiers::CONTROL),
                &InputMode::Search,
                &NORMAL_DISPLAY
            ),
            Some(Action::VolumeUp)
        );
        assert_eq!(
            map_key(
                modified_key(KeyCode::Char('m'), KeyModifiers::CONTROL),
                &InputMode::Search,
                &NORMAL_DISPLAY
            ),
            Some(Action::ToggleMute)
        );
    }

    #[test]
    fn search_mode_alt_audio_keys_bypass_text_input() {
        assert_eq!(
            map_key(
                modified_key(KeyCode::Char('-'), KeyModifiers::ALT),
                &InputMode::Search,
                &NORMAL_DISPLAY
            ),
            Some(Action::VolumeDown)
        );
        assert_eq!(
            map_key(
                modified_key(KeyCode::Char('='), KeyModifiers::ALT),
                &InputMode::Search,
                &NORMAL_DISPLAY
            ),
            Some(Action::VolumeUp)
        );
        assert_eq!(
            map_key(
                modified_key(KeyCode::Char('m'), KeyModifiers::ALT),
                &InputMode::Search,
                &NORMAL_DISPLAY
            ),
            Some(Action::ToggleMute)
        );
    }

    #[test]
    fn normal_mode_search_shortcuts_enter_search() {
        assert_eq!(
            map_key(key(KeyCode::Char('/')), &InputMode::Normal, &NORMAL_DISPLAY),
            Some(Action::EnterSearch)
        );
        assert_eq!(
            map_key(key(KeyCode::F(3)), &InputMode::Normal, &NORMAL_DISPLAY),
            Some(Action::EnterSearch)
        );
        assert_eq!(
            map_key(
                modified_key(KeyCode::Char('f'), KeyModifiers::CONTROL),
                &InputMode::Normal,
                &NORMAL_DISPLAY
            ),
            Some(Action::EnterSearch)
        );
    }

    #[test]
    fn normal_mode_command_palette_shortcuts_open_palette() {
        assert_eq!(
            map_key(key(KeyCode::Char(':')), &InputMode::Normal, &NORMAL_DISPLAY),
            Some(Action::OpenCommandPalette)
        );
        assert_eq!(
            map_key(
                modified_key(KeyCode::Char('p'), KeyModifiers::CONTROL),
                &InputMode::Normal,
                &NORMAL_DISPLAY
            ),
            Some(Action::OpenCommandPalette)
        );
    }

    #[test]
    fn command_palette_mode_maps_text_navigation_and_close() {
        assert_eq!(
            map_key(
                key(KeyCode::Char('s')),
                &InputMode::CommandPalette,
                &NORMAL_DISPLAY
            ),
            Some(Action::CommandPaletteInput('s'))
        );
        assert_eq!(
            map_key(
                key(KeyCode::Down),
                &InputMode::CommandPalette,
                &NORMAL_DISPLAY
            ),
            Some(Action::CommandPaletteNext)
        );
        assert_eq!(
            map_key(
                key(KeyCode::Up),
                &InputMode::CommandPalette,
                &NORMAL_DISPLAY
            ),
            Some(Action::CommandPalettePrev)
        );
        assert_eq!(
            map_key(
                key(KeyCode::Esc),
                &InputMode::CommandPalette,
                &NORMAL_DISPLAY
            ),
            Some(Action::CommandPaletteClose)
        );
    }

    #[test]
    fn normal_mode_right_steps_setting_forward() {
        assert_eq!(
            map_key(key(KeyCode::Right), &InputMode::Normal, &NORMAL_DISPLAY),
            Some(Action::StepSettingForward)
        );
    }
    #[test]
    fn normal_mode_left_steps_setting_backward() {
        assert_eq!(
            map_key(key(KeyCode::Left), &InputMode::Normal, &NORMAL_DISPLAY),
            Some(Action::StepSettingBackward)
        );
    }
    #[test]
    fn normal_mode_l_steps_setting_forward() {
        assert_eq!(
            map_key(key(KeyCode::Char('l')), &InputMode::Normal, &NORMAL_DISPLAY),
            Some(Action::StepSettingForward)
        );
    }
    #[test]
    fn normal_mode_a_steps_setting_backward() {
        assert_eq!(
            map_key(key(KeyCode::Char('a')), &InputMode::Normal, &NORMAL_DISPLAY),
            Some(Action::StepSettingBackward)
        );
    }
    #[test]
    fn normal_mode_f_removes_library_selection() {
        assert_eq!(
            map_key(key(KeyCode::Char('f')), &InputMode::Normal, &NORMAL_DISPLAY),
            Some(Action::RemoveLibrarySelection)
        );
    }
    #[test]
    fn normal_mode_u_undoes_library_removal() {
        assert_eq!(
            map_key(key(KeyCode::Char('u')), &InputMode::Normal, &NORMAL_DISPLAY),
            Some(Action::UndoRemoveLibrarySelection)
        );
    }

    #[test]
    fn normal_mode_context_shortcuts_open_overlays_and_retry() {
        assert_eq!(
            map_key(key(KeyCode::Char('i')), &InputMode::Normal, &NORMAL_DISPLAY),
            Some(Action::ToggleStationDetails)
        );
        assert_eq!(
            map_key(key(KeyCode::Char('g')), &InputMode::Normal, &NORMAL_DISPLAY),
            Some(Action::ToggleRecentTracks)
        );
        assert_eq!(
            map_key(key(KeyCode::Char('r')), &InputMode::Normal, &NORMAL_DISPLAY),
            Some(Action::RetryStream)
        );
        assert_eq!(
            map_key(key(KeyCode::Char('d')), &InputMode::Normal, &NORMAL_DISPLAY),
            Some(Action::TogglePlaybackDoctor)
        );
    }

    #[test]
    fn normal_mode_context_shortcuts_tolerate_non_control_modifiers() {
        assert_eq!(
            map_key(
                modified_key(KeyCode::Char('I'), KeyModifiers::SHIFT),
                &InputMode::Normal,
                &NORMAL_DISPLAY
            ),
            Some(Action::ToggleStationDetails)
        );
        assert_eq!(
            map_key(
                modified_key(KeyCode::Char('r'), KeyModifiers::ALT),
                &InputMode::Normal,
                &NORMAL_DISPLAY
            ),
            Some(Action::RetryStream)
        );
        assert_eq!(
            map_key(
                modified_key(KeyCode::Char('D'), KeyModifiers::SHIFT),
                &InputMode::Normal,
                &NORMAL_DISPLAY
            ),
            Some(Action::TogglePlaybackDoctor)
        );
    }

    #[test]
    fn normal_mode_context_shortcuts_do_not_capture_control_combos() {
        assert_eq!(
            map_key(
                modified_key(KeyCode::Char('i'), KeyModifiers::CONTROL),
                &InputMode::Normal,
                &NORMAL_DISPLAY
            ),
            None
        );
        assert_eq!(
            map_key(
                modified_key(KeyCode::Char('g'), KeyModifiers::CONTROL),
                &InputMode::Normal,
                &NORMAL_DISPLAY
            ),
            None
        );
        assert_eq!(
            map_key(
                modified_key(KeyCode::Char('r'), KeyModifiers::CONTROL),
                &InputMode::Normal,
                &NORMAL_DISPLAY
            ),
            None
        );
        assert_eq!(
            map_key(
                modified_key(KeyCode::Char('d'), KeyModifiers::CONTROL),
                &InputMode::Normal,
                &NORMAL_DISPLAY
            ),
            None
        );
    }

    #[test]
    fn search_mode_space_auditions_selected_result() {
        assert_eq!(
            map_key(key(KeyCode::Char(' ')), &InputMode::Search, &NORMAL_DISPLAY),
            Some(Action::SearchAudition)
        );
    }
    #[test]
    fn search_mode_ctrl_enter_auditions_selected_result_when_supported() {
        assert_eq!(
            map_key(
                modified_key(KeyCode::Enter, KeyModifiers::CONTROL),
                &InputMode::Search,
                &NORMAL_DISPLAY
            ),
            Some(Action::SearchAudition)
        );
    }
    #[test]
    fn search_mode_enter_adds_and_plays_selected_result() {
        assert_eq!(
            map_key(key(KeyCode::Enter), &InputMode::Search, &NORMAL_DISPLAY),
            Some(Action::SearchConfirm)
        );
    }

    #[test]
    fn normal_mode_removed_legacy_keys_are_unmapped() {
        let removed_plain_keys = [
            KeyCode::Char('p'),
            KeyCode::Char('o'),
            KeyCode::Char('R'),
            KeyCode::Char('M'),
            KeyCode::Char('y'),
            KeyCode::Char('n'),
            KeyCode::Char('K'),
            KeyCode::Char('T'),
            KeyCode::Delete,
        ];
        for code in removed_plain_keys {
            assert_eq!(
                map_key(key(code), &InputMode::Normal, &NORMAL_DISPLAY),
                None
            );
        }
        assert_eq!(
            map_key(
                modified_key(KeyCode::Char('r'), KeyModifiers::CONTROL),
                &InputMode::Normal,
                &NORMAL_DISPLAY
            ),
            None
        );
    }

    #[test]
    fn test_normal_mode_t_and_e_bindings() {
        assert_eq!(
            map_key(key(KeyCode::Char('t')), &InputMode::Normal, &NORMAL_DISPLAY),
            Some(Action::ToggleSleepTimer)
        );
        assert_eq!(
            map_key(key(KeyCode::Char('e')), &InputMode::Normal, &NORMAL_DISPLAY),
            Some(Action::ExportLibrary)
        );
    }

    #[test]
    fn library_filter_mode_esc_exits() {
        assert_eq!(
            map_key(
                key(KeyCode::Esc),
                &InputMode::LibraryFilter,
                &NORMAL_DISPLAY
            ),
            Some(Action::ExitLibraryFilter)
        );
    }
    #[test]
    fn library_filter_mode_enter_confirms() {
        assert_eq!(
            map_key(
                key(KeyCode::Enter),
                &InputMode::LibraryFilter,
                &NORMAL_DISPLAY
            ),
            Some(Action::LibraryFilterConfirm)
        );
    }
    #[test]
    fn library_filter_mode_backspace_deletes() {
        assert_eq!(
            map_key(
                key(KeyCode::Backspace),
                &InputMode::LibraryFilter,
                &NORMAL_DISPLAY
            ),
            Some(Action::LibraryFilterBackspace)
        );
    }

    #[test]
    fn library_filter_mode_printable_chars_are_input() {
        assert_eq!(
            map_key(
                key(KeyCode::Char('a')),
                &InputMode::LibraryFilter,
                &NORMAL_DISPLAY
            ),
            Some(Action::LibraryFilterInput('a'))
        );
        assert_eq!(
            map_key(
                key(KeyCode::Char('z')),
                &InputMode::LibraryFilter,
                &NORMAL_DISPLAY
            ),
            Some(Action::LibraryFilterInput('z'))
        );
        assert_eq!(
            map_key(
                key(KeyCode::Char('5')),
                &InputMode::LibraryFilter,
                &NORMAL_DISPLAY
            ),
            Some(Action::LibraryFilterInput('5'))
        );
        assert_eq!(
            map_key(
                key(KeyCode::Char(' ')),
                &InputMode::LibraryFilter,
                &NORMAL_DISPLAY
            ),
            Some(Action::LibraryFilterInput(' '))
        );
        assert_eq!(
            map_key(
                key(KeyCode::Char('-')),
                &InputMode::LibraryFilter,
                &NORMAL_DISPLAY
            ),
            Some(Action::LibraryFilterInput('-'))
        );
    }

    #[test]
    fn library_filter_mode_j_navigates_next() {
        assert_eq!(
            map_key(
                key(KeyCode::Char('j')),
                &InputMode::LibraryFilter,
                &NORMAL_DISPLAY
            ),
            Some(Action::NextStation)
        );
    }
    #[test]
    fn library_filter_mode_k_navigates_prev() {
        assert_eq!(
            map_key(
                key(KeyCode::Char('k')),
                &InputMode::LibraryFilter,
                &NORMAL_DISPLAY
            ),
            Some(Action::PrevStation)
        );
    }
    #[test]
    fn library_filter_mode_up_navigates_prev() {
        assert_eq!(
            map_key(key(KeyCode::Up), &InputMode::LibraryFilter, &NORMAL_DISPLAY),
            Some(Action::PrevStation)
        );
    }
    #[test]
    fn library_filter_mode_down_navigates_next() {
        assert_eq!(
            map_key(
                key(KeyCode::Down),
                &InputMode::LibraryFilter,
                &NORMAL_DISPLAY
            ),
            Some(Action::NextStation)
        );
    }
    #[test]
    fn library_filter_mode_ctrl_c_quits() {
        assert_eq!(
            map_key(
                modified_key(KeyCode::Char('c'), KeyModifiers::CONTROL),
                &InputMode::LibraryFilter,
                &NORMAL_DISPLAY
            ),
            Some(Action::Quit)
        );
    }

    #[test]
    fn library_filter_mode_unbound_keys_are_none() {
        assert_eq!(
            map_key(
                key(KeyCode::Tab),
                &InputMode::LibraryFilter,
                &NORMAL_DISPLAY
            ),
            None
        );
        assert_eq!(
            map_key(
                key(KeyCode::F(1)),
                &InputMode::LibraryFilter,
                &NORMAL_DISPLAY
            ),
            None
        );
        assert_eq!(
            map_key(
                key(KeyCode::Delete),
                &InputMode::LibraryFilter,
                &NORMAL_DISPLAY
            ),
            None
        );
    }

    #[test]
    fn normal_mode_ctrl_l_enters_library_filter() {
        assert_eq!(
            map_key(
                modified_key(KeyCode::Char('l'), KeyModifiers::CONTROL),
                &InputMode::Normal,
                &NORMAL_DISPLAY
            ),
            Some(Action::EnterLibraryFilter)
        );
    }

    #[test]
    fn normal_mode_alt_1_through_5_plays_slot() {
        for digit in 1u8..=5 {
            let c = (b'0' + digit) as char;
            assert_eq!(
                map_key(
                    modified_key(KeyCode::Char(c), KeyModifiers::ALT),
                    &InputMode::Normal,
                    &NORMAL_DISPLAY
                ),
                Some(Action::PlaySlot(digit))
            );
        }
    }

    #[test]
    fn normal_mode_star_toggles_favorite() {
        assert_eq!(
            map_key(key(KeyCode::Char('*')), &InputMode::Normal, &NORMAL_DISPLAY),
            Some(Action::ToggleFavorite)
        );
    }

    #[test]
    fn normal_mode_digit_keys_produce_number_jump_digit() {
        for c in '0'..='9' {
            assert_eq!(
                map_key(key(KeyCode::Char(c)), &InputMode::Normal, &NORMAL_DISPLAY),
                Some(Action::NumberJumpDigit(c))
            );
        }
    }

    #[test]
    fn normal_mode_uppercase_g_confirms_number_jump() {
        assert_eq!(
            map_key(
                modified_key(KeyCode::Char('G'), KeyModifiers::SHIFT),
                &InputMode::Normal,
                &NORMAL_DISPLAY
            ),
            Some(Action::NumberJumpConfirm)
        );
    }

    #[test]
    fn normal_mode_digit_keys_with_ctrl_are_not_number_jump() {
        for c in ['0', '6', '7', '8', '9'] {
            assert_eq!(
                map_key(
                    modified_key(KeyCode::Char(c), KeyModifiers::CONTROL),
                    &InputMode::Normal,
                    &NORMAL_DISPLAY
                ),
                None
            );
        }
    }

    #[test]
    fn normal_mode_ctrl_1_through_5_assign_slot() {
        for digit in 1u8..=5 {
            let c = (b'0' + digit) as char;
            assert_eq!(
                map_key(
                    modified_key(KeyCode::Char(c), KeyModifiers::CONTROL),
                    &InputMode::Normal,
                    &NORMAL_DISPLAY
                ),
                Some(Action::AssignSlot(digit))
            );
        }
    }

    #[test]
    fn normal_mode_digit_keys_with_alt_beyond_5_are_not_mapped() {
        for c in ['6', '7', '8', '9'] {
            assert_eq!(
                map_key(
                    modified_key(KeyCode::Char(c), KeyModifiers::ALT),
                    &InputMode::Normal,
                    &NORMAL_DISPLAY
                ),
                None
            );
        }
    }

    #[test]
    fn normal_mode_lowercase_g_still_toggles_recent_tracks() {
        assert_eq!(
            map_key(key(KeyCode::Char('g')), &InputMode::Normal, &NORMAL_DISPLAY),
            Some(Action::ToggleRecentTracks)
        );
    }
    #[test]
    fn normal_mode_f6_toggles_mini_mode() {
        assert_eq!(
            map_key(key(KeyCode::F(6)), &InputMode::Normal, &NORMAL_DISPLAY),
            Some(Action::ToggleMiniMode)
        );
    }
    #[test]
    fn library_filter_mode_f6_toggles_mini_mode() {
        assert_eq!(
            map_key(
                key(KeyCode::F(6)),
                &InputMode::LibraryFilter,
                &NORMAL_DISPLAY
            ),
            Some(Action::ToggleMiniMode)
        );
    }

    // --- Mini mode key mapping tests ---

    #[test]
    fn mini_mode_space_toggles_pause() {
        assert_eq!(
            map_mini_mode_key(key(KeyCode::Char(' '))),
            Some(Action::TogglePause),
        );
    }

    #[test]
    fn mini_mode_plus_and_equals_volume_up() {
        assert_eq!(
            map_mini_mode_key(key(KeyCode::Char('+'))),
            Some(Action::VolumeUp),
        );
        assert_eq!(
            map_mini_mode_key(key(KeyCode::Char('='))),
            Some(Action::VolumeUp),
        );
    }

    #[test]
    fn mini_mode_minus_volume_down() {
        assert_eq!(
            map_mini_mode_key(key(KeyCode::Char('-'))),
            Some(Action::VolumeDown),
        );
    }

    #[test]
    fn mini_mode_s_stops_playback() {
        assert_eq!(
            map_mini_mode_key(key(KeyCode::Char('s'))),
            Some(Action::Stop),
        );
    }

    #[test]
    fn mini_mode_q_quits() {
        assert_eq!(
            map_mini_mode_key(key(KeyCode::Char('q'))),
            Some(Action::Quit),
        );
    }

    #[test]
    fn mini_mode_ctrl_c_quits() {
        assert_eq!(
            map_mini_mode_key(modified_key(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            Some(Action::Quit),
        );
    }

    #[test]
    fn mini_mode_m_toggles_mute() {
        assert_eq!(
            map_mini_mode_key(key(KeyCode::Char('m'))),
            Some(Action::ToggleMute),
        );
    }

    #[test]
    fn mini_mode_f6_toggles_mini_mode() {
        assert_eq!(
            map_mini_mode_key(key(KeyCode::F(6))),
            Some(Action::ToggleMiniMode),
        );
    }

    #[test]
    fn mini_mode_disallowed_keys_return_none() {
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
                map_mini_mode_key(key(code)),
                None,
                "Expected None for {:?}",
                code,
            );
        }
    }
}

#[cfg(test)]
mod registry_integration_tests {
    use super::*;
    use crate::keybindings::{KeyBinding, KeySpec, Modifier, NamedKey};

    fn binding(key: KeySpec, mods: Vec<Modifier>, action: Action, mode: InputMode) -> KeyBinding {
        KeyBinding {
            key,
            modifiers: mods,
            action,
            mode,
        }
    }

    #[test]
    fn custom_binding_overrides_hardcoded_table() {
        // 'q' normally maps to Quit in Normal mode via defaults; override it to Stop.
        let mut registry = KeybindingRegistry::defaults();
        registry.customs.push(binding(
            KeySpec::Char('q'),
            vec![],
            Action::Stop,
            InputMode::Normal,
        ));

        let key_event = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        let result = map_key_with_registry(
            key_event,
            &InputMode::Normal,
            &DisplayMode::Normal,
            &registry,
        );

        assert_eq!(result, Some(Action::Stop));
    }

    #[test]
    fn unmatched_registry_falls_through_to_defaults() {
        // Registry with defaults → 'q' resolves via defaults.
        let registry = KeybindingRegistry::defaults();

        let key_event = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        let result = map_key_with_registry(
            key_event,
            &InputMode::Normal,
            &DisplayMode::Normal,
            &registry,
        );

        assert_eq!(result, Some(Action::Quit));
    }

    #[test]
    fn missing_file_uses_defaults_only() {
        // A defaults-only registry (simulating missing file) → defaults work.
        let registry = KeybindingRegistry::defaults();

        let key_event = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let result = map_key_with_registry(
            key_event,
            &InputMode::Normal,
            &DisplayMode::Normal,
            &registry,
        );

        assert_eq!(result, Some(Action::PlaySelected));
    }

    #[test]
    fn invalid_file_uses_defaults_with_warning() {
        // Malformed JSON produces an empty-customs registry with a warning.
        let mut warnings = Vec::new();
        let custom = KeybindingRegistry::from_json(b"not json{{", &mut warnings);

        assert!(!warnings.is_empty());
        assert!(warnings[0].contains("Malformed"));

        // Build a registry with defaults + the (empty) customs:
        let mut registry = KeybindingRegistry::defaults();
        registry.customs = custom.customs;

        // Defaults still work:
        let key_event = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        let result = map_key_with_registry(
            key_event,
            &InputMode::Normal,
            &DisplayMode::Normal,
            &registry,
        );

        assert_eq!(result, Some(Action::Quit));
    }

    #[test]
    fn custom_binding_with_modifiers_resolves() {
        let mut registry = KeybindingRegistry::defaults();
        registry.customs.push(binding(
            KeySpec::Char('x'),
            vec![Modifier::Ctrl],
            Action::ExportLibrary,
            InputMode::Normal,
        ));

        let key_event = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL);
        let result = map_key_with_registry(
            key_event,
            &InputMode::Normal,
            &DisplayMode::Normal,
            &registry,
        );

        assert_eq!(result, Some(Action::ExportLibrary));
    }

    #[test]
    fn custom_binding_mode_specific_does_not_leak() {
        // Bind 'x' to VolumeUp only in Search mode.
        let mut registry = KeybindingRegistry::defaults();
        registry.customs.push(binding(
            KeySpec::Char('x'),
            vec![],
            Action::VolumeUp,
            InputMode::Search,
        ));

        // In Normal mode, 'x' should be unmapped (no default for 'x' in Normal).
        let key_event = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE);
        let result = map_key_with_registry(
            key_event,
            &InputMode::Normal,
            &DisplayMode::Normal,
            &registry,
        );

        assert_eq!(result, None);
    }

    #[test]
    fn key_release_events_are_ignored() {
        let mut registry = KeybindingRegistry::defaults();
        registry.customs.push(binding(
            KeySpec::Char('q'),
            vec![],
            Action::Stop,
            InputMode::Normal,
        ));

        let mut key_event = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        key_event.kind = crossterm::event::KeyEventKind::Release;

        let result = map_key_with_registry(
            key_event,
            &InputMode::Normal,
            &DisplayMode::Normal,
            &registry,
        );

        assert_eq!(result, None);
    }

    // --- Conversion helper tests ---

    #[test]
    fn key_code_to_spec_converts_char() {
        assert_eq!(
            key_code_to_spec(KeyCode::Char('a')),
            Some(KeySpec::Char('a'))
        );
    }

    #[test]
    fn key_code_to_spec_converts_function_key() {
        assert_eq!(key_code_to_spec(KeyCode::F(5)), Some(KeySpec::Function(5)));
    }

    #[test]
    fn key_code_to_spec_converts_named_keys() {
        assert_eq!(
            key_code_to_spec(KeyCode::Enter),
            Some(KeySpec::Named(NamedKey::Enter))
        );
        assert_eq!(
            key_code_to_spec(KeyCode::Esc),
            Some(KeySpec::Named(NamedKey::Esc))
        );
        assert_eq!(
            key_code_to_spec(KeyCode::Up),
            Some(KeySpec::Named(NamedKey::Up))
        );
        assert_eq!(
            key_code_to_spec(KeyCode::Down),
            Some(KeySpec::Named(NamedKey::Down))
        );
        assert_eq!(
            key_code_to_spec(KeyCode::Backspace),
            Some(KeySpec::Named(NamedKey::Backspace))
        );
    }

    #[test]
    fn key_code_to_spec_returns_none_for_unsupported() {
        assert_eq!(key_code_to_spec(KeyCode::Null), None);
    }

    #[test]
    fn key_modifiers_to_vec_converts_all_modifiers() {
        let mods = KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SHIFT;
        let result = key_modifiers_to_vec(mods);
        assert!(result.contains(&Modifier::Ctrl));
        assert!(result.contains(&Modifier::Alt));
        assert!(result.contains(&Modifier::Shift));
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn key_modifiers_to_vec_returns_empty_for_none() {
        let result = key_modifiers_to_vec(KeyModifiers::NONE);
        assert!(result.is_empty());
    }
}
