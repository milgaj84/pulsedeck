// Unified TOML configuration — loader, parser, and writer for pulsedeck.toml.

pub mod io;
pub mod parse;
pub mod serialize;

/// Scrobble service backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrobbleService {
    LastFm,
    ListenBrainz,
}

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
}

impl Default for PlaybackConfig {
    fn default() -> Self {
        Self {
            autoplay_last: false,
            save_history: false,
        }
    }
}

/// Scrobble integration settings.
#[derive(Debug, Clone, PartialEq)]
pub struct ScrobbleConfig {
    pub enabled: bool,
    pub service: ScrobbleService,
    pub api_key: String,
}

impl Default for ScrobbleConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            service: ScrobbleService::LastFm,
            api_key: String::new(),
        }
    }
}

/// Keybinding file path override.
#[derive(Debug, Clone, PartialEq)]
pub struct KeybindingsConfig {
    pub path: Option<String>,
}

impl Default for KeybindingsConfig {
    fn default() -> Self {
        Self { path: None }
    }
}

/// Top-level application configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct AppConfig {
    pub audio: AudioConfig,
    pub ui: UiConfig,
    pub playback: PlaybackConfig,
    pub scrobble: ScrobbleConfig,
    pub keybindings: KeybindingsConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            audio: AudioConfig::default(),
            ui: UiConfig::default(),
            playback: PlaybackConfig::default(),
            scrobble: ScrobbleConfig::default(),
            keybindings: KeybindingsConfig::default(),
        }
    }
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
    }

    #[test]
    fn test_scrobble_config_default_values() {
        let config = ScrobbleConfig::default();

        assert!(!config.enabled);
        assert_eq!(config.service, ScrobbleService::LastFm);
        assert_eq!(config.api_key, "");
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
        assert_eq!(config.scrobble, ScrobbleConfig::default());
        assert_eq!(config.keybindings, KeybindingsConfig::default());
    }
}
