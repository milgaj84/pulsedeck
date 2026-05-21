use crate::action::Action;
use crate::audio::{AudioCommand, AudioEngine, AudioStatus};
use crate::favorites::Library;
use crate::radio::Station;

/// Input mode determines how keyboard events are routed.
#[derive(Debug, Clone, PartialEq)]
pub enum InputMode {
    Normal,
    Search,
}

/// Playback state visible to the UI.
#[derive(Debug, Clone, PartialEq)]
pub enum PlaybackState {
    Stopped,
    Connecting,
    Playing,
    Paused,
    Error(String),
}

/// Tape recorder capturing states
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RecordingState {
    Off,
    Pending,
    Active,
}

/// Core application state.
///
/// Two completely separate data sources:
/// - `library` = your saved stations (shown in Normal mode)
/// - `search_results` = temporary API results (shown in Search mode)
///
/// They NEVER mix.
pub struct App {
    // Your station library (persisted to disk)
    pub library: Library,

    // Search results (temporary, separate from library)
    pub search_results: Vec<Station>,

    pub selected: usize,
    pub playback: PlaybackState,
    pub playing_url: Option<String>,
    pub volume: u8,         // 0–100
    pub muted: bool,
    pub should_quit: bool,

    // Input mode
    pub input_mode: InputMode,

    // Search state
    pub search_query: String,

    // API search state — main.rs checks these to spawn async fetches
    pub pending_api_search: Option<String>,
    pub searching_api: bool,
    last_api_query: String,

    pub selected_genre_idx: usize,
    pub current_track: Option<String>,
    pub tick_count: u64,

    pub show_right_panel: bool,
    pub show_help: bool,
    pub active_deck_page: usize,
    pub song_history: Vec<String>,

    pub show_settings: bool,
    pub selected_setting_idx: usize,

    pub recording_state: RecordingState,
    pub active_record_filepath: Option<String>,
    pub buffer_percent: u8,
    pub buffer_seconds: u32,

    audio: AudioEngine,
}

impl App {
    pub fn new(library: Library) -> Self {
        let audio = AudioEngine::spawn();

        let mut app = Self {
            library,
            search_results: Vec::new(),
            selected: 0,
            playback: PlaybackState::Stopped,
            playing_url: None,
            volume: 80,
            muted: false,
            should_quit: false,
            input_mode: InputMode::Normal,
            search_query: String::new(),
            pending_api_search: None,
            searching_api: false,
            last_api_query: String::new(),
            selected_genre_idx: 0,
            current_track: None,
            tick_count: 0,
            show_right_panel: true,
            show_help: false,
            active_deck_page: 0,
            song_history: Vec::new(),
            show_settings: false,
            selected_setting_idx: 0,
            recording_state: RecordingState::Off,
            active_record_filepath: None,
            buffer_percent: 0,
            buffer_seconds: 0,
            audio,
        };

        // Autoplay last played station on boot if enabled
        if app.library.settings.autoplay_last {
            if let Some(ref url) = app.library.settings.last_played_url {
                if let Some(pos) = app.library.stations.iter().position(|s| s.url == *url) {
                    app.selected = pos;
                    app.playing_url = Some(url.clone());
                    app.audio.send(AudioCommand::Play(url.clone()));
                    app.sync_volume();
                }
            }
        }

        app
    }

    /// Poll for audio status updates (non-blocking).
    pub fn poll_audio_status(&mut self) {
        while let Ok(status) = self.audio.status_rx.try_recv() {
            match status {
                AudioStatus::TrackChanged { url, title } => {
                    // Safety check: discard track updates that do not match the current playing URL!
                    if Some(&url) == self.playing_url.as_ref() {
                        let is_new = !title.is_empty() && self.current_track.as_ref() != Some(&title);
                        self.current_track = Some(title.clone());
                        
                        if !title.is_empty() && self.song_history.last() != Some(&title) {
                            self.song_history.push(title.clone());
                            if self.song_history.len() > 100 {
                                self.song_history.remove(0);
                            }
                        }

                        // Fire native OS system notifications if enabled and it's a new track title
                        if is_new && self.library.settings.notifications_enabled {
                            let station_name = self.now_playing()
                                .map(|s| s.name.clone())
                                .unwrap_or_else(|| "Radio Stream".to_string());
                            
                            let _ = notify_rust::Notification::new()
                                .summary("DriftFM ✦ Now Playing")
                                .body(&format!("♫ {}\nStation: {}", title, station_name))
                                .icon("audio-card")
                                .timeout(4000)
                                .show();
                        }
                    }
                }
                AudioStatus::RecordingStateChanged { state, filepath } => {
                    self.recording_state = match state {
                        1 => RecordingState::Pending,
                        2 => RecordingState::Active,
                        _ => RecordingState::Off,
                    };
                    self.active_record_filepath = filepath;
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
                            self.current_track = None;
                            self.recording_state = RecordingState::Off;
                            self.active_record_filepath = None;
                            self.buffer_percent = 0;
                            self.buffer_seconds = 0;
                            PlaybackState::Stopped
                        }
                        AudioStatus::Error(e) => {
                            self.current_track = None;
                            self.recording_state = RecordingState::Off;
                            self.active_record_filepath = None;
                            self.buffer_percent = 0;
                            self.buffer_seconds = 0;
                            PlaybackState::Error(e)
                        }
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

    /// The currently visible list. In Normal mode: library. In Search mode: search results.
    pub fn visible_stations(&self) -> Vec<&Station> {
        match self.input_mode {
            InputMode::Normal => {
                if let Some(genre) = self.library.available_genres.get(self.selected_genre_idx) {
                    if genre == "All" {
                        self.library.stations.iter().collect()
                    } else {
                        self.library.stations.iter()
                            .filter(|s| crate::favorites::resolve_parent_genre(&s.genre).eq_ignore_ascii_case(genre))
                            .collect()
                    }
                } else {
                    self.library.stations.iter().collect()
                }
            }
            InputMode::Search => self.search_results.iter().collect(),
        }
    }

    /// Process an action and update state accordingly.
    pub fn update(&mut self, action: Action) {
        if self.show_settings {
            match action {
                Action::NextStation => {
                    self.selected_setting_idx = (self.selected_setting_idx + 1) % 5;
                    return;
                }
                Action::PrevStation => {
                    self.selected_setting_idx = if self.selected_setting_idx == 0 {
                        4
                    } else {
                        self.selected_setting_idx - 1
                    };
                    return;
                }
                Action::PlaySelected | Action::TogglePause => {
                    match self.selected_setting_idx {
                        0 => {
                            self.library.settings.notifications_enabled = !self.library.settings.notifications_enabled;
                        }
                        1 => {
                            self.library.settings.autoplay_last = !self.library.settings.autoplay_last;
                        }
                        2 => {
                            self.library.settings.recording_dir = match self.library.settings.recording_dir.as_str() {
                                "./recordings" => "./music".to_string(),
                                "./music" => "./driftfm-captures".to_string(),
                                _ => "./recordings".to_string(),
                            };
                        }
                        3 => {
                            self.library.settings.keep_snippets = !self.library.settings.keep_snippets;
                        }
                        4 => {
                            use crate::ui::theme::ThemeName;
                            let current = ThemeName::from_key(&self.library.settings.theme);
                            let next = current.next();
                            self.library.settings.theme = next.key().to_string();
                            crate::ui::theme::set_active(next);
                        }
                        _ => {}
                    }
                    self.library.save();
                    return;
                }
                Action::ToggleSettings => {
                    self.show_settings = false;
                    return;
                }
                Action::Quit => {
                    self.show_settings = false;
                    return;
                }
                Action::Tick => {
                    self.tick_count += 1;
                    self.poll_audio_status();
                    return;
                }
                _ => {}
            }
        }

        match action {
            Action::NextStation => {
                let count = self.visible_stations().len();
                if count > 0 {
                    self.selected = (self.selected + 1) % count;
                }
            }
            Action::PrevStation => {
                let count = self.visible_stations().len();
                if count > 0 {
                    self.selected = if self.selected == 0 {
                        count - 1
                    } else {
                        self.selected - 1
                    };
                }
            }
            Action::PlaySelected => {
                let station = self.visible_stations().get(self.selected).copied().cloned();
                if let Some(station) = station {
                    self.playing_url = Some(station.url.clone());
                    
                    // Persist last played station URL
                    self.library.settings.last_played_url = Some(station.url.clone());
                    self.library.save();

                    self.audio.send(AudioCommand::Play(station.url));
                    self.sync_volume();
                }
            }
            Action::TogglePause => match self.playback {
                PlaybackState::Playing => {
                    self.audio.send(AudioCommand::Pause);
                }
                PlaybackState::Paused => {
                    self.audio.send(AudioCommand::Resume);
                }
                PlaybackState::Stopped | PlaybackState::Error(_) => {
                    self.update(Action::PlaySelected);
                }
                PlaybackState::Connecting => {
                    self.update(Action::Stop);
                }
            },
            Action::Stop => {
                self.audio.send(AudioCommand::Stop);
                self.playing_url = None;
            }
            Action::VolumeUp => {
                self.volume = (self.volume + 5).min(100);
                self.muted = false;
                self.sync_volume();
            }
            Action::VolumeDown => {
                self.volume = self.volume.saturating_sub(5);
                self.sync_volume();
            }
            Action::ToggleMute => {
                self.muted = !self.muted;
                self.sync_volume();
            }

            // ── Search ───────────────────────────────────────────
            Action::EnterSearch => {
                self.input_mode = InputMode::Search;
                self.search_query.clear();
                self.search_results.clear();
                self.last_api_query.clear();
                self.selected = 0;
            }
            Action::ExitSearch => {
                self.input_mode = InputMode::Normal;
                self.search_query.clear();
                self.search_results.clear();
                self.last_api_query.clear();
                self.selected = 0;
                // Re-select the playing station in the library
                self.select_playing();
            }
            Action::SearchInput(c) => {
                self.search_query.push(c);
                self.trigger_api_search();
            }
            Action::SearchBackspace => {
                self.search_query.pop();
                if self.search_query.is_empty() {
                    self.search_results.clear();
                }
                self.trigger_api_search();
            }
            Action::SearchConfirm => {
                // Add the selected search result to library + play it
                if let Some(station) = self.search_results.get(self.selected).cloned() {
                    self.library.add(station.clone());
                    self.playing_url = Some(station.url.clone());

                    // Persist last played station URL
                    self.library.settings.last_played_url = Some(station.url.clone());
                    self.library.save();

                    self.audio.send(AudioCommand::Play(station.url));
                    self.sync_volume();
                }
                // Exit search
                self.input_mode = InputMode::Normal;
                self.search_query.clear();
                self.search_results.clear();
                self.last_api_query.clear();
                self.selected = 0;
                self.select_playing();
            }

            // ── Favorites (library management) ────────────────────
            Action::ToggleFavorite => {
                // In Normal mode: remove station from library
                // In Search mode: add station to library
                match self.input_mode {
                    InputMode::Normal => {
                        if let Some(station) = self.visible_stations().get(self.selected) {
                            let url = station.url.clone();
                            self.library.remove(&url);
                            // Clamp selection
                            let count = self.visible_stations().len();
                            if self.selected >= count && self.selected > 0 {
                                self.selected = count - 1;
                            }
                        }
                    }
                    InputMode::Search => {
                        if let Some(station) = self.search_results.get(self.selected).cloned() {
                            self.library.add(station);
                        }
                    }
                }
            }

            Action::NextGenre => {
                if self.input_mode == InputMode::Normal {
                    let count = self.library.available_genres.len();
                    if count > 0 {
                        self.selected_genre_idx = (self.selected_genre_idx + 1) % count;
                        self.selected = 0;
                    }
                }
            }
            Action::PrevGenre => {
                if self.input_mode == InputMode::Normal {
                    let count = self.library.available_genres.len();
                    if count > 0 {
                        self.selected_genre_idx = if self.selected_genre_idx == 0 {
                            count - 1
                        } else {
                            self.selected_genre_idx - 1
                        };
                        self.selected = 0;
                    }
                }
            }


            Action::ToggleHelp => {
                self.show_help = !self.show_help;
                if self.show_help {
                    self.show_settings = false;
                }
            }
            Action::ToggleSettings => {
                self.show_settings = !self.show_settings;
                if self.show_settings {
                    self.show_help = false;
                }
            }

            Action::ToggleRecording => {
                if self.playing_url.is_some() {
                    match self.recording_state {
                        RecordingState::Off => {
                            let category = self.now_playing()
                                .map(|s| s.genre.clone())
                                .unwrap_or_else(|| "Unknown".to_string());
                            let rec_dir = self.library.settings.recording_dir.clone();
                            let keep_snippets = self.library.settings.keep_snippets;
                            let min_secs = self.library.settings.min_song_duration_secs;
                            
                            self.audio.send(AudioCommand::StartRecording {
                                recording_dir: rec_dir,
                                category,
                                keep_snippets,
                                min_song_duration_secs: min_secs,
                            });
                            self.recording_state = RecordingState::Pending;
                        }
                        RecordingState::Pending | RecordingState::Active => {
                            self.audio.send(AudioCommand::StopRecording);
                            self.recording_state = RecordingState::Off;
                            self.active_record_filepath = None;
                        }
                    }
                }
            }
            Action::ToggleRightPanel => {
                self.show_right_panel = !self.show_right_panel;
            }
            Action::NextDeckPage => {
                self.active_deck_page = (self.active_deck_page + 1) % 2;
            }
            Action::Tick => {
                self.tick_count += 1;
                self.poll_audio_status();
            }
            Action::Quit => {
                if self.show_help {
                    self.show_help = false;
                } else {
                    self.audio.send(AudioCommand::Stop);
                    self.should_quit = true;
                }
            }
        }
    }

    /// Merge API search results (replaces current results for that query).
    pub fn set_search_results(&mut self, results: Vec<Station>) {
        self.searching_api = false;
        self.search_results = results;
        self.selected = 0;
    }

    /// Signal that the main loop should fire an API search.
    fn trigger_api_search(&mut self) {
        let query = self.search_query.trim().to_string();
        if query.len() >= 2 && query != self.last_api_query {
            self.pending_api_search = Some(query.clone());
            self.last_api_query = query;
            self.searching_api = true;
        }
    }

    /// Try to select the currently playing station in the library.
    fn select_playing(&mut self) {
        if let Some(ref url) = self.playing_url {
            if let Some(pos) = self.visible_stations().iter().position(|s| s.url == *url) {
                self.selected = pos;
            }
        }
    }

    /// Sync volume to audio engine, respecting mute state.
    fn sync_volume(&self) {
        let vol = if self.muted {
            0.0
        } else {
            self.volume as f32 / 100.0
        };
        self.audio.send(AudioCommand::SetVolume(vol));
    }

    /// Get the currently playing station, if any.
    pub fn now_playing(&self) -> Option<&Station> {
        self.playing_url.as_ref().and_then(|url| {
            self.library.stations.iter().find(|s| s.url == *url)
                .or_else(|| self.search_results.iter().find(|s| s.url == *url))
        })
    }
}
