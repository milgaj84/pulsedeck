// Keybinding customization — registry for loading and resolving key-to-action mappings.

pub mod defaults;
mod registry;

use crate::action::Action;

pub use registry::KeybindingRegistry;
pub use registry::detect_shadows;
pub use registry::format_key_description;
pub use registry::format_mode_name;

/// Input mode for keybinding resolution.
/// Mirrors app-layer InputMode but lives in the domain layer to avoid circular deps.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(dead_code)]
pub enum InputMode {
    Normal,
    Search,
    CommandPalette,
    SleepTimer,
    LibraryFilter,
}

/// Key specification matching crossterm KeyCode names.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum KeySpec {
    Char(char),
    Function(u8),
    Named(NamedKey),
}

/// Named keys (non-character, non-function).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NamedKey {
    Enter,
    Esc,
    Up,
    Down,
    Left,
    Right,
    Tab,
    Backspace,
    Home,
    End,
    PageUp,
    PageDown,
    Delete,
    Insert,
}

/// Keyboard modifier keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Modifier {
    Ctrl,
    Alt,
    Shift,
}

/// A single keybinding entry parsed from JSON.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyBinding {
    pub key: KeySpec,
    pub modifiers: Vec<Modifier>,
    pub action: Action,
    pub mode: InputMode,
}

/// Parse a key specification string into a `KeySpec`.
///
/// Accepts: "char(k)", "enter", "esc", "up", "down", "left", "right",
/// "tab", "backspace", "home", "end", "pageup", "pagedown", "delete",
/// "insert", "f1"–"f12".
pub fn parse_key_spec(input: &str) -> Option<KeySpec> {
    let trimmed = input.trim();
    let lower = trimmed.to_lowercase();

    if let Some(inner) = extract_char_spec(trimmed) {
        return Some(KeySpec::Char(inner));
    }

    if let Some(f_num) = extract_function_key(&lower) {
        return Some(KeySpec::Function(f_num));
    }

    parse_named_key(&lower).map(KeySpec::Named)
}

/// Parse a modifier string into a `Modifier` (case-insensitive).
pub fn parse_modifier(input: &str) -> Option<Modifier> {
    match input.trim().to_lowercase().as_str() {
        "ctrl" => Some(Modifier::Ctrl),
        "alt" => Some(Modifier::Alt),
        "shift" => Some(Modifier::Shift),
        _ => None,
    }
}

fn extract_char_spec(input: &str) -> Option<char> {
    let lower = input.to_lowercase();
    if !lower.starts_with("char(") || !lower.ends_with(')') {
        return None;
    }
    let inner = &input[5..input.len() - 1];
    let mut chars = inner.chars();
    let ch = chars.next()?;
    if chars.next().is_some() {
        return None; // more than one char inside parens
    }
    Some(ch)
}

fn extract_function_key(input: &str) -> Option<u8> {
    let num_str = input.strip_prefix('f')?;
    let num: u8 = num_str.parse().ok()?;
    if (1..=12).contains(&num) {
        Some(num)
    } else {
        None
    }
}

fn parse_named_key(input: &str) -> Option<NamedKey> {
    match input {
        "enter" => Some(NamedKey::Enter),
        "esc" => Some(NamedKey::Esc),
        "up" => Some(NamedKey::Up),
        "down" => Some(NamedKey::Down),
        "left" => Some(NamedKey::Left),
        "right" => Some(NamedKey::Right),
        "tab" => Some(NamedKey::Tab),
        "backspace" => Some(NamedKey::Backspace),
        "home" => Some(NamedKey::Home),
        "end" => Some(NamedKey::End),
        "pageup" => Some(NamedKey::PageUp),
        "pagedown" => Some(NamedKey::PageDown),
        "delete" => Some(NamedKey::Delete),
        "insert" => Some(NamedKey::Insert),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- parse_key_spec: valid inputs ---

    #[test]
    fn test_parse_key_spec_char_lowercase() {
        assert_eq!(parse_key_spec("char(k)"), Some(KeySpec::Char('k')));
    }

    #[test]
    fn test_parse_key_spec_char_uppercase() {
        assert_eq!(parse_key_spec("char(K)"), Some(KeySpec::Char('K')));
    }

    #[test]
    fn test_parse_key_spec_char_space() {
        assert_eq!(parse_key_spec("char( )"), Some(KeySpec::Char(' ')));
    }

    #[test]
    fn test_parse_key_spec_enter() {
        assert_eq!(
            parse_key_spec("enter"),
            Some(KeySpec::Named(NamedKey::Enter))
        );
    }

    #[test]
    fn test_parse_key_spec_esc() {
        assert_eq!(parse_key_spec("esc"), Some(KeySpec::Named(NamedKey::Esc)));
    }

    #[test]
    fn test_parse_key_spec_up() {
        assert_eq!(parse_key_spec("up"), Some(KeySpec::Named(NamedKey::Up)));
    }

    #[test]
    fn test_parse_key_spec_down() {
        assert_eq!(parse_key_spec("down"), Some(KeySpec::Named(NamedKey::Down)));
    }

    #[test]
    fn test_parse_key_spec_tab() {
        assert_eq!(parse_key_spec("tab"), Some(KeySpec::Named(NamedKey::Tab)));
    }

    #[test]
    fn test_parse_key_spec_backspace() {
        assert_eq!(
            parse_key_spec("backspace"),
            Some(KeySpec::Named(NamedKey::Backspace))
        );
    }

    #[test]
    fn test_parse_key_spec_function_f3() {
        assert_eq!(parse_key_spec("f3"), Some(KeySpec::Function(3)));
    }

    #[test]
    fn test_parse_key_spec_function_f12() {
        assert_eq!(parse_key_spec("f12"), Some(KeySpec::Function(12)));
    }

    #[test]
    fn test_parse_key_spec_case_insensitive() {
        assert_eq!(
            parse_key_spec("Enter"),
            Some(KeySpec::Named(NamedKey::Enter))
        );
        assert_eq!(
            parse_key_spec("BACKSPACE"),
            Some(KeySpec::Named(NamedKey::Backspace))
        );
    }

    #[test]
    fn test_parse_key_spec_with_whitespace() {
        assert_eq!(
            parse_key_spec("  enter  "),
            Some(KeySpec::Named(NamedKey::Enter))
        );
    }

    #[test]
    fn test_parse_key_spec_pageup_pagedown() {
        assert_eq!(
            parse_key_spec("pageup"),
            Some(KeySpec::Named(NamedKey::PageUp))
        );
        assert_eq!(
            parse_key_spec("pagedown"),
            Some(KeySpec::Named(NamedKey::PageDown))
        );
    }

    #[test]
    fn test_parse_key_spec_home_end() {
        assert_eq!(parse_key_spec("home"), Some(KeySpec::Named(NamedKey::Home)));
        assert_eq!(parse_key_spec("end"), Some(KeySpec::Named(NamedKey::End)));
    }

    #[test]
    fn test_parse_key_spec_delete_insert() {
        assert_eq!(
            parse_key_spec("delete"),
            Some(KeySpec::Named(NamedKey::Delete))
        );
        assert_eq!(
            parse_key_spec("insert"),
            Some(KeySpec::Named(NamedKey::Insert))
        );
    }

    // --- parse_key_spec: invalid inputs ---

    #[test]
    fn test_parse_key_spec_empty_string() {
        assert_eq!(parse_key_spec(""), None);
    }

    #[test]
    fn test_parse_key_spec_unknown_name() {
        assert_eq!(parse_key_spec("capslock"), None);
    }

    #[test]
    fn test_parse_key_spec_char_multiple_chars() {
        assert_eq!(parse_key_spec("char(ab)"), None);
    }

    #[test]
    fn test_parse_key_spec_char_empty() {
        assert_eq!(parse_key_spec("char()"), None);
    }

    #[test]
    fn test_parse_key_spec_function_f0() {
        assert_eq!(parse_key_spec("f0"), None);
    }

    #[test]
    fn test_parse_key_spec_function_f13() {
        assert_eq!(parse_key_spec("f13"), None);
    }

    #[test]
    fn test_parse_key_spec_function_not_a_number() {
        assert_eq!(parse_key_spec("fx"), None);
    }

    // --- parse_modifier ---

    #[test]
    fn test_parse_modifier_ctrl() {
        assert_eq!(parse_modifier("ctrl"), Some(Modifier::Ctrl));
    }

    #[test]
    fn test_parse_modifier_alt() {
        assert_eq!(parse_modifier("alt"), Some(Modifier::Alt));
    }

    #[test]
    fn test_parse_modifier_shift() {
        assert_eq!(parse_modifier("shift"), Some(Modifier::Shift));
    }

    #[test]
    fn test_parse_modifier_case_insensitive() {
        assert_eq!(parse_modifier("CTRL"), Some(Modifier::Ctrl));
        assert_eq!(parse_modifier("Alt"), Some(Modifier::Alt));
        assert_eq!(parse_modifier("SHIFT"), Some(Modifier::Shift));
    }

    #[test]
    fn test_parse_modifier_with_whitespace() {
        assert_eq!(parse_modifier("  ctrl  "), Some(Modifier::Ctrl));
    }

    #[test]
    fn test_parse_modifier_invalid() {
        assert_eq!(parse_modifier("meta"), None);
        assert_eq!(parse_modifier(""), None);
        assert_eq!(parse_modifier("super"), None);
    }

    // --- KeyBinding struct ---

    #[test]
    fn test_keybinding_construction() {
        let binding = KeyBinding {
            key: KeySpec::Char('k'),
            modifiers: vec![Modifier::Ctrl],
            action: Action::PrevStation,
            mode: InputMode::Normal,
        };
        assert_eq!(binding.key, KeySpec::Char('k'));
        assert_eq!(binding.modifiers, vec![Modifier::Ctrl]);
        assert_eq!(binding.action, Action::PrevStation);
        assert_eq!(binding.mode, InputMode::Normal);
    }
}
