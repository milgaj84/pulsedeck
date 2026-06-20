use super::*;
use crate::audio::{AudioCommand, AudioEngine, AudioStatus};
use crate::radio::station_url_matches;

const NOTICE_INFO_TICKS: u16 = 90;
const NOTICE_ERROR_TICKS: u16 = 150;
const SONG_HISTORY_CAP: usize = 100;
const NOTIFY_IDLE_MS: u64 = 120_000;

fn last_played_station_position(stations: &[Station], last_played_url: &str) -> Option<usize> {
    stations
        .iter()
        .position(|station| station_url_matches(&station.url, last_played_url))
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
            metadata_refresh_pending: false,
            metadata_refresh_running: false,
            persist: persist::PersistFlags::default(),
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
                let station_name = self
                    .now_playing()
                    .map(|s| s.name.clone())
                    .unwrap_or_else(|| "Radio Stream".to_string());

                super::notifier::notify_now_playing(&title, &station_name);
            }
        }
    }

    fn handle_audio_stopped(&mut self) {
        let was_playing = self.playback.view.playing_url.is_some();
        if self.playback.view.intentional_stop || !was_playing {
            self.playback.view.intentional_stop = false;
            self.playback.view.playing_url = None;
            self.playback.view.current_track = None;
            self.playback.view.buffer_percent = 0;
            self.playback.view.buffer_seconds = 0;
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
            self.playback.reconnect.arm(url.clone(), std::time::Instant::now());
            if self
                .library
                .mark_station_failure(&url, unix_now_string(), &error)
            {
                self.mark_library_dirty();
            }
        }
        self.playback.view.current_track = None;
        self.playback.view.buffer_percent = 0;
        self.playback.view.buffer_seconds = 0;
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

    fn test_parts(library: Library) -> AppParts {
        AppParts {
            library,
            ui_state: super::super::ui_state::UiState::from_app_values(
                37,
                true,
                LayoutMode::RightOnly,
                2,
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
        assert_eq!(app.ui.visualizer_mode, 2);
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
        assert_eq!(app.playback.view.playing_url.as_deref(), Some("http://stream"));
        assert_eq!(
            app.playback.view.state,
            PlaybackState::Error("Audio engine stopped".to_string())
        );
    }

    #[test]
    fn last_played_station_position_matches_normalized_urls() {
        let stations = vec![Station::basic("A", " HTTP://STREAM/ ", "Radio", "US", 128)];

        assert_eq!(last_played_station_position(&stations, "http://stream"), Some(0));
    }

    #[test]
    fn last_played_station_position_allows_missing_library_match() {
        let stations = vec![Station::basic("A", "http://a", "Radio", "US", 128)];

        assert_eq!(last_played_station_position(&stations, "http://other"), None);
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

        assert_eq!(app.playback.view.current_track.as_deref(), Some("Artist - Title"));
    }
}
