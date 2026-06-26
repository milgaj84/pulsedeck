use super::*;
use crate::config_toml::AppConfig;
use std::time::Instant;

impl App {
    /// Check config file for changes and apply hot-reloadable settings.
    pub(super) fn check_config_reload(&mut self, now: Instant) {
        use crate::config_toml::hot_reload::ReloadResult;

        match self.config_watcher.check_reload(now) {
            ReloadResult::Unchanged => {}
            ReloadResult::Reloaded(new_config, new_preserved) => {
                self.apply_hot_reload(new_config, new_preserved);
                self.set_info_notice("Config reloaded");
            }
            ReloadResult::Error(msg) => {
                self.set_error_notice(format!("Config reload failed: {msg}"));
            }
        }
    }

    /// Check keybinding file for changes and rebuild registry on reload.
    pub(super) fn check_keybinding_reload(&mut self, now: Instant) {
        use crate::keybindings::watcher::KeybindingReloadResult;

        match self.keybinding_watcher.check_reload(now) {
            KeybindingReloadResult::Unchanged => {}
            KeybindingReloadResult::Reloaded(customs) => {
                self.keybinding_registry.customs = customs;
                self.set_info_notice("Keybindings reloaded");
            }
            KeybindingReloadResult::Error(msg) => {
                self.set_error_notice(format!("Keybinding reload failed: {msg}"));
            }
        }
    }

    /// Apply hot-reloadable config fields (ui, audio.default_volume, playback, discover).
    /// Does NOT apply keybindings (requires restart).
    pub(super) fn apply_hot_reload(&mut self, new_config: AppConfig, new_preserved: toml::Value) {
        self.config.ui = new_config.ui.clone();
        self.config.audio.default_volume = new_config.audio.default_volume;
        self.config.playback = new_config.playback.clone();
        self.config.discover = new_config.discover.clone();
        self.config_preserved = new_preserved;

        self.playback.reconnect.update_params(
            self.config.playback.reconnect_max_attempts,
            self.config.playback.reconnect_backoff_seconds.clone(),
        );

        self.library.settings.theme = new_config.ui.theme.clone();
        self.library.settings.notifications_enabled = new_config.ui.notifications_enabled;
        self.library.settings.stream_metadata_enabled = new_config.ui.stream_metadata_enabled;
        self.library.settings.autoplay_last = new_config.playback.autoplay_last;
        self.library.settings.save_history = new_config.playback.save_history;

        let theme = crate::theme_name::ThemeName::from_key(&new_config.ui.theme);
        crate::ui::theme::set_active(theme);
        self.sync_stream_metadata();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::startup::tests::test_parts;
    use crate::favorites::Library;
    use std::time::{Duration, Instant};

    #[test]
    fn check_config_reload_unchanged_does_nothing() {
        let mut app = App::from_parts(test_parts(Library::in_memory(vec![])));
        // ConfigWatcher points to nonexistent path → always Unchanged
        app.check_config_reload(Instant::now());

        assert!(app.ui.notice.current.is_none());
    }

    #[test]
    fn check_config_reload_reloaded_applies_ui_settings() {
        let dir = std::env::temp_dir()
            .join("pulsedeck_hot_reload_integration")
            .join("ui_settings");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("pulsedeck.toml");
        std::fs::write(
            &path,
            "[ui]\ntheme = \"Terminal\"\nnotifications_enabled = false\nstream_metadata_enabled = false\n\n[playback]\nautoplay_last = true\nsave_history = true\n",
        ).unwrap();

        let mut app = App::from_parts(test_parts(Library::in_memory(vec![])));
        app.config_watcher = crate::config_toml::hot_reload::ConfigWatcher::new(path);

        let t0 = Instant::now();
        app.check_config_reload(t0); // detect change, start debounce
        app.check_config_reload(t0 + Duration::from_millis(500)); // debounce fires

        assert_eq!(app.library.settings.theme, "Terminal");
        assert!(!app.library.settings.notifications_enabled);
        assert!(!app.library.settings.stream_metadata_enabled);
        assert!(app.library.settings.autoplay_last);
        assert!(app.library.settings.save_history);
        assert!(matches!(
            app.ui.notice.current,
            Some(AppNotice::Info(ref msg)) if msg == "Config reloaded"
        ));
    }

    #[test]
    fn check_config_reload_applies_audio_default_volume() {
        let dir = std::env::temp_dir()
            .join("pulsedeck_hot_reload_integration")
            .join("audio_vol");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("pulsedeck.toml");
        std::fs::write(&path, "[audio]\ndefault_volume = 42\n").unwrap();

        let mut app = App::from_parts(test_parts(Library::in_memory(vec![])));
        app.config_watcher = crate::config_toml::hot_reload::ConfigWatcher::new(path);

        let t0 = Instant::now();
        app.check_config_reload(t0);
        app.check_config_reload(t0 + Duration::from_millis(500));

        assert_eq!(app.config.audio.default_volume, 42);
    }

    #[test]
    fn check_config_reload_does_not_apply_keybindings() {
        let dir = std::env::temp_dir()
            .join("pulsedeck_hot_reload_integration")
            .join("no_keybindings");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("pulsedeck.toml");
        std::fs::write(&path, "[keybindings]\npath = \"/custom/path.json\"\n").unwrap();

        let mut app = App::from_parts(test_parts(Library::in_memory(vec![])));
        let original_keybindings = app.config.keybindings.clone();
        app.config_watcher = crate::config_toml::hot_reload::ConfigWatcher::new(path);

        let t0 = Instant::now();
        app.check_config_reload(t0);
        app.check_config_reload(t0 + Duration::from_millis(500));

        assert_eq!(app.config.keybindings, original_keybindings);
    }

    #[test]
    fn check_config_reload_error_retains_previous_config() {
        let dir = std::env::temp_dir()
            .join("pulsedeck_hot_reload_integration")
            .join("error_retain");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("pulsedeck.toml");
        std::fs::write(&path, "this is [[[not valid toml").unwrap();

        let mut app = App::from_parts(test_parts(Library::in_memory(vec![])));
        let original_theme = app.library.settings.theme.clone();
        app.config_watcher = crate::config_toml::hot_reload::ConfigWatcher::new(path);

        let t0 = Instant::now();
        app.check_config_reload(t0);
        app.check_config_reload(t0 + Duration::from_millis(500));

        assert_eq!(app.library.settings.theme, original_theme);
        assert!(matches!(
            app.ui.notice.current,
            Some(AppNotice::Error(ref msg)) if msg.contains("Config reload failed")
        ));
    }

    #[test]
    fn check_config_reload_shows_info_notice_on_success() {
        let dir = std::env::temp_dir()
            .join("pulsedeck_hot_reload_integration")
            .join("info_notice");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("pulsedeck.toml");
        std::fs::write(&path, "[audio]\ndefault_volume = 55\n").unwrap();

        let mut app = App::from_parts(test_parts(Library::in_memory(vec![])));
        app.config_watcher = crate::config_toml::hot_reload::ConfigWatcher::new(path);

        let t0 = Instant::now();
        app.check_config_reload(t0);
        app.check_config_reload(t0 + Duration::from_millis(500));

        assert!(matches!(
            app.ui.notice.current,
            Some(AppNotice::Info(ref msg)) if msg == "Config reloaded"
        ));
    }

    /// **Validates: Requirements 2.1, 2.3**
    #[test]
    fn hot_reload_propagates_discover_config() {
        let mut app = App::from_parts(test_parts(Library::in_memory(vec![])));

        let mut new_config = app.config.clone();
        new_config.discover.genre_weight = 10;
        new_config.discover.tag_weight = 5;
        new_config.discover.country_weight = 8;
        new_config.discover.exclude_tags = vec!["rock".to_string(), "metal".to_string()];
        new_config.discover.exclude_countries = vec!["XX".to_string()];

        let preserved = toml::Value::Table(toml::map::Map::new());
        app.apply_hot_reload(new_config.clone(), preserved);

        assert_eq!(app.config.discover, new_config.discover);
    }

    /// **Validates: Requirements 2.2, 2.4**
    #[test]
    fn hot_reload_updates_reconnect_params() {
        let mut app = App::from_parts(test_parts(Library::in_memory(vec![])));

        let mut new_config = app.config.clone();
        new_config.playback.reconnect_max_attempts = 7;
        new_config.playback.reconnect_backoff_seconds = vec![1, 2, 4, 8];

        let preserved = toml::Value::Table(toml::map::Map::new());
        app.apply_hot_reload(new_config, preserved);

        assert_eq!(app.playback.reconnect.max(), 7);
        // Verify the new backoff is used: first backoff should be 1s
        let now = std::time::Instant::now();
        app.playback.reconnect.arm("http://test".to_string(), now);
        let t1 = now + Duration::from_secs(1);
        assert_eq!(
            app.playback.reconnect.take_due(t1),
            Some("http://test".to_string())
        );
    }

    #[test]
    fn test_keybinding_reload_updates_registry_and_shows_notice() {
        let dir = std::env::temp_dir()
            .join("pulsedeck_keybinding_reload_tests")
            .join("success");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("keybindings.json");
        std::fs::write(
            &path,
            r#"[{"key": "char(x)", "modifiers": [], "action": "quit"}]"#,
        )
        .unwrap();

        let mut app = App::from_parts(test_parts(Library::in_memory(vec![])));
        app.keybinding_watcher = crate::keybindings::watcher::KeybindingWatcher::new(Some(path));

        let t0 = Instant::now();
        // After 500ms startup cooldown, detect mtime change
        app.check_keybinding_reload(t0 + Duration::from_millis(501));
        // After additional 500ms debounce, reload fires
        app.check_keybinding_reload(t0 + Duration::from_millis(1001));

        assert_eq!(app.keybinding_registry.customs.len(), 1);
        assert_eq!(
            app.keybinding_registry.customs[0].action,
            crate::action::Action::Quit
        );
        assert!(matches!(
            app.ui.notice.current,
            Some(AppNotice::Info(ref msg)) if msg == "Keybindings reloaded"
        ));
    }

    #[test]
    fn test_keybinding_reload_error_keeps_existing_registry() {
        let dir = std::env::temp_dir()
            .join("pulsedeck_keybinding_reload_tests")
            .join("error");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("keybindings.json");
        std::fs::write(&path, "not valid json{{{").unwrap();

        let mut app = App::from_parts(test_parts(Library::in_memory(vec![])));
        let original_customs = app.keybinding_registry.customs.clone();
        app.keybinding_watcher = crate::keybindings::watcher::KeybindingWatcher::new(Some(path));

        let t0 = Instant::now();
        // After 500ms startup cooldown, detect mtime change
        app.check_keybinding_reload(t0 + Duration::from_millis(501));
        // After additional 500ms debounce, reload fires (with error)
        app.check_keybinding_reload(t0 + Duration::from_millis(1001));

        assert_eq!(app.keybinding_registry.customs, original_customs);
        assert!(matches!(
            app.ui.notice.current,
            Some(AppNotice::Error(ref msg)) if msg.contains("Keybinding reload failed")
        ));
    }
}

#[cfg(test)]
mod hot_reload_proptests {
    use super::*;
    use crate::app::startup::tests::test_parts;
    use crate::config_toml::{AppConfig, DiscoverConfig};
    use crate::favorites::Library;
    use proptest::prelude::*;

    fn arb_discover_config() -> impl Strategy<Value = DiscoverConfig> {
        (
            0u32..=10,
            0u32..=10,
            0u32..=10,
            proptest::collection::vec("[a-z]{1,10}", 0..=5),
            proptest::collection::vec("[A-Z]{2,3}", 0..=3),
        )
            .prop_map(
                |(genre_weight, tag_weight, country_weight, exclude_tags, exclude_countries)| {
                    DiscoverConfig {
                        genre_weight,
                        tag_weight,
                        country_weight,
                        exclude_tags,
                        exclude_countries,
                    }
                },
            )
    }

    fn arb_reconnect_params() -> impl Strategy<Value = (u8, Vec<u64>)> {
        (1u8..=10, proptest::collection::vec(1u64..=60, 1..=10))
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Validates: Requirements 2.1, 2.2**
        #[test]
        fn prop_hot_reload_propagates_discover_and_reconnect(
            discover in arb_discover_config(),
            reconnect in arb_reconnect_params(),
        ) {
            let (max_attempts, backoff_seconds) = reconnect;

            let parts = test_parts(Library::in_memory(vec![]));
            let mut app = App::from_parts(parts);

            let mut new_config = AppConfig::default();
            new_config.discover = discover.clone();
            new_config.playback.reconnect_max_attempts = max_attempts;
            new_config.playback.reconnect_backoff_seconds = backoff_seconds.clone();

            let new_preserved = toml::Value::Table(toml::map::Map::new());
            app.apply_hot_reload(new_config, new_preserved);

            prop_assert_eq!(&app.config.discover, &discover);
            prop_assert_eq!(
                app.config.playback.reconnect_max_attempts, max_attempts
            );
            prop_assert_eq!(
                &app.config.playback.reconnect_backoff_seconds, &backoff_seconds
            );
            prop_assert_eq!(app.playback.reconnect.max(), max_attempts);
        }
    }
}
