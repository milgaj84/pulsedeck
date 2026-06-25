// Config validation: round-trip serialize → re-parse → compare.

use super::AppConfig;
use crate::config_toml::parse::parse_toml;
use crate::config_toml::serialize::serialize_toml;

/// Validate a config by round-tripping: serialize → re-parse → compare.
/// Returns Ok(()) if the round-trip produces an equivalent AppConfig.
/// Returns Err(description) if re-parse fails or the configs differ.
pub fn validate_config(config: &AppConfig, preserved: &toml::Value) -> Result<(), String> {
    let toml_string = serialize_toml(config, preserved);

    let parsed = parse_toml(&toml_string).map_err(|e| format!("re-parse failed: {}", e))?;

    if parsed.config != *config {
        return Err("round-trip produced different config".to_string());
    }

    Ok(())
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use crate::config_toml::{
        AppConfig, AudioConfig, DiscoverConfig, KeybindingsConfig, PlaybackConfig, UiConfig,
    };
    use crate::theme_name::ThemeName;
    use proptest::prelude::*;

    // Feature: v100-features, Property 8: Config validation round-trip

    fn valid_theme_strategy() -> impl Strategy<Value = String> {
        prop::sample::select(
            ThemeName::ALL
                .iter()
                .map(|t| t.label().to_string())
                .collect::<Vec<_>>(),
        )
    }

    fn valid_app_config_strategy() -> impl Strategy<Value = AppConfig> {
        let base = (
            any::<Option<String>>(),
            0u8..=100u8,
            valid_theme_strategy(),
            any::<bool>(),
            any::<bool>(),
            any::<bool>(),
            any::<bool>(),
            any::<Option<String>>(),
        );
        let playback_ext = (
            1u8..=10u8,
            prop::collection::vec(1u64..=60, 1..=10),
            1u8..=5u8,
            100u64..=5000u64,
        );
        let discover = (
            0u32..=10u32,
            0u32..=10u32,
            0u32..=10u32,
            prop::collection::vec("[a-z]{1,20}", 0..=5),
            prop::collection::vec("[A-Z]{2}", 0..=5),
        );

        (base, playback_ext, discover).prop_map(
            |(
                (
                    output_device,
                    default_volume,
                    theme,
                    notifications_enabled,
                    stream_metadata_enabled,
                    autoplay_last,
                    save_history,
                    keybindings_path,
                ),
                (
                    reconnect_max_attempts,
                    reconnect_backoff_seconds,
                    device_recovery_attempts,
                    device_recovery_delay_ms,
                ),
                (genre_weight, tag_weight, country_weight, exclude_tags, exclude_countries),
            )| {
                AppConfig {
                    audio: AudioConfig {
                        output_device,
                        default_volume,
                    },
                    ui: UiConfig {
                        theme,
                        notifications_enabled,
                        stream_metadata_enabled,
                    },
                    playback: PlaybackConfig {
                        autoplay_last,
                        save_history,
                        reconnect_max_attempts,
                        reconnect_backoff_seconds,
                        device_recovery_attempts,
                        device_recovery_delay_ms,
                    },
                    keybindings: KeybindingsConfig {
                        path: keybindings_path,
                    },
                    discover: DiscoverConfig {
                        genre_weight,
                        tag_weight,
                        country_weight,
                        exclude_tags,
                        exclude_countries,
                    },
                }
            },
        )
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Validates: Requirements 4.1, 4.2, 4.7**
        #[test]
        fn config_validation_round_trip(config in valid_app_config_strategy()) {
            let preserved = toml::Value::Table(toml::map::Map::new());
            let result = validate_config(&config, &preserved);
            prop_assert!(result.is_ok(), "validate_config failed: {:?}", result.err());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config_toml::{
        AudioConfig, DiscoverConfig, KeybindingsConfig, PlaybackConfig, UiConfig,
    };

    #[test]
    fn test_validate_default_config_succeeds() {
        let config = AppConfig::default();
        let preserved = toml::Value::Table(toml::map::Map::new());

        let result = validate_config(&config, &preserved);

        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_custom_config_succeeds() {
        let config = AppConfig {
            audio: AudioConfig {
                output_device: Some("Headphones".to_string()),
                default_volume: 65,
            },
            ui: UiConfig {
                theme: "Terminal".to_string(),
                notifications_enabled: false,
                stream_metadata_enabled: true,
            },
            playback: PlaybackConfig {
                autoplay_last: true,
                save_history: true,
                reconnect_max_attempts: 5,
                reconnect_backoff_seconds: vec![2, 4, 8],
                device_recovery_attempts: 3,
                device_recovery_delay_ms: 2000,
            },
            keybindings: KeybindingsConfig {
                path: Some("keys.json".to_string()),
            },
            discover: DiscoverConfig {
                genre_weight: 7,
                tag_weight: 3,
                country_weight: 5,
                exclude_tags: vec!["talk".to_string(), "news".to_string()],
                exclude_countries: vec!["US".to_string()],
            },
        };
        let preserved = toml::Value::Table(toml::map::Map::new());

        let result = validate_config(&config, &preserved);

        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_with_preserved_unknown_keys_succeeds() {
        let config = AppConfig::default();
        let mut table = toml::map::Map::new();
        let mut custom = toml::map::Map::new();
        custom.insert("foo".into(), toml::Value::String("bar".into()));
        table.insert("custom_section".into(), toml::Value::Table(custom));
        let preserved = toml::Value::Table(table);

        let result = validate_config(&config, &preserved);

        assert!(result.is_ok());
    }
}
