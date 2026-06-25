// TOML parsing: deserializes a TOML string into AppConfig + preserved Value.

use crate::theme_name::ThemeName;

use super::{
    AppConfig, AudioConfig, KeybindingsConfig, PlaybackConfig, ScrobbleConfig, ScrobbleService,
    UiConfig,
};

/// Error wrapper for TOML parse failures.
#[derive(Debug)]
pub struct ParseError {
    pub message: String,
    pub line: Option<usize>,
    pub col: Option<usize>,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (self.line, self.col) {
            (Some(l), Some(c)) => write!(f, "{} (line {}, col {})", self.message, l, c),
            _ => write!(f, "{}", self.message),
        }
    }
}

impl std::error::Error for ParseError {}

/// Result of parsing TOML: config, preserved full document, and any warnings.
#[derive(Debug)]
pub struct ParseResult {
    pub config: AppConfig,
    pub preserved: toml::Value,
    pub warnings: Vec<String>,
}

/// Parse a TOML string into AppConfig + preserved Value + warnings.
pub fn parse_toml(input: &str) -> Result<ParseResult, ParseError> {
    let value: toml::Value = input.parse().map_err(|e: toml::de::Error| {
        let msg = e.message().to_string();
        let span = e.span();
        let (line, col) = span
            .map(|s| line_col_from_offset(input, s.start))
            .unwrap_or((None, None));
        ParseError {
            message: msg,
            line,
            col,
        }
    })?;

    let mut warnings = Vec::new();
    let table = value.as_table().cloned().unwrap_or_default();

    let audio = parse_audio_section(&table, &mut warnings);
    let ui = parse_ui_section(&table, &mut warnings);
    let playback = parse_playback_section(&table);
    let scrobble = parse_scrobble_section(&table, &mut warnings);
    let keybindings = parse_keybindings_section(&table);

    let config = AppConfig {
        audio,
        ui,
        playback,
        scrobble,
        keybindings,
    };

    Ok(ParseResult {
        config,
        preserved: value,
        warnings,
    })
}

fn parse_audio_section(table: &toml::map::Map<String, toml::Value>, warnings: &mut Vec<String>) -> AudioConfig {
    let section = table.get("audio").and_then(|v| v.as_table());
    let Some(sec) = section else {
        return AudioConfig::default();
    };

    let output_device = sec
        .get("output_device")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let default_volume = sec
        .get("default_volume")
        .and_then(|v| v.as_integer())
        .map(|v| clamp_volume(v, warnings))
        .unwrap_or(80);

    AudioConfig {
        output_device,
        default_volume,
    }
}

fn clamp_volume(raw: i64, warnings: &mut Vec<String>) -> u8 {
    if raw < 0 {
        warnings.push(format!("audio.default_volume: '{}' is invalid, clamped to 0", raw));
        0
    } else if raw > 100 {
        warnings.push(format!("audio.default_volume: '{}' is invalid, clamped to 100", raw));
        100
    } else {
        raw as u8
    }
}

fn parse_ui_section(table: &toml::map::Map<String, toml::Value>, warnings: &mut Vec<String>) -> UiConfig {
    let section = table.get("ui").and_then(|v| v.as_table());
    let Some(sec) = section else {
        return UiConfig::default();
    };

    let theme_str = sec.get("theme").and_then(|v| v.as_str()).unwrap_or("Retrowave");
    let theme = validate_theme(theme_str, warnings);

    let notifications_enabled = sec
        .get("notifications_enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let stream_metadata_enabled = sec
        .get("stream_metadata_enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    UiConfig {
        theme,
        notifications_enabled,
        stream_metadata_enabled,
    }
}

fn validate_theme(raw: &str, warnings: &mut Vec<String>) -> String {
    let is_known = ThemeName::ALL.iter().any(|t| t.label() == raw);
    if is_known {
        raw.to_string()
    } else {
        warnings.push(format!("ui.theme: '{}' is invalid, using default 'Retrowave'", raw));
        "Retrowave".to_string()
    }
}

fn parse_playback_section(table: &toml::map::Map<String, toml::Value>) -> PlaybackConfig {
    let section = table.get("playback").and_then(|v| v.as_table());
    let Some(sec) = section else {
        return PlaybackConfig::default();
    };

    let autoplay_last = sec.get("autoplay_last").and_then(|v| v.as_bool()).unwrap_or(false);
    let save_history = sec.get("save_history").and_then(|v| v.as_bool()).unwrap_or(false);

    PlaybackConfig {
        autoplay_last,
        save_history,
    }
}

fn parse_scrobble_section(
    table: &toml::map::Map<String, toml::Value>,
    warnings: &mut Vec<String>,
) -> ScrobbleConfig {
    let section = table.get("scrobble").and_then(|v| v.as_table());
    let Some(sec) = section else {
        return ScrobbleConfig::default();
    };

    let enabled = sec.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false);
    let api_key = sec
        .get("api_key")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let service_str = sec.get("service").and_then(|v| v.as_str()).unwrap_or("lastfm");
    let (service, svc_enabled) = resolve_scrobble_service(service_str, enabled, warnings);

    ScrobbleConfig {
        enabled: svc_enabled,
        service,
        api_key,
    }
}

fn resolve_scrobble_service(
    raw: &str,
    enabled: bool,
    warnings: &mut Vec<String>,
) -> (ScrobbleService, bool) {
    match raw {
        "lastfm" => (ScrobbleService::LastFm, enabled),
        "listenbrainz" => (ScrobbleService::ListenBrainz, enabled),
        _ => {
            warnings.push(format!(
                "scrobble.service: '{}' is invalid, disabling scrobble",
                raw
            ));
            (ScrobbleService::LastFm, false)
        }
    }
}

fn parse_keybindings_section(table: &toml::map::Map<String, toml::Value>) -> KeybindingsConfig {
    let section = table.get("keybindings").and_then(|v| v.as_table());
    let Some(sec) = section else {
        return KeybindingsConfig::default();
    };

    let path = sec.get("path").and_then(|v| v.as_str()).map(|s| s.to_string());

    KeybindingsConfig { path }
}

fn line_col_from_offset(input: &str, offset: usize) -> (Option<usize>, Option<usize>) {
    let before = &input[..offset.min(input.len())];
    let line = before.lines().count();
    let last_newline = before.rfind('\n').map(|i| i + 1).unwrap_or(0);
    let col = offset - last_newline + 1;
    (Some(line), Some(col))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL_CONFIG: &str = r#"
[audio]
output_device = "Built-in Speakers"
default_volume = 80

[ui]
theme = "Retrowave"
notifications_enabled = true
stream_metadata_enabled = true

[playback]
autoplay_last = false
save_history = false

[scrobble]
enabled = false
service = "lastfm"
api_key = ""

[keybindings]
path = "keybindings.json"
"#;

    #[test]
    fn test_parse_toml_full_config_parses_correctly() {
        let result = parse_toml(FULL_CONFIG).unwrap();

        assert_eq!(result.config.audio.output_device, Some("Built-in Speakers".to_string()));
        assert_eq!(result.config.audio.default_volume, 80);
        assert_eq!(result.config.ui.theme, "Retrowave");
        assert!(result.config.ui.notifications_enabled);
        assert!(result.config.ui.stream_metadata_enabled);
        assert!(!result.config.playback.autoplay_last);
        assert!(!result.config.playback.save_history);
        assert!(!result.config.scrobble.enabled);
        assert_eq!(result.config.scrobble.service, ScrobbleService::LastFm);
        assert_eq!(result.config.scrobble.api_key, "");
        assert_eq!(result.config.keybindings.path, Some("keybindings.json".to_string()));
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_parse_toml_partial_config_uses_defaults() {
        let input = "[audio]\ndefault_volume = 50\n";
        let result = parse_toml(input).unwrap();

        assert_eq!(result.config.audio.default_volume, 50);
        assert_eq!(result.config.audio.output_device, None);
        // Other sections use defaults
        assert_eq!(result.config.ui, UiConfig::default());
        assert_eq!(result.config.playback, PlaybackConfig::default());
        assert_eq!(result.config.scrobble, ScrobbleConfig::default());
        assert_eq!(result.config.keybindings, KeybindingsConfig::default());
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_parse_toml_empty_string_returns_all_defaults() {
        let result = parse_toml("").unwrap();

        assert_eq!(result.config, AppConfig::default());
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_parse_toml_volume_above_100_clamped() {
        let input = "[audio]\ndefault_volume = 150\n";
        let result = parse_toml(input).unwrap();

        assert_eq!(result.config.audio.default_volume, 100);
        assert_eq!(result.warnings.len(), 1);
        assert_eq!(
            result.warnings[0],
            "audio.default_volume: '150' is invalid, clamped to 100"
        );
    }

    #[test]
    fn test_parse_toml_volume_below_0_clamped() {
        let input = "[audio]\ndefault_volume = -5\n";
        let result = parse_toml(input).unwrap();

        assert_eq!(result.config.audio.default_volume, 0);
        assert_eq!(result.warnings.len(), 1);
        assert_eq!(
            result.warnings[0],
            "audio.default_volume: '-5' is invalid, clamped to 0"
        );
    }

    #[test]
    fn test_parse_toml_unknown_theme_falls_back_to_retrowave() {
        let input = "[ui]\ntheme = \"NonExistent\"\n";
        let result = parse_toml(input).unwrap();

        assert_eq!(result.config.ui.theme, "Retrowave");
        assert_eq!(result.warnings.len(), 1);
        assert_eq!(
            result.warnings[0],
            "ui.theme: 'NonExistent' is invalid, using default 'Retrowave'"
        );
    }

    #[test]
    fn test_parse_toml_valid_themes_accepted() {
        for theme in ["Retrowave", "Catppuccin Mocha", "Catppuccin Macchiato",
                      "Catppuccin Frappé", "Catppuccin Latte", "Terminal"] {
            let input = format!("[ui]\ntheme = \"{}\"\n", theme);
            let result = parse_toml(&input).unwrap();
            assert_eq!(result.config.ui.theme, theme);
            assert!(result.warnings.is_empty(), "unexpected warning for theme '{}'", theme);
        }
    }

    #[test]
    fn test_parse_toml_unknown_scrobble_service_disables() {
        let input = "[scrobble]\nenabled = true\nservice = \"spotify\"\napi_key = \"abc\"\n";
        let result = parse_toml(input).unwrap();

        assert!(!result.config.scrobble.enabled);
        assert_eq!(result.warnings.len(), 1);
        assert_eq!(
            result.warnings[0],
            "scrobble.service: 'spotify' is invalid, disabling scrobble"
        );
    }

    #[test]
    fn test_parse_toml_listenbrainz_service_parses() {
        let input = "[scrobble]\nenabled = true\nservice = \"listenbrainz\"\napi_key = \"key\"\n";
        let result = parse_toml(input).unwrap();

        assert!(result.config.scrobble.enabled);
        assert_eq!(result.config.scrobble.service, ScrobbleService::ListenBrainz);
    }

    #[test]
    fn test_parse_toml_unknown_keys_preserved() {
        let input = r#"
[audio]
default_volume = 75
unknown_key = true

[custom_section]
foo = "bar"
"#;
        let result = parse_toml(input).unwrap();

        assert_eq!(result.config.audio.default_volume, 75);
        // The preserved Value contains the unknown keys
        let preserved_table = result.preserved.as_table().unwrap();
        assert!(preserved_table.contains_key("custom_section"));
        let audio_table = preserved_table.get("audio").unwrap().as_table().unwrap();
        assert!(audio_table.contains_key("unknown_key"));
    }

    #[test]
    fn test_parse_toml_invalid_toml_returns_error() {
        let input = "this is not valid [toml";
        let result = parse_toml(input);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(!err.message.is_empty());
    }

    #[test]
    fn test_parse_toml_error_includes_location() {
        let input = "[audio]\ndefault_volume = ???\n";
        let result = parse_toml(input);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.line.is_some());
        assert!(err.col.is_some());
    }

    #[test]
    fn test_warning_format_volume_uses_dotted_path_pattern() {
        let input = "[audio]\ndefault_volume = 200\n";
        let result = parse_toml(input).unwrap();

        let warning = &result.warnings[0];
        // Format: "{field_path}: '{value}' is invalid, {action_taken}"
        assert!(warning.starts_with("audio.default_volume: '"));
        assert!(warning.contains("' is invalid, "));
        assert!(warning.ends_with("clamped to 100"));
    }

    #[test]
    fn test_warning_format_theme_uses_dotted_path_pattern() {
        let input = "[ui]\ntheme = \"Bogus\"\n";
        let result = parse_toml(input).unwrap();

        let warning = &result.warnings[0];
        // Format: "{field_path}: '{value}' is invalid, {action_taken}"
        assert!(warning.starts_with("ui.theme: '"));
        assert!(warning.contains("' is invalid, "));
        assert!(warning.ends_with("using default 'Retrowave'"));
    }

    #[test]
    fn test_warning_format_scrobble_service_uses_dotted_path_pattern() {
        let input = "[scrobble]\nenabled = true\nservice = \"pandora\"\napi_key = \"k\"\n";
        let result = parse_toml(input).unwrap();

        let warning = &result.warnings[0];
        // Format: "{field_path}: '{value}' is invalid, {action_taken}"
        assert!(warning.starts_with("scrobble.service: '"));
        assert!(warning.contains("' is invalid, "));
        assert!(warning.ends_with("disabling scrobble"));
    }

    #[test]
    fn test_warning_format_volume_negative_includes_value() {
        let input = "[audio]\ndefault_volume = -999\n";
        let result = parse_toml(input).unwrap();

        assert_eq!(
            result.warnings[0],
            "audio.default_volume: '-999' is invalid, clamped to 0"
        );
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use crate::config_toml::serialize::serialize_toml;
    use crate::theme_name::ThemeName;
    use proptest::prelude::*;

    // Feature: v080-features, Property 14: Volume clamping invariant
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Validates: Requirements 8.3, 8.4**
        #[test]
        fn prop_volume_clamping(value in proptest::num::i64::ANY) {
            let input = format!("[audio]\ndefault_volume = {}\n", value);
            let result = parse_toml(&input).unwrap();
            let vol = result.config.audio.default_volume;

            // Volume must always be in [0, 100]
            prop_assert!(vol <= 100, "volume {} out of range for input {}", vol, value);

            // Verify exact clamping behavior
            if value < 0 {
                prop_assert_eq!(vol, 0, "negative input {} should clamp to 0", value);
            } else if value > 100 {
                prop_assert_eq!(vol, 100, "input {} above 100 should clamp to 100", value);
            } else {
                prop_assert_eq!(vol, value as u8, "input {} in range should be preserved", value);
            }
        }
    }

    /// Generate a valid TOML section name (alphabetic, no conflicts with known sections).
    fn arb_unknown_section_name() -> impl Strategy<Value = String> {
        "[a-z]{3,8}".prop_filter("must not collide with known sections", |s| {
            !matches!(s.as_str(), "audio" | "ui" | "playback" | "scrobble" | "keybindings")
        })
    }

    /// Generate a simple TOML-safe key name.
    fn arb_key_name() -> impl Strategy<Value = String> {
        "[a-z_]{2,10}"
    }

    /// Generate a simple TOML-safe string value.
    fn arb_string_value() -> impl Strategy<Value = String> {
        "[a-zA-Z0-9 ]{1,20}"
    }

    // Feature: v080-features, Property 13: Unknown key preservation
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Validates: Requirements 7.9, 8.2**
        #[test]
        fn prop_unknown_key_preservation(
            section_name in arb_unknown_section_name(),
            keys in proptest::collection::vec(
                (arb_key_name(), arb_string_value()),
                1..=3
            ),
        ) {
            // Build a TOML string with valid config + unknown section
            let mut toml_str = String::from("[audio]\ndefault_volume = 80\n\n");
            toml_str.push_str(&format!("[{}]\n", section_name));
            for (key, val) in &keys {
                toml_str.push_str(&format!("{} = \"{}\"\n", key, val));
            }

            // Parse → serialize → re-parse
            let parsed = parse_toml(&toml_str).unwrap();
            let serialized = serialize_toml(&parsed.config, &parsed.preserved);
            let reparsed: toml::Value = serialized.parse().unwrap();

            // Assert unknown section and keys are still present
            let table = reparsed.as_table().unwrap();
            prop_assert!(
                table.contains_key(&section_name),
                "unknown section '{}' was lost after round-trip", section_name
            );

            let section = table.get(&section_name).unwrap().as_table().unwrap();
            for (key, val) in &keys {
                prop_assert!(
                    section.contains_key(key),
                    "unknown key '{}' in section '{}' was lost", key, section_name
                );
                prop_assert_eq!(
                    section.get(key).unwrap().as_str().unwrap(),
                    val.as_str(),
                    "value for key '{}' changed after round-trip", key
                );
            }
        }
    }

    /// Pick from the valid theme labels.
    fn arb_valid_theme() -> impl Strategy<Value = String> {
        prop::sample::select(
            ThemeName::ALL.iter().map(|t| t.label().to_string()).collect::<Vec<_>>()
        )
    }

    /// Generate a random string that may or may not be a valid theme.
    fn arb_theme_string() -> impl Strategy<Value = String> {
        prop_oneof![
            arb_valid_theme(),
            "[a-zA-Z0-9 ]{1,20}",
        ]
    }

    // Feature: v080-features, Property 15: Theme validation invariant
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Validates: Requirements 8.5, 8.6**
        #[test]
        fn prop_theme_validation(theme_input in arb_theme_string()) {
            let input = format!("[ui]\ntheme = \"{}\"\n", theme_input);
            let result = parse_toml(&input).unwrap();
            let loaded_theme = &result.config.ui.theme;

            // The loaded theme must always be a valid ThemeName label
            let valid_labels: Vec<&str> = ThemeName::ALL.iter().map(|t| t.label()).collect();
            prop_assert!(
                valid_labels.contains(&loaded_theme.as_str()),
                "loaded theme '{}' is not a valid theme name", loaded_theme
            );

            // If input was valid, it should be preserved; otherwise default to Retrowave
            if valid_labels.contains(&theme_input.as_str()) {
                prop_assert_eq!(loaded_theme, &theme_input);
            } else {
                prop_assert_eq!(loaded_theme.as_str(), "Retrowave");
            }
        }
    }

    /// Generate a random subset of config fields to include in the TOML.
    fn arb_partial_config() -> impl Strategy<Value = (
        Option<u8>,         // default_volume
        Option<String>,     // output_device
        Option<bool>,       // notifications_enabled
        Option<bool>,       // stream_metadata_enabled
        Option<bool>,       // autoplay_last
        Option<bool>,       // save_history
        Option<bool>,       // scrobble enabled
        Option<String>,     // api_key
        Option<String>,     // keybindings path
    )> {
        (
            proptest::option::of(0..=100u8),
            proptest::option::of("[a-zA-Z ]{3,15}"),
            proptest::option::of(proptest::bool::ANY),
            proptest::option::of(proptest::bool::ANY),
            proptest::option::of(proptest::bool::ANY),
            proptest::option::of(proptest::bool::ANY),
            proptest::option::of(proptest::bool::ANY),
            proptest::option::of("[a-zA-Z0-9]{5,20}"),
            proptest::option::of("[a-zA-Z0-9./]{3,15}"),
        )
    }

    // Feature: v080-features, Property 16: Missing fields produce correct defaults
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Validates: Requirements 8.1**
        #[test]
        fn prop_missing_fields_produce_defaults(
            partial in arb_partial_config()
        ) {
            let (volume, device, notif, meta, autoplay, history, scr_enabled, api_key, kb_path) = partial;

            let mut toml_str = String::new();

            // Build audio section only if we have audio fields
            if volume.is_some() || device.is_some() {
                toml_str.push_str("[audio]\n");
                if let Some(v) = volume {
                    toml_str.push_str(&format!("default_volume = {}\n", v));
                }
                if let Some(ref d) = device {
                    toml_str.push_str(&format!("output_device = \"{}\"\n", d));
                }
            }

            // Build ui section only if we have ui fields
            if notif.is_some() || meta.is_some() {
                toml_str.push_str("[ui]\n");
                if let Some(n) = notif {
                    toml_str.push_str(&format!("notifications_enabled = {}\n", n));
                }
                if let Some(m) = meta {
                    toml_str.push_str(&format!("stream_metadata_enabled = {}\n", m));
                }
            }

            // Build playback section
            if autoplay.is_some() || history.is_some() {
                toml_str.push_str("[playback]\n");
                if let Some(a) = autoplay {
                    toml_str.push_str(&format!("autoplay_last = {}\n", a));
                }
                if let Some(h) = history {
                    toml_str.push_str(&format!("save_history = {}\n", h));
                }
            }

            // Build scrobble section
            if scr_enabled.is_some() || api_key.is_some() {
                toml_str.push_str("[scrobble]\n");
                if let Some(e) = scr_enabled {
                    toml_str.push_str(&format!("enabled = {}\n", e));
                }
                if let Some(ref k) = api_key {
                    toml_str.push_str(&format!("api_key = \"{}\"\n", k));
                }
            }

            // Build keybindings section
            if let Some(ref p) = kb_path {
                toml_str.push_str("[keybindings]\n");
                toml_str.push_str(&format!("path = \"{}\"\n", p));
            }

            let result = parse_toml(&toml_str).unwrap();
            let defaults = AppConfig::default();

            // Present fields retain their values
            if let Some(v) = volume {
                prop_assert_eq!(result.config.audio.default_volume, v);
            } else {
                prop_assert_eq!(result.config.audio.default_volume, defaults.audio.default_volume);
            }

            if device.is_some() {
                prop_assert_eq!(&result.config.audio.output_device, &device);
            } else {
                prop_assert_eq!(result.config.audio.output_device, defaults.audio.output_device);
            }

            if let Some(n) = notif {
                prop_assert_eq!(result.config.ui.notifications_enabled, n);
            } else {
                prop_assert_eq!(result.config.ui.notifications_enabled, defaults.ui.notifications_enabled);
            }

            if let Some(m) = meta {
                prop_assert_eq!(result.config.ui.stream_metadata_enabled, m);
            } else {
                prop_assert_eq!(result.config.ui.stream_metadata_enabled, defaults.ui.stream_metadata_enabled);
            }

            if let Some(a) = autoplay {
                prop_assert_eq!(result.config.playback.autoplay_last, a);
            } else {
                prop_assert_eq!(result.config.playback.autoplay_last, defaults.playback.autoplay_last);
            }

            if let Some(h) = history {
                prop_assert_eq!(result.config.playback.save_history, h);
            } else {
                prop_assert_eq!(result.config.playback.save_history, defaults.playback.save_history);
            }

            if let Some(e) = scr_enabled {
                prop_assert_eq!(result.config.scrobble.enabled, e);
            } else {
                prop_assert_eq!(result.config.scrobble.enabled, defaults.scrobble.enabled);
            }

            if let Some(ref k) = api_key {
                prop_assert_eq!(&result.config.scrobble.api_key, k);
            } else {
                prop_assert_eq!(result.config.scrobble.api_key, defaults.scrobble.api_key);
            }

            if kb_path.is_some() {
                prop_assert_eq!(&result.config.keybindings.path, &kb_path);
            } else {
                prop_assert_eq!(result.config.keybindings.path, defaults.keybindings.path);
            }
        }
    }
}
