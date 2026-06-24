use super::*;
use crate::audio::{AudioCommand, AudioEngine, AudioStatus};
use crate::config_toml::AppConfig;
use crate::keybindings::KeybindingRegistry;
use crate::radio::{find_station_by_url, find_station_index_by_url, station_url_matches};
use crate::scrobble::parse_track_metadata;
use std::time::{Duration, Instant};

const NOTICE_INFO_TICKS: u16 = 90;
const NOTICE_ERROR_TICKS: u16 = 150;
const SONG_HISTORY_CAP: usize = 100;
const NOTIFY_IDLE_MS: u64 = 120_000;
const KEYBINDINGS_FILE: &str = "keybindings.json";

pub(super) const NOTIFICATION_COOLDOWN: Duration = Duration::from_secs(5);

pub(crate) struct NotificationCooldown {
    last_notified: Option<Instant>,
}

impl NotificationCooldown {
    pub fn new() -> Self {
        Self {
            last_notified: None,
        }
    }

    pub fn may_notify(&self, now: Instant) -> bool {
        match self.last_notified {
            None => true,
            Some(last) => now.duration_since(last) >= NOTIFICATION_COOLDOWN,
        }
    }

    pub fn record_notification(&mut self, now: Instant) {
        self.last_notified = Some(now);
    }
}

fn last_played_station_position(stations: &[Station], last_played_url: &str) -> Option<usize> {
    find_station_index_by_url(stations, last_played_url)
}

fn unix_now_string() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string())
}

#[derive(Default)]
pub struct NoticeState {
    pub current: Option<AppNotice>,
    ticks_remaining: u16,
}

pub(super) struct AppParts {
    pub library: Library,
    pub ui_state: super::ui_state::UiState,
    pub ui_state_warning: Option<String>,
    pub history: crate::history::History,
    pub history_warning: Option<String>,
    pub audio: AudioEngine,
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
        let audio = AudioEngine::spawn(sample_buffer.clone());
        let (history, history_warning) = crate::history::History::load_with_warning();
        let (config, config_preserved, config_warnings, config_loaded_from_file) =
            load_toml_config();

        Self {
            library,
            ui_state,
            ui_state_warning,
            history,
            history_warning,
            audio,
            sample_buffer,
            config,
            config_preserved,
            config_warnings,
            config_loaded_from_file,
        }
    }
}

/// Load keybinding registry from `keybindings.json` in the config directory.
/// Returns an empty registry (defaults only) if the file is missing or invalid.
fn load_keybinding_registry() -> KeybindingRegistry {
    let Some(path) = crate::config::config_path(KEYBINDINGS_FILE) else {
        return KeybindingRegistry::new_with_defaults(Vec::new());
    };

    if !path.exists() {
        return KeybindingRegistry::new_with_defaults(Vec::new());
    }

    match std::fs::read(&path) {
        Ok(bytes) => {
            let mut warnings = Vec::new();
            let registry = KeybindingRegistry::from_json(&bytes, &mut warnings);
            for warning in &warnings {
                eprintln!("[keybindings] {warning}");
            }
            registry
        }
        Err(err) => {
            eprintln!("[keybindings] Could not read {}: {err}", path.display());
            KeybindingRegistry::new_with_defaults(Vec::new())
        }
    }
}

/// Load TOML config from the config directory with library.json fallback.
/// Returns defaults if no config directory is available.
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
    (result.config, result.preserved, result.warnings, loaded_from_file)
}

impl App {
    pub fn new(library: Library) -> Self {
        Self::from_parts(AppParts::load(library))
    }

    pub(super) fn from_parts(parts: AppParts) -> Self {
        let config = parts.config;
        let config_preserved = parts.config_preserved;
        let config_loaded_from_file = parts.config_loaded_from_file;

        let output_device = config.audio.output_device.as_deref()
            .or(parts.library.settings.output_device_name.as_deref());
        let diagnostics_output_device =
            crate::audio::output_device_display_name(output_device);
        let diagnostics_metadata_enabled = config.ui.stream_metadata_enabled;
        let ui = UiRuntimeState::from_ui_state(&parts.ui_state);
        let playback = PlaybackRuntime::new(
            &parts.ui_state,
            diagnostics_output_device,
            diagnostics_metadata_enabled,
            parts.audio,
            parts.sample_buffer,
        );

        let keybinding_registry = load_keybinding_registry();
        let scrobble_enabled = config.scrobble.enabled;

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
            notification_cooldown: NotificationCooldown::new(),
            discover_results: Vec::new(),
            scrobble_tracker: ScrobbleTracker::new(scrobble_enabled),
            config,
            config_preserved,
            #[cfg(test)]
            notification_count: 0,
        };

        if config_loaded_from_file {
            app.apply_config_to_settings();
        }
        app.sync_startup_audio_settings();
        app.apply_startup_warnings(parts.ui_state_warning, parts.history_warning);
        app.apply_config_warnings(parts.config_warnings);
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

        if let Some(pos) = last_played_station_position(&self.library.stations, &url) {
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

    pub(super) fn set_info_notice(&mut self, message: impl Into<String>) {
        self.ui.notice.current = Some(AppNotice::Info(message.into()));
        self.ui.notice.ticks_remaining = NOTICE_INFO_TICKS;
    }

    pub(super) fn set_error_notice(&mut self, message: impl Into<String>) {
        self.ui.notice.current = Some(AppNotice::Error(message.into()));
        self.ui.notice.ticks_remaining = NOTICE_ERROR_TICKS;
    }

    /// Convenience: set an error notice with a context prefix and error details.
    pub(super) fn set_operation_error_notice(
        &mut self,
        context: &str,
        err: &dyn std::fmt::Display,
    ) {
        self.set_error_notice(format!("{context}: {err}"));
    }

    pub(super) fn tick_notice(&mut self) {
        if self.ui.notice.ticks_remaining > 0 {
            self.ui.notice.ticks_remaining -= 1;
        } else {
            self.ui.notice.current = None;
        }
    }

    pub fn poll_audio_status(&mut self) {
        while let Ok(status) = self.playback.audio.status_rx.try_recv() {
            match status {
                AudioStatus::TrackChanged { url, title } => {
                    self.handle_track_changed(url, title);
                }
                AudioStatus::Playing => {
                    self.playback.view.state = PlaybackState::Playing;
                    self.playback.reconnect.disarm();
                    if let Some(url) = self.playback.view.playing_url.clone() {
                        if self.library.mark_station_success(&url, unix_now_string()) {
                            self.mark_library_dirty();
                        }
                    }
                    self.playback.diagnostics.decoder_state = DecoderState::Playing;
                    self.playback.diagnostics.last_event = Some("Playback started".to_string());
                    self.playback.diagnostics.last_error = None;
                }
                AudioStatus::Paused => {
                    self.playback.view.state = PlaybackState::Paused;
                    self.playback.diagnostics.last_event = Some("Playback paused".to_string());
                }
                AudioStatus::Stopped => {
                    self.handle_audio_stopped();
                }
                AudioStatus::Error(error) => {
                    self.playback.diagnostics.decoder_state = DecoderState::Failed;
                    self.playback.diagnostics.last_error = Some(error.clone());
                    self.handle_audio_error(error);
                }
                AudioStatus::FadingOut { current_volume } => {
                    self.playback.view.state = PlaybackState::FadingOut {
                        current_volume: current_volume.clamp(0.0, 1.0),
                    };
                    self.playback.diagnostics.last_event = Some("Fading out".to_string());
                }
                AudioStatus::Connecting => {
                    self.playback.view.current_track = None;
                    self.playback.view.state = PlaybackState::Connecting;
                    self.playback.diagnostics.decoder_state = DecoderState::Connecting;
                    self.playback.diagnostics.last_event = Some("Connecting to stream".to_string());
                }
                AudioStatus::Buffering { percent } => {
                    self.playback.diagnostics.decoder_state = DecoderState::Connecting;
                    self.playback.diagnostics.last_event = Some(format!("Buffering ({percent}%)"));
                }
            }
        }
    }

    pub(super) fn handle_track_changed(&mut self, url: String, title: String) {
        if !self
            .playback
            .view
            .playing_url
            .as_deref()
            .is_some_and(|playing_url| station_url_matches(playing_url, &url))
        {
            return;
        }

        let is_new = !title.is_empty() && self.playback.view.current_track.as_ref() != Some(&title);
        self.playback.view.current_track = Some(title.clone());

        if is_new {
            let meta = parse_track_metadata(&title);
            self.scrobble_tracker.on_track_change(meta);
        }

        if !title.is_empty() && self.song_history.back() != Some(&title) {
            self.song_history.push_back(title.clone());
            while self.song_history.len() > SONG_HISTORY_CAP {
                self.song_history.pop_front();
            }
            if self.library.settings.save_history {
                let station_name = self
                    .now_playing()
                    .map(|s| s.name.clone())
                    .unwrap_or_else(|| "Radio Stream".to_string());
                self.history.record(title.clone(), station_name);
                self.mark_history_dirty();
            }
        }

        if is_new && self.library.settings.notifications_enabled {
            let user_is_active = super::idle::get_user_idle_ms()
                .map(|idle_ms| idle_ms <= NOTIFY_IDLE_MS)
                .unwrap_or(true);

            if user_is_active {
                let now = Instant::now();
                if self.notification_cooldown.may_notify(now) {
                    self.notification_cooldown.record_notification(now);

                    let station_name = self
                        .now_playing()
                        .map(|s| s.name.clone())
                        .unwrap_or_else(|| "Radio Stream".to_string());

                    super::notifier::notify_now_playing(&title, &station_name);
                    #[cfg(test)]
                    {
                        self.notification_count += 1;
                    }
                }
            }
        }
    }

    fn handle_audio_stopped(&mut self) {
        let was_playing = self.playback.view.playing_url.is_some();
        if self.playback.view.intentional_stop || !was_playing {
            self.playback.view.intentional_stop = false;
            self.playback.view.playing_url = None;
            self.playback.view.reset_transient_status();
            self.playback.view.state = PlaybackState::Stopped;
            self.playback.diagnostics.decoder_state = DecoderState::Idle;
            self.playback.diagnostics.buffer_percent = 0;
            self.playback.diagnostics.buffer_seconds = 0;
            self.playback.diagnostics.last_event = Some("Playback stopped".to_string());
            self.playback.reconnect.disarm();
        } else if let Some(url) = self.playback.view.playing_url.clone() {
            self.playback.reconnect.arm(url, std::time::Instant::now());
            self.playback.view.state = PlaybackState::Connecting;
        }
    }

    fn handle_audio_error(&mut self, error: String) {
        if let Some(url) = self.playback.view.playing_url.clone() {
            self.playback
                .reconnect
                .arm(url.clone(), std::time::Instant::now());
            if self
                .library
                .mark_station_failure(&url, unix_now_string(), &error)
            {
                self.mark_library_dirty();
            }
        }
        self.playback.view.reset_transient_status();
        self.playback.diagnostics.buffer_percent = 0;
        self.playback.diagnostics.buffer_seconds = 0;
        self.playback.diagnostics.reconnect_attempts = 1;
        self.playback.diagnostics.last_recovery = Some("Queued automatic reconnect".to_string());
        self.playback.view.state = PlaybackState::Error(error);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn test_parts(library: Library) -> AppParts {
        AppParts {
            library,
            ui_state: super::super::ui_state::UiState::from_app_values(
                37,
                true,
                LayoutMode::RightOnly,
                VisualizerMode::SimOscilloscope,
                DisplayMode::Normal,
            ),
            ui_state_warning: None,
            history: crate::history::History::default(),
            history_warning: None,
            audio: AudioEngine::disconnected_for_test(),
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
    fn last_played_station_position_matches_normalized_urls() {
        let stations = vec![Station::basic("A", " HTTP://STREAM/ ", "Radio", "US", 128)];

        assert_eq!(
            last_played_station_position(&stations, "http://stream"),
            Some(0)
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
    fn last_played_station_position_allows_missing_library_match() {
        let stations = vec![Station::basic("A", "http://a", "Radio", "US", 128)];

        assert_eq!(
            last_played_station_position(&stations, "http://other"),
            None
        );
    }

    #[test]
    fn track_changed_matches_normalized_playing_url() {
        let mut app = App::new(Library::in_memory(vec![Station::basic(
            "A",
            "HTTP://STREAM/",
            "Radio",
            "US",
            128,
        )]));
        app.playback.view.playing_url = Some("http://stream".to_string());

        app.handle_track_changed(" HTTP://STREAM/ ".to_string(), "Artist - Title".to_string());

        assert_eq!(
            app.playback.view.current_track.as_deref(),
            Some("Artist - Title")
        );
    }

    #[test]
    fn test_single_title_fires_notification() {
        let station_url = "http://stream";
        let mut app = App::from_parts(test_parts(Library::in_memory(vec![Station::basic(
            "Test Station",
            station_url,
            "Radio",
            "US",
            128,
        )])));
        app.library.settings.notifications_enabled = true;
        app.playback.view.playing_url = Some(station_url.to_string());

        app.handle_track_changed(station_url.to_string(), "Artist - New Song".to_string());

        assert_eq!(app.notification_count, 1);
    }

    #[test]
    fn test_disabled_notifications_suppresses() {
        let station_url = "http://stream";
        let mut app = App::from_parts(test_parts(Library::in_memory(vec![Station::basic(
            "Test Station",
            station_url,
            "Radio",
            "US",
            128,
        )])));
        app.library.settings.notifications_enabled = false;
        app.playback.view.playing_url = Some(station_url.to_string());

        app.handle_track_changed(station_url.to_string(), "Artist - New Song".to_string());

        assert_eq!(app.notification_count, 0);
    }

    #[test]
    fn test_duplicate_title_no_notification() {
        let station_url = "http://stream";
        let mut app = App::from_parts(test_parts(Library::in_memory(vec![Station::basic(
            "Test Station",
            station_url,
            "Radio",
            "US",
            128,
        )])));
        app.library.settings.notifications_enabled = true;
        app.playback.view.playing_url = Some(station_url.to_string());
        app.playback.view.current_track = Some("Already Playing".to_string());

        app.handle_track_changed(station_url.to_string(), "Already Playing".to_string());

        assert_eq!(app.notification_count, 0);
    }

    #[test]
    fn test_empty_title_no_notification() {
        let station_url = "http://stream";
        let mut app = App::from_parts(test_parts(Library::in_memory(vec![Station::basic(
            "Test Station",
            station_url,
            "Radio",
            "US",
            128,
        )])));
        app.library.settings.notifications_enabled = true;
        app.playback.view.playing_url = Some(station_url.to_string());

        app.handle_track_changed(station_url.to_string(), String::new());

        assert_eq!(app.notification_count, 0);
    }

    #[test]
    fn test_current_track_always_updated() {
        let station_url = "http://stream";
        let mut app = App::from_parts(test_parts(Library::in_memory(vec![Station::basic(
            "Test Station",
            station_url,
            "Radio",
            "US",
            128,
        )])));
        app.library.settings.notifications_enabled = false;
        app.playback.view.playing_url = Some(station_url.to_string());

        app.handle_track_changed(station_url.to_string(), "Latest Title".to_string());

        assert_eq!(
            app.playback.view.current_track.as_deref(),
            Some("Latest Title"),
        );
    }

    #[test]
    fn test_song_history_records_new_title() {
        let station_url = "http://stream";
        let mut app = App::from_parts(test_parts(Library::in_memory(vec![Station::basic(
            "Test Station",
            station_url,
            "Radio",
            "US",
            128,
        )])));
        app.library.settings.notifications_enabled = true;
        app.playback.view.playing_url = Some(station_url.to_string());

        app.handle_track_changed(station_url.to_string(), "Song Alpha".to_string());
        app.handle_track_changed(station_url.to_string(), "Song Beta".to_string());

        assert!(app.song_history.contains(&"Song Alpha".to_string()));
        assert!(app.song_history.contains(&"Song Beta".to_string()));
    }

    #[test]
    fn test_burst_titles_produce_at_most_one_notification() {
        let station_url = "http://stream";
        let mut app = App::from_parts(test_parts(Library::in_memory(vec![Station::basic(
            "Test Station",
            station_url,
            "Radio",
            "US",
            128,
        )])));
        app.library.settings.notifications_enabled = true;
        app.playback.view.playing_url = Some(station_url.to_string());

        let burst_titles = [
            "Connecting...",
            "Ad Break",
            "Artist A - Song One",
            "Artist B - Song Two",
            "Artist C - Song Three",
        ];

        for title in &burst_titles {
            app.handle_track_changed(station_url.to_string(), title.to_string());
        }

        assert!(app.notification_count <= 1);
    }

    #[test]
    fn test_notification_cooldown_new_has_no_last_notified() {
        let cooldown = NotificationCooldown::new();
        let now = Instant::now();
        assert!(cooldown.may_notify(now));
    }

    #[test]
    fn test_may_notify_true_when_no_previous_notification() {
        let cooldown = NotificationCooldown::new();
        let now = Instant::now();
        assert!(cooldown.may_notify(now));
    }

    #[test]
    fn test_may_notify_true_when_elapsed_gte_cooldown() {
        let mut cooldown = NotificationCooldown::new();
        let t1 = Instant::now();
        cooldown.record_notification(t1);

        let t2 = t1 + Duration::from_secs(5);
        assert!(cooldown.may_notify(t2));
    }

    #[test]
    fn test_may_notify_false_when_elapsed_lt_cooldown() {
        let mut cooldown = NotificationCooldown::new();
        let t1 = Instant::now();
        cooldown.record_notification(t1);

        let t2 = t1 + Duration::from_millis(4999);
        assert!(!cooldown.may_notify(t2));
    }

    #[test]
    fn test_record_notification_updates_timestamp() {
        let mut cooldown = NotificationCooldown::new();
        let t1 = Instant::now();
        cooldown.record_notification(t1);

        let t2 = t1 + Duration::from_secs(2);
        assert!(!cooldown.may_notify(t2));

        let t3 = t1 + Duration::from_secs(6);
        assert!(cooldown.may_notify(t3));
    }

    #[test]
    fn test_may_notify_boundary_exactly_5000ms_returns_true() {
        let mut cooldown = NotificationCooldown::new();
        let t1 = Instant::now();
        cooldown.record_notification(t1);

        let t2 = t1 + Duration::from_millis(5000);
        assert!(cooldown.may_notify(t2));
    }

    #[test]
    fn test_may_notify_boundary_4999ms_returns_false() {
        let mut cooldown = NotificationCooldown::new();
        let t1 = Instant::now();
        cooldown.record_notification(t1);

        let t2 = t1 + Duration::from_millis(4999);
        assert!(!cooldown.may_notify(t2));
    }

    #[test]
    fn test_burst_5_titles_at_most_1_notification() {
        let station_url = "http://stream";
        let mut app = App::from_parts(test_parts(Library::in_memory(vec![Station::basic(
            "Test Station",
            station_url,
            "Radio",
            "US",
            128,
        )])));
        app.library.settings.notifications_enabled = true;
        app.playback.view.playing_url = Some(station_url.to_string());

        let titles = ["Title A", "Title B", "Title C", "Title D", "Title E"];
        for title in &titles {
            app.handle_track_changed(station_url.to_string(), title.to_string());
        }

        assert_eq!(app.notification_count, 1);
    }

    #[test]
    fn test_single_title_after_cooldown_fires_immediately() {
        let station_url = "http://stream";
        let mut app = App::from_parts(test_parts(Library::in_memory(vec![Station::basic(
            "Test Station",
            station_url,
            "Radio",
            "US",
            128,
        )])));
        app.library.settings.notifications_enabled = true;
        app.playback.view.playing_url = Some(station_url.to_string());

        let past = Instant::now() - Duration::from_secs(10);
        app.notification_cooldown.record_notification(past);

        app.handle_track_changed(station_url.to_string(), "Fresh Song".to_string());

        assert_eq!(app.notification_count, 1);
    }

    #[test]
    fn test_title_during_cooldown_suppresses_notification() {
        let station_url = "http://stream";
        let mut app = App::from_parts(test_parts(Library::in_memory(vec![Station::basic(
            "Test Station",
            station_url,
            "Radio",
            "US",
            128,
        )])));
        app.library.settings.notifications_enabled = true;
        app.playback.view.playing_url = Some(station_url.to_string());

        app.handle_track_changed(station_url.to_string(), "First Song".to_string());
        assert_eq!(app.notification_count, 1);

        app.handle_track_changed(station_url.to_string(), "Second Song".to_string());
        assert_eq!(app.notification_count, 1);
        assert_eq!(
            app.playback.view.current_track.as_deref(),
            Some("Second Song"),
        );
    }

    #[test]
    fn test_title_during_cooldown_song_history_still_updated() {
        let station_url = "http://stream";
        let mut app = App::from_parts(test_parts(Library::in_memory(vec![Station::basic(
            "Test Station",
            station_url,
            "Radio",
            "US",
            128,
        )])));
        app.library.settings.notifications_enabled = true;
        app.playback.view.playing_url = Some(station_url.to_string());

        app.handle_track_changed(station_url.to_string(), "Song Alpha".to_string());
        app.handle_track_changed(station_url.to_string(), "Song Beta".to_string());

        assert!(app.song_history.contains(&"Song Alpha".to_string()));
        assert!(app.song_history.contains(&"Song Beta".to_string()));
    }

    #[test]
    fn test_notification_cooldown_second_record_resets_window() {
        let mut cooldown = NotificationCooldown::new();
        let t0 = Instant::now();
        let t1 = t0 + Duration::from_secs(3);
        let t2 = t0 + Duration::from_secs(6);

        cooldown.record_notification(t0);
        assert!(!cooldown.may_notify(t1));

        cooldown.record_notification(t1);
        assert!(!cooldown.may_notify(t2));

        let t3 = t1 + Duration::from_secs(5);
        assert!(cooldown.may_notify(t3));
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
    fn from_parts_scrobble_config_enables_tracker() {
        let library = Library::in_memory(vec![]);
        let mut parts = test_parts(library);
        parts.config.scrobble.enabled = true;

        let app = App::from_parts(parts);

        assert!(app.scrobble_tracker.is_enabled());
    }

    #[test]
    fn from_parts_scrobble_config_disabled_by_default() {
        let library = Library::in_memory(vec![]);
        let parts = test_parts(library);

        let app = App::from_parts(parts);

        assert!(!app.scrobble_tracker.is_enabled());
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

        assert!(app.library.load_warnings.contains(&"bad config value".to_string()));
    }
}

#[cfg(test)]
mod cooldown_proptests {
    use super::*;
    use proptest::prelude::*;
    use std::time::{Duration, Instant};

    proptest! {
        #[test]
        fn may_notify_true_when_elapsed_gte_5s(elapsed_ms in 5000u64..=600_000u64) {
            let mut cooldown = NotificationCooldown::new();
            let t1 = Instant::now();
            cooldown.record_notification(t1);

            let t2 = t1 + Duration::from_millis(elapsed_ms);
            prop_assert!(cooldown.may_notify(t2),
                "Expected may_notify=true for elapsed={}ms", elapsed_ms);
        }
    }

    proptest! {
        #[test]
        fn may_notify_false_when_elapsed_lt_5s(elapsed_ms in 0u64..5000u64) {
            let mut cooldown = NotificationCooldown::new();
            let t1 = Instant::now();
            cooldown.record_notification(t1);

            let t2 = t1 + Duration::from_millis(elapsed_ms);
            prop_assert!(!cooldown.may_notify(t2),
                "Expected may_notify=false for elapsed={}ms", elapsed_ms);
        }
    }

    proptest! {
        #[test]
        fn may_notify_always_true_when_fresh(offset_ms in 0u64..=1_000_000u64) {
            let cooldown = NotificationCooldown::new();
            let now = Instant::now() + Duration::from_millis(offset_ms);
            prop_assert!(cooldown.may_notify(now),
                "Fresh cooldown should always allow notification");
        }
    }

    proptest! {
        #[test]
        fn record_then_immediate_check_returns_false(
            first_offset_ms in 0u64..=100_000u64,
            gap_ms in 0u64..5000u64,
        ) {
            let mut cooldown = NotificationCooldown::new();
            let t1 = Instant::now() + Duration::from_millis(first_offset_ms);
            cooldown.record_notification(t1);

            let t2 = t1 + Duration::from_millis(gap_ms);
            prop_assert!(!cooldown.may_notify(t2),
                "After record at t1, may_notify at t1+{}ms should be false", gap_ms);
        }
    }
}
