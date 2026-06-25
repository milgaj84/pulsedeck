// TOML parsing: deserializes a TOML string into AppConfig + preserved Value.

use crate::theme_name::ThemeName;

use super::{AppConfig, AudioConfig, DiscoverConfig, KeybindingsConfig, PlaybackConfig, UiConfig};

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
    let playback = parse_playback_section(&table, &mut warnings);
    let keybindings = parse_keybindings_section(&table);

    let discover = parse_discover_section(&table, &mut warnings);

    let config = AppConfig {
        audio,
        ui,
        playback,
        keybindings,
        discover,
    };

    Ok(ParseResult {
        config,
        preserved: value,
        warnings,
    })
}

fn parse_audio_section(
    table: &toml::map::Map<String, toml::Value>,
    warnings: &mut Vec<String>,
) -> AudioConfig {
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
        warnings.push(format!(
            "audio.default_volume: '{}' is invalid, clamped to 0",
            raw
        ));
        0
    } else if raw > 100 {
        warnings.push(format!(
            "audio.default_volume: '{}' is invalid, clamped to 100",
            raw
        ));
        100
    } else {
        raw as u8
    }
}

fn parse_ui_section(
    table: &toml::map::Map<String, toml::Value>,
    warnings: &mut Vec<String>,
) -> UiConfig {
    let section = table.get("ui").and_then(|v| v.as_table());
    let Some(sec) = section else {
        return UiConfig::default();
    };

    let theme_str = sec
        .get("theme")
        .and_then(|v| v.as_str())
        .unwrap_or("Retrowave");
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
        warnings.push(format!(
            "ui.theme: '{}' is invalid, using default 'Retrowave'",
            raw
        ));
        "Retrowave".to_string()
    }
}

fn parse_playback_section(
    table: &toml::map::Map<String, toml::Value>,
    warnings: &mut Vec<String>,
) -> PlaybackConfig {
    let section = table.get("playback").and_then(|v| v.as_table());
    let Some(sec) = section else {
        return PlaybackConfig::default();
    };

    let autoplay_last = sec
        .get("autoplay_last")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let save_history = sec
        .get("save_history")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let reconnect_max_attempts = parse_reconnect_max_attempts(sec, warnings);
    let reconnect_backoff_seconds = parse_reconnect_backoff_seconds(sec, warnings);
    let device_recovery_attempts = parse_device_recovery_attempts(sec, warnings);
    let device_recovery_delay_ms = parse_device_recovery_delay_ms(sec, warnings);

    PlaybackConfig {
        autoplay_last,
        save_history,
        reconnect_max_attempts,
        reconnect_backoff_seconds,
        device_recovery_attempts,
        device_recovery_delay_ms,
    }
}

fn parse_reconnect_max_attempts(
    sec: &toml::map::Map<String, toml::Value>,
    warnings: &mut Vec<String>,
) -> u8 {
    sec.get("reconnect_max_attempts")
        .and_then(|v| v.as_integer())
        .map(|v| clamp_i64(v, 1, 10, "playback.reconnect_max_attempts", warnings) as u8)
        .unwrap_or(3)
}

fn parse_reconnect_backoff_seconds(
    sec: &toml::map::Map<String, toml::Value>,
    warnings: &mut Vec<String>,
) -> Vec<u64> {
    let default = vec![3, 6, 12];
    let Some(val) = sec.get("reconnect_backoff_seconds") else {
        return default;
    };
    let Some(arr) = val.as_array() else {
        return default;
    };

    if arr.is_empty() {
        warnings.push(
            "playback.reconnect_backoff_seconds: '[]' is invalid, using default [3, 6, 12]"
                .to_string(),
        );
        return default;
    }

    let mut result: Vec<u64> = arr
        .iter()
        .filter_map(|v| v.as_integer())
        .map(|v| {
            clamp_i64(
                v,
                1,
                60,
                "playback.reconnect_backoff_seconds element",
                warnings,
            ) as u64
        })
        .collect();

    if result.is_empty() {
        return default;
    }

    if result.len() > 10 {
        warnings.push(format!(
            "playback.reconnect_backoff_seconds: '{}' entries is invalid, truncated to 10",
            result.len()
        ));
        result.truncate(10);
    }

    result
}

fn parse_device_recovery_attempts(
    sec: &toml::map::Map<String, toml::Value>,
    warnings: &mut Vec<String>,
) -> u8 {
    sec.get("device_recovery_attempts")
        .and_then(|v| v.as_integer())
        .map(|v| clamp_i64(v, 1, 5, "playback.device_recovery_attempts", warnings) as u8)
        .unwrap_or(2)
}

fn parse_device_recovery_delay_ms(
    sec: &toml::map::Map<String, toml::Value>,
    warnings: &mut Vec<String>,
) -> u64 {
    sec.get("device_recovery_delay_ms")
        .and_then(|v| v.as_integer())
        .map(|v| clamp_i64(v, 100, 5000, "playback.device_recovery_delay_ms", warnings) as u64)
        .unwrap_or(1000)
}

fn clamp_i64(raw: i64, min: i64, max: i64, field: &str, warnings: &mut Vec<String>) -> i64 {
    if raw < min {
        warnings.push(format!(
            "{}: '{}' is invalid, clamped to {}",
            field, raw, min
        ));
        min
    } else if raw > max {
        warnings.push(format!(
            "{}: '{}' is invalid, clamped to {}",
            field, raw, max
        ));
        max
    } else {
        raw
    }
}

fn parse_keybindings_section(table: &toml::map::Map<String, toml::Value>) -> KeybindingsConfig {
    let section = table.get("keybindings").and_then(|v| v.as_table());
    let Some(sec) = section else {
        return KeybindingsConfig::default();
    };

    let path = sec
        .get("path")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    KeybindingsConfig { path }
}

pub fn parse_discover_section(
    table: &toml::map::Map<String, toml::Value>,
    warnings: &mut Vec<String>,
) -> DiscoverConfig {
    let section = table.get("discover").and_then(|v| v.as_table());
    let Some(sec) = section else {
        return DiscoverConfig::default();
    };

    let genre_weight = parse_weight(sec, "genre_weight", 3, warnings);
    let tag_weight = parse_weight(sec, "tag_weight", 1, warnings);
    let country_weight = parse_weight(sec, "country_weight", 1, warnings);
    let exclude_tags = parse_string_list(sec, "exclude_tags", Normalization::Lower, 100, warnings);
    let exclude_countries =
        parse_string_list(sec, "exclude_countries", Normalization::Upper, 10, warnings);

    DiscoverConfig {
        genre_weight,
        tag_weight,
        country_weight,
        exclude_tags,
        exclude_countries,
    }
}

fn parse_weight(
    sec: &toml::map::Map<String, toml::Value>,
    field: &str,
    default: u32,
    warnings: &mut Vec<String>,
) -> u32 {
    let Some(raw) = sec.get(field).and_then(|v| v.as_integer()) else {
        return default;
    };
    clamp_weight(raw, field, warnings)
}

fn clamp_weight(raw: i64, field: &str, warnings: &mut Vec<String>) -> u32 {
    if raw < 0 {
        warnings.push(format!(
            "discover.{}: '{}' is invalid, clamped to 0",
            field, raw
        ));
        0
    } else if raw > 10 {
        warnings.push(format!(
            "discover.{}: '{}' is invalid, clamped to 10",
            field, raw
        ));
        10
    } else {
        raw as u32
    }
}

enum Normalization {
    Lower,
    Upper,
}

fn parse_string_list(
    sec: &toml::map::Map<String, toml::Value>,
    field: &str,
    normalization: Normalization,
    max_entry_chars: usize,
    warnings: &mut Vec<String>,
) -> Vec<String> {
    let Some(arr) = sec.get(field).and_then(|v| v.as_array()) else {
        return Vec::new();
    };

    let mut result: Vec<String> = arr
        .iter()
        .filter_map(|v| v.as_str())
        .map(|s| normalize_entry(s, &normalization, max_entry_chars))
        .filter(|s| !s.is_empty())
        .collect();

    if result.len() > 50 {
        warnings.push(format!(
            "discover.{}: list exceeds 50 entries, truncated",
            field
        ));
        result.truncate(50);
    }

    result
}

fn normalize_entry(raw: &str, normalization: &Normalization, max_chars: usize) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let normalized = match normalization {
        Normalization::Lower => trimmed.to_lowercase(),
        Normalization::Upper => trimmed.to_uppercase(),
    };
    let truncated = if normalized.chars().count() > max_chars {
        normalized
            .chars()
            .take(max_chars)
            .collect::<String>()
            .trim_end()
            .to_string()
    } else {
        normalized
    };
    if truncated.is_empty() {
        String::new()
    } else {
        truncated
    }
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

[keybindings]
path = "keybindings.json"
"#;

    #[test]
    fn test_parse_toml_full_config_parses_correctly() {
        let result = parse_toml(FULL_CONFIG).unwrap();

        assert_eq!(
            result.config.audio.output_device,
            Some("Built-in Speakers".to_string())
        );
        assert_eq!(result.config.audio.default_volume, 80);
        assert_eq!(result.config.ui.theme, "Retrowave");
        assert!(result.config.ui.notifications_enabled);
        assert!(result.config.ui.stream_metadata_enabled);
        assert!(!result.config.playback.autoplay_last);
        assert!(!result.config.playback.save_history);
        assert_eq!(
            result.config.keybindings.path,
            Some("keybindings.json".to_string())
        );
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
        for theme in [
            "Retrowave",
            "Catppuccin Mocha",
            "Catppuccin Macchiato",
            "Catppuccin Frappé",
            "Catppuccin Latte",
            "Terminal",
        ] {
            let input = format!("[ui]\ntheme = \"{}\"\n", theme);
            let result = parse_toml(&input).unwrap();
            assert_eq!(result.config.ui.theme, theme);
            assert!(
                result.warnings.is_empty(),
                "unexpected warning for theme '{}'",
                theme
            );
        }
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
    fn test_warning_format_volume_negative_includes_value() {
        let input = "[audio]\ndefault_volume = -999\n";
        let result = parse_toml(input).unwrap();

        assert_eq!(
            result.warnings[0],
            "audio.default_volume: '-999' is invalid, clamped to 0"
        );
    }

    // --- Playback section: reconnect_max_attempts ---

    #[test]
    fn test_reconnect_max_attempts_valid_value_parsed() {
        let input = "[playback]\nreconnect_max_attempts = 7\n";
        let result = parse_toml(input).unwrap();

        assert_eq!(result.config.playback.reconnect_max_attempts, 7);
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_reconnect_max_attempts_below_range_clamped() {
        let input = "[playback]\nreconnect_max_attempts = 0\n";
        let result = parse_toml(input).unwrap();

        assert_eq!(result.config.playback.reconnect_max_attempts, 1);
        assert_eq!(result.warnings.len(), 1);
        assert_eq!(
            result.warnings[0],
            "playback.reconnect_max_attempts: '0' is invalid, clamped to 1"
        );
    }

    #[test]
    fn test_reconnect_max_attempts_above_range_clamped() {
        let input = "[playback]\nreconnect_max_attempts = 99\n";
        let result = parse_toml(input).unwrap();

        assert_eq!(result.config.playback.reconnect_max_attempts, 10);
        assert_eq!(result.warnings.len(), 1);
        assert_eq!(
            result.warnings[0],
            "playback.reconnect_max_attempts: '99' is invalid, clamped to 10"
        );
    }

    #[test]
    fn test_reconnect_max_attempts_missing_uses_default() {
        let input = "[playback]\nautoplay_last = true\n";
        let result = parse_toml(input).unwrap();

        assert_eq!(result.config.playback.reconnect_max_attempts, 3);
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_reconnect_max_attempts_wrong_type_uses_default() {
        let input = "[playback]\nreconnect_max_attempts = \"hello\"\n";
        let result = parse_toml(input).unwrap();

        assert_eq!(result.config.playback.reconnect_max_attempts, 3);
        assert!(result.warnings.is_empty());
    }

    // --- Playback section: reconnect_backoff_seconds ---

    #[test]
    fn test_reconnect_backoff_seconds_valid_values_parsed() {
        let input = "[playback]\nreconnect_backoff_seconds = [2, 5, 10, 30]\n";
        let result = parse_toml(input).unwrap();

        assert_eq!(
            result.config.playback.reconnect_backoff_seconds,
            vec![2, 5, 10, 30]
        );
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_reconnect_backoff_seconds_elements_clamped() {
        let input = "[playback]\nreconnect_backoff_seconds = [0, 5, 100]\n";
        let result = parse_toml(input).unwrap();

        assert_eq!(
            result.config.playback.reconnect_backoff_seconds,
            vec![1, 5, 60]
        );
        assert_eq!(result.warnings.len(), 2);
        assert!(result.warnings[0].contains("clamped to 1"));
        assert!(result.warnings[1].contains("clamped to 60"));
    }

    #[test]
    fn test_reconnect_backoff_seconds_empty_list_uses_default() {
        let input = "[playback]\nreconnect_backoff_seconds = []\n";
        let result = parse_toml(input).unwrap();

        assert_eq!(
            result.config.playback.reconnect_backoff_seconds,
            vec![3, 6, 12]
        );
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].contains("using default [3, 6, 12]"));
    }

    #[test]
    fn test_reconnect_backoff_seconds_oversized_list_truncated() {
        let input = "[playback]\nreconnect_backoff_seconds = [1,2,3,4,5,6,7,8,9,10,11,12]\n";
        let result = parse_toml(input).unwrap();

        assert_eq!(result.config.playback.reconnect_backoff_seconds.len(), 10);
        assert_eq!(
            result.config.playback.reconnect_backoff_seconds,
            vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
        );
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].contains("truncated to 10"));
    }

    #[test]
    fn test_reconnect_backoff_seconds_missing_uses_default() {
        let input = "[playback]\nautoplay_last = true\n";
        let result = parse_toml(input).unwrap();

        assert_eq!(
            result.config.playback.reconnect_backoff_seconds,
            vec![3, 6, 12]
        );
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_reconnect_backoff_seconds_wrong_type_uses_default() {
        let input = "[playback]\nreconnect_backoff_seconds = \"not an array\"\n";
        let result = parse_toml(input).unwrap();

        assert_eq!(
            result.config.playback.reconnect_backoff_seconds,
            vec![3, 6, 12]
        );
        assert!(result.warnings.is_empty());
    }

    // --- Playback section: device_recovery_attempts ---

    #[test]
    fn test_device_recovery_attempts_valid_value_parsed() {
        let input = "[playback]\ndevice_recovery_attempts = 4\n";
        let result = parse_toml(input).unwrap();

        assert_eq!(result.config.playback.device_recovery_attempts, 4);
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_device_recovery_attempts_below_range_clamped() {
        let input = "[playback]\ndevice_recovery_attempts = 0\n";
        let result = parse_toml(input).unwrap();

        assert_eq!(result.config.playback.device_recovery_attempts, 1);
        assert_eq!(result.warnings.len(), 1);
        assert_eq!(
            result.warnings[0],
            "playback.device_recovery_attempts: '0' is invalid, clamped to 1"
        );
    }

    #[test]
    fn test_device_recovery_attempts_above_range_clamped() {
        let input = "[playback]\ndevice_recovery_attempts = 20\n";
        let result = parse_toml(input).unwrap();

        assert_eq!(result.config.playback.device_recovery_attempts, 5);
        assert_eq!(result.warnings.len(), 1);
        assert_eq!(
            result.warnings[0],
            "playback.device_recovery_attempts: '20' is invalid, clamped to 5"
        );
    }

    #[test]
    fn test_device_recovery_attempts_missing_uses_default() {
        let input = "[playback]\nautoplay_last = true\n";
        let result = parse_toml(input).unwrap();

        assert_eq!(result.config.playback.device_recovery_attempts, 2);
    }

    #[test]
    fn test_device_recovery_attempts_wrong_type_uses_default() {
        let input = "[playback]\ndevice_recovery_attempts = \"bad\"\n";
        let result = parse_toml(input).unwrap();

        assert_eq!(result.config.playback.device_recovery_attempts, 2);
        assert!(result.warnings.is_empty());
    }

    // --- Playback section: device_recovery_delay_ms ---

    #[test]
    fn test_device_recovery_delay_ms_valid_value_parsed() {
        let input = "[playback]\ndevice_recovery_delay_ms = 2500\n";
        let result = parse_toml(input).unwrap();

        assert_eq!(result.config.playback.device_recovery_delay_ms, 2500);
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_device_recovery_delay_ms_below_range_clamped() {
        let input = "[playback]\ndevice_recovery_delay_ms = 10\n";
        let result = parse_toml(input).unwrap();

        assert_eq!(result.config.playback.device_recovery_delay_ms, 100);
        assert_eq!(result.warnings.len(), 1);
        assert_eq!(
            result.warnings[0],
            "playback.device_recovery_delay_ms: '10' is invalid, clamped to 100"
        );
    }

    #[test]
    fn test_device_recovery_delay_ms_above_range_clamped() {
        let input = "[playback]\ndevice_recovery_delay_ms = 99999\n";
        let result = parse_toml(input).unwrap();

        assert_eq!(result.config.playback.device_recovery_delay_ms, 5000);
        assert_eq!(result.warnings.len(), 1);
        assert_eq!(
            result.warnings[0],
            "playback.device_recovery_delay_ms: '99999' is invalid, clamped to 5000"
        );
    }

    #[test]
    fn test_device_recovery_delay_ms_missing_uses_default() {
        let input = "[playback]\nautoplay_last = true\n";
        let result = parse_toml(input).unwrap();

        assert_eq!(result.config.playback.device_recovery_delay_ms, 1000);
    }

    #[test]
    fn test_device_recovery_delay_ms_wrong_type_uses_default() {
        let input = "[playback]\ndevice_recovery_delay_ms = true\n";
        let result = parse_toml(input).unwrap();

        assert_eq!(result.config.playback.device_recovery_delay_ms, 1000);
        assert!(result.warnings.is_empty());
    }

    // --- Playback section: all new fields together ---

    #[test]
    fn test_playback_all_new_fields_valid() {
        let input = r#"
[playback]
autoplay_last = true
save_history = true
reconnect_max_attempts = 5
reconnect_backoff_seconds = [1, 3, 7]
device_recovery_attempts = 3
device_recovery_delay_ms = 500
"#;
        let result = parse_toml(input).unwrap();
        let pb = &result.config.playback;

        assert!(pb.autoplay_last);
        assert!(pb.save_history);
        assert_eq!(pb.reconnect_max_attempts, 5);
        assert_eq!(pb.reconnect_backoff_seconds, vec![1, 3, 7]);
        assert_eq!(pb.device_recovery_attempts, 3);
        assert_eq!(pb.device_recovery_delay_ms, 500);
        assert!(result.warnings.is_empty());
    }

    // --- Discover section tests ---

    #[test]
    fn test_parse_discover_valid_config() {
        let input = r#"
[discover]
genre_weight = 5
tag_weight = 2
country_weight = 4
exclude_tags = ["jazz", "blues"]
exclude_countries = ["US", "GB"]
"#;
        let result = parse_toml(input).unwrap();

        assert_eq!(result.config.discover.genre_weight, 5);
        assert_eq!(result.config.discover.tag_weight, 2);
        assert_eq!(result.config.discover.country_weight, 4);
        assert_eq!(result.config.discover.exclude_tags, vec!["jazz", "blues"]);
        assert_eq!(result.config.discover.exclude_countries, vec!["US", "GB"]);
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_parse_discover_weights_clamped_above() {
        let input = "[discover]\ngenre_weight = 15\ntag_weight = 20\ncountry_weight = 99\n";
        let result = parse_toml(input).unwrap();

        assert_eq!(result.config.discover.genre_weight, 10);
        assert_eq!(result.config.discover.tag_weight, 10);
        assert_eq!(result.config.discover.country_weight, 10);
        assert_eq!(result.warnings.len(), 3);
        assert_eq!(
            result.warnings[0],
            "discover.genre_weight: '15' is invalid, clamped to 10"
        );
        assert_eq!(
            result.warnings[1],
            "discover.tag_weight: '20' is invalid, clamped to 10"
        );
        assert_eq!(
            result.warnings[2],
            "discover.country_weight: '99' is invalid, clamped to 10"
        );
    }

    #[test]
    fn test_parse_discover_weights_clamped_below() {
        let input = "[discover]\ngenre_weight = -3\ntag_weight = -1\n";
        let result = parse_toml(input).unwrap();

        assert_eq!(result.config.discover.genre_weight, 0);
        assert_eq!(result.config.discover.tag_weight, 0);
        assert_eq!(result.config.discover.country_weight, 1); // default
        assert_eq!(result.warnings.len(), 2);
        assert_eq!(
            result.warnings[0],
            "discover.genre_weight: '-3' is invalid, clamped to 0"
        );
        assert_eq!(
            result.warnings[1],
            "discover.tag_weight: '-1' is invalid, clamped to 0"
        );
    }

    #[test]
    fn test_parse_discover_missing_section_returns_defaults() {
        let input = "[audio]\ndefault_volume = 50\n";
        let result = parse_toml(input).unwrap();

        assert_eq!(result.config.discover, DiscoverConfig::default());
    }

    #[test]
    fn test_parse_discover_missing_fields_returns_defaults() {
        let input = "[discover]\n";
        let result = parse_toml(input).unwrap();

        assert_eq!(result.config.discover.genre_weight, 3);
        assert_eq!(result.config.discover.tag_weight, 1);
        assert_eq!(result.config.discover.country_weight, 1);
        assert!(result.config.discover.exclude_tags.is_empty());
        assert!(result.config.discover.exclude_countries.is_empty());
    }

    #[test]
    fn test_parse_discover_tags_trimmed_and_lowercased() {
        let input = r#"
[discover]
exclude_tags = ["  Jazz  ", "BLUES", " Rock "]
"#;
        let result = parse_toml(input).unwrap();

        assert_eq!(
            result.config.discover.exclude_tags,
            vec!["jazz", "blues", "rock"]
        );
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_parse_discover_tags_discard_empty_and_whitespace() {
        let input = r#"
[discover]
exclude_tags = ["jazz", "", "  ", "blues"]
"#;
        let result = parse_toml(input).unwrap();

        assert_eq!(result.config.discover.exclude_tags, vec!["jazz", "blues"]);
    }

    #[test]
    fn test_parse_discover_countries_trimmed_and_uppercased() {
        let input = r#"
[discover]
exclude_countries = ["  us  ", "gb", " De "]
"#;
        let result = parse_toml(input).unwrap();

        assert_eq!(
            result.config.discover.exclude_countries,
            vec!["US", "GB", "DE"]
        );
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_parse_discover_countries_discard_empty_and_whitespace() {
        let input = r#"
[discover]
exclude_countries = ["US", "", "  ", "GB"]
"#;
        let result = parse_toml(input).unwrap();

        assert_eq!(result.config.discover.exclude_countries, vec!["US", "GB"]);
    }

    #[test]
    fn test_parse_discover_tags_truncated_at_50_entries() {
        let tags: Vec<String> = (0..60).map(|i| format!("\"tag{}\"", i)).collect();
        let input = format!("[discover]\nexclude_tags = [{}]\n", tags.join(", "));
        let result = parse_toml(&input).unwrap();

        assert_eq!(result.config.discover.exclude_tags.len(), 50);
        assert_eq!(result.warnings.len(), 1);
        assert_eq!(
            result.warnings[0],
            "discover.exclude_tags: list exceeds 50 entries, truncated"
        );
    }

    #[test]
    fn test_parse_discover_countries_truncated_at_50_entries() {
        let countries: Vec<String> = (0..55).map(|i| format!("\"C{}\"", i)).collect();
        let input = format!(
            "[discover]\nexclude_countries = [{}]\n",
            countries.join(", ")
        );
        let result = parse_toml(&input).unwrap();

        assert_eq!(result.config.discover.exclude_countries.len(), 50);
        assert_eq!(result.warnings.len(), 1);
        assert_eq!(
            result.warnings[0],
            "discover.exclude_countries: list exceeds 50 entries, truncated"
        );
    }

    #[test]
    fn test_parse_discover_wrong_type_weight_uses_default() {
        let input = "[discover]\ngenre_weight = \"hello\"\n";
        let result = parse_toml(input).unwrap();

        assert_eq!(result.config.discover.genre_weight, 3); // default
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_parse_discover_tag_max_100_chars_truncated() {
        let long_tag = "a".repeat(150);
        let input = format!("[discover]\nexclude_tags = [\"{}\"]\n", long_tag);
        let result = parse_toml(&input).unwrap();

        assert_eq!(result.config.discover.exclude_tags[0].len(), 100);
    }

    #[test]
    fn test_parse_discover_country_max_10_chars_truncated() {
        let long_country = "A".repeat(15);
        let input = format!("[discover]\nexclude_countries = [\"{}\"]\n", long_country);
        let result = parse_toml(&input).unwrap();

        assert_eq!(result.config.discover.exclude_countries[0].len(), 10);
    }

    // --- normalize_entry Unicode truncation tests ---

    #[test]
    fn test_normalize_entry_multibyte_truncated_at_char_boundary() {
        let result = normalize_entry("αβγδεζηθικ_extra", &Normalization::Lower, 10);
        assert_eq!(result, "αβγδεζηθικ");
        assert_eq!(result.chars().count(), 10);
    }

    #[test]
    fn test_normalize_entry_at_exact_limit_preserved() {
        let input: String = "a".repeat(100);
        let result = normalize_entry(&input, &Normalization::Lower, 100);
        assert_eq!(result, input);
        assert_eq!(result.chars().count(), 100);
    }

    #[test]
    fn test_normalize_entry_below_limit_preserved() {
        let input: String = "a".repeat(50);
        let result = normalize_entry(&input, &Normalization::Lower, 100);
        assert_eq!(result, input);
        assert_eq!(result.chars().count(), 50);
    }

    #[test]
    fn test_normalize_entry_trailing_whitespace_after_truncation_trimmed() {
        // Place spaces at positions 99 and 100 (0-indexed chars), so truncation at 100
        // leaves trailing space that gets trimmed.
        let mut input: String = "a".repeat(99);
        input.push_str("  extra");
        let result = normalize_entry(&input, &Normalization::Lower, 100);
        assert_eq!(result, "a".repeat(99));
        assert!(!result.ends_with(' '));
    }

    #[test]
    fn test_normalize_entry_emoji_truncation() {
        let input = "🎵🎶🎸🎹🎺🎻🥁🎤🎧🎼_extra";
        let result = normalize_entry(input, &Normalization::Lower, 10);
        assert_eq!(result.chars().count(), 10);
        assert_eq!(result, "🎵🎶🎸🎹🎺🎻🥁🎤🎧🎼");
        // Ensure valid UTF-8 (would panic on construction if not)
        let _ = result.as_bytes();
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
            !matches!(s.as_str(), "audio" | "ui" | "playback" | "keybindings")
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
            keys in proptest::collection::hash_map(
                arb_key_name(), arb_string_value(),
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
            ThemeName::ALL
                .iter()
                .map(|t| t.label().to_string())
                .collect::<Vec<_>>(),
        )
    }

    /// Generate a random string that may or may not be a valid theme.
    fn arb_theme_string() -> impl Strategy<Value = String> {
        prop_oneof![arb_valid_theme(), "[a-zA-Z0-9 ]{1,20}",]
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
    fn arb_partial_config() -> impl Strategy<
        Value = (
            Option<u8>,     // default_volume
            Option<String>, // output_device
            Option<bool>,   // notifications_enabled
            Option<bool>,   // stream_metadata_enabled
            Option<bool>,   // autoplay_last
            Option<bool>,   // save_history
            Option<String>, // keybindings path
        ),
    > {
        (
            proptest::option::of(0..=100u8),
            proptest::option::of("[a-zA-Z ]{3,15}"),
            proptest::option::of(proptest::bool::ANY),
            proptest::option::of(proptest::bool::ANY),
            proptest::option::of(proptest::bool::ANY),
            proptest::option::of(proptest::bool::ANY),
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
            let (volume, device, notif, meta, autoplay, history, kb_path) = partial;

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

            if kb_path.is_some() {
                prop_assert_eq!(&result.config.keybindings.path, &kb_path);
            } else {
                prop_assert_eq!(result.config.keybindings.path, defaults.keybindings.path);
            }
        }
    }

    // Feature: v090-features, Property 1: Reconnect max_attempts clamping invariant
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Validates: Requirements 1.2, 1.8**
        #[test]
        fn prop_reconnect_max_attempts_clamping(value in proptest::num::i64::ANY) {
            let input = format!("[playback]\nreconnect_max_attempts = {}\n", value);
            let result = parse_toml(&input).unwrap();
            let attempts = result.config.playback.reconnect_max_attempts;

            // Must always be in [1, 10]
            prop_assert!(attempts >= 1 && attempts <= 10,
                "reconnect_max_attempts {} out of [1, 10] for input {}", attempts, value);

            // Verify exact clamping behavior
            if value < 1 {
                prop_assert_eq!(attempts, 1, "input {} below 1 should clamp to 1", value);
            } else if value > 10 {
                prop_assert_eq!(attempts, 10, "input {} above 10 should clamp to 10", value);
            } else {
                prop_assert_eq!(attempts, value as u8, "input {} in range should be preserved", value);
            }
        }
    }

    // Feature: v090-features, Property 2: Reconnect backoff_seconds list clamping invariant
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Validates: Requirements 1.3, 1.9, 1.10**
        #[test]
        fn prop_reconnect_backoff_seconds_clamping(
            values in proptest::collection::vec(proptest::num::i64::ANY, 0..=15)
        ) {
            let arr_str = values.iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            let input = format!("[playback]\nreconnect_backoff_seconds = [{}]\n", arr_str);
            let result = parse_toml(&input).unwrap();
            let backoff = &result.config.playback.reconnect_backoff_seconds;

            // If input was empty, result should be the default [3, 6, 12]
            if values.is_empty() {
                prop_assert_eq!(backoff, &vec![3u64, 6, 12],
                    "empty input should produce default [3, 6, 12]");
            } else {
                // Result list length must be in [1, 10]
                prop_assert!(backoff.len() >= 1 && backoff.len() <= 10,
                    "backoff list length {} out of [1, 10]", backoff.len());

                // Each element must be in [1, 60]
                for &elem in backoff.iter() {
                    prop_assert!(elem >= 1 && elem <= 60,
                        "backoff element {} out of [1, 60]", elem);
                }
            }
        }
    }

    // Feature: v090-features, Property 4: Device recovery attempts clamping invariant
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Validates: Requirements 2.2**
        #[test]
        fn prop_device_recovery_attempts_clamping(value in proptest::num::i64::ANY) {
            let input = format!("[playback]\ndevice_recovery_attempts = {}\n", value);
            let result = parse_toml(&input).unwrap();
            let attempts = result.config.playback.device_recovery_attempts;

            // Must always be in [1, 5]
            prop_assert!(attempts >= 1 && attempts <= 5,
                "device_recovery_attempts {} out of [1, 5] for input {}", attempts, value);
        }
    }

    // Feature: v090-features, Property 5: Device recovery delay clamping invariant
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Validates: Requirements 2.3**
        #[test]
        fn prop_device_recovery_delay_ms_clamping(value in proptest::num::i64::ANY) {
            let input = format!("[playback]\ndevice_recovery_delay_ms = {}\n", value);
            let result = parse_toml(&input).unwrap();
            let delay = result.config.playback.device_recovery_delay_ms;

            // Must always be in [100, 5000]
            prop_assert!(delay >= 100 && delay <= 5000,
                "device_recovery_delay_ms {} out of [100, 5000] for input {}", delay, value);
        }
    }

    // Feature: v090-features, Property 12: Discover weight clamping invariant
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Validates: Requirements 5.2**
        #[test]
        fn prop_discover_weight_clamping(
            genre in proptest::num::i64::ANY,
            tag in proptest::num::i64::ANY,
            country in proptest::num::i64::ANY,
        ) {
            let input = format!(
                "[discover]\ngenre_weight = {}\ntag_weight = {}\ncountry_weight = {}\n",
                genre, tag, country
            );
            let result = parse_toml(&input).unwrap();

            // Each weight must be in [0, 10]
            prop_assert!(result.config.discover.genre_weight <= 10,
                "genre_weight {} out of [0, 10] for input {}", result.config.discover.genre_weight, genre);
            prop_assert!(result.config.discover.tag_weight <= 10,
                "tag_weight {} out of [0, 10] for input {}", result.config.discover.tag_weight, tag);
            prop_assert!(result.config.discover.country_weight <= 10,
                "country_weight {} out of [0, 10] for input {}", result.config.discover.country_weight, country);
        }
    }

    /// Generate a random string for exclude_tags entries (mixed case, whitespace, up to 120 chars).
    fn arb_tag_entry() -> impl Strategy<Value = String> {
        "[a-zA-Z0-9 \t]{0,120}"
    }

    /// Generate a random string for exclude_countries entries (mixed case, whitespace, up to 15 chars).
    fn arb_country_entry() -> impl Strategy<Value = String> {
        "[a-zA-Z0-9 ]{0,15}"
    }

    // Feature: v090-features, Property 15: Exclude_tags normalization invariant
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Validates: Requirements 6.2**
        #[test]
        fn prop_exclude_tags_normalization(
            entries in proptest::collection::vec(arb_tag_entry(), 0..=10)
        ) {
            let escaped: Vec<String> = entries.iter()
                .map(|s| format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")))
                .collect();
            let input = format!("[discover]\nexclude_tags = [{}]\n", escaped.join(", "));
            let result = parse_toml(&input).unwrap();

            for tag in &result.config.discover.exclude_tags {
                // Non-empty
                prop_assert!(!tag.is_empty(), "exclude_tags entry must be non-empty");
                // Trimmed (no leading/trailing whitespace)
                prop_assert_eq!(tag, &tag.trim().to_string(),
                    "exclude_tags entry '{}' must be trimmed", tag);
                // Lowercased
                prop_assert_eq!(tag, &tag.to_lowercase(),
                    "exclude_tags entry '{}' must be lowercased", tag);
                // At most 100 characters
                prop_assert!(tag.len() <= 100,
                    "exclude_tags entry length {} exceeds 100", tag.len());
            }
        }
    }

    // Feature: v090-features, Property 16: Exclude_countries normalization invariant
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Validates: Requirements 6.3**
        #[test]
        fn prop_exclude_countries_normalization(
            entries in proptest::collection::vec(arb_country_entry(), 0..=10)
        ) {
            let escaped: Vec<String> = entries.iter()
                .map(|s| format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")))
                .collect();
            let input = format!("[discover]\nexclude_countries = [{}]\n", escaped.join(", "));
            let result = parse_toml(&input).unwrap();

            for country in &result.config.discover.exclude_countries {
                // Non-empty
                prop_assert!(!country.is_empty(), "exclude_countries entry must be non-empty");
                // Trimmed (no leading/trailing whitespace)
                prop_assert_eq!(country, &country.trim().to_string(),
                    "exclude_countries entry '{}' must be trimmed", country);
                // Uppercased
                prop_assert_eq!(country, &country.to_uppercase(),
                    "exclude_countries entry '{}' must be uppercased", country);
                // At most 10 characters
                prop_assert!(country.len() <= 10,
                    "exclude_countries entry length {} exceeds 10", country.len());
            }
        }
    }

    /// Generate a random UTF-8 string of up to 150 chars for truncation testing.
    fn arb_utf8_string() -> impl Strategy<Value = String> {
        proptest::collection::vec(proptest::char::any(), 1..=150)
            .prop_map(|chars| chars.into_iter().collect::<String>())
    }

    // Feature: v091-polish, Property 2: normalize_entry character-count truncation invariant
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Validates: Requirements 3.1, 3.3, 3.5**
        #[test]
        fn prop_normalize_entry_truncation_invariant(
            input in arb_utf8_string(),
            max_chars in prop_oneof![Just(10usize), Just(100usize)],
        ) {
            let output = normalize_entry(&input, &Normalization::Lower, max_chars);

            // Output char count must not exceed max_chars
            prop_assert!(
                output.chars().count() <= max_chars,
                "output chars().count() {} exceeds max_chars {} for input '{}'",
                output.chars().count(), max_chars, input
            );

            // Output must be valid UTF-8 (always true in Rust, but let's confirm no panic)
            let _ = output.as_bytes();

            // If output is non-empty, it must have no trailing whitespace
            if !output.is_empty() {
                prop_assert!(
                    !output.ends_with(char::is_whitespace),
                    "output '{}' has trailing whitespace", output
                );
            }
        }
    }

    /// Generate a non-whitespace string whose char count is within 1..=max_chars.
    fn arb_short_non_whitespace(max_chars: usize) -> impl Strategy<Value = String> {
        proptest::collection::vec(
            proptest::char::any().prop_filter("non-whitespace", |c| !c.is_whitespace()),
            1..=max_chars,
        )
        .prop_map(|chars| chars.into_iter().collect::<String>())
    }

    // Feature: v091-polish, Property 3: normalize_entry preserves entries at or below character limit
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Validates: Requirements 3.4**
        #[test]
        fn prop_normalize_entry_preserves_short_entries(
            max_chars in prop_oneof![Just(10usize), Just(100usize)],
            input in arb_short_non_whitespace(100),
        ) {
            // Only test inputs whose trimmed+lowercased form fits within max_chars
            let expected = input.trim().to_lowercase();
            prop_assume!(expected.chars().count() <= max_chars);
            prop_assume!(!expected.is_empty());

            let output = normalize_entry(&input, &Normalization::Lower, max_chars);

            prop_assert_eq!(
                &output, &expected,
                "Entry at or below limit should be preserved: input='{}', expected='{}', got='{}'",
                input, expected, output
            );
        }
    }
}
