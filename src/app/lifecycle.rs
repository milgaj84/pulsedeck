use super::*;
use crate::audio::{AudioCommand, AudioEngine, AudioStatus};

const NOTICE_INFO_TICKS: u16 = 90;
const NOTICE_ERROR_TICKS: u16 = 150;
const SONG_HISTORY_CAP: usize = 100;
const NOTIFY_IDLE_MS: u64 = 120_000;

fn last_played_station_position(stations: &[Station], last_played_url: &str) -> Option<usize> {
    let needle = normalized_playback_url(last_played_url);
    stations
        .iter()
        .position(|station| normalized_playback_url(&station.url) == needle)
}

fn normalized_playback_url(url: &str) -> String {
    url.trim().trim_end_matches('/').to_ascii_lowercase()
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

impl App {
    pub fn new(library: Library) -> Self {
        let (ui_state, ui_state_warning) = super::ui_state::UiState::load_with_warning();
        let sample_buffer = Arc::new(Mutex::new(VecDeque::with_capacity(4096)));
        let audio = AudioEngine::spawn(sample_buffer.clone());
        let (history, history_warning) = crate::history::History::load_with_warning();
        let diagnostics_output_device = crate::audio::output_device_display_name(
            library.settings.output_device_name.as_deref(),
        );
        let diagnostics_metadata_enabled = library.settings.stream_metadata_enabled;

        let mut app = Self {
            library,
            nav: Navigation::default(),
            search: SearchState::default(),
            command_palette: CommandPaletteState::default(),
            player: PlaybackView::default(),
            volume: ui_state.volume(),
            muted: ui_state.muted(),
            should_quit: false,
            notice: NoticeState::default(),
            input_mode: InputMode::Normal,
            tick_count: 0,
            layout_mode: ui_state.layout_mode(),
            overlays: Overlays::default(),
            song_history: VecDeque::new(),
            undo_history: VecDeque::new(),
            reconnect: Reconnect::default(),
            diagnostics: PlaybackDiagnostics {
                output_device: diagnostics_output_device,
                metadata_enabled: diagnostics_metadata_enabled,
                reconnect_limit: 3,
                ..PlaybackDiagnostics::default()
            },
            sleep_timer: SleepTimer::default(),
            history,
            metadata_refresh_pending: false,
            metadata_refresh_running: false,
            persist: persist::PersistFlags::default(),
            audio,
            sample_buffer,
            visualizer_mode: ui_state.visualizer_mode(),
            visualizer_peaks: Vec::new(),
        };

        app.sync_output_device();
        app.sync_stream_metadata();
        app.sync_volume();

        let mut startup_warnings = app.library.load_warnings.clone();
        if let Some(warning) = ui_state_warning {
            startup_warnings.push(warning);
        }
        if let Some(warning) = history_warning {
            startup_warnings.push(warning);
        }

        match startup_warnings.len() {
            0 => {}
            1 => app.set_error_notice(startup_warnings.remove(0)),
            count => app.set_error_notice(format!(
                "{count} config files had load warnings; using safe defaults where needed"
            )),
        }

        if app.library.settings.autoplay_last {
            if let Some(url) = app.library.settings.last_played_url.clone() {
                if let Some(pos) = last_played_station_position(&app.library.stations, &url) {
                    app.nav.selected = pos;
                }
                app.player.playing_url = Some(url.clone());
                app.player.state = PlaybackState::Connecting;
                app.audio.send(AudioCommand::Play(url));
                app.sync_volume();
            }
        }

        app
    }

    pub(super) fn set_info_notice(&mut self, message: impl Into<String>) {
        self.notice.current = Some(AppNotice::Info(message.into()));
        self.notice.ticks_remaining = NOTICE_INFO_TICKS;
    }

    pub(super) fn set_error_notice(&mut self, message: impl Into<String>) {
        self.notice.current = Some(AppNotice::Error(message.into()));
        self.notice.ticks_remaining = NOTICE_ERROR_TICKS;
    }

    pub(super) fn tick_notice(&mut self) {
        if self.notice.ticks_remaining > 0 {
            self.notice.ticks_remaining -= 1;
        } else {
            self.notice.current = None;
        }
    }

    pub fn poll_audio_status(&mut self) {
        while let Ok(status) = self.audio.status_rx.try_recv() {
            match status {
                AudioStatus::TrackChanged { url, title } => {
                    self.handle_track_changed(url, title);
                }
                AudioStatus::Playing => {
                    self.player.state = PlaybackState::Playing;
                    self.reconnect.disarm();
                    if let Some(url) = self.player.playing_url.clone() {
                        if self.library.mark_station_success(&url, unix_now_string()) {
                            self.mark_library_dirty();
                        }
                    }
                    self.diagnostics.decoder_state = DecoderState::Playing;
                    self.diagnostics.last_event = Some("Playback started".to_string());
                    self.diagnostics.last_error = None;
                }
                AudioStatus::Paused => {
                    self.player.state = PlaybackState::Paused;
                    self.diagnostics.last_event = Some("Playback paused".to_string());
                }
                AudioStatus::Stopped => {
                    self.handle_audio_stopped();
                }
                AudioStatus::Error(error) => {
                    self.diagnostics.decoder_state = DecoderState::Failed;
                    self.diagnostics.last_error = Some(error.clone());
                    self.handle_audio_error(error);
                }
                AudioStatus::FadingOut { current_volume } => {
                    self.player.state = PlaybackState::FadingOut {
                        current_volume: current_volume.clamp(0.0, 1.0),
                    };
                    self.diagnostics.last_event = Some("Fading out".to_string());
                }
                AudioStatus::Connecting => {
                    self.player.current_track = None;
                    self.player.state = PlaybackState::Connecting;
                    self.diagnostics.decoder_state = DecoderState::Connecting;
                    self.diagnostics.last_event = Some("Connecting to stream".to_string());
                }
            }
        }
    }

    fn handle_track_changed(&mut self, url: String, title: String) {
        if Some(&url) != self.player.playing_url.as_ref() {
            return;
        }

        let is_new = !title.is_empty() && self.player.current_track.as_ref() != Some(&title);
        self.player.current_track = Some(title.clone());

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
        let was_playing = self.player.playing_url.is_some();
        if self.player.intentional_stop || !was_playing {
            self.player.intentional_stop = false;
            self.player.playing_url = None;
            self.player.current_track = None;
            self.player.buffer_percent = 0;
            self.player.buffer_seconds = 0;
            self.player.state = PlaybackState::Stopped;
            self.diagnostics.decoder_state = DecoderState::Idle;
            self.diagnostics.buffer_percent = 0;
            self.diagnostics.buffer_seconds = 0;
            self.diagnostics.last_event = Some("Playback stopped".to_string());
            self.reconnect.disarm();
        } else if let Some(url) = self.player.playing_url.clone() {
            self.reconnect.arm(url, std::time::Instant::now());
            self.player.state = PlaybackState::Connecting;
        }
    }

    fn handle_audio_error(&mut self, error: String) {
        if let Some(url) = self.player.playing_url.clone() {
            self.reconnect.arm(url.clone(), std::time::Instant::now());
            if self
                .library
                .mark_station_failure(&url, unix_now_string(), &error)
            {
                self.mark_library_dirty();
            }
        }
        self.player.current_track = None;
        self.player.buffer_percent = 0;
        self.player.buffer_seconds = 0;
        self.diagnostics.buffer_percent = 0;
        self.diagnostics.buffer_seconds = 0;
        self.diagnostics.reconnect_attempts = 1;
        self.diagnostics.last_recovery = Some("Queued automatic reconnect".to_string());
        self.player.state = PlaybackState::Error(error);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
