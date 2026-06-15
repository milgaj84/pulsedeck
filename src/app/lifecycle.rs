use super::*;
use crate::audio::{AudioCommand, AudioEngine, AudioStatus};

impl App {
    pub fn new(library: Library) -> Self {
        let ui_state = super::ui_state::UiState::load();
        let sample_buffer = Arc::new(Mutex::new(VecDeque::with_capacity(4096)));
        let audio = AudioEngine::spawn(sample_buffer.clone());
        let history = crate::history::History::load();

        let mut app = Self {
            library,
            search_results: Vec::new(),
            selected: 0,
            normal_selected_snapshot: 0,
            search_selected_snapshot: 0,
            genre_selection_memory: HashMap::new(),
            playback: PlaybackState::Stopped,
            playing_url: None,
            volume: ui_state.volume(),
            muted: ui_state.muted(),
            should_quit: false,
            notice: None,
            notice_ticks_remaining: 0,
            input_mode: InputMode::Normal,
            search_query: String::new(),
            search_status: SearchStatus::WaitingForInput,
            pending_api_search: None,
            searching_api: false,
            last_api_query: String::new(),
            selected_genre_idx: 0,
            current_track: None,
            tick_count: 0,
            layout_mode: ui_state.layout_mode(),
            show_help: false,
            show_station_details: false,
            show_recent_tracks: false,
            song_history: VecDeque::new(),
            show_settings: false,
            selected_setting_idx: 0,
            show_sleep_timer: false,
            buffer_percent: 0,
            buffer_seconds: 0,
            undo_history: VecDeque::new(),
            reconnect: Reconnect::default(),
            sleep_timer: SleepTimer::default(),
            history,
            intentional_stop: false,
            audio,
            sample_buffer,
            visualizer_mode: ui_state.visualizer_mode(),
            visualizer_peaks: Vec::new(),
        };

        app.sync_output_device();
        app.sync_volume();

        if app.library.settings.autoplay_last {
            if let Some(ref url) = app.library.settings.last_played_url {
                if let Some(pos) = app.library.stations.iter().position(|s| s.url == *url) {
                    app.selected = pos;
                    app.playing_url = Some(url.clone());
                    app.playback = PlaybackState::Connecting;
                    app.audio.send(AudioCommand::Play(url.clone()));
                    app.sync_volume();
                }
            }
        }

        app
    }

    pub(super) fn set_info_notice(&mut self, message: impl Into<String>) {
        self.notice = Some(AppNotice::Info(message.into()));
        self.notice_ticks_remaining = 90;
    }

    pub(super) fn set_error_notice(&mut self, message: impl Into<String>) {
        self.notice = Some(AppNotice::Error(message.into()));
        self.notice_ticks_remaining = 150;
    }

    pub(super) fn tick_notice(&mut self) {
        if self.notice_ticks_remaining > 0 {
            self.notice_ticks_remaining -= 1;
        } else {
            self.notice = None;
        }
    }

    pub fn poll_audio_status(&mut self) {
        while let Ok(status) = self.audio.status_rx.try_recv() {
            match status {
                AudioStatus::TrackChanged { url, title } => {
                    if Some(&url) == self.playing_url.as_ref() {
                        let is_new =
                            !title.is_empty() && self.current_track.as_ref() != Some(&title);
                        self.current_track = Some(title.clone());

                        if !title.is_empty() && self.song_history.back() != Some(&title) {
                            self.song_history.push_back(title.clone());
                            while self.song_history.len() > 100 {
                                self.song_history.pop_front();
                            }
                            if self.library.settings.save_history {
                                let station_name = self
                                    .now_playing()
                                    .map(|s| s.name.clone())
                                    .unwrap_or_else(|| "Radio Stream".to_string());
                                self.history.record(title.clone(), station_name);
                                let _ = self.history.save();
                            }
                        }

                        if is_new && self.library.settings.notifications_enabled {
                            let mut should_notify = true;
                            if let Some(idle_ms) = super::idle::get_user_idle_ms() {
                                if idle_ms > 120_000 {
                                    should_notify = false;
                                }
                            }

                            if should_notify {
                                let station_name = self
                                    .now_playing()
                                    .map(|s| s.name.clone())
                                    .unwrap_or_else(|| "Radio Stream".to_string());

                                super::notifier::notify_now_playing(&title, &station_name);
                            }
                        }
                    }
                }
                AudioStatus::BufferLevel { percent, seconds } => {
                    self.buffer_percent = percent;
                    self.buffer_seconds = seconds;
                }
                other => match other {
                    AudioStatus::Playing => {
                        self.playback = PlaybackState::Playing;
                        self.reconnect.disarm();
                    }
                    AudioStatus::Paused => {
                        self.playback = PlaybackState::Paused;
                    }
                    AudioStatus::Stopped => {
                        let was_playing = self.playing_url.is_some();
                        if self.intentional_stop || !was_playing {
                            self.intentional_stop = false;
                            self.playing_url = None;
                            self.current_track = None;
                            self.buffer_percent = 0;
                            self.buffer_seconds = 0;
                            self.playback = PlaybackState::Stopped;
                            self.reconnect.disarm();
                        } else if let Some(url) = self.playing_url.clone() {
                            self.reconnect.arm(url, std::time::Instant::now());
                            self.playback = PlaybackState::Connecting;
                        }
                    }
                    AudioStatus::Error(e) => {
                        if let Some(url) = self.playing_url.clone() {
                            self.reconnect.arm(url, std::time::Instant::now());
                        }
                        self.current_track = None;
                        self.buffer_percent = 0;
                        self.buffer_seconds = 0;
                        self.playback = PlaybackState::Error(e);
                    }
                    AudioStatus::FadingOut { current_volume } => {
                        self.playback = PlaybackState::FadingOut {
                            current_volume: current_volume.clamp(0.0, 1.0),
                        };
                    }
                    AudioStatus::Connecting => {
                        self.current_track = None;
                        self.playback = PlaybackState::Connecting;
                    }
                    AudioStatus::TrackChanged { .. } | AudioStatus::BufferLevel { .. } => {}
                },
            }
        }
    }
}
