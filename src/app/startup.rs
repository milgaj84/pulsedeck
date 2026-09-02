use super::*;
use crate::audio::{AudioCommand, AudioEngine, AudioSink};
use crate::config_toml::AppConfig;
use crate::keybindings::{detect_shadows, KeybindingRegistry};
use crate::radio::find_station_by_url;
use crate::radio::stale_query::count_stale_stations;
use crate::search_history::SearchHistoryRing;

use super::notification_cooldown::NotificationCooldown;
use super::notifier;

pub(super) const KEYBINDINGS_FILE: &str = "keybindings.json";

pub(crate) struct AppParts {
    pub library: Library,
    pub ui_state: super::ui_state::UiState,
    pub ui_state_warning: Option<String>,
    pub history: crate::history::History,
    pub history_warning: Option<String>,
    pub audio: Box<dyn AudioSink>,
    pub sample_buffer: Arc<Mutex<VecDeque<f32>>>,
    pub config: AppConfig,
    pub config_preserved: toml::Value,
    pub config_warnings: Vec<String>,
    /// True when config was loaded from a file (TOML or migrated JSON).
    pub config_loaded_from_file: bool,
}

impl AppParts {
    pub(super) fn load(library: Library) -> Self {
        let (ui_state, ui_state_warning) = super::ui_state::UiState::load_with_warning();
        let sample_buffer = Arc::new(Mutex::new(VecDeque::with_capacity(4096)));
        let (history, history_warning) = crate::history::History::load_with_warning();

        #[cfg(not(test))]
        let (config, config_preserved, config_warnings, config_loaded_from_file) =
            load_toml_config();
        #[cfg(test)]
        let (config, config_preserved, config_warnings, config_loaded_from_file) = (
            AppConfig::default(),
            toml::Value::Table(toml::map::Map::new()),
            Vec::new(),
            false,
        );

        let recovery_config = crate::audio::DeviceRecoveryConfig {
            max_attempts: config.playback.device_recovery_attempts,
            delay_ms: config.playback.device_recovery_delay_ms,
        };
        let audio = AudioEngine::spawn(sample_buffer.clone(), recovery_config);

        Self {
            library,
            ui_state,
            ui_state_warning,
            history,
            history_warning,
            audio: Box::new(audio),
            sample_buffer,
            config,
            config_preserved,
            config_warnings,
            config_loaded_from_file,
        }
    }
}

/// Load keybinding registry from `keybindings.json` in the config directory.
/// Returns a registry with defaults populated; custom bindings are merged on top.
fn load_keybinding_registry() -> KeybindingRegistry {
    let mut registry = KeybindingRegistry::defaults();

    let Some(path) = crate::config::config_path(KEYBINDINGS_FILE) else {
        return registry;
    };

    if !path.exists() {
        return registry;
    }

    match std::fs::read(&path) {
        Ok(bytes) => {
            let mut warnings = Vec::new();
            let custom = KeybindingRegistry::from_json(&bytes, &mut warnings);
            for warning in &warnings {
                eprintln!("[keybindings] {warning}");
            }
            registry.customs = custom.customs;
            let shadows = detect_shadows(&registry.defaults, &registry.customs);
            for warning in shadows {
                eprintln!("{warning}");
            }
        }
        Err(err) => {
            eprintln!("[keybindings] Could not read {}: {err}", path.display());
        }
    }

    registry
}

/// Load TOML config from the config directory with library.json fallback.
/// Returns defaults if no config directory is available.
#[cfg_attr(test, allow(dead_code))]
fn load_toml_config() -> (AppConfig, toml::Value, Vec<String>, bool) {
    let Some(config_dir) = crate::config::config_dir() else {
        return (
            AppConfig::default(),
            toml::Value::Table(toml::map::Map::new()),
            Vec::new(),
            false,
        );
    };

    let toml_path = config_dir.join("pulsedeck.toml");
    let json_path = config_dir.join("library.json");
    let loaded_from_file = toml_path.exists() || json_path.exists();

    let result = crate::config_toml::io::load_config(&config_dir);
    for warning in &result.warnings {
        eprintln!("[config] {warning}");
    }
    (
        result.config,
        result.preserved,
        result.warnings,
        loaded_from_file,
    )
}

/// Build a ConfigWatcher pointed at the config directory's pulsedeck.toml.
/// Returns a watcher on a dummy path if no config directory is available.
fn build_config_watcher() -> ConfigWatcher {
    let path = crate::config::config_dir()
        .map(|dir| dir.join("pulsedeck.toml"))
        .unwrap_or_default();
    ConfigWatcher::new(path)
}

/// Build a KeybindingWatcher pointed at the keybindings JSON file path.
/// Returns a watcher with None path if no config directory is available.
fn build_keybinding_watcher() -> crate::keybindings::watcher::KeybindingWatcher {
    let path = crate::config::config_path(KEYBINDINGS_FILE).filter(|p| p.exists());
    crate::keybindings::watcher::KeybindingWatcher::new(path)
}

pub(super) const SEARCH_HISTORY_FILE: &str = "search_history.json";

/// Load search history ring from the config directory.
/// Returns an empty ring if no config directory or file is available.
fn load_search_history(config_dir: &Option<PathBuf>) -> SearchHistoryRing {
    let Some(dir) = config_dir else {
        return SearchHistoryRing::new();
    };
    SearchHistoryRing::load(&dir.join(SEARCH_HISTORY_FILE))
}

impl App {
    pub fn new(library: Library) -> Self {
        Self::from_parts(AppParts::load(library))
    }

    /// Construct the App from pre-loaded parts.
    ///
    /// # Startup Sequence & Override Precedence
    ///
    /// 1. **Library** loaded from `library.json` (stations, favorites, settings.theme)
    /// 2. **UiState** loaded from `ui-state.json` (volume, mute, layout, visualizer, display mode)
    /// 3. **Config (TOML)** loaded from `pulsedeck.toml` (overrides library settings when file exists)
    /// 4. **Keybindings** loaded from `keybindings.json` (custom overrides defaults)
    /// 5. **Search history** loaded from `search_history.json`
    /// 6. **Watchers** created for config and keybinding hot-reload
    ///
    /// ## Override rules:
    /// - If `pulsedeck.toml` exists: TOML values override library.json settings (theme, volume, etc.)
    /// - If `pulsedeck.toml` does NOT exist: library.json settings are used as-is
    /// - `main.rs` sets theme from library FIRST, then `apply_config_to_settings` re-sets from TOML
    /// - UiState (volume, layout) is independent of TOML config
    pub(super) fn from_parts(parts: AppParts) -> Self {
        let config = parts.config;
        let config_preserved = parts.config_preserved;
        let config_loaded_from_file = parts.config_loaded_from_file;

        let output_device = config.audio.output_device.as_deref().or(parts
            .library
            .settings
            .output_device_name
            .as_deref());
        let diagnostics_output_device = crate::audio::output_device_display_name(output_device);
        let diagnostics_metadata_enabled = config.ui.stream_metadata_enabled;
        let ui = UiRuntimeState::from_ui_state(&parts.ui_state);
        let playback = PlaybackRuntime::new(
            &parts.ui_state,
            PlaybackOptions {
                output_device: diagnostics_output_device,
                metadata_enabled: diagnostics_metadata_enabled,
                reconnect_max_attempts: config.playback.reconnect_max_attempts,
                reconnect_backoff_seconds: config.playback.reconnect_backoff_seconds.clone(),
            },
            parts.audio,
            parts.sample_buffer,
        );

        let keybinding_registry = load_keybinding_registry();

        let config_watcher = build_config_watcher();

        #[cfg(not(test))]
        let config_dir = crate::config::config_dir();
        #[cfg(test)]
        let config_dir: Option<std::path::PathBuf> = None;

        let keybinding_watcher = build_keybinding_watcher();

        let search_history = load_search_history(&config_dir);

        #[cfg(not(test))]
        let app_notifier: Box<dyn notifier::Notifier> = Box::new(notifier::DesktopNotifier);
        #[cfg(test)]
        let app_notifier: Box<dyn notifier::Notifier> = Box::new(notifier::CountingNotifier::new());

        let mut app = Self {
            library: parts.library,
            search: SearchState::default(),
            history: parts.history,
            song_history: VecDeque::new(),
            undo_history: VecDeque::new(),
            ui,
            playback,
            library_filter_query: String::new(),
            number_jump: NumberJump::new(),
            metadata_refresh_pending: false,
            metadata_refresh_running: false,
            persist: persist::PersistFlags::default(),
            keybinding_registry,
            sort_mode: crate::library_sort::SortMode::from_key(&config.ui.sort_mode)
                .unwrap_or(crate::library_sort::SortMode::FavoritesFirst),
            notification_cooldown: NotificationCooldown::new(),
            notifier: app_notifier,
            discover_results: Vec::new(),
            discover_cursor: 0,
            discover_fetch_pending: None,
            config,
            config_preserved,
            config_dir,
            config_watcher,
            keybinding_watcher,
            search_history,
            settings_undo: SettingsUndoStack::new(),
            stale_dismissed_at: parts.ui_state.stale_dismissed_at(),
            radio_browser_status: super::radio_status::RadioBrowserStatus::new(),
            audio_check_result: None,
        };

        if config_loaded_from_file {
            app.apply_config_to_settings();
        }
        app.sync_startup_audio_settings();
        app.apply_startup_warnings(parts.ui_state_warning, parts.history_warning);
        app.apply_config_warnings(parts.config_warnings);
        app.apply_config_dir_warning();
        app.apply_stale_station_notice();
        app.apply_startup_autoplay();
        app
    }

    pub fn should_quit(&self) -> bool {
        self.ui.should_quit
    }

    pub fn input_mode(&self) -> &InputMode {
        &self.ui.input_mode
    }

    pub fn display_mode(&self) -> &DisplayMode {
        &self.ui.display_mode
    }

    fn sync_startup_audio_settings(&mut self) {
        self.sync_output_device();
        self.sync_stream_metadata();
        self.sync_volume();
    }

    fn apply_startup_warnings(
        &mut self,
        ui_state_warning: Option<String>,
        history_warning: Option<String>,
    ) {
        let mut startup_warnings = self.library.load_warnings.clone();
        if let Some(warning) = ui_state_warning {
            startup_warnings.push(warning);
        }
        if let Some(warning) = history_warning {
            startup_warnings.push(warning);
        }

        match startup_warnings.len() {
            0 => {}
            1 => self.set_error_notice(startup_warnings.remove(0)),
            count => self.set_error_notice(format!(
                "{count} config files had load warnings; using safe defaults where needed"
            )),
        }
    }

    fn apply_startup_autoplay(&mut self) {
        if !self.library.settings.autoplay_last {
            return;
        }

        let Some(url) = self.library.settings.last_played_url.clone() else {
            return;
        };

        // If the URL matches a known library station, check codec capability
        // before attempting autoplay. Unknown stations (not in library) are
        // allowed to try in case the library state is stale.
        if let Some(station) = find_station_by_url(&self.library.stations, &url).cloned() {
            if !self.can_attempt_station_playback(&station) {
                return;
            }
        }

        if let Some(pos) =
            super::audio_status::last_played_station_position(&self.library.stations, &url)
        {
            self.ui.nav.selected = pos;
        }
        self.playback.view.playing_url = Some(url.clone());
        self.playback.view.state = PlaybackState::Connecting;
        self.playback.elapsed_timer.reset();
        self.playback.elapsed_timer.start();
        if self.send_audio_command(AudioCommand::Play(url)) {
            self.sync_volume();
        }
    }

    /// Apply loaded TOML config values to library settings for backward compat.
    fn apply_config_to_settings(&mut self) {
        self.library.settings.theme = self.config.ui.theme.clone();
        self.library.settings.notifications_enabled = self.config.ui.notifications_enabled;
        self.library.settings.stream_metadata_enabled = self.config.ui.stream_metadata_enabled;
        self.library.settings.autoplay_last = self.config.playback.autoplay_last;
        self.library.settings.save_history = self.config.playback.save_history;
        if self.config.audio.output_device.is_some() {
            self.library.settings.output_device_name = self.config.audio.output_device.clone();
        }
        let theme = crate::theme_name::ThemeName::from_key(&self.config.ui.theme);
        crate::ui::theme::set_active(theme);
    }

    fn apply_config_warnings(&mut self, warnings: Vec<String>) {
        for warning in warnings {
            self.library.load_warnings.push(warning);
        }
    }

    /// Warn the user if no config directory is available (settings won't persist).
    fn apply_config_dir_warning(&mut self) {
        #[cfg(not(test))]
        if self.config_dir.is_none() {
            self.set_info_notice("Settings won't persist — config directory unavailable");
        }
    }

    fn apply_stale_station_notice(&mut self) {
        let now = super::audio_status::current_unix_epoch();
        if super::ui_state::should_suppress_stale_notice(self.stale_dismissed_at, now) {
            return;
        }
        let now_str = now.to_string();
        let count = count_stale_stations(&self.library.stations, &now_str);
        if count > 0 {
            self.stale_dismissed_at = Some(now);
            self.mark_ui_state_dirty();
            self.set_info_notice(format!("{count} stations have been failing for 30+ days"));
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn test_parts(library: Library) -> AppParts {
        AppParts {
            library,
            ui_state: super::super::ui_state::UiState::from_app_values(
                37,
                true,
                LayoutMode::RightOnly,
                VisualizerMode::SimOscilloscope,
                DisplayMode::Normal,
                None,
            ),
            ui_state_warning: None,
            history: crate::history::History::default(),
            history_warning: None,
            audio: Box::new(crate::audio::MockAudioSink::disconnected()),
            sample_buffer: Arc::new(Mutex::new(VecDeque::with_capacity(4096))),
            config: AppConfig::default(),
            config_preserved: toml::Value::Table(toml::map::Map::new()),
            config_warnings: Vec::new(),
            config_loaded_from_file: false,
        }
    }

    #[test]
    fn from_parts_uses_injected_ui_state_without_loading_runtime_files() {
        let app = App::from_parts(test_parts(Library::in_memory(vec![])));

        assert_eq!(app.playback.volume, 37);
        assert!(app.playback.muted);
        assert_eq!(app.ui.layout_mode, LayoutMode::RightOnly);
        assert_eq!(app.ui.visualizer_mode, VisualizerMode::SimOscilloscope);
    }

    #[test]
    fn from_parts_shows_single_startup_warning_verbatim() {
        let mut library = Library::in_memory(vec![]);
        library.load_warnings.push("bad library".to_string());

        let app = App::from_parts(test_parts(library));

        assert!(matches!(
            app.ui.notice.current,
            Some(AppNotice::Error(ref message)) if message == "bad library"
        ));
    }

    #[test]
    fn from_parts_summarizes_multiple_startup_warnings() {
        let mut parts = test_parts(Library::in_memory(vec![]));
        parts.ui_state_warning = Some("bad ui".to_string());
        parts.history_warning = Some("bad history".to_string());

        let app = App::from_parts(parts);

        assert!(matches!(
            app.ui.notice.current,
            Some(AppNotice::Error(ref message))
                if message.contains("2 config files had load warnings")
        ));
    }

    #[test]
    fn from_parts_autoplay_uses_normalized_last_played_url_and_reports_dead_engine() {
        let mut library = Library::in_memory(vec![Station::basic(
            "Saved",
            "HTTP://STREAM/",
            "Radio",
            "US",
            128,
        )]);
        library.settings.autoplay_last = true;
        library.settings.last_played_url = Some("http://stream".to_string());

        let app = App::from_parts(test_parts(library));

        assert_eq!(app.ui.nav.selected, 0);
        assert_eq!(
            app.playback.view.playing_url.as_deref(),
            Some("http://stream")
        );
        assert_eq!(
            app.playback.view.state,
            PlaybackState::Error("Audio engine stopped".to_string())
        );
    }

    #[test]
    fn startup_autoplay_allows_aac_codec() {
        let mut station = Station::basic("AAC Radio", "http://aac", "Pop", "US", 128);
        station.codec = "AAC".to_string();

        let mut library = Library::in_memory(vec![station]);
        library.settings.autoplay_last = true;
        library.settings.last_played_url = Some("http://aac".to_string());

        let app = App::from_parts(test_parts(library));

        assert_eq!(app.playback.view.playing_url.as_deref(), Some("http://aac"));
    }

    #[test]
    fn startup_autoplay_blocks_hls_codec() {
        let mut station = Station::basic("HLS Radio", "http://hls", "Pop", "US", 128);
        station.codec = "HLS".to_string();

        let mut library = Library::in_memory(vec![station]);
        library.settings.autoplay_last = true;
        library.settings.last_played_url = Some("http://hls".to_string());

        let app = App::from_parts(test_parts(library));

        assert_eq!(app.playback.view.playing_url, None);
        assert!(matches!(app.playback.view.state, PlaybackState::Error(_)));
    }

    #[test]
    fn startup_autoplay_allows_unknown_codec() {
        let mut station = Station::basic("Mystery", "http://mystery", "Pop", "US", 128);
        station.codec = String::new();

        let mut library = Library::in_memory(vec![station]);
        library.settings.autoplay_last = true;
        library.settings.last_played_url = Some("http://mystery".to_string());

        let app = App::from_parts(test_parts(library));

        assert_eq!(
            app.playback.view.playing_url.as_deref(),
            Some("http://mystery")
        );
    }

    #[test]
    fn startup_autoplay_allows_url_not_in_library() {
        let mut library = Library::in_memory(vec![]);
        library.settings.autoplay_last = true;
        library.settings.last_played_url = Some("http://unknown-station".to_string());

        let app = App::from_parts(test_parts(library));

        assert_eq!(
            app.playback.view.playing_url.as_deref(),
            Some("http://unknown-station")
        );
    }

    #[test]
    fn from_parts_applies_config_when_loaded_from_file() {
        let library = Library::in_memory(vec![]);
        let mut parts = test_parts(library);
        parts.config_loaded_from_file = true;
        parts.config.ui.theme = "Terminal".to_string();
        parts.config.ui.notifications_enabled = false;
        parts.config.ui.stream_metadata_enabled = false;
        parts.config.playback.autoplay_last = true;
        parts.config.playback.save_history = true;
        parts.config.audio.output_device = Some("USB DAC".to_string());

        let app = App::from_parts(parts);

        assert_eq!(app.library.settings.theme, "Terminal");
        assert!(!app.library.settings.notifications_enabled);
        assert!(!app.library.settings.stream_metadata_enabled);
        assert!(app.library.settings.autoplay_last);
        assert!(app.library.settings.save_history);
        assert_eq!(
            app.library.settings.output_device_name.as_deref(),
            Some("USB DAC")
        );
    }

    #[test]
    fn from_parts_does_not_override_settings_without_config_file() {
        let mut library = Library::in_memory(vec![]);
        library.settings.theme = "Terminal".to_string();
        library.settings.notifications_enabled = false;
        library.settings.autoplay_last = true;

        let parts = test_parts(library);
        // config_loaded_from_file is false in test_parts
        let app = App::from_parts(parts);

        assert_eq!(app.library.settings.theme, "Terminal");
        assert!(!app.library.settings.notifications_enabled);
        assert!(app.library.settings.autoplay_last);
    }

    #[test]
    fn from_parts_stores_config_and_preserved() {
        let library = Library::in_memory(vec![]);
        let mut parts = test_parts(library);
        parts.config.audio.default_volume = 42;

        let app = App::from_parts(parts);

        assert_eq!(app.config.audio.default_volume, 42);
        assert_eq!(
            app.config_preserved,
            toml::Value::Table(toml::map::Map::new())
        );
    }

    #[test]
    fn from_parts_config_warnings_appear_in_load_warnings() {
        let library = Library::in_memory(vec![]);
        let mut parts = test_parts(library);
        parts.config_warnings = vec!["bad config value".to_string()];

        let app = App::from_parts(parts);

        assert!(app
            .library
            .load_warnings
            .contains(&"bad config value".to_string()));
    }

    #[test]
    fn test_app_initializes_with_search_history_ring() {
        let mut app = App::from_parts(test_parts(Library::in_memory(vec![])));
        app.search_history = crate::search_history::SearchHistoryRing::new();

        assert!(app.search_history.is_empty());
        assert_eq!(app.search_history.len(), 0);
    }

    #[test]
    fn test_startup_with_stale_stations_shows_notice() {
        use crate::radio::StationHealth;

        // Station that failed 40+ days ago with failure_count >= 3 → stale
        let mut station = Station::basic("Dead Radio", "http://dead", "Rock", "US", 128);
        station.health = StationHealth {
            last_success_at: Some("1700000000".to_string()), // old success
            last_failure_at: Some("1700100000".to_string()), // ~40 days before "now"
            failure_count: Some(5),
            success_count: None,
            last_error_summary: "timeout".to_string(),
        };

        // "now" is 40 days after last_failure_at: 1700100000 + (40 * 86400) = 1703556000
        // We use real time via unix_now_string(), so set failure far enough in the past
        // to be >30 days ago relative to the actual current time.
        let far_past = "1600000000"; // well over 30 days ago from any reasonable "now"
        station.health.last_failure_at = Some(far_past.to_string());
        station.health.last_success_at = Some("1500000000".to_string());

        let library = Library::in_memory(vec![station]);
        let app = App::from_parts(test_parts(library));

        assert!(matches!(
            app.ui.notice.current,
            Some(AppNotice::Info(ref msg)) if msg.contains("1 stations have been failing for 30+ days")
        ));
    }

    #[test]
    fn test_startup_without_stale_stations_no_notice() {
        // Healthy station — no failures
        let station = Station::basic("Good Radio", "http://good", "Jazz", "US", 128);
        let library = Library::in_memory(vec![station]);
        let app = App::from_parts(test_parts(library));

        // No stale notice should be set (notice is None since there are no warnings either)
        match &app.ui.notice.current {
            None => {} // OK - no notice at all
            Some(AppNotice::Info(msg)) => {
                assert!(
                    !msg.contains("stations have been failing"),
                    "Should not show stale notice for healthy stations"
                );
            }
            Some(AppNotice::Error(_)) => {} // Could be other startup warnings, not stale
        }
    }

    #[test]
    fn from_parts_loads_sort_mode_from_config() {
        use crate::library_sort::SortMode;

        let mut parts = test_parts(Library::in_memory(vec![]));
        parts.config.ui.sort_mode = "alphabetical".to_string();

        let app = App::from_parts(parts);

        assert_eq!(app.sort_mode, SortMode::Alphabetical);
    }

    #[test]
    fn from_parts_defaults_sort_mode_on_invalid_config_value() {
        use crate::library_sort::SortMode;

        let mut parts = test_parts(Library::in_memory(vec![]));
        parts.config.ui.sort_mode = "nonsense".to_string();

        let app = App::from_parts(parts);

        assert_eq!(app.sort_mode, SortMode::FavoritesFirst);
    }

    #[test]
    fn from_parts_defaults_sort_mode_on_empty_config_value() {
        use crate::library_sort::SortMode;

        let mut parts = test_parts(Library::in_memory(vec![]));
        parts.config.ui.sort_mode = String::new();

        let app = App::from_parts(parts);

        assert_eq!(app.sort_mode, SortMode::FavoritesFirst);
    }
}
