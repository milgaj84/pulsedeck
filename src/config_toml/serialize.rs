// TOML serialization: serializes AppConfig back to TOML string, merging preserved unknown keys.

use super::{
    AppConfig, AudioConfig, KeybindingsConfig, PlaybackConfig, ScrobbleConfig, ScrobbleService,
    UiConfig,
};

/// Serialize AppConfig back to TOML string, merging preserved unknown keys.
pub fn serialize_toml(config: &AppConfig, preserved: &toml::Value) -> String {
    let mut table = preserved
        .as_table()
        .cloned()
        .unwrap_or_default();

    set_audio_section(&mut table, &config.audio);
    set_ui_section(&mut table, &config.ui);
    set_playback_section(&mut table, &config.playback);
    set_scrobble_section(&mut table, &config.scrobble);
    set_keybindings_section(&mut table, &config.keybindings);

    toml::to_string_pretty(&table).unwrap_or_default()
}

fn set_audio_section(table: &mut toml::map::Map<String, toml::Value>, audio: &AudioConfig) {
    let section = table
        .entry("audio")
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
    let sec = section.as_table_mut().unwrap();
    match &audio.output_device {
        Some(device) => { sec.insert("output_device".into(), toml::Value::String(device.clone())); }
        None => { sec.remove("output_device"); }
    }
    sec.insert("default_volume".into(), toml::Value::Integer(audio.default_volume as i64));
}

fn set_ui_section(table: &mut toml::map::Map<String, toml::Value>, ui: &UiConfig) {
    let section = table
        .entry("ui")
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
    let sec = section.as_table_mut().unwrap();
    sec.insert("theme".into(), toml::Value::String(ui.theme.clone()));
    sec.insert("notifications_enabled".into(), toml::Value::Boolean(ui.notifications_enabled));
    sec.insert("stream_metadata_enabled".into(), toml::Value::Boolean(ui.stream_metadata_enabled));
}

fn set_playback_section(table: &mut toml::map::Map<String, toml::Value>, playback: &PlaybackConfig) {
    let section = table
        .entry("playback")
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
    let sec = section.as_table_mut().unwrap();
    sec.insert("autoplay_last".into(), toml::Value::Boolean(playback.autoplay_last));
    sec.insert("save_history".into(), toml::Value::Boolean(playback.save_history));
}

fn set_scrobble_section(table: &mut toml::map::Map<String, toml::Value>, scrobble: &ScrobbleConfig) {
    let section = table
        .entry("scrobble")
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
    let sec = section.as_table_mut().unwrap();
    sec.insert("enabled".into(), toml::Value::Boolean(scrobble.enabled));
    sec.insert("service".into(), toml::Value::String(service_to_str(scrobble.service).into()));
    sec.insert("api_key".into(), toml::Value::String(scrobble.api_key.clone()));
}

fn service_to_str(service: ScrobbleService) -> &'static str {
    match service {
        ScrobbleService::LastFm => "lastfm",
        ScrobbleService::ListenBrainz => "listenbrainz",
    }
}

fn set_keybindings_section(table: &mut toml::map::Map<String, toml::Value>, kb: &KeybindingsConfig) {
    let section = table
        .entry("keybindings")
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
    let sec = section.as_table_mut().unwrap();
    match &kb.path {
        Some(path) => { sec.insert("path".into(), toml::Value::String(path.clone())); }
        None => { sec.remove("path"); }
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use crate::config_toml::parse::parse_toml;
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

    fn valid_service_strategy() -> impl Strategy<Value = ScrobbleService> {
        prop_oneof![
            Just(ScrobbleService::LastFm),
            Just(ScrobbleService::ListenBrainz),
        ]
    }

    fn valid_app_config_strategy() -> impl Strategy<Value = AppConfig> {
        (
            any::<Option<String>>(),
            0u8..=100u8,
            valid_theme_strategy(),
            any::<bool>(),
            any::<bool>(),
            any::<bool>(),
            any::<bool>(),
            any::<bool>(),
            valid_service_strategy(),
            "[a-zA-Z0-9]{0,64}",
            any::<Option<String>>(),
        )
            .prop_map(
                |(
                    output_device,
                    default_volume,
                    theme,
                    notifications_enabled,
                    stream_metadata_enabled,
                    autoplay_last,
                    save_history,
                    scrobble_enabled,
                    service,
                    api_key,
                    keybindings_path,
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
                        },
                        scrobble: ScrobbleConfig {
                            enabled: scrobble_enabled,
                            service,
                            api_key,
                        },
                        keybindings: KeybindingsConfig {
                            path: keybindings_path,
                        },
                    }
                },
            )
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Validates: Requirements 7.10**
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
            },
            scrobble: ScrobbleConfig {
                enabled: true,
                service: ScrobbleService::ListenBrainz,
                api_key: "my-api-key".to_string(),
            },
            keybindings: KeybindingsConfig {
                path: Some("keys.json".to_string()),
            },
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
    fn test_serialize_toml_lastfm_service_serialized_correctly() {
        let config = AppConfig {
            scrobble: ScrobbleConfig {
                enabled: true,
                service: ScrobbleService::LastFm,
                api_key: "key".to_string(),
            },
            ..AppConfig::default()
        };
        let preserved = toml::Value::Table(toml::map::Map::new());
        let output = serialize_toml(&config, &preserved);

        assert!(output.contains("service = \"lastfm\""));
    }

    #[test]
    fn test_serialize_toml_listenbrainz_service_serialized_correctly() {
        let config = AppConfig {
            scrobble: ScrobbleConfig {
                enabled: true,
                service: ScrobbleService::ListenBrainz,
                api_key: "key".to_string(),
            },
            ..AppConfig::default()
        };
        let preserved = toml::Value::Table(toml::map::Map::new());
        let output = serialize_toml(&config, &preserved);

        assert!(output.contains("service = \"listenbrainz\""));
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
}
