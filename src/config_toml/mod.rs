// Unified TOML configuration — loader, parser, and writer for pulsedeck.toml.

pub mod hot_reload;
pub mod io;
pub mod parse;
pub mod serialize;
pub mod validate;

/// Audio output settings.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioConfig {
    pub output_device: Option<String>,
    /// Volume level clamped to 0–100.
    pub default_volume: u8,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            output_device: None,
            default_volume: 80,
        }
    }
}

/// UI appearance settings.
#[derive(Debug, Clone, PartialEq)]
pub struct UiConfig {
    pub theme: String,
    pub notifications_enabled: bool,
    pub stream_metadata_enabled: bool,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: "Retrowave".to_string(),
            notifications_enabled: true,
            stream_metadata_enabled: true,
        }
    }
}

/// Playback behavior settings.
#[derive(Debug, Clone, PartialEq)]
pub struct PlaybackConfig {
    pub autoplay_last: bool,
    pub save_history: bool,
    /// Maximum reconnect attempts after connection failure, range 1–10.
    pub reconnect_max_attempts: u8,
    /// Backoff durations (seconds) between reconnect attempts, each 1–60, max 10 entries.
    pub reconnect_backoff_seconds: Vec<u64>,
    /// Maximum device recovery attempts for output device failures, range 1–5.
    pub device_recovery_attempts: u8,
    /// Delay in milliseconds between device recovery attempts, range 100–5000.
    pub device_recovery_delay_ms: u64,
}

impl Default for PlaybackConfig {
    fn default() -> Self {
        Self {
            autoplay_last: false,
            save_history: false,
            reconnect_max_attempts: 3,
            reconnect_backoff_seconds: vec![3, 6, 12],
            device_recovery_attempts: 2,
            device_recovery_delay_ms: 1000,
        }
    }
}

/// Keybinding file path override.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct KeybindingsConfig {
    pub path: Option<String>,
}

/// Discover scoring weights and exclusion lists.
#[derive(Debug, Clone, PartialEq)]
pub struct DiscoverConfig {
    pub genre_weight: u32,
    pub tag_weight: u32,
    pub country_weight: u32,
    pub exclude_tags: Vec<String>,
    pub exclude_countries: Vec<String>,
}

impl Default for DiscoverConfig {
    fn default() -> Self {
        Self {
            genre_weight: 3,
            tag_weight: 1,
            country_weight: 1,
            exclude_tags: Vec::new(),
            exclude_countries: Vec::new(),
        }
    }
}

/// Top-level application configuration.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AppConfig {
    pub audio: AudioConfig,
    pub ui: UiConfig,
    pub playback: PlaybackConfig,
    pub keybindings: KeybindingsConfig,
    pub discover: DiscoverConfig,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_config_default_values() {
        let config = AudioConfig::default();

        assert_eq!(config.output_device, None);
        assert_eq!(config.default_volume, 80);
    }

    #[test]
    fn test_ui_config_default_values() {
        let config = UiConfig::default();

        assert_eq!(config.theme, "Retrowave");
        assert!(config.notifications_enabled);
        assert!(config.stream_metadata_enabled);
    }

    #[test]
    fn test_playback_config_default_values() {
        let config = PlaybackConfig::default();

        assert!(!config.autoplay_last);
        assert!(!config.save_history);
        assert_eq!(config.reconnect_max_attempts, 3);
        assert_eq!(config.reconnect_backoff_seconds, vec![3, 6, 12]);
        assert_eq!(config.device_recovery_attempts, 2);
        assert_eq!(config.device_recovery_delay_ms, 1000);
    }

    #[test]
    fn test_keybindings_config_default_values() {
        let config = KeybindingsConfig::default();

        assert_eq!(config.path, None);
    }

    #[test]
    fn test_app_config_default_composes_sub_defaults() {
        let config = AppConfig::default();

        assert_eq!(config.audio, AudioConfig::default());
        assert_eq!(config.ui, UiConfig::default());
        assert_eq!(config.playback, PlaybackConfig::default());
        assert_eq!(config.keybindings, KeybindingsConfig::default());
        assert_eq!(config.discover, DiscoverConfig::default());
    }

    #[test]
    fn test_discover_config_default_values() {
        let config = DiscoverConfig::default();

        assert_eq!(config.genre_weight, 3);
        assert_eq!(config.tag_weight, 1);
        assert_eq!(config.country_weight, 1);
        assert!(config.exclude_tags.is_empty());
        assert!(config.exclude_countries.is_empty());
    }
}
