// Filesystem I/O for config: load from pulsedeck.toml with library.json fallback, save to TOML.

use std::fs;
use std::path::Path;

use super::parse::{parse_toml, ParseResult};
use super::serialize::serialize_toml;
use super::{AppConfig, AudioConfig, PlaybackConfig, UiConfig};

const TOML_FILENAME: &str = "pulsedeck.toml";
const LEGACY_FILENAME: &str = "library.json";

/// Result of loading configuration from disk.
#[derive(Debug)]
pub struct LoadResult {
    pub config: AppConfig,
    pub preserved: toml::Value,
    pub warnings: Vec<String>,
}

/// Load config: tries pulsedeck.toml, falls back to library.json settings.
pub fn load_config(config_dir: &Path) -> LoadResult {
    let toml_path = config_dir.join(TOML_FILENAME);
    if toml_path.exists() {
        return load_from_toml(&toml_path);
    }

    let json_path = config_dir.join(LEGACY_FILENAME);
    if json_path.exists() {
        return migrate_from_library_json(&json_path);
    }

    LoadResult {
        config: AppConfig::default(),
        preserved: toml::Value::Table(toml::map::Map::new()),
        warnings: Vec::new(),
    }
}

/// Save config to pulsedeck.toml, preserving unknown keys.
pub fn save_config(
    config_dir: &Path,
    config: &AppConfig,
    preserved: &toml::Value,
) -> Result<(), String> {
    if let Err(err) = fs::create_dir_all(config_dir) {
        return Err(format!("Could not create config directory: {err}"));
    }
    let content = serialize_toml(config, preserved);
    let toml_path = config_dir.join(TOML_FILENAME);
    fs::write(&toml_path, content)
        .map_err(|err| format!("Could not write {}: {err}", toml_path.display()))
}

fn load_from_toml(path: &Path) -> LoadResult {
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(err) => {
            let warning = format!("Could not read {}: {err}", path.display());
            return LoadResult {
                config: AppConfig::default(),
                preserved: toml::Value::Table(toml::map::Map::new()),
                warnings: vec![warning],
            };
        }
    };

    match parse_toml(&content) {
        Ok(ParseResult { config, preserved, warnings }) => {
            LoadResult { config, preserved, warnings }
        }
        Err(err) => {
            let warning = format!("Could not parse {}: {err}", path.display());
            LoadResult {
                config: AppConfig::default(),
                preserved: toml::Value::Table(toml::map::Map::new()),
                warnings: vec![warning],
            }
        }
    }
}

fn migrate_from_library_json(path: &Path) -> LoadResult {
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(err) => {
            let warning = format!("Could not read {}: {err}", path.display());
            return LoadResult {
                config: AppConfig::default(),
                preserved: toml::Value::Table(toml::map::Map::new()),
                warnings: vec![warning],
            };
        }
    };

    let config = extract_settings_from_json(&content);
    let warnings = vec![format!("Migrated settings from {}", path.display())];
    LoadResult {
        config,
        preserved: toml::Value::Table(toml::map::Map::new()),
        warnings,
    }
}

fn extract_settings_from_json(content: &str) -> AppConfig {
    let json: serde_json::Value = match serde_json::from_str(content) {
        Ok(v) => v,
        Err(_) => return AppConfig::default(),
    };

    let settings = match json.get("settings") {
        Some(s) => s,
        None => return AppConfig::default(),
    };

    let audio = extract_audio_from_json(settings);
    let ui = extract_ui_from_json(settings);
    let playback = extract_playback_from_json(settings);

    AppConfig {
        audio,
        ui,
        playback,
        ..AppConfig::default()
    }
}

fn extract_audio_from_json(settings: &serde_json::Value) -> AudioConfig {
    let output_device = settings
        .get("output_device_name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    AudioConfig {
        output_device,
        ..AudioConfig::default()
    }
}

fn extract_ui_from_json(settings: &serde_json::Value) -> UiConfig {
    let theme = settings
        .get("theme")
        .and_then(|v| v.as_str())
        .unwrap_or("Retrowave")
        .to_string();
    let notifications_enabled = settings
        .get("notifications_enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let stream_metadata_enabled = settings
        .get("stream_metadata_enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    UiConfig { theme, notifications_enabled, stream_metadata_enabled }
}

fn extract_playback_from_json(settings: &serde_json::Value) -> PlaybackConfig {
    let autoplay_last = settings
        .get("autoplay_last")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let save_history = settings
        .get("save_history")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    PlaybackConfig { autoplay_last, save_history }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn unique_temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir()
            .join("pulsedeck_config_io_tests")
            .join(name);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn test_load_config_fresh_dir_returns_defaults() {
        let dir = unique_temp_dir("fresh_defaults");
        let result = load_config(&dir);

        assert_eq!(result.config, AppConfig::default());
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_load_config_existing_toml_loads_values() {
        let dir = unique_temp_dir("existing_toml");
        let toml_content = r#"
[audio]
default_volume = 42
output_device = "Headphones"

[ui]
theme = "Terminal"
notifications_enabled = false
stream_metadata_enabled = false

[playback]
autoplay_last = true
save_history = true
"#;
        fs::write(dir.join(TOML_FILENAME), toml_content).unwrap();
        let result = load_config(&dir);

        assert_eq!(result.config.audio.default_volume, 42);
        assert_eq!(result.config.audio.output_device, Some("Headphones".to_string()));
        assert_eq!(result.config.ui.theme, "Terminal");
        assert!(!result.config.ui.notifications_enabled);
        assert!(!result.config.ui.stream_metadata_enabled);
        assert!(result.config.playback.autoplay_last);
        assert!(result.config.playback.save_history);
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_load_config_migration_from_library_json() {
        let dir = unique_temp_dir("migration_json");
        let json_content = r#"{
            "version": 1,
            "stations": [],
            "settings": {
                "notifications_enabled": false,
                "autoplay_last": true,
                "theme": "Terminal",
                "output_device_name": "Speakers",
                "save_history": true,
                "stream_metadata_enabled": false
            }
        }"#;
        fs::write(dir.join(LEGACY_FILENAME), json_content).unwrap();
        let result = load_config(&dir);

        assert_eq!(result.config.audio.output_device, Some("Speakers".to_string()));
        assert_eq!(result.config.ui.theme, "Terminal");
        assert!(!result.config.ui.notifications_enabled);
        assert!(!result.config.ui.stream_metadata_enabled);
        assert!(result.config.playback.autoplay_last);
        assert!(result.config.playback.save_history);
        assert!(result.warnings.iter().any(|w| w.contains("Migrated")));
    }

    #[test]
    fn test_load_config_toml_preferred_over_library_json() {
        let dir = unique_temp_dir("toml_preferred");
        let toml_content = "[audio]\ndefault_volume = 99\n";
        let json_content = r#"{"version":1,"stations":[],"settings":{"theme":"Terminal"}}"#;
        fs::write(dir.join(TOML_FILENAME), toml_content).unwrap();
        fs::write(dir.join(LEGACY_FILENAME), json_content).unwrap();
        let result = load_config(&dir);

        // TOML is preferred; theme should be default "Retrowave", not Terminal from JSON
        assert_eq!(result.config.audio.default_volume, 99);
        assert_eq!(result.config.ui.theme, "Retrowave");
    }

    #[test]
    fn test_load_config_invalid_toml_returns_defaults_with_warning() {
        let dir = unique_temp_dir("invalid_toml");
        fs::write(dir.join(TOML_FILENAME), "this is [[[not valid toml").unwrap();
        let result = load_config(&dir);

        assert_eq!(result.config, AppConfig::default());
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].contains("Could not parse"));
    }

    #[test]
    fn test_load_config_unreadable_toml_returns_defaults_with_warning() {
        let dir = unique_temp_dir("unreadable_toml");
        // Create a directory where the file should be — read_to_string will fail
        fs::create_dir_all(dir.join(TOML_FILENAME)).unwrap();
        let result = load_config(&dir);

        assert_eq!(result.config, AppConfig::default());
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].contains("Could not read"));
    }

    #[test]
    fn test_save_config_creates_dir_and_writes_file() {
        let dir = unique_temp_dir("save_creates");
        let subdir = dir.join("nested").join("config");
        let config = AppConfig {
            audio: AudioConfig { output_device: Some("USB".to_string()), default_volume: 65 },
            ..AppConfig::default()
        };
        let preserved = toml::Value::Table(toml::map::Map::new());

        let result = save_config(&subdir, &config, &preserved);
        assert!(result.is_ok());

        let written = fs::read_to_string(subdir.join(TOML_FILENAME)).unwrap();
        assert!(written.contains("default_volume = 65"));
        assert!(written.contains("output_device = \"USB\""));
    }

    #[test]
    fn test_save_config_round_trips_with_load() {
        let dir = unique_temp_dir("save_round_trip");
        let config = AppConfig {
            audio: AudioConfig { output_device: Some("DAC".to_string()), default_volume: 30 },
            ui: UiConfig {
                theme: "Terminal".to_string(),
                notifications_enabled: false,
                stream_metadata_enabled: false,
            },
            playback: PlaybackConfig { autoplay_last: true, save_history: true },
            ..AppConfig::default()
        };
        let preserved = toml::Value::Table(toml::map::Map::new());

        save_config(&dir, &config, &preserved).unwrap();
        let loaded = load_config(&dir);

        assert_eq!(loaded.config, config);
    }

    #[test]
    fn test_save_config_write_failure_returns_error() {
        // Use a path that can't be created (file blocking directory creation)
        let dir = unique_temp_dir("write_failure");
        let blocker = dir.join("blocker_file");
        fs::write(&blocker, "I block dir creation").unwrap();
        let impossible_dir = blocker.join("subdir");

        let config = AppConfig::default();
        let preserved = toml::Value::Table(toml::map::Map::new());
        let result = save_config(&impossible_dir, &config, &preserved);

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Could not create config directory"));
    }

    #[test]
    fn test_load_config_migration_malformed_json_returns_defaults() {
        let dir = unique_temp_dir("malformed_json");
        fs::write(dir.join(LEGACY_FILENAME), "{not valid json}}}").unwrap();
        let result = load_config(&dir);

        assert_eq!(result.config, AppConfig::default());
        assert!(result.warnings.iter().any(|w| w.contains("Migrated")));
    }

    #[test]
    fn test_load_config_migration_missing_settings_field_returns_defaults() {
        let dir = unique_temp_dir("no_settings_field");
        let json_content = r#"{"version":1,"stations":[]}"#;
        fs::write(dir.join(LEGACY_FILENAME), json_content).unwrap();
        let result = load_config(&dir);

        assert_eq!(result.config, AppConfig::default());
    }

    #[test]
    fn test_save_config_preserves_unknown_keys() {
        let dir = unique_temp_dir("preserve_unknown");
        let mut preserved_table = toml::map::Map::new();
        let mut custom = toml::map::Map::new();
        custom.insert("key".into(), toml::Value::String("value".into()));
        preserved_table.insert("custom_section".into(), toml::Value::Table(custom));
        let preserved = toml::Value::Table(preserved_table);

        let config = AppConfig::default();
        save_config(&dir, &config, &preserved).unwrap();

        let loaded_content = fs::read_to_string(dir.join(TOML_FILENAME)).unwrap();
        assert!(loaded_content.contains("custom_section"));
        assert!(loaded_content.contains("key = \"value\""));
    }
}
