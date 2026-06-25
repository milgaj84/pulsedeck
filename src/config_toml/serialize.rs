// TOML serialization: serializes AppConfig back to TOML string, merging preserved unknown keys.

use super::{AppConfig, AudioConfig, DiscoverConfig, KeybindingsConfig, PlaybackConfig, UiConfig};

/// Serialize AppConfig back to TOML string, merging preserved unknown keys.
pub fn serialize_toml(config: &AppConfig, preserved: &toml::Value) -> String {
    let mut table = preserved.as_table().cloned().unwrap_or_default();

    set_audio_section(&mut table, &config.audio);
    set_ui_section(&mut table, &config.ui);
    set_playback_section(&mut table, &config.playback);
    set_keybindings_section(&mut table, &config.keybindings);
    set_discover_section(&mut table, &config.discover);

    toml::to_string_pretty(&table).unwrap_or_default()
}

fn set_audio_section(table: &mut toml::map::Map<String, toml::Value>, audio: &AudioConfig) {
    let section = table
        .entry("audio")
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
    let sec = section.as_table_mut().unwrap();
    match &audio.output_device {
        Some(device) => {
            sec.insert("output_device".into(), toml::Value::String(device.clone()));
        }
        None => {
            sec.remove("output_device");
        }
    }
    sec.insert(
        "default_volume".into(),
        toml::Value::Integer(audio.default_volume as i64),
    );
}

fn set_ui_section(table: &mut toml::map::Map<String, toml::Value>, ui: &UiConfig) {
    let section = table
        .entry("ui")
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
    let sec = section.as_table_mut().unwrap();
    sec.insert("theme".into(), toml::Value::String(ui.theme.clone()));
    sec.insert(
        "notifications_enabled".into(),
        toml::Value::Boolean(ui.notifications_enabled),
    );
    sec.insert(
        "stream_metadata_enabled".into(),
        toml::Value::Boolean(ui.stream_metadata_enabled),
    );
}

fn set_playback_section(
    table: &mut toml::map::Map<String, toml::Value>,
    playback: &PlaybackConfig,
) {
    let section = table
        .entry("playback")
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
    let sec = section.as_table_mut().unwrap();
    sec.insert(
        "autoplay_last".into(),
        toml::Value::Boolean(playback.autoplay_last),
    );
    sec.insert(
        "save_history".into(),
        toml::Value::Boolean(playback.save_history),
    );
    sec.insert(
        "reconnect_max_attempts".into(),
        toml::Value::Integer(playback.reconnect_max_attempts as i64),
    );
    sec.insert(
        "reconnect_backoff_seconds".into(),
        toml::Value::Array(
            playback
                .reconnect_backoff_seconds
                .iter()
                .map(|&v| toml::Value::Integer(v as i64))
                .collect(),
        ),
    );
    sec.insert(
        "device_recovery_attempts".into(),
        toml::Value::Integer(playback.device_recovery_attempts as i64),
    );
    sec.insert(
        "device_recovery_delay_ms".into(),
        toml::Value::Integer(playback.device_recovery_delay_ms as i64),
    );
}

fn set_keybindings_section(
    table: &mut toml::map::Map<String, toml::Value>,
    kb: &KeybindingsConfig,
) {
    let section = table
        .entry("keybindings")
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
    let sec = section.as_table_mut().unwrap();
    match &kb.path {
        Some(path) => {
            sec.insert("path".into(), toml::Value::String(path.clone()));
        }
        None => {
            sec.remove("path");
        }
    }
}

fn set_discover_section(table: &mut toml::map::Map<String, toml::Value>, discover: &DiscoverConfig) {
    let section = table
        .entry("discover")
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
    let sec = section.as_table_mut().unwrap();
    sec.insert(
        "genre_weight".into(),
        toml::Value::Integer(discover.genre_weight as i64),
    );
    sec.insert(
        "tag_weight".into(),
        toml::Value::Integer(discover.tag_weight as i64),
    );
    sec.insert(
        "country_weight".into(),
        toml::Value::Integer(discover.country_weight as i64),
    );
    sec.insert(
        "exclude_tags".into(),
        toml::Value::Array(
            discover
                .exclude_tags
                .iter()
                .map(|s| toml::Value::String(s.clone()))
                .collect(),
        ),
    );
    sec.insert(
        "exclude_countries".into(),
        toml::Value::Array(
            discover
                .exclude_countries
                .iter()
                .map(|s| toml::Value::String(s.clone()))
                .collect(),
        ),
    );
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use crate::config_toml::parse::parse_toml;
    use crate::config_toml::DiscoverConfig;
    use crate::theme_name::ThemeName;
    use proptest::prelude::*;

    // Feature: v080-features, Property 12: TOML configuration round-trip

    fn valid_theme_strategy() -> impl Strategy<Value = String> {
        prop::sample::select(
            ThemeName::ALL
                .iter()
                .map(|t| t.label().to_string())
                .collect::<Vec<_>>(),
        )
    }

    fn valid_backoff_strategy() -> impl Strategy<Value = Vec<u64>> {
        prop::collection::vec(1u64..=60, 1..=10)
    }

    fn valid_exclude_tags_strategy() -> impl Strategy<Value = Vec<String>> {
        prop::collection::vec("[a-z]{1,20}", 0..=5)
    }

    fn valid_exclude_countries_strategy() -> impl Strategy<Value = Vec<String>> {
        prop::collection::vec("[A-Z]{2}", 0..=5)
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
            valid_backoff_strategy(),
            1u8..=5u8,
            100u64..=5000u64,
        );
        let discover = (
            0u32..=10u32,
            0u32..=10u32,
            0u32..=10u32,
            valid_exclude_tags_strategy(),
            valid_exclude_countries_strategy(),
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

        /// **Validates: Requirements 1.2, 1.3, 2.2, 2.3, 5.2, 6.2, 6.3**
        #[test]
        fn toml_round_trip_produces_equivalent_config(config in valid_app_config_strategy()) {
            let preserved = toml::Value::Table(toml::map::Map::new());
            let serialized = serialize_toml(&config, &preserved);
            let parsed = parse_toml(&serialized).expect("serialized TOML must be parseable");
            prop_assert_eq!(parsed.config, config);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config_toml::parse::parse_toml;
    use crate::config_toml::DiscoverConfig;

    #[test]
    fn test_serialize_toml_round_trip_produces_equivalent_config() {
        let config = AppConfig {
            audio: AudioConfig {
                output_device: Some("Built-in Speakers".to_string()),
                default_volume: 75,
            },
            ui: UiConfig {
                theme: "Retrowave".to_string(),
                notifications_enabled: false,
                stream_metadata_enabled: true,
            },
            playback: PlaybackConfig {
                autoplay_last: true,
                save_history: true,
                ..PlaybackConfig::default()
            },
            keybindings: KeybindingsConfig {
                path: Some("keys.json".to_string()),
            },
            discover: DiscoverConfig::default(),
        };

        let preserved = toml::Value::Table(toml::map::Map::new());
        let output = serialize_toml(&config, &preserved);
        let result = parse_toml(&output).unwrap();

        assert_eq!(result.config, config);
    }

    #[test]
    fn test_serialize_toml_default_config_round_trips() {
        let config = AppConfig::default();
        let preserved = toml::Value::Table(toml::map::Map::new());
        let output = serialize_toml(&config, &preserved);
        let result = parse_toml(&output).unwrap();

        assert_eq!(result.config, config);
    }

    #[test]
    fn test_serialize_toml_unknown_keys_preserved_in_output() {
        let input = r#"
[audio]
default_volume = 80
unknown_audio_key = "keep_me"

[custom_section]
foo = "bar"
nested = { a = 1, b = 2 }

[ui]
theme = "Retrowave"
notifications_enabled = true
stream_metadata_enabled = true
"#;
        let parsed = parse_toml(input).unwrap();
        let output = serialize_toml(&parsed.config, &parsed.preserved);

        // Re-parse the output and check unknown keys are still there
        let reparsed: toml::Value = output.parse().unwrap();
        let table = reparsed.as_table().unwrap();

        assert!(table.contains_key("custom_section"));
        let custom = table.get("custom_section").unwrap().as_table().unwrap();
        assert_eq!(custom.get("foo").unwrap().as_str(), Some("bar"));

        let audio = table.get("audio").unwrap().as_table().unwrap();
        assert_eq!(
            audio.get("unknown_audio_key").unwrap().as_str(),
            Some("keep_me")
        );
    }

    #[test]
    fn test_serialize_toml_none_output_device_not_present() {
        let config = AppConfig {
            audio: AudioConfig {
                output_device: None,
                default_volume: 50,
            },
            ..AppConfig::default()
        };
        let preserved = toml::Value::Table(toml::map::Map::new());
        let output = serialize_toml(&config, &preserved);

        assert!(!output.contains("output_device"));
    }

    #[test]
    fn test_serialize_toml_none_keybindings_path_not_present() {
        let config = AppConfig {
            keybindings: KeybindingsConfig { path: None },
            ..AppConfig::default()
        };
        let preserved = toml::Value::Table(toml::map::Map::new());
        let output = serialize_toml(&config, &preserved);

        // keybindings section may exist but path key should not be in it
        let reparsed: toml::Value = output.parse().unwrap();
        let table = reparsed.as_table().unwrap();
        if let Some(kb) = table.get("keybindings") {
            let kb_table = kb.as_table().unwrap();
            assert!(!kb_table.contains_key("path"));
        }
    }

    #[test]
    fn test_serialize_toml_playback_reconnect_fields_round_trip() {
        let config = AppConfig {
            playback: PlaybackConfig {
                autoplay_last: true,
                save_history: false,
                reconnect_max_attempts: 7,
                reconnect_backoff_seconds: vec![2, 4, 8, 16],
                device_recovery_attempts: 4,
                device_recovery_delay_ms: 2500,
            },
            ..AppConfig::default()
        };

        let preserved = toml::Value::Table(toml::map::Map::new());
        let output = serialize_toml(&config, &preserved);
        let result = parse_toml(&output).unwrap();

        assert_eq!(result.config.playback, config.playback);
    }

    #[test]
    fn test_serialize_toml_discover_fields_round_trip() {
        let config = AppConfig {
            discover: DiscoverConfig {
                genre_weight: 5,
                tag_weight: 2,
                country_weight: 8,
                exclude_tags: vec!["jazz".to_string(), "blues".to_string()],
                exclude_countries: vec!["US".to_string(), "GB".to_string()],
            },
            ..AppConfig::default()
        };

        let preserved = toml::Value::Table(toml::map::Map::new());
        let output = serialize_toml(&config, &preserved);
        let result = parse_toml(&output).unwrap();

        assert_eq!(result.config.discover, config.discover);
    }

    #[test]
    fn test_serialize_toml_discover_empty_lists_round_trip() {
        let config = AppConfig {
            discover: DiscoverConfig {
                genre_weight: 0,
                tag_weight: 10,
                country_weight: 0,
                exclude_tags: vec![],
                exclude_countries: vec![],
            },
            ..AppConfig::default()
        };

        let preserved = toml::Value::Table(toml::map::Map::new());
        let output = serialize_toml(&config, &preserved);
        let result = parse_toml(&output).unwrap();

        assert_eq!(result.config.discover, config.discover);
    }

    #[test]
    fn test_serialize_toml_all_new_fields_together_round_trip() {
        let config = AppConfig {
            audio: AudioConfig {
                output_device: Some("Headphones".to_string()),
                default_volume: 60,
            },
            ui: UiConfig {
                theme: "Terminal".to_string(),
                notifications_enabled: false,
                stream_metadata_enabled: false,
            },
            playback: PlaybackConfig {
                autoplay_last: true,
                save_history: true,
                reconnect_max_attempts: 10,
                reconnect_backoff_seconds: vec![1, 2, 3, 5, 8, 13],
                device_recovery_attempts: 5,
                device_recovery_delay_ms: 100,
            },
            keybindings: KeybindingsConfig {
                path: Some("custom.json".to_string()),
            },
            discover: DiscoverConfig {
                genre_weight: 10,
                tag_weight: 5,
                country_weight: 3,
                exclude_tags: vec!["talk".to_string(), "news".to_string(), "sports".to_string()],
                exclude_countries: vec!["CN".to_string(), "RU".to_string()],
            },
        };

        let preserved = toml::Value::Table(toml::map::Map::new());
        let output = serialize_toml(&config, &preserved);
        let result = parse_toml(&output).unwrap();

        assert_eq!(result.config, config);
    }
}
