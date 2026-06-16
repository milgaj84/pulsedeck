use super::*;
use crate::audio::{AudioCommand, AudioEngine, AudioStatus};

const NOTICE_INFO_TICKS: u16 = 90;
const NOTICE_ERROR_TICKS: u16 = 150;
const SONG_HISTORY_CAP: usize = 100;
const NOTIFY_IDLE_MS: u64 = 120_000;

#[derive(Default)]
pub struct NoticeState {
    pub current: Option<AppNotice>,
    ticks_remaining: u16,
}

impl App {
    pub fn new(library: Library) -> Self {
        let ui_state = super::ui_state::UiState::load();
        let sample_buffer = Arc::new(Mutex::new(VecDeque::with_capacity(4096)));
        let audio = AudioEngine::spawn(sample_buffer.clone());
        let history = crate::history::History::load();

        let mut app = Self {
            library,
            nav: Navigation::default(),
            search: SearchState::default(),
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
            sleep_timer: SleepTimer::default(),
            history,
            persist: persist::PersistFlags::default(),
            audio,
            sample_buffer,
            visualizer_mode: ui_state.visualizer_mode(),
            visualizer_peaks: Vec::new(),
        };

        app.sync_output_device();
        app.sync_volume();

        if let Some(warning) = app.library.load_warnings.first().cloned() {
            app.set_error_notice(warning);
        }

        if app.library.settings.autoplay_last {
            if let Some(ref url) = app.library.settings.last_played_url {
                if let Some(pos) = app.library.stations.iter().position(|s| s.url == *url) {
                    app.nav.selected = pos;
                    app.player.playing_url = Some(url.clone());
                    app.player.state = PlaybackState::Connecting;
                    app.audio.send(AudioCommand::Play(url.clone()));
                    app.sync_volume();
                }
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
                AudioStatus::BufferLevel { percent, seconds } => {
                    self.player.buffer_percent = percent;
                    self.player.buffer_seconds = seconds;
                }
                AudioStatus::Playing => {
                    self.player.state = PlaybackState::Playing;
                    self.reconnect.disarm();
                }
                AudioStatus::Paused => {
                    self.player.state = PlaybackState::Paused;
                }
                AudioStatus::Stopped => {
                    self.handle_audio_stopped();
                }
                AudioStatus::Error(error) => {
                    self.handle_audio_error(error);
                }
                AudioStatus::FadingOut { current_volume } => {
                    self.player.state = PlaybackState::FadingOut {
                        current_volume: current_volume.clamp(0.0, 1.0),
                    };
                }
                AudioStatus::Connecting => {
                    self.player.current_track = None;
                    self.player.state = PlaybackState::Connecting;
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
            self.reconnect.disarm();
        } else if let Some(url) = self.player.playing_url.clone() {
            self.reconnect.arm(url, std::time::Instant::now());
            self.player.state = PlaybackState::Connecting;
        }
    }

    fn handle_audio_error(&mut self, error: String) {
        if let Some(url) = self.player.playing_url.clone() {
            self.reconnect.arm(url, std::time::Instant::now());
        }
        self.player.current_track = None;
        self.player.buffer_percent = 0;
        self.player.buffer_seconds = 0;
        self.player.state = PlaybackState::Error(error);
    }
}
