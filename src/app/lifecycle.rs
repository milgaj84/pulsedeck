use super::*;
use crate::audio::{AudioCommand, AudioEngine, AudioStatus};

impl App {
    pub fn new(library: Library) -> Self {
        let ui_state = super::ui_state::UiState::load();
        let recording_dir = library.settings.recording_dir.clone();
        let recording_recovery_notice =
            crate::recording_journal::detect_recovery_journal(&recording_dir)
                .ok()
                .flatten()
                .map(|recovery| recovery.summary());
        let sample_buffer = Arc::new(Mutex::new(VecDeque::with_capacity(4096)));
        let audio = AudioEngine::spawn(sample_buffer.clone());

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
            active_deck_page: 0,
            song_history: VecDeque::new(),
            tape_archive: TapeArchive::new(recording_dir),
            tape_archive_scan_requested: false,
            tape_archive_scan_inflight: false,
            local_playback_path: None,
            pending_tape_delete: None,
            show_settings: false,
            selected_setting_idx: 0,
            recording_state: RecordingState::Off,
            active_record_filepath: None,
            recording_started_at: None,
            recording_station_name: None,
            recording_station_url: None,
            recording_category: None,
            recording_recovery_notice,
            buffer_percent: 0,
            buffer_seconds: 0,
            undo_history: VecDeque::new(),
            audio,
            sample_buffer,
            visualizer_mode: ui_state.visualizer_mode(),
            visualizer_peaks: Vec::new(),
        };

        app.sync_output_device();
        app.sync_volume();

        // Autoplay last played station on boot if enabled.
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

    /// Poll for audio status updates (non-blocking).
    pub fn poll_audio_status(&mut self) {
        while let Ok(status) = self.audio.status_rx.try_recv() {
            match status {
                AudioStatus::LocalFileFinished { path } => {
                    if let Some(next) = self.tape_archive.next_track_after(&path) {
                        let title = next.title.clone();
                        self.play_tape_track(next);
                        self.set_info_notice(format!("Playing next tape: {title}"));
                    } else {
                        self.local_playback_path = None;
                        self.current_track = None;
                        self.playback = PlaybackState::Stopped;
                        self.set_info_notice("End of tape folder");
                    }
                }
                AudioStatus::LocalFilePlaying { path, title } => {
                    self.playing_url = None;
                    self.local_playback_path = Some(path);
                    self.current_track = Some(title);
                    self.recording_state = RecordingState::Off;
                    self.active_record_filepath = None;
                    self.buffer_percent = 0;
                    self.buffer_seconds = 0;
                    self.playback = PlaybackState::Playing;
                }
                AudioStatus::TrackChanged { url, title } => {
                    // Safety check: discard track updates that do not match the current playing URL.
                    if Some(&url) == self.playing_url.as_ref() {
                        let is_new =
                            !title.is_empty() && self.current_track.as_ref() != Some(&title);
                        self.current_track = Some(title.clone());

                        if !title.is_empty() && self.song_history.back() != Some(&title) {
                            self.song_history.push_back(title.clone());
                            while self.song_history.len() > 100 {
                                self.song_history.pop_front();
                            }
                        }

                        // Fire native OS system notifications if enabled and this is a new title.
                        if is_new && self.library.settings.notifications_enabled {
                            let mut should_notify = true;
                            if let Some(idle_ms) = super::idle::get_user_idle_ms() {
                                if idle_ms > 120_000 {
                                    // 2 minutes of system idle suppresses toast popups.
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
                AudioStatus::RecordingStateChanged { state, filepath } => {
                    self.sync_recording_status(state, filepath);
                }
                AudioStatus::BufferLevel { percent, seconds } => {
                    self.buffer_percent = percent;
                    self.buffer_seconds = seconds;
                }
                other => {
                    self.playback = match other {
                        AudioStatus::Playing => PlaybackState::Playing,
                        AudioStatus::Paused => PlaybackState::Paused,
                        AudioStatus::Stopped => {
                            self.playing_url = None;
                            self.local_playback_path = None;
                            self.current_track = None;
                            self.recording_state = RecordingState::Off;
                            self.active_record_filepath = None;
                            self.buffer_percent = 0;
                            self.buffer_seconds = 0;
                            PlaybackState::Stopped
                        }
                        AudioStatus::Error(e) => {
                            self.local_playback_path = None;
                            self.current_track = None;
                            self.recording_state = RecordingState::Off;
                            self.active_record_filepath = None;
                            self.buffer_percent = 0;
                            self.buffer_seconds = 0;
                            PlaybackState::Error(e)
                        }
                        AudioStatus::FadingOut { current_volume } => PlaybackState::FadingOut {
                            current_volume: current_volume.clamp(0.0, 1.0),
                        },
                        AudioStatus::Connecting => {
                            self.current_track = None;
                            PlaybackState::Connecting
                        }
                        _ => self.playback.clone(),
                    };
                }
            }
        }
    }
}
