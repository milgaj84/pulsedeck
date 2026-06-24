// Keybinding registry — loads custom bindings from JSON, validates entries.

use std::collections::HashSet;

use super::{parse_key_spec, parse_modifier, InputMode, KeyBinding, KeySpec, Modifier};
use crate::action::Action;

const MAX_BINDINGS: usize = 512;

/// Set-based modifier comparison (order-independent).
fn modifiers_match(binding_mods: &[Modifier], event_mods: &[Modifier]) -> bool {
    let a: HashSet<&Modifier> = binding_mods.iter().collect();
    let b: HashSet<&Modifier> = event_mods.iter().collect();
    a == b
}

/// The registry holding default + custom bindings.
pub struct KeybindingRegistry {
    pub(crate) defaults: Vec<KeyBinding>,
    pub(crate) customs: Vec<KeyBinding>,
}

impl KeybindingRegistry {
    /// Create a registry with pre-built default bindings (for testing).
    pub fn new_with_defaults(defaults: Vec<KeyBinding>) -> Self {
        Self {
            defaults,
            customs: Vec::new(),
        }
    }

    /// Resolve a key event + mode to an Action.
    /// Searches customs (last→first), then defaults (last→first).
    pub fn resolve(
        &self,
        key: &KeySpec,
        modifiers: &[Modifier],
        mode: &InputMode,
    ) -> Option<Action> {
        if let Some(action) = Self::find_match(&self.customs, key, modifiers, mode) {
            return Some(action);
        }
        Self::find_match(&self.defaults, key, modifiers, mode)
    }

    /// Search bindings in reverse order; return first match's action.
    fn find_match(
        bindings: &[KeyBinding],
        key: &KeySpec,
        modifiers: &[Modifier],
        mode: &InputMode,
    ) -> Option<Action> {
        bindings.iter().rev().find_map(|b| {
            if &b.key == key && &b.mode == mode && modifiers_match(&b.modifiers, modifiers) {
                Some(b.action.clone())
            } else {
                None
            }
        })
    }

    /// Load custom bindings from JSON bytes.
    /// Invalid entries are skipped with warnings.
    /// Malformed JSON → empty customs with warning.
    pub fn from_json(json: &[u8], warnings: &mut Vec<String>) -> Self {
        let entries = match parse_json_entries(json) {
            Ok(entries) => entries,
            Err(err) => {
                warnings.push(format!("Malformed keybindings JSON: {err}"));
                return Self::empty();
            }
        };

        let mut customs = Vec::new();
        for entry in entries {
            match parse_entry(&entry) {
                Ok(binding) => customs.push(binding),
                Err(reason) => warnings.push(reason),
            }
        }

        if customs.len() > MAX_BINDINGS {
            warnings.push(format!(
                "Keybindings truncated from {} to {MAX_BINDINGS}",
                customs.len()
            ));
            customs.truncate(MAX_BINDINGS);
        }

        Self {
            defaults: Vec::new(),
            customs,
        }
    }

    fn empty() -> Self {
        Self {
            defaults: Vec::new(),
            customs: Vec::new(),
        }
    }
}

/// Raw JSON entry deserialized from the keybindings array.
#[derive(serde::Deserialize)]
struct RawEntry {
    key: String,
    modifiers: Vec<String>,
    action: String,
    mode: Option<String>,
}

fn parse_json_entries(json: &[u8]) -> Result<Vec<RawEntry>, String> {
    serde_json::from_slice::<Vec<RawEntry>>(json).map_err(|e| e.to_string())
}

fn parse_entry(entry: &RawEntry) -> Result<KeyBinding, String> {
    let key = parse_key_spec(&entry.key)
        .ok_or_else(|| format!("Invalid key spec: '{}'", entry.key))?;

    let modifiers = parse_modifiers(&entry.modifiers)?;
    let action = parse_action_name(&entry.action)
        .ok_or_else(|| format!("Invalid action name: '{}'", entry.action))?;
    let mode = parse_input_mode(entry.mode.as_deref())?;

    Ok(KeyBinding {
        key,
        modifiers,
        action,
        mode,
    })
}

fn parse_modifiers(raw: &[String]) -> Result<Vec<Modifier>, String> {
    raw.iter()
        .map(|m| {
            parse_modifier(m)
                .ok_or_else(|| format!("Invalid modifier: '{m}'"))
        })
        .collect()
}

fn parse_input_mode(raw: Option<&str>) -> Result<InputMode, String> {
    match raw {
        None | Some("Normal") => Ok(InputMode::Normal),
        Some("Search") => Ok(InputMode::Search),
        Some("CommandPalette") => Ok(InputMode::CommandPalette),
        Some("SleepTimer") => Ok(InputMode::SleepTimer),
        Some("LibraryFilter") => Ok(InputMode::LibraryFilter),
        Some(other) => Err(format!("Invalid mode: '{other}'")),
    }
}

/// Parse a snake_case action name into an `Action` variant.
/// Supports parameterized actions like "play_slot(3)" and "sleep_timer_preset(30)".
pub fn parse_action_name(input: &str) -> Option<Action> {
    // Check for parameterized format: "variant_name(value)"
    if let Some((name, value)) = extract_parameterized(input) {
        return parse_parameterized_action(name, value);
    }
    parse_simple_action(input)
}

fn extract_parameterized(input: &str) -> Option<(&str, &str)> {
    let open = input.find('(')?;
    if !input.ends_with(')') {
        return None;
    }
    let name = &input[..open];
    let value = &input[open + 1..input.len() - 1];
    Some((name, value))
}

fn parse_parameterized_action(name: &str, value: &str) -> Option<Action> {
    match name {
        "play_slot" => value.parse::<u8>().ok().map(Action::PlaySlot),
        "assign_slot" => value.parse::<u8>().ok().map(Action::AssignSlot),
        "sleep_timer_preset" => value.parse::<u16>().ok().map(Action::SleepTimerPreset),
        "search_input" => value.chars().next().map(Action::SearchInput),
        "command_palette_input" => value.chars().next().map(Action::CommandPaletteInput),
        "library_filter_input" => value.chars().next().map(Action::LibraryFilterInput),
        "number_jump_digit" => value.chars().next().map(Action::NumberJumpDigit),
        _ => None,
    }
}

fn parse_simple_action(input: &str) -> Option<Action> {
    match input {
        "next_station" => Some(Action::NextStation),
        "prev_station" => Some(Action::PrevStation),
        "play_selected" => Some(Action::PlaySelected),
        "toggle_pause" => Some(Action::TogglePause),
        "stop" => Some(Action::Stop),
        "retry_stream" => Some(Action::RetryStream),
        "volume_up" => Some(Action::VolumeUp),
        "volume_down" => Some(Action::VolumeDown),
        "toggle_mute" => Some(Action::ToggleMute),
        "enter_search" => Some(Action::EnterSearch),
        "exit_search" => Some(Action::ExitSearch),
        "search_backspace" => Some(Action::SearchBackspace),
        "search_confirm" => Some(Action::SearchConfirm),
        "search_audition" => Some(Action::SearchAudition),
        "open_command_palette" => Some(Action::OpenCommandPalette),
        "command_palette_confirm" => Some(Action::CommandPaletteConfirm),
        "command_palette_close" => Some(Action::CommandPaletteClose),
        "command_palette_backspace" => Some(Action::CommandPaletteBackspace),
        "command_palette_next" => Some(Action::CommandPaletteNext),
        "command_palette_prev" => Some(Action::CommandPalettePrev),
        "remove_library_selection" => Some(Action::RemoveLibrarySelection),
        "undo_remove_library_selection" => Some(Action::UndoRemoveLibrarySelection),
        "next_genre" => Some(Action::NextGenre),
        "prev_genre" => Some(Action::PrevGenre),
        "enter_library_filter" => Some(Action::EnterLibraryFilter),
        "exit_library_filter" => Some(Action::ExitLibraryFilter),
        "library_filter_backspace" => Some(Action::LibraryFilterBackspace),
        "library_filter_confirm" => Some(Action::LibraryFilterConfirm),
        "toggle_favorite" => Some(Action::ToggleFavorite),
        "number_jump_confirm" => Some(Action::NumberJumpConfirm),
        "number_jump_cancel" => Some(Action::NumberJumpCancel),
        "cycle_layout" => Some(Action::CycleLayout),
        "toggle_help" => Some(Action::ToggleHelp),
        "toggle_station_details" => Some(Action::ToggleStationDetails),
        "toggle_recent_tracks" => Some(Action::ToggleRecentTracks),
        "toggle_playback_doctor" => Some(Action::TogglePlaybackDoctor),
        "step_setting_forward" => Some(Action::StepSettingForward),
        "step_setting_backward" => Some(Action::StepSettingBackward),
        "toggle_settings" => Some(Action::ToggleSettings),
        "cycle_theme_setting" => Some(Action::CycleThemeSetting),
        "toggle_stream_metadata" => Some(Action::ToggleStreamMetadata),
        "refresh_library_metadata" => Some(Action::RefreshLibraryMetadata),
        "toggle_visualizer_mode" => Some(Action::ToggleVisualizerMode),
        "toggle_mini_mode" => Some(Action::ToggleMiniMode),
        "quit" => Some(Action::Quit),
        "toggle_sleep_timer" => Some(Action::ToggleSleepTimer),
        "sleep_timer_increase" => Some(Action::SleepTimerIncrease),
        "sleep_timer_decrease" => Some(Action::SleepTimerDecrease),
        "sleep_timer_clear" => Some(Action::SleepTimerClear),
        "export_library" => Some(Action::ExportLibrary),
        "tick" => Some(Action::Tick),
        "discover" => Some(Action::Discover),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keybindings::KeySpec;

    fn valid_json() -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!([
            {"key": "char(k)", "modifiers": ["ctrl"], "action": "prev_station", "mode": "Normal"},
            {"key": "enter", "modifiers": [], "action": "play_selected"}
        ]))
        .unwrap()
    }

    #[test]
    fn test_from_json_valid_entries_load() {
        let mut warnings = Vec::new();
        let registry = KeybindingRegistry::from_json(&valid_json(), &mut warnings);

        assert!(warnings.is_empty());
        assert_eq!(registry.customs.len(), 2);
        assert_eq!(registry.customs[0].action, Action::PrevStation);
        assert_eq!(registry.customs[0].key, KeySpec::Char('k'));
        assert_eq!(registry.customs[0].modifiers, vec![Modifier::Ctrl]);
        assert_eq!(registry.customs[0].mode, InputMode::Normal);
        assert_eq!(registry.customs[1].action, Action::PlaySelected);
        assert_eq!(registry.customs[1].mode, InputMode::Normal);
    }

    #[test]
    fn test_from_json_invalid_action_skipped() {
        let json = serde_json::to_vec(&serde_json::json!([
            {"key": "enter", "modifiers": [], "action": "nonexistent_action"},
            {"key": "char(q)", "modifiers": [], "action": "quit"}
        ]))
        .unwrap();

        let mut warnings = Vec::new();
        let registry = KeybindingRegistry::from_json(&json, &mut warnings);

        assert_eq!(registry.customs.len(), 1);
        assert_eq!(registry.customs[0].action, Action::Quit);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("Invalid action name"));
    }

    #[test]
    fn test_from_json_invalid_key_spec_skipped() {
        let json = serde_json::to_vec(&serde_json::json!([
            {"key": "capslock", "modifiers": [], "action": "quit"},
            {"key": "esc", "modifiers": [], "action": "quit"}
        ]))
        .unwrap();

        let mut warnings = Vec::new();
        let registry = KeybindingRegistry::from_json(&json, &mut warnings);

        assert_eq!(registry.customs.len(), 1);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("Invalid key spec"));
    }

    #[test]
    fn test_from_json_invalid_modifier_skipped() {
        let json = serde_json::to_vec(&serde_json::json!([
            {"key": "enter", "modifiers": ["meta"], "action": "quit"},
            {"key": "esc", "modifiers": [], "action": "quit"}
        ]))
        .unwrap();

        let mut warnings = Vec::new();
        let registry = KeybindingRegistry::from_json(&json, &mut warnings);

        assert_eq!(registry.customs.len(), 1);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("Invalid modifier"));
    }

    #[test]
    fn test_from_json_malformed_json_returns_defaults() {
        let mut warnings = Vec::new();
        let registry = KeybindingRegistry::from_json(b"not valid json{{{", &mut warnings);

        assert!(registry.customs.is_empty());
        assert!(registry.defaults.is_empty());
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("Malformed keybindings JSON"));
    }

    #[test]
    fn test_from_json_truncates_at_max_bindings() {
        let entries: Vec<serde_json::Value> = (0..600)
            .map(|_| {
                serde_json::json!({"key": "enter", "modifiers": [], "action": "quit"})
            })
            .collect();
        let json = serde_json::to_vec(&entries).unwrap();

        let mut warnings = Vec::new();
        let registry = KeybindingRegistry::from_json(&json, &mut warnings);

        assert_eq!(registry.customs.len(), MAX_BINDINGS);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("truncated"));
    }

    #[test]
    fn test_from_json_mode_defaults_to_normal() {
        let json = serde_json::to_vec(&serde_json::json!([
            {"key": "char(q)", "modifiers": [], "action": "quit"}
        ]))
        .unwrap();

        let mut warnings = Vec::new();
        let registry = KeybindingRegistry::from_json(&json, &mut warnings);

        assert_eq!(registry.customs[0].mode, InputMode::Normal);
    }

    #[test]
    fn test_from_json_explicit_mode_applied() {
        let json = serde_json::to_vec(&serde_json::json!([
            {"key": "esc", "modifiers": [], "action": "exit_search", "mode": "Search"}
        ]))
        .unwrap();

        let mut warnings = Vec::new();
        let registry = KeybindingRegistry::from_json(&json, &mut warnings);

        assert_eq!(registry.customs[0].mode, InputMode::Search);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_parse_action_name_simple() {
        assert_eq!(parse_action_name("play_selected"), Some(Action::PlaySelected));
        assert_eq!(parse_action_name("volume_up"), Some(Action::VolumeUp));
        assert_eq!(parse_action_name("quit"), Some(Action::Quit));
    }

    #[test]
    fn test_parse_action_name_parameterized() {
        assert_eq!(parse_action_name("play_slot(3)"), Some(Action::PlaySlot(3)));
        assert_eq!(
            parse_action_name("sleep_timer_preset(30)"),
            Some(Action::SleepTimerPreset(30))
        );
    }

    #[test]
    fn test_parse_action_name_invalid() {
        assert_eq!(parse_action_name("nonexistent"), None);
        assert_eq!(parse_action_name(""), None);
        assert_eq!(parse_action_name("play_slot(abc)"), None);
    }

    #[test]
    fn test_from_json_empty_array() {
        let mut warnings = Vec::new();
        let registry = KeybindingRegistry::from_json(b"[]", &mut warnings);

        assert!(registry.customs.is_empty());
        assert!(warnings.is_empty());
    }

    // --- resolve tests ---

    fn binding(key: KeySpec, modifiers: Vec<Modifier>, action: Action, mode: InputMode) -> KeyBinding {
        KeyBinding { key, modifiers, action, mode }
    }

    #[test]
    fn test_resolve_custom_overrides_default() {
        let defaults = vec![binding(
            KeySpec::Char('q'),
            vec![],
            Action::Quit,
            InputMode::Normal,
        )];
        let mut registry = KeybindingRegistry::new_with_defaults(defaults);
        registry.customs.push(binding(
            KeySpec::Char('q'),
            vec![],
            Action::Stop,
            InputMode::Normal,
        ));

        let result = registry.resolve(&KeySpec::Char('q'), &[], &InputMode::Normal);
        assert_eq!(result, Some(Action::Stop));
    }

    #[test]
    fn test_resolve_mode_isolation() {
        let mut registry = KeybindingRegistry::new_with_defaults(Vec::new());
        registry.customs.push(binding(
            KeySpec::Char('q'),
            vec![],
            Action::Quit,
            InputMode::Normal,
        ));

        let result = registry.resolve(&KeySpec::Char('q'), &[], &InputMode::Search);
        assert_eq!(result, None);
    }

    #[test]
    fn test_resolve_unbound_key_returns_none() {
        let registry = KeybindingRegistry::new_with_defaults(Vec::new());

        let result = registry.resolve(&KeySpec::Char('z'), &[], &InputMode::Normal);
        assert_eq!(result, None);
    }

    #[test]
    fn test_resolve_last_custom_wins_for_duplicates() {
        let mut registry = KeybindingRegistry::new_with_defaults(Vec::new());
        registry.customs.push(binding(
            KeySpec::Char('q'),
            vec![],
            Action::Quit,
            InputMode::Normal,
        ));
        registry.customs.push(binding(
            KeySpec::Char('q'),
            vec![],
            Action::Stop,
            InputMode::Normal,
        ));

        let result = registry.resolve(&KeySpec::Char('q'), &[], &InputMode::Normal);
        assert_eq!(result, Some(Action::Stop));
    }

    #[test]
    fn test_resolve_modifier_order_independent() {
        let mut registry = KeybindingRegistry::new_with_defaults(Vec::new());
        registry.customs.push(binding(
            KeySpec::Char('c'),
            vec![Modifier::Ctrl, Modifier::Shift],
            Action::Quit,
            InputMode::Normal,
        ));

        // Query with modifiers in reverse order
        let result = registry.resolve(
            &KeySpec::Char('c'),
            &[Modifier::Shift, Modifier::Ctrl],
            &InputMode::Normal,
        );
        assert_eq!(result, Some(Action::Quit));
    }

    #[test]
    fn test_resolve_falls_through_to_defaults() {
        let defaults = vec![binding(
            KeySpec::Named(super::super::NamedKey::Enter),
            vec![],
            Action::PlaySelected,
            InputMode::Normal,
        )];
        let registry = KeybindingRegistry::new_with_defaults(defaults);

        let result = registry.resolve(
            &KeySpec::Named(super::super::NamedKey::Enter),
            &[],
            &InputMode::Normal,
        );
        assert_eq!(result, Some(Action::PlaySelected));
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use crate::keybindings::{InputMode, KeyBinding, KeySpec, Modifier, NamedKey};
    use proptest::prelude::*;

    // Feature: v080-features, Property 9: Keybinding serialization round-trip

    /// Serialize a KeySpec to its JSON string representation.
    fn serialize_key_spec(key: &KeySpec) -> String {
        match key {
            KeySpec::Char(c) => format!("char({c})"),
            KeySpec::Function(n) => format!("f{n}"),
            KeySpec::Named(k) => serialize_named_key(k).to_string(),
        }
    }

    fn serialize_named_key(key: &NamedKey) -> &'static str {
        match key {
            NamedKey::Enter => "enter",
            NamedKey::Esc => "esc",
            NamedKey::Up => "up",
            NamedKey::Down => "down",
            NamedKey::Left => "left",
            NamedKey::Right => "right",
            NamedKey::Tab => "tab",
            NamedKey::Backspace => "backspace",
            NamedKey::Home => "home",
            NamedKey::End => "end",
            NamedKey::PageUp => "pageup",
            NamedKey::PageDown => "pagedown",
            NamedKey::Delete => "delete",
            NamedKey::Insert => "insert",
        }
    }

    fn serialize_modifier(m: &Modifier) -> &'static str {
        match m {
            Modifier::Ctrl => "ctrl",
            Modifier::Alt => "alt",
            Modifier::Shift => "shift",
        }
    }

    fn serialize_action(action: &Action) -> &'static str {
        match action {
            Action::NextStation => "next_station",
            Action::PrevStation => "prev_station",
            Action::PlaySelected => "play_selected",
            Action::TogglePause => "toggle_pause",
            Action::Stop => "stop",
            Action::RetryStream => "retry_stream",
            Action::VolumeUp => "volume_up",
            Action::VolumeDown => "volume_down",
            Action::ToggleMute => "toggle_mute",
            Action::EnterSearch => "enter_search",
            Action::ExitSearch => "exit_search",
            Action::SearchBackspace => "search_backspace",
            Action::SearchConfirm => "search_confirm",
            Action::SearchAudition => "search_audition",
            Action::OpenCommandPalette => "open_command_palette",
            Action::CommandPaletteConfirm => "command_palette_confirm",
            Action::CommandPaletteClose => "command_palette_close",
            Action::CommandPaletteBackspace => "command_palette_backspace",
            Action::CommandPaletteNext => "command_palette_next",
            Action::CommandPalettePrev => "command_palette_prev",
            Action::RemoveLibrarySelection => "remove_library_selection",
            Action::UndoRemoveLibrarySelection => "undo_remove_library_selection",
            Action::NextGenre => "next_genre",
            Action::PrevGenre => "prev_genre",
            Action::EnterLibraryFilter => "enter_library_filter",
            Action::ExitLibraryFilter => "exit_library_filter",
            Action::LibraryFilterBackspace => "library_filter_backspace",
            Action::LibraryFilterConfirm => "library_filter_confirm",
            Action::ToggleFavorite => "toggle_favorite",
            Action::NumberJumpConfirm => "number_jump_confirm",
            Action::NumberJumpCancel => "number_jump_cancel",
            Action::CycleLayout => "cycle_layout",
            Action::ToggleHelp => "toggle_help",
            Action::ToggleStationDetails => "toggle_station_details",
            Action::ToggleRecentTracks => "toggle_recent_tracks",
            Action::TogglePlaybackDoctor => "toggle_playback_doctor",
            Action::StepSettingForward => "step_setting_forward",
            Action::StepSettingBackward => "step_setting_backward",
            Action::ToggleSettings => "toggle_settings",
            Action::CycleThemeSetting => "cycle_theme_setting",
            Action::ToggleStreamMetadata => "toggle_stream_metadata",
            Action::RefreshLibraryMetadata => "refresh_library_metadata",
            Action::ToggleVisualizerMode => "toggle_visualizer_mode",
            Action::ToggleMiniMode => "toggle_mini_mode",
            Action::Quit => "quit",
            Action::ToggleSleepTimer => "toggle_sleep_timer",
            Action::SleepTimerIncrease => "sleep_timer_increase",
            Action::SleepTimerDecrease => "sleep_timer_decrease",
            Action::SleepTimerClear => "sleep_timer_clear",
            Action::ExportLibrary => "export_library",
            Action::Tick => "tick",
            Action::Discover => "discover",
            _ => unreachable!("only simple actions used in generator"),
        }
    }

    fn serialize_mode(mode: &InputMode) -> &'static str {
        match mode {
            InputMode::Normal => "Normal",
            InputMode::Search => "Search",
            InputMode::CommandPalette => "CommandPalette",
            InputMode::SleepTimer => "SleepTimer",
            InputMode::LibraryFilter => "LibraryFilter",
        }
    }

    /// Serialize a Vec<KeyBinding> to JSON bytes.
    fn serialize_bindings_to_json(bindings: &[KeyBinding]) -> Vec<u8> {
        let entries: Vec<serde_json::Value> = bindings
            .iter()
            .map(|b| {
                serde_json::json!({
                    "key": serialize_key_spec(&b.key),
                    "modifiers": b.modifiers.iter()
                        .map(serialize_modifier)
                        .collect::<Vec<_>>(),
                    "action": serialize_action(&b.action),
                    "mode": serialize_mode(&b.mode),
                })
            })
            .collect();
        serde_json::to_vec(&entries).unwrap()
    }

    // --- Proptest strategies ---

    fn arb_named_key() -> impl Strategy<Value = NamedKey> {
        prop_oneof![
            Just(NamedKey::Enter),
            Just(NamedKey::Esc),
            Just(NamedKey::Up),
            Just(NamedKey::Down),
            Just(NamedKey::Left),
            Just(NamedKey::Right),
            Just(NamedKey::Tab),
            Just(NamedKey::Backspace),
            Just(NamedKey::Home),
            Just(NamedKey::End),
            Just(NamedKey::PageUp),
            Just(NamedKey::PageDown),
            Just(NamedKey::Delete),
            Just(NamedKey::Insert),
        ]
    }

    fn arb_key_spec() -> impl Strategy<Value = KeySpec> {
        prop_oneof![
            // Valid chars: printable ASCII excluding '(' and ')' to avoid parse ambiguity
            (0x21u8..=0x7Eu8)
                .prop_filter("no parens", |c| *c != b'(' && *c != b')')
                .prop_map(|c| KeySpec::Char(c as char)),
            (1u8..=12u8).prop_map(KeySpec::Function),
            arb_named_key().prop_map(KeySpec::Named),
        ]
    }

    fn arb_modifier() -> impl Strategy<Value = Modifier> {
        prop_oneof![Just(Modifier::Ctrl), Just(Modifier::Alt), Just(Modifier::Shift),]
    }

    fn arb_modifiers() -> impl Strategy<Value = Vec<Modifier>> {
        proptest::collection::vec(arb_modifier(), 0..=3)
            .prop_map(|mods| {
                // Deduplicate to avoid repeated modifiers
                let mut seen = std::collections::HashSet::new();
                mods.into_iter().filter(|m| seen.insert(*m)).collect()
            })
    }

    fn arb_simple_action() -> impl Strategy<Value = Action> {
        prop_oneof![
            Just(Action::NextStation),
            Just(Action::PrevStation),
            Just(Action::PlaySelected),
            Just(Action::TogglePause),
            Just(Action::Stop),
            Just(Action::RetryStream),
            Just(Action::VolumeUp),
            Just(Action::VolumeDown),
            Just(Action::ToggleMute),
            Just(Action::EnterSearch),
            Just(Action::ExitSearch),
            Just(Action::SearchBackspace),
            Just(Action::SearchConfirm),
            Just(Action::SearchAudition),
            Just(Action::OpenCommandPalette),
            Just(Action::CommandPaletteConfirm),
            Just(Action::CommandPaletteClose),
            Just(Action::CommandPaletteBackspace),
            Just(Action::CommandPaletteNext),
            Just(Action::CommandPalettePrev),
            Just(Action::RemoveLibrarySelection),
            Just(Action::UndoRemoveLibrarySelection),
            Just(Action::NextGenre),
            Just(Action::PrevGenre),
            Just(Action::EnterLibraryFilter),
            Just(Action::ExitLibraryFilter),
            Just(Action::LibraryFilterBackspace),
            Just(Action::LibraryFilterConfirm),
            Just(Action::ToggleFavorite),
            Just(Action::NumberJumpConfirm),
            Just(Action::NumberJumpCancel),
            Just(Action::CycleLayout),
        ]
    }

    fn arb_simple_action_2() -> impl Strategy<Value = Action> {
        prop_oneof![
            Just(Action::ToggleHelp),
            Just(Action::ToggleStationDetails),
            Just(Action::ToggleRecentTracks),
            Just(Action::TogglePlaybackDoctor),
            Just(Action::StepSettingForward),
            Just(Action::StepSettingBackward),
            Just(Action::ToggleSettings),
            Just(Action::CycleThemeSetting),
            Just(Action::ToggleStreamMetadata),
            Just(Action::RefreshLibraryMetadata),
            Just(Action::ToggleVisualizerMode),
            Just(Action::ToggleMiniMode),
            Just(Action::Quit),
            Just(Action::ToggleSleepTimer),
            Just(Action::SleepTimerIncrease),
            Just(Action::SleepTimerDecrease),
            Just(Action::SleepTimerClear),
            Just(Action::ExportLibrary),
            Just(Action::Tick),
            Just(Action::Discover),
        ]
    }

    fn arb_action() -> impl Strategy<Value = Action> {
        prop_oneof![arb_simple_action(), arb_simple_action_2(),]
    }

    fn arb_input_mode() -> impl Strategy<Value = InputMode> {
        prop_oneof![
            Just(InputMode::Normal),
            Just(InputMode::Search),
            Just(InputMode::CommandPalette),
            Just(InputMode::SleepTimer),
            Just(InputMode::LibraryFilter),
        ]
    }

    fn arb_keybinding() -> impl Strategy<Value = KeyBinding> {
        (arb_key_spec(), arb_modifiers(), arb_action(), arb_input_mode()).prop_map(
            |(key, modifiers, action, mode)| KeyBinding {
                key,
                modifiers,
                action,
                mode,
            },
        )
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Validates: Requirements 5.10, 6.3, 6.4**
        #[test]
        fn prop_keybinding_serialization_round_trip(
            bindings in proptest::collection::vec(arb_keybinding(), 0..=512)
        ) {
            let json = serialize_bindings_to_json(&bindings);
            let mut warnings = Vec::new();
            let registry = KeybindingRegistry::from_json(&json, &mut warnings);

            // No warnings should be produced for valid bindings
            prop_assert!(
                warnings.is_empty(),
                "Unexpected warnings: {:?}",
                warnings
            );

            // Same number of bindings
            prop_assert_eq!(registry.customs.len(), bindings.len());

            // Each binding round-trips correctly
            for (original, parsed) in bindings.iter().zip(registry.customs.iter()) {
                prop_assert_eq!(&parsed.key, &original.key);
                prop_assert_eq!(&parsed.action, &original.action);
                prop_assert_eq!(&parsed.mode, &original.mode);
                // Modifiers: compare as sets (order-independent)
                let orig_set: std::collections::HashSet<&Modifier> =
                    original.modifiers.iter().collect();
                let parsed_set: std::collections::HashSet<&Modifier> =
                    parsed.modifiers.iter().collect();
                prop_assert_eq!(orig_set, parsed_set);
            }
        }
    }

    /// Two distinct actions guaranteed to differ.
    fn arb_distinct_actions() -> impl Strategy<Value = (Action, Action)> {
        let actions = vec![
            Action::Quit,
            Action::Stop,
            Action::VolumeUp,
            Action::VolumeDown,
            Action::TogglePause,
            Action::NextStation,
            Action::PrevStation,
            Action::PlaySelected,
            Action::ToggleMute,
            Action::EnterSearch,
        ];
        let len = actions.len();
        (0..len, 0..len)
            .prop_filter("distinct actions", |(a, b)| a != b)
            .prop_map(move |(a, b)| (actions[a].clone(), actions[b].clone()))
    }

    /// Three distinct actions guaranteed to all differ.
    fn arb_three_distinct_actions() -> impl Strategy<Value = (Action, Action, Action)> {
        let actions = vec![
            Action::Quit,
            Action::Stop,
            Action::VolumeUp,
            Action::VolumeDown,
            Action::TogglePause,
            Action::NextStation,
            Action::PrevStation,
            Action::PlaySelected,
            Action::ToggleMute,
            Action::EnterSearch,
        ];
        let len = actions.len();
        (0..len, 0..len, 0..len)
            .prop_filter("all distinct", |(a, b, c)| a != b && b != c && a != c)
            .prop_map(move |(a, b, c)| {
                (actions[a].clone(), actions[b].clone(), actions[c].clone())
            })
    }

    // Feature: v080-features, Property 10: Custom binding precedence and last-wins
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Validates: Requirements 5.7, 6.6**
        #[test]
        fn custom_binding_overrides_default(
            key in arb_key_spec(),
            modifiers in arb_modifiers(),
            mode in arb_input_mode(),
            (default_action, custom_action) in arb_distinct_actions(),
        ) {
            let default_binding = KeyBinding {
                key: key.clone(),
                modifiers: modifiers.clone(),
                action: default_action,
                mode: mode.clone(),
            };
            let custom_binding = KeyBinding {
                key: key.clone(),
                modifiers: modifiers.clone(),
                action: custom_action.clone(),
                mode: mode.clone(),
            };

            let mut registry = KeybindingRegistry::new_with_defaults(vec![default_binding]);
            registry.customs.push(custom_binding);

            let result = registry.resolve(&key, &modifiers, &mode);
            prop_assert_eq!(result, Some(custom_action));
        }

        /// **Validates: Requirements 5.7, 6.6**
        #[test]
        fn last_custom_binding_wins(
            key in arb_key_spec(),
            modifiers in arb_modifiers(),
            mode in arb_input_mode(),
            (default_action, first_custom, last_custom) in arb_three_distinct_actions(),
        ) {
            let default_binding = KeyBinding {
                key: key.clone(),
                modifiers: modifiers.clone(),
                action: default_action,
                mode: mode.clone(),
            };
            let first_binding = KeyBinding {
                key: key.clone(),
                modifiers: modifiers.clone(),
                action: first_custom,
                mode: mode.clone(),
            };
            let last_binding = KeyBinding {
                key: key.clone(),
                modifiers: modifiers.clone(),
                action: last_custom.clone(),
                mode: mode.clone(),
            };

            let mut registry = KeybindingRegistry::new_with_defaults(vec![default_binding]);
            registry.customs.push(first_binding);
            registry.customs.push(last_binding);

            let result = registry.resolve(&key, &modifiers, &mode);
            prop_assert_eq!(result, Some(last_custom));
        }
    }

    /// Two distinct InputModes guaranteed to differ.
    fn arb_distinct_modes() -> impl Strategy<Value = (InputMode, InputMode)> {
        let modes = vec![
            InputMode::Normal,
            InputMode::Search,
            InputMode::CommandPalette,
            InputMode::SleepTimer,
            InputMode::LibraryFilter,
        ];
        let len = modes.len();
        (0..len, 0..len)
            .prop_filter("distinct modes", |(a, b)| a != b)
            .prop_map(move |(a, b)| (modes[a].clone(), modes[b].clone()))
    }

    // Feature: v080-features, Property 11: Mode-specific binding isolation
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Validates: Requirements 5.8**
        #[test]
        fn mode_specific_binding_isolation(
            key in arb_key_spec(),
            modifiers in arb_modifiers(),
            action in arb_action(),
            (binding_mode, query_mode) in arb_distinct_modes(),
        ) {
            let binding = KeyBinding {
                key: key.clone(),
                modifiers: modifiers.clone(),
                action,
                mode: binding_mode,
            };

            let mut registry = KeybindingRegistry::new_with_defaults(Vec::new());
            registry.customs.push(binding);

            let result = registry.resolve(&key, &modifiers, &query_mode);
            prop_assert_eq!(result, None);
        }
    }
}
