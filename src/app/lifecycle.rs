use super::*;
use crate::audio::{AudioCommand, AudioEngine, AudioStatus};
use crate::radio::{find_station_by_url, find_station_index_by_url, station_url_matches};
use std::time::{Duration, Instant};

const NOTICE_INFO_TICKS: u16 = 90;
const NOTICE_ERROR_TICKS: u16 = 150;
const SONG_HISTORY_CAP: usize = 100;
const NOTIFY_IDLE_MS: u64 = 120_000;

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
}

impl AppParts {
    pub(super) fn load(library: Library) -> Self {
        let (ui_state, ui_state_warning) = super::ui_state::UiState::load_with_warning();
        let sample_buffer = Arc::new(Mutex::new(VecDeque::with_capacity(4096)));
        let audio = AudioEngine::spawn(sample_buffer.clone());
        let (history, history_warning) = crate::history::History::load_with_warning();

        Self {
            library,
            ui_state,
            ui_state_warning,
            history,
            history_warning,
            audio,
            sample_buffer,
        }
    }
}

impl App {
    pub fn new(library: Library) -> Self {
        Self::from_parts(AppParts::load(library))
    }

    pub(super) fn from_parts(parts: AppParts) -> Self {
        let diagnostics_output_device = crate::audio::output_device_display_name(
            parts.library.settings.output_device_name.as_deref(),
        );
        let diagnostics_metadata_enabled = parts.library.settings.stream_metadata_enabled;
        let ui = UiRuntimeState::from_ui_state(&parts.ui_state);
        let playback = PlaybackRuntime::new(
            &parts.ui_state,
            diagnostics_output_device,
            diagnostics_metadata_enabled,
            parts.audio,
            parts.sample_buffer,
        );

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
            notification_cooldown: NotificationCooldown::new(),
            #[cfg(test)]
            notification_count: 0,
        };

        app.sync_startup_audio_settings();
        app.apply_startup_warnings(parts.ui_state_warning, parts.history_warning);
        app.apply_startup_autoplay();
        app
    }

    pub fn should_quit(&self) -> bool {
        self.ui.should_quit
    }

    pub fn input_mode(&self) -> &InputMode {
        &self.ui.input_mode
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
        if self.send_audio_command(AudioCommand::Play(url)) {
            self.sync_volume();
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

    fn handle_track_changed(&mut self, url: String, title: String) {
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
            ),
            ui_state_warning: None,
            history: crate::history::History::default(),
            history_warning: None,
            audio: AudioEngine::disconnected_for_test(),
            sample_buffer: Arc::new(Mutex::new(VecDeque::with_capacity(4096))),
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
        // AAC is now supported via Symphonia; autoplay should proceed (not be blocked).
        let mut station = Station::basic("AAC Radio", "http://aac", "Pop", "US", 128);
        station.codec = "AAC".to_string();

        let mut library = Library::in_memory(vec![station]);
        library.settings.autoplay_last = true;
        library.settings.last_played_url = Some("http://aac".to_string());

        let app = App::from_parts(test_parts(library));

        // Audio engine is disconnected in test_parts so we get an Error,
        // but playing_url was set — the codec gate did NOT block it.
        assert_eq!(app.playback.view.playing_url.as_deref(), Some("http://aac"));
    }

    #[test]
    fn startup_autoplay_blocks_hls_codec() {
        // HLS remains unsupported; autoplay should be blocked.
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

        // Audio engine is disconnected in test_parts, so it goes to Error,
        // but the important thing is playing_url was set (codec was not blocked).
        assert_eq!(
            app.playback.view.playing_url.as_deref(),
            Some("http://mystery")
        );
    }

    #[test]
    fn startup_autoplay_allows_url_not_in_library() {
        // URL not in the library: capability gate should not block it.
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

    // =========================================================================
    // Preservation tests — verify existing correct behaviors that must not
    // regress after the notification cooldown fix is applied.
    // =========================================================================

    /// A single new distinct title (with notifications enabled, user active,
    /// matching URL) fires exactly one notification.
    ///
    /// **Validates: Requirement 3.1**
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

        assert_eq!(
            app.notification_count, 1,
            "A single new title should fire exactly 1 notification"
        );
    }

    /// With `notifications_enabled = false`, no notification fires regardless
    /// of the title content.
    ///
    /// **Validates: Requirement 3.2**
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

        assert_eq!(
            app.notification_count, 0,
            "No notification should fire when notifications are disabled"
        );
    }

    /// If the title is the same as current_track, no notification fires.
    ///
    /// **Validates: Requirement 3.4**
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
        // Pre-set current_track to simulate an already-known title
        app.playback.view.current_track = Some("Already Playing".to_string());

        app.handle_track_changed(station_url.to_string(), "Already Playing".to_string());

        assert_eq!(
            app.notification_count, 0,
            "Duplicate title should not fire a notification"
        );
    }

    /// If the title is empty, no notification fires.
    ///
    /// **Validates: Requirement 3.5**
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

        assert_eq!(
            app.notification_count, 0,
            "Empty title should not fire a notification"
        );
    }

    /// `current_track` always reflects the latest title after handle_track_changed,
    /// regardless of whether a notification fired.
    ///
    /// **Validates: Requirement 3.6**
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
        app.library.settings.notifications_enabled = false; // disabled — but state should still update
        app.playback.view.playing_url = Some(station_url.to_string());

        app.handle_track_changed(station_url.to_string(), "Latest Title".to_string());

        assert_eq!(
            app.playback.view.current_track.as_deref(),
            Some("Latest Title"),
            "current_track must reflect the latest title regardless of notification state"
        );
    }

    /// song_history records new distinct titles.
    ///
    /// **Validates: Requirement 3.6**
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

        assert!(
            app.song_history.contains(&"Song Alpha".to_string()),
            "song_history should contain 'Song Alpha'"
        );
        assert!(
            app.song_history.contains(&"Song Beta".to_string()),
            "song_history should contain 'Song Beta'"
        );
    }

    // =========================================================================
    // Bug condition exploration test
    // =========================================================================

    /// Bug condition exploration test: demonstrates the notification swarm bug.
    ///
    /// This test asserts the EXPECTED (fixed) behavior: at most 1 notification
    /// fires when multiple distinct titles arrive in immediate succession.
    /// On UNFIXED code, this FAILS because each distinct title fires its own
    /// notification — confirming the bug exists.
    ///
    /// **Validates: Requirements 1.1, 1.2, 1.3**
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

        // Simulate a burst of 5 distinct TrackChanged events in immediate succession
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

        // EXPECTED (fixed) behavior: at most 1 notification per burst window.
        // UNFIXED behavior: 5 notifications fire (one per distinct title).
        assert!(
            app.notification_count <= 1,
            "Expected at most 1 notification for a burst of {} titles, but got {}",
            burst_titles.len(),
            app.notification_count,
        );
    }

    // =========================================================================
    // Task 3.1: Unit tests for NotificationCooldown
    // =========================================================================

    /// **Validates: Requirements 2.1**
    #[test]
    fn test_notification_cooldown_new_has_no_last_notified() {
        let cooldown = NotificationCooldown::new();
        let now = Instant::now();
        assert!(
            cooldown.may_notify(now),
            "A fresh cooldown should allow notification (last_notified is None)"
        );
    }

    /// **Validates: Requirements 2.1**
    #[test]
    fn test_may_notify_true_when_no_previous_notification() {
        let cooldown = NotificationCooldown::new();
        let now = Instant::now();
        assert!(cooldown.may_notify(now));
    }

    /// **Validates: Requirements 2.1**
    #[test]
    fn test_may_notify_true_when_elapsed_gte_cooldown() {
        let mut cooldown = NotificationCooldown::new();
        let t1 = Instant::now();
        cooldown.record_notification(t1);

        let t2 = t1 + Duration::from_secs(5);
        assert!(
            cooldown.may_notify(t2),
            "may_notify should return true when elapsed >= 5s"
        );
    }

    /// **Validates: Requirements 2.1**
    #[test]
    fn test_may_notify_false_when_elapsed_lt_cooldown() {
        let mut cooldown = NotificationCooldown::new();
        let t1 = Instant::now();
        cooldown.record_notification(t1);

        let t2 = t1 + Duration::from_millis(4999);
        assert!(
            !cooldown.may_notify(t2),
            "may_notify should return false when elapsed < 5s"
        );
    }

    /// **Validates: Requirements 2.1**
    #[test]
    fn test_record_notification_updates_timestamp() {
        let mut cooldown = NotificationCooldown::new();
        let t1 = Instant::now();
        cooldown.record_notification(t1);

        // Within cooldown of t1 → should be false
        let t2 = t1 + Duration::from_secs(2);
        assert!(!cooldown.may_notify(t2));

        // After cooldown of t1 → should be true
        let t3 = t1 + Duration::from_secs(6);
        assert!(cooldown.may_notify(t3));
    }

    /// Boundary: exactly 5000ms elapsed → returns true.
    ///
    /// **Validates: Requirements 2.1**
    #[test]
    fn test_may_notify_boundary_exactly_5000ms_returns_true() {
        let mut cooldown = NotificationCooldown::new();
        let t1 = Instant::now();
        cooldown.record_notification(t1);

        let t2 = t1 + Duration::from_millis(5000);
        assert!(
            cooldown.may_notify(t2),
            "Exactly 5000ms elapsed should return true (>= boundary)"
        );
    }

    /// Boundary: 4999ms elapsed → returns false.
    ///
    /// **Validates: Requirements 2.1**
    #[test]
    fn test_may_notify_boundary_4999ms_returns_false() {
        let mut cooldown = NotificationCooldown::new();
        let t1 = Instant::now();
        cooldown.record_notification(t1);

        let t2 = t1 + Duration::from_millis(4999);
        assert!(
            !cooldown.may_notify(t2),
            "4999ms elapsed should return false (< boundary)"
        );
    }

    // =========================================================================
    // Task 4.1: Integration tests for handle_track_changed with cooldown
    // =========================================================================

    /// Burst of 5 distinct titles in immediate succession → exactly 1
    /// notification fires (the first one; subsequent ones suppressed by cooldown).
    ///
    /// **Validates: Requirements 1.1, 1.2, 2.1**
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

        assert_eq!(
            app.notification_count, 1,
            "Burst of 5 distinct titles should fire exactly 1 notification"
        );
    }

    /// Single title after ≥5s since last notification → fires immediately.
    ///
    /// **Validates: Requirements 3.1**
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

        // Simulate a past notification well beyond cooldown
        let past = Instant::now() - Duration::from_secs(10);
        app.notification_cooldown.record_notification(past);

        app.handle_track_changed(station_url.to_string(), "Fresh Song".to_string());

        assert_eq!(
            app.notification_count, 1,
            "Title after cooldown elapsed should fire immediately"
        );
    }

    /// Title during cooldown window → notification suppressed but current_track
    /// updated.
    ///
    /// **Validates: Requirements 2.1, 3.6**
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

        // First title fires (records cooldown)
        app.handle_track_changed(station_url.to_string(), "First Song".to_string());
        assert_eq!(app.notification_count, 1);

        // Second title during cooldown → suppressed
        app.handle_track_changed(station_url.to_string(), "Second Song".to_string());
        assert_eq!(
            app.notification_count, 1,
            "Second title within cooldown should NOT fire notification"
        );
        assert_eq!(
            app.playback.view.current_track.as_deref(),
            Some("Second Song"),
            "current_track must still update even when notification suppressed"
        );
    }

    /// Title during cooldown window → song_history still updated.
    ///
    /// **Validates: Requirements 2.1, 3.6**
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

        // First title fires (records cooldown)
        app.handle_track_changed(station_url.to_string(), "Song Alpha".to_string());
        // Second title during cooldown → suppressed, but history updated
        app.handle_track_changed(station_url.to_string(), "Song Beta".to_string());

        assert!(
            app.song_history.contains(&"Song Alpha".to_string()),
            "song_history should contain first title"
        );
        assert!(
            app.song_history.contains(&"Song Beta".to_string()),
            "song_history should contain second title even when notification suppressed"
        );
    }

    #[test]
    fn test_notification_cooldown_second_record_resets_window() {
        let mut cooldown = NotificationCooldown::new();
        let t0 = Instant::now();
        let t1 = t0 + Duration::from_secs(3);
        let t2 = t0 + Duration::from_secs(6); // 6s after t0, but only 3s after t1

        cooldown.record_notification(t0);
        assert!(!cooldown.may_notify(t1)); // 3s < 5s

        // Record again at t1 — this should reset the window
        cooldown.record_notification(t1);
        assert!(!cooldown.may_notify(t2)); // only 3s since t1

        let t3 = t1 + Duration::from_secs(5); // 5s after t1
        assert!(cooldown.may_notify(t3)); // now allowed
    }
}

// =============================================================================
// Task 3.2: Property-based tests for NotificationCooldown
// =============================================================================
#[cfg(test)]
mod cooldown_proptests {
    use super::*;
    use proptest::prelude::*;
    use std::time::{Duration, Instant};

    /// **Validates: Requirements 2.1**
    ///
    /// Property: for any elapsed duration >= 5s, may_notify returns true
    /// after a prior record_notification.
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

    /// **Validates: Requirements 2.1**
    ///
    /// Property: for any elapsed duration < 5s, may_notify returns false
    /// after a prior record_notification.
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

    /// **Validates: Requirements 2.1**
    ///
    /// Property: a fresh NotificationCooldown always allows notification
    /// regardless of what Instant is provided.
    proptest! {
        #[test]
        fn may_notify_always_true_when_fresh(offset_ms in 0u64..=1_000_000u64) {
            let cooldown = NotificationCooldown::new();
            let now = Instant::now() + Duration::from_millis(offset_ms);
            prop_assert!(cooldown.may_notify(now),
                "Fresh cooldown should always allow notification");
        }
    }

    /// **Validates: Requirements 2.1**
    ///
    /// Property: record_notification always updates so that subsequent
    /// may_notify within cooldown returns false.
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
