#!/usr/bin/env python3
"""Apply Phase 3 app-state refactor in one guarded pass.

The script keeps the public `crate::app` boundary stable while splitting the
monolithic app reducer into focused modules. It also adds contract tests, app
architecture docs, changelog notes, and removes this temporary script.
"""

from pathlib import Path

APP_RS = Path("src/app.rs")
APP_DIR = Path("src/app")
DOC = Path("docs/app-architecture.md")
CHANGELOG = Path("CHANGELOG.md")
THIS_SCRIPT = Path("scripts/apply_phase3_app_state_refactor.py")


def write(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text.strip() + "\n", encoding="utf-8")


def replace_once(text: str, old: str, new: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"Expected exactly one match, found {count}: {old[:120]!r}")
    return text.replace(old, new, 1)


def write_app_root() -> None:
    write(APP_RS, r'''
mod idle;
mod library;
mod lifecycle;
mod overlays;
mod playback;
mod recording;
mod search;
mod selectors;
mod settings;
mod types;
mod update;
mod visualizer;

use crate::audio::AudioEngine;
use crate::favorites::Library;
use crate::radio::Station;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

pub use types::{AppNotice, InputMode, LayoutMode, PlaybackState, RecordingState, SearchStatus};

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
    pub volume: u8, // 0-100
    pub muted: bool,
    pub should_quit: bool,
    pub notice: Option<AppNotice>,
    notice_ticks_remaining: u16,

    // Input mode
    pub input_mode: InputMode,

    // Search state
    pub search_query: String,
    pub search_status: SearchStatus,

    // API search state - main.rs checks these to spawn async fetches
    pub pending_api_search: Option<String>,
    pub searching_api: bool,
    last_api_query: String,

    pub selected_genre_idx: usize,
    pub current_track: Option<String>,
    pub tick_count: u64,

    pub layout_mode: LayoutMode,
    pub show_help: bool,
    pub active_deck_page: usize,
    pub song_history: VecDeque<String>,

    pub show_settings: bool,
    pub selected_setting_idx: usize,

    pub recording_state: RecordingState,
    pub active_record_filepath: Option<String>,
    pub buffer_percent: u8,
    pub buffer_seconds: u32,

    audio: AudioEngine,
    pub sample_buffer: Arc<Mutex<VecDeque<f32>>>,
    pub visualizer_mode: usize, // 0 = Spectrum, 1 = Oscilloscope, 2 = Simulated
    pub visualizer_peaks: Vec<f32>,
}
''')


def write_types() -> None:
    write(APP_DIR / "types.rs", r'''
pub(super) const SEARCH_MIN_CHARS: usize = 2;

/// Input mode determines how keyboard events are routed.
#[derive(Debug, Clone, PartialEq)]
pub enum InputMode {
    Normal,
    Search,
}

/// Explicit search state for UI messages and stale-response handling.
#[derive(Debug, Clone, PartialEq)]
pub enum SearchStatus {
    WaitingForInput,
    Debouncing { query: String },
    Searching { query: String },
    Ready { query: String },
    Empty { query: String },
    Error { query: String, message: String },
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

/// Tape recorder capturing states.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RecordingState {
    Off,
    Pending,
    Active,
}

/// TUI Dashboard layout configurations.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum LayoutMode {
    Split,     // Mode 0: Station list on left (55%), Bento Tape Deck on right (45%)
    LeftOnly,  // Mode 1: Closed Bento, Station list full width (100%)
    RightOnly, // Mode 2: Only Bento, Tape Deck full width (100%)
}

#[derive(Debug, Clone, PartialEq)]
pub enum AppNotice {
    Info(String),
    Error(String),
}
''')


def write_lifecycle() -> None:
    write(APP_DIR / "lifecycle.rs", r'''
use super::*;
use crate::audio::{AudioCommand, AudioEngine, AudioStatus};

impl App {
    pub fn new(library: Library) -> Self {
        let sample_buffer = Arc::new(Mutex::new(VecDeque::with_capacity(4096)));
        let audio = AudioEngine::spawn(sample_buffer.clone());

        let mut app = Self {
            library,
            search_results: Vec::new(),
            selected: 0,
            playback: PlaybackState::Stopped,
            playing_url: None,
            volume: 80,
            muted: false,
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
            layout_mode: LayoutMode::Split,
            show_help: false,
            active_deck_page: 0,
            song_history: VecDeque::new(),
            show_settings: false,
            selected_setting_idx: 0,
            recording_state: RecordingState::Off,
            active_record_filepath: None,
            buffer_percent: 0,
            buffer_seconds: 0,
            audio,
            sample_buffer,
            visualizer_mode: 0,
            visualizer_peaks: Vec::new(),
        };

        // Autoplay last played station on boot if enabled.
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

                                let _ = notify_rust::Notification::new()
                                    .summary("DriftFM - Now Playing")
                                    .body(&format!("♫ {}\nStation: {}", title, station_name))
                                    .icon("audio-card")
                                    .timeout(4000)
                                    .show();
                            }
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
}
''')


def write_idle() -> None:
    write(APP_DIR / "idle.rs", r'''
#[cfg(target_os = "windows")]
pub(super) fn get_user_idle_ms() -> Option<u64> {
    #[repr(C)]
    #[allow(clippy::upper_case_acronyms)] // Mirrors the Win32 API struct name.
    struct LASTINPUTINFO {
        cb_size: u32,
        dw_time: u32,
    }

    extern "system" {
        fn GetLastInputInfo(plii: *mut LASTINPUTINFO) -> i32;
        fn GetTickCount64() -> u64;
    }

    let mut lii = LASTINPUTINFO {
        cb_size: std::mem::size_of::<LASTINPUTINFO>() as u32,
        dw_time: 0,
    };

    unsafe {
        if GetLastInputInfo(&mut lii) != 0 {
            let tick = GetTickCount64();
            // LASTINPUTINFO.dwTime is still u32, but GetTickCount64 is u64.
            // We compare only the lower 32 bits for the delta.
            let last_input_64 = lii.dw_time as u64;
            let tick_low = tick & 0xFFFF_FFFF;
            let idle = if tick_low >= last_input_64 {
                tick_low - last_input_64
            } else {
                // u32 rollover: last_input was near u32::MAX, tick_low wrapped.
                (0x1_0000_0000u64 - last_input_64) + tick_low
            };
            Some(idle)
        } else {
            None
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub(super) fn get_user_idle_ms() -> Option<u64> {
    None
}
''')


def write_selectors() -> None:
    write(APP_DIR / "selectors.rs", r'''
use super::*;
use crate::favorites::resolve_parent_genre;

impl App {
    /// The currently visible list. In Normal mode: library. In Search mode: search results.
    pub fn visible_stations(&self) -> Vec<&Station> {
        match self.input_mode {
            InputMode::Normal => {
                if let Some(genre) = self.library.available_genres.get(self.selected_genre_idx) {
                    if genre == "All" {
                        self.library.stations.iter().collect()
                    } else {
                        self.library
                            .stations
                            .iter()
                            .filter(|s| resolve_parent_genre(&s.genre).eq_ignore_ascii_case(genre))
                            .collect()
                    }
                } else {
                    self.library.stations.iter().collect()
                }
            }
            InputMode::Search => self.search_results.iter().collect(),
        }
    }

    /// Get the currently playing station, if any.
    pub fn now_playing(&self) -> Option<&Station> {
        self.playing_url.as_ref().and_then(|url| {
            self.library
                .stations
                .iter()
                .find(|s| s.url == *url)
                .or_else(|| self.search_results.iter().find(|s| s.url == *url))
        })
    }

    /// Count visible stations without allocating a Vec.
    pub fn visible_count(&self) -> usize {
        match self.input_mode {
            InputMode::Normal => {
                if let Some(genre) = self.library.available_genres.get(self.selected_genre_idx) {
                    if genre == "All" {
                        self.library.stations.len()
                    } else {
                        self.library
                            .stations
                            .iter()
                            .filter(|s| resolve_parent_genre(&s.genre).eq_ignore_ascii_case(genre))
                            .count()
                    }
                } else {
                    self.library.stations.len()
                }
            }
            InputMode::Search => self.search_results.len(),
        }
    }

    /// Try to select the currently playing station in the visible list.
    pub(super) fn select_playing(&mut self) {
        if let Some(ref url) = self.playing_url {
            if let Some(pos) = self.visible_stations().iter().position(|s| s.url == *url) {
                self.selected = pos;
            }
        }
    }
}
''')


def write_update() -> None:
    write(APP_DIR / "update.rs", r'''
use super::*;
use crate::action::Action;

impl App {
    /// Process an action and update state accordingly.
    pub fn update(&mut self, action: Action) {
        if self.show_settings {
            self.handle_settings_action(action);
            return;
        }

        match action {
            Action::NextStation => self.next_station(),
            Action::PrevStation => self.prev_station(),

            Action::PlaySelected => self.play_selected(),
            Action::TogglePause => self.toggle_pause(),
            Action::Stop => self.stop_playback(),

            Action::VolumeUp => self.volume_up(),
            Action::VolumeDown => self.volume_down(),
            Action::ToggleMute => self.toggle_mute(),

            Action::EnterSearch => self.enter_search(),
            Action::ExitSearch => self.exit_search(),
            Action::SearchInput(c) => self.search_input(c),
            Action::SearchBackspace => self.search_backspace(),
            Action::SearchConfirm => self.confirm_search(),

            Action::RemoveLibrarySelection => self.remove_library_selection(),
            Action::NextGenre => self.next_genre(),
            Action::PrevGenre => self.prev_genre(),

            Action::ToggleHelp => self.toggle_help(),
            Action::ToggleSettings => self.toggle_settings(),
            Action::ToggleRecording => self.toggle_recording(),
            Action::CycleLayout => self.cycle_layout(),
            Action::NextDeckPage => self.next_deck_page(),
            Action::ToggleVisualizerMode => self.toggle_visualizer_mode(),

            Action::Tick => self.tick(),
            Action::Quit => self.quit(),
        }
    }

    pub(super) fn tick(&mut self) {
        self.tick_count += 1;
        self.poll_audio_status();
        self.update_visualizer();
    }

    pub(super) fn quit(&mut self) {
        if self.show_help {
            self.show_help = false;
        } else {
            self.stop_audio_before_quit();
            self.should_quit = true;
        }
    }

    fn next_station(&mut self) {
        let count = self.visible_count();
        if count > 0 {
            self.selected = (self.selected + 1) % count;
        }
    }

    fn prev_station(&mut self) {
        let count = self.visible_count();
        if count > 0 {
            self.selected = if self.selected == 0 {
                count - 1
            } else {
                self.selected - 1
            };
        }
    }
}
''')


def write_playback() -> None:
    write(APP_DIR / "playback.rs", r'''
use super::*;
use crate::audio::AudioCommand;

impl App {
    pub(super) fn play_selected(&mut self) {
        let station = self.visible_stations().get(self.selected).copied().cloned();
        if let Some(station) = station {
            self.playing_url = Some(station.url.clone());

            // Persist last played station URL.
            self.library.settings.last_played_url = Some(station.url.clone());
            self.save_library_or_notice("last played station");

            self.audio.send(AudioCommand::Play(station.url));
            self.sync_volume();
        }
    }

    pub(super) fn toggle_pause(&mut self) {
        match self.playback.clone() {
            PlaybackState::Playing => {
                self.audio.send(AudioCommand::Pause);
            }
            PlaybackState::Paused => {
                self.audio.send(AudioCommand::Resume);
            }
            PlaybackState::Stopped | PlaybackState::Error(_) => {
                self.play_selected();
            }
            PlaybackState::Connecting => {
                self.stop_playback();
            }
        }
    }

    pub(super) fn stop_playback(&mut self) {
        self.audio.send(AudioCommand::Stop);
        self.playing_url = None;
    }

    pub(super) fn stop_audio_before_quit(&mut self) {
        self.audio.send(AudioCommand::Stop);
    }

    pub(super) fn volume_up(&mut self) {
        self.volume = (self.volume + 5).min(100);
        self.muted = false;
        self.sync_volume();
    }

    pub(super) fn volume_down(&mut self) {
        self.volume = self.volume.saturating_sub(5);
        self.sync_volume();
    }

    pub(super) fn toggle_mute(&mut self) {
        self.muted = !self.muted;
        self.sync_volume();
    }

    /// Sync volume to audio engine, respecting mute state.
    pub(super) fn sync_volume(&self) {
        let vol = if self.muted {
            0.0
        } else {
            self.volume as f32 / 100.0
        };
        self.audio.send(AudioCommand::SetVolume(vol));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::favorites::Library;
    use crate::radio::Station;

    fn station(name: &str, url: &str) -> Station {
        Station {
            name: name.to_string(),
            url: url.to_string(),
            genre: "Synthwave".to_string(),
            country: "US".to_string(),
            bitrate: 128,
        }
    }

    fn test_app() -> App {
        App::new(Library::in_memory(vec![station("A", "http://a")]))
    }

    #[test]
    fn play_selected_sets_playing_url_and_last_played_url() {
        let mut app = test_app();

        app.play_selected();

        assert_eq!(app.playing_url.as_deref(), Some("http://a"));
        assert_eq!(
            app.library.settings.last_played_url.as_deref(),
            Some("http://a")
        );
    }

    #[test]
    fn stop_clears_playing_url() {
        let mut app = test_app();
        app.playing_url = Some("http://a".to_string());

        app.stop_playback();

        assert_eq!(app.playing_url, None);
    }

    #[test]
    fn volume_up_unmutes() {
        let mut app = test_app();
        app.volume = 80;
        app.muted = true;

        app.volume_up();

        assert_eq!(app.volume, 85);
        assert!(!app.muted);
    }

    #[test]
    fn toggle_mute_preserves_volume_number() {
        let mut app = test_app();
        app.volume = 65;

        app.toggle_mute();

        assert!(app.muted);
        assert_eq!(app.volume, 65);
    }
}
''')


def write_search() -> None:
    write(APP_DIR / "search.rs", r'''
use super::*;
use crate::audio::AudioCommand;

impl App {
    pub(super) fn enter_search(&mut self) {
        self.input_mode = InputMode::Search;
        self.search_query.clear();
        self.search_results.clear();
        self.last_api_query.clear();
        self.search_status = SearchStatus::WaitingForInput;
        self.searching_api = false;
        self.pending_api_search = None;
        self.selected = 0;
    }

    pub(super) fn exit_search(&mut self) {
        self.input_mode = InputMode::Normal;
        self.search_query.clear();
        self.search_results.clear();
        self.last_api_query.clear();
        self.search_status = SearchStatus::WaitingForInput;
        self.searching_api = false;
        self.pending_api_search = None;
        self.selected = 0;
        self.select_playing();
    }

    pub(super) fn search_input(&mut self, c: char) {
        self.search_query.push(c);
        self.refresh_search_state();
    }

    pub(super) fn search_backspace(&mut self) {
        self.search_query.pop();
        self.refresh_search_state();
    }

    pub(super) fn confirm_search(&mut self) {
        // Add the selected search result to library and play it.
        if let Some(station) = self.search_results.get(self.selected).cloned() {
            match self.library.add(station.clone()) {
                Ok(true) => self.set_info_notice("Station saved to library"),
                Ok(false) => {}
                Err(err) => self.set_error_notice(format!(
                    "Station added in memory, but could not save library: {err}"
                )),
            }
            self.playing_url = Some(station.url.clone());

            // Persist last played station URL.
            self.library.settings.last_played_url = Some(station.url.clone());
            self.save_library_or_notice("last played station");

            self.audio.send(AudioCommand::Play(station.url));
            self.sync_volume();
        }

        self.exit_search();
    }

    /// Return the query currently waiting for debounce, if any.
    pub fn current_debounce_query(&self) -> Option<&str> {
        match &self.search_status {
            SearchStatus::Debouncing { query } => Some(query.as_str()),
            _ => None,
        }
    }

    /// Mark a debounced query as actively searching.
    pub fn mark_search_started(&mut self, query: &str) -> bool {
        let current_query = self.search_query.trim();
        if self.input_mode != InputMode::Search || current_query != query {
            return false;
        }

        if matches!(&self.search_status, SearchStatus::Debouncing { query: q } if q == query) {
            self.search_status = SearchStatus::Searching {
                query: query.to_string(),
            };
            self.searching_api = true;
            self.last_api_query = query.to_string();
            self.pending_api_search = None;
            true
        } else {
            false
        }
    }

    /// Apply a query-tagged search response. Returns false when the response was stale.
    pub fn apply_search_response(
        &mut self,
        query: String,
        result: Result<Vec<Station>, String>,
    ) -> bool {
        let current_query = self.search_query.trim();
        let is_current_search = self.input_mode == InputMode::Search
            && current_query == query
            && matches!(&self.search_status, SearchStatus::Searching { query: q } if q == &query);

        if !is_current_search {
            return false;
        }

        self.searching_api = false;
        self.selected = 0;

        match result {
            Ok(results) => {
                self.search_results = results;
                if self.search_results.is_empty() {
                    self.search_status = SearchStatus::Empty { query };
                } else {
                    self.search_status = SearchStatus::Ready { query };
                }
            }
            Err(message) => {
                self.search_results.clear();
                self.search_status = SearchStatus::Error { query, message };
            }
        }

        true
    }

    pub(super) fn refresh_search_state(&mut self) {
        let query = self.search_query.trim().to_string();

        if query.chars().count() < types::SEARCH_MIN_CHARS {
            self.search_results.clear();
            self.selected = 0;
            self.searching_api = false;
            self.pending_api_search = None;
            self.search_status = SearchStatus::WaitingForInput;
            return;
        }

        let is_already_current = matches!(
            &self.search_status,
            SearchStatus::Debouncing { query: q }
                | SearchStatus::Searching { query: q }
                | SearchStatus::Ready { query: q }
                | SearchStatus::Empty { query: q }
                | SearchStatus::Error { query: q, .. } if q == &query
        );

        if is_already_current {
            return;
        }

        self.search_results.clear();
        self.selected = 0;
        self.searching_api = false;
        self.pending_api_search = None;
        self.search_status = SearchStatus::Debouncing { query };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Action;
    use crate::favorites::Library;
    use crate::radio::Station;

    fn station(name: &str, url: &str) -> Station {
        Station {
            name: name.to_string(),
            url: url.to_string(),
            genre: "Synthwave".to_string(),
            country: "US".to_string(),
            bitrate: 128,
        }
    }

    fn test_app() -> App {
        App::new(Library::in_memory(vec![]))
    }

    #[test]
    fn short_search_query_clears_results_and_waits_for_input() {
        let mut app = test_app();
        app.update(Action::EnterSearch);
        app.search_results = vec![station("Old", "http://old")];

        app.update(Action::SearchInput('l'));

        assert!(app.search_results.is_empty());
        assert_eq!(app.search_status, SearchStatus::WaitingForInput);
        assert!(app.current_debounce_query().is_none());
        assert!(!app.searching_api);
    }

    #[test]
    fn valid_search_query_enters_debounce_state() {
        let mut app = test_app();
        app.update(Action::EnterSearch);

        app.update(Action::SearchInput('l'));
        app.update(Action::SearchInput('o'));

        assert_eq!(
            app.search_status,
            SearchStatus::Debouncing {
                query: "lo".to_string()
            }
        );
        assert_eq!(app.current_debounce_query(), Some("lo"));
        assert!(!app.searching_api);
    }

    #[test]
    fn mark_search_started_moves_debounced_query_to_searching() {
        let mut app = test_app();
        app.update(Action::EnterSearch);
        app.update(Action::SearchInput('l'));
        app.update(Action::SearchInput('o'));

        assert!(app.mark_search_started("lo"));
        assert_eq!(
            app.search_status,
            SearchStatus::Searching {
                query: "lo".to_string()
            }
        );
        assert!(app.searching_api);
    }

    #[test]
    fn current_query_success_response_is_accepted() {
        let mut app = test_app();
        app.update(Action::EnterSearch);
        app.update(Action::SearchInput('l'));
        app.update(Action::SearchInput('o'));
        app.mark_search_started("lo");

        let accepted = app.apply_search_response(
            "lo".to_string(),
            Ok(vec![station("Lo-Fi Radio", "http://lofi")]),
        );

        assert!(accepted);
        assert_eq!(app.search_results.len(), 1);
        assert_eq!(
            app.search_status,
            SearchStatus::Ready {
                query: "lo".to_string()
            }
        );
        assert!(!app.searching_api);
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn current_query_empty_response_sets_empty_status() {
        let mut app = test_app();
        app.update(Action::EnterSearch);
        app.update(Action::SearchInput('z'));
        app.update(Action::SearchInput('z'));
        app.mark_search_started("zz");

        let accepted = app.apply_search_response("zz".to_string(), Ok(vec![]));

        assert!(accepted);
        assert!(app.search_results.is_empty());
        assert_eq!(
            app.search_status,
            SearchStatus::Empty {
                query: "zz".to_string()
            }
        );
    }

    #[test]
    fn current_query_error_response_sets_error_status() {
        let mut app = test_app();
        app.update(Action::EnterSearch);
        app.update(Action::SearchInput('l'));
        app.update(Action::SearchInput('o'));
        app.mark_search_started("lo");

        let accepted = app.apply_search_response("lo".to_string(), Err("network down".to_string()));

        assert!(accepted);
        assert!(app.search_results.is_empty());
        assert_eq!(
            app.search_status,
            SearchStatus::Error {
                query: "lo".to_string(),
                message: "network down".to_string()
            }
        );
        assert!(!app.searching_api);
    }

    #[test]
    fn stale_search_response_is_ignored() {
        let mut app = test_app();
        app.update(Action::EnterSearch);
        app.update(Action::SearchInput('l'));
        app.update(Action::SearchInput('o'));
        app.update(Action::SearchInput('f'));
        app.update(Action::SearchInput('i'));
        app.mark_search_started("lofi");

        let accepted = app.apply_search_response(
            "lo".to_string(),
            Ok(vec![station("Old Result", "http://old")]),
        );

        assert!(!accepted);
        assert!(app.search_results.is_empty());
        assert_eq!(
            app.search_status,
            SearchStatus::Searching {
                query: "lofi".to_string()
            }
        );
    }

    #[test]
    fn normal_mode_search_response_is_ignored() {
        let mut app = test_app();

        let accepted = app.apply_search_response(
            "lo".to_string(),
            Ok(vec![station("Ignored", "http://ignored")]),
        );

        assert!(!accepted);
        assert!(app.search_results.is_empty());
        assert_eq!(app.search_status, SearchStatus::WaitingForInput);
    }

    #[test]
    fn search_confirm_adds_result_exits_search_and_selects_playing() {
        let mut app = test_app();
        app.update(Action::EnterSearch);
        app.search_results = vec![station("Lo-Fi Radio", "http://lofi")];
        app.selected = 0;

        app.update(Action::SearchConfirm);

        assert_eq!(app.input_mode, InputMode::Normal);
        assert_eq!(app.playing_url.as_deref(), Some("http://lofi"));
        assert!(app.library.contains("http://lofi"));
        assert_eq!(app.search_status, SearchStatus::WaitingForInput);
        assert!(app.search_results.is_empty());
    }

    #[test]
    fn search_confirm_without_result_exits_search_without_playing() {
        let mut app = test_app();
        app.update(Action::EnterSearch);

        app.update(Action::SearchConfirm);

        assert_eq!(app.input_mode, InputMode::Normal);
        assert_eq!(app.playing_url, None);
        assert!(app.search_results.is_empty());
    }
}
''')


def write_settings() -> None:
    write(APP_DIR / "settings.rs", r'''
use super::*;
use crate::action::Action;
use crate::ui::theme::ThemeName;

impl App {
    pub(super) fn handle_settings_action(&mut self, action: Action) {
        match action {
            Action::NextStation => {
                self.selected_setting_idx = (self.selected_setting_idx + 1) % 6;
            }
            Action::PrevStation => {
                self.selected_setting_idx = if self.selected_setting_idx == 0 {
                    5
                } else {
                    self.selected_setting_idx - 1
                };
            }
            Action::PlaySelected | Action::TogglePause => {
                self.apply_selected_setting();
                self.save_library_or_notice("settings");
            }
            Action::ToggleSettings => {
                self.show_settings = false;
            }
            Action::Quit => {
                self.show_settings = false;
            }
            Action::Tick => {
                self.tick_count += 1;
                self.tick_notice();
                self.poll_audio_status();
                self.update_visualizer();
            }
            _ => {
                // Block all other actions while settings are open.
            }
        }
    }

    pub(super) fn apply_selected_setting(&mut self) {
        match self.selected_setting_idx {
            0 => {
                self.library.settings.notifications_enabled =
                    !self.library.settings.notifications_enabled;
            }
            1 => {
                self.library.settings.autoplay_last = !self.library.settings.autoplay_last;
            }
            2 => {
                self.library.settings.recording_dir =
                    match self.library.settings.recording_dir.as_str() {
                        "./recordings" => "./music".to_string(),
                        "./music" => "./driftfm-captures".to_string(),
                        _ => "./recordings".to_string(),
                    };
            }
            3 => {
                self.library.settings.keep_snippets = !self.library.settings.keep_snippets;
            }
            4 => {
                // Cycle min duration: 30 -> 60 -> 90 -> 120 -> 180
                self.library.settings.min_song_duration_secs =
                    match self.library.settings.min_song_duration_secs {
                        30 => 60,
                        60 => 90,
                        90 => 120,
                        120 => 180,
                        _ => 30,
                    };
            }
            5 => {
                let current = ThemeName::from_key(&self.library.settings.theme);
                let next = current.next();
                self.library.settings.theme = next.key().to_string();
                crate::ui::theme::set_active(next);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::favorites::Library;
    use crate::radio::Station;

    fn station(name: &str, url: &str) -> Station {
        Station {
            name: name.to_string(),
            url: url.to_string(),
            genre: "Synthwave".to_string(),
            country: "US".to_string(),
            bitrate: 128,
        }
    }

    fn test_app() -> App {
        App::new(Library::in_memory(vec![station("A", "http://a")]))
    }

    #[test]
    fn settings_blocks_play_selected() {
        let mut app = test_app();
        app.show_settings = true;
        app.selected_setting_idx = 0;
        let before = app.library.settings.notifications_enabled;

        app.update(Action::PlaySelected);

        assert_eq!(app.playing_url, None);
        assert_eq!(app.library.settings.notifications_enabled, !before);
    }

    #[test]
    fn settings_blocks_search_entry() {
        let mut app = test_app();
        app.show_settings = true;

        app.update(Action::EnterSearch);

        assert_eq!(app.input_mode, InputMode::Normal);
        assert!(app.show_settings);
    }

    #[test]
    fn settings_quit_closes_settings_without_quitting_app() {
        let mut app = test_app();
        app.show_settings = true;

        app.update(Action::Quit);

        assert!(!app.show_settings);
        assert!(!app.should_quit);
    }

    #[test]
    fn settings_tick_still_polls_and_updates() {
        let mut app = test_app();
        app.show_settings = true;
        app.notice = Some(AppNotice::Info("hello".to_string()));

        app.update(Action::Tick);

        assert_eq!(app.tick_count, 1);
        assert!(app.notice.is_some());
    }
}
''')


def write_library() -> None:
    write(APP_DIR / "library.rs", r'''
use super::*;

impl App {
    pub(super) fn remove_library_selection(&mut self) {
        if self.input_mode == InputMode::Normal {
            if let Some(station) = self.visible_stations().get(self.selected) {
                let url = station.url.clone();
                match self.library.remove(&url) {
                    Ok(true) => self.set_info_notice("Station removed"),
                    Ok(false) => {}
                    Err(err) => self.set_error_notice(format!(
                        "Station removed in memory, but could not save library: {err}"
                    )),
                }
                // Clamp selection.
                let count = self.visible_count();
                if self.selected >= count && self.selected > 0 {
                    self.selected = count - 1;
                }
            }
        }
    }

    pub(super) fn next_genre(&mut self) {
        if self.input_mode == InputMode::Normal {
            let count = self.library.available_genres.len();
            if count > 0 {
                self.selected_genre_idx = (self.selected_genre_idx + 1) % count;
                self.selected = 0;
            }
        }
    }

    pub(super) fn prev_genre(&mut self) {
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

    pub(super) fn save_library_or_notice(&mut self, context: &str) {
        if let Err(err) = self.library.save() {
            self.set_error_notice(format!("Could not save {context}: {err}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::favorites::Library;
    use crate::radio::Station;

    fn station(name: &str, url: &str, genre: &str) -> Station {
        Station {
            name: name.to_string(),
            url: url.to_string(),
            genre: genre.to_string(),
            country: "US".to_string(),
            bitrate: 128,
        }
    }

    #[test]
    fn remove_library_selection_removes_selected_station() {
        let mut app = App::new(Library::in_memory(vec![
            station("A", "http://a", "Synthwave"),
            station("B", "http://b", "Synthwave"),
        ]));
        app.selected = 0;

        app.remove_library_selection();

        assert!(!app.library.contains("http://a"));
        assert!(app.library.contains("http://b"));
    }

    #[test]
    fn next_genre_resets_selection() {
        let mut app = App::new(Library::in_memory(vec![
            station("A", "http://a", "Synthwave"),
            station("B", "http://b", "Ambient"),
        ]));
        app.selected = 1;

        app.next_genre();

        assert_eq!(app.selected, 0);
    }
}
''')


def write_recording() -> None:
    write(APP_DIR / "recording.rs", r'''
use super::*;
use crate::audio::AudioCommand;

impl App {
    pub(super) fn toggle_recording(&mut self) {
        if self.playing_url.is_some() {
            match self.recording_state {
                RecordingState::Off => {
                    let category = self
                        .now_playing()
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::favorites::Library;
    use crate::radio::Station;

    fn station(name: &str, url: &str) -> Station {
        Station {
            name: name.to_string(),
            url: url.to_string(),
            genre: "Synthwave".to_string(),
            country: "US".to_string(),
            bitrate: 128,
        }
    }

    fn test_app() -> App {
        App::new(Library::in_memory(vec![station("A", "http://a")]))
    }

    #[test]
    fn toggle_recording_without_playing_does_nothing() {
        let mut app = test_app();

        app.toggle_recording();

        assert_eq!(app.recording_state, RecordingState::Off);
    }

    #[test]
    fn toggle_recording_when_playing_sets_pending() {
        let mut app = test_app();
        app.playing_url = Some("http://a".to_string());

        app.toggle_recording();

        assert_eq!(app.recording_state, RecordingState::Pending);
    }

    #[test]
    fn toggle_recording_when_pending_turns_off() {
        let mut app = test_app();
        app.playing_url = Some("http://a".to_string());
        app.recording_state = RecordingState::Pending;
        app.active_record_filepath = Some("capture.mp3".to_string());

        app.toggle_recording();

        assert_eq!(app.recording_state, RecordingState::Off);
        assert_eq!(app.active_record_filepath, None);
    }
}
''')


def write_overlays() -> None:
    write(APP_DIR / "overlays.rs", r'''
use super::*;

impl App {
    pub(super) fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
        if self.show_help {
            self.show_settings = false;
        }
    }

    pub(super) fn toggle_settings(&mut self) {
        self.show_settings = !self.show_settings;
        if self.show_settings {
            self.show_help = false;
        }
    }

    pub(super) fn cycle_layout(&mut self) {
        self.layout_mode = match self.layout_mode {
            LayoutMode::Split => LayoutMode::LeftOnly,
            LayoutMode::LeftOnly => LayoutMode::RightOnly,
            LayoutMode::RightOnly => LayoutMode::Split,
        };
    }

    pub(super) fn next_deck_page(&mut self) {
        self.active_deck_page = (self.active_deck_page + 1) % 2;
    }

    pub(super) fn toggle_visualizer_mode(&mut self) {
        self.visualizer_mode = (self.visualizer_mode + 1) % 3;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::favorites::Library;

    fn test_app() -> App {
        App::new(Library::in_memory(vec![]))
    }

    #[test]
    fn toggle_help_closes_settings() {
        let mut app = test_app();
        app.show_settings = true;

        app.toggle_help();

        assert!(app.show_help);
        assert!(!app.show_settings);
    }

    #[test]
    fn toggle_settings_closes_help() {
        let mut app = test_app();
        app.show_help = true;

        app.toggle_settings();

        assert!(app.show_settings);
        assert!(!app.show_help);
    }

    #[test]
    fn cycle_layout_wraps() {
        let mut app = test_app();

        app.cycle_layout();
        assert_eq!(app.layout_mode, LayoutMode::LeftOnly);
        app.cycle_layout();
        assert_eq!(app.layout_mode, LayoutMode::RightOnly);
        app.cycle_layout();
        assert_eq!(app.layout_mode, LayoutMode::Split);
    }

    #[test]
    fn next_deck_page_wraps() {
        let mut app = test_app();

        app.next_deck_page();
        assert_eq!(app.active_deck_page, 1);
        app.next_deck_page();
        assert_eq!(app.active_deck_page, 0);
    }
}
''')


def write_visualizer() -> None:
    write(APP_DIR / "visualizer.rs", r'''
use super::*;

impl App {
    /// Run Fast Fourier Transform (FFT) on the audio samples and update the spectrum peaks with gravity decay.
    pub fn update_visualizer(&mut self) {
        if self.playback != PlaybackState::Playing {
            // Gradually decay peaks when stopped/paused.
            for peak in &mut self.visualizer_peaks {
                *peak = (*peak * 0.82).max(0.0);
            }
            return;
        }

        // Extract raw samples from the circular buffer.
        let mut samples = Vec::new();
        if let Ok(buf) = self.sample_buffer.lock() {
            let n = buf.len();
            let window_size = 512;
            if n >= window_size {
                let start_idx = n - window_size;
                samples.extend(buf.iter().skip(start_idx).take(window_size).copied());
            } else {
                samples.extend(buf.iter().copied());
                while samples.len() < window_size {
                    samples.push(0.0);
                }
            }
        }

        if samples.is_empty() {
            return;
        }

        let n = samples.len();
        // 1. Apply Hanning window to minimize spectral leakage.
        let mut windowed = vec![0.0; n];
        for i in 0..n {
            let w = 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / (n - 1) as f32).cos());
            windowed[i] = samples[i] * w;
        }

        // 2. Perform Radix-2 Cooley-Tukey FFT.
        let fft_input: Vec<Complex> = windowed.into_iter().map(Complex::from_real).collect();
        let mut fft_output = vec![Complex::zero(); n];
        fft_rec(&fft_input, &mut fft_output);

        // 3. Map bins logarithmically to equal-width frequency bands.
        let num_bands = 40;
        let bins_count = n / 2;

        if self.visualizer_peaks.len() != num_bands {
            self.visualizer_peaks = vec![0.0; num_bands];
        }

        for x in 0..num_bands {
            let t = x as f32 / num_bands as f32;
            let min_bin = 1.0_f32;
            let max_bin = bins_count as f32;
            let bin_start_f = min_bin * (max_bin / min_bin).powf(t);
            let bin_end_f = min_bin * (max_bin / min_bin).powf((x + 1) as f32 / num_bands as f32);

            let start = (bin_start_f.floor() as usize).clamp(0, bins_count - 1);
            let end = (bin_end_f.ceil() as usize).clamp(start + 1, bins_count);

            let mut sum = 0.0;
            let mut count = 0;
            for bin in fft_output.iter().take(end).skip(start) {
                sum += bin.norm() / n as f32;
                count += 1;
            }
            let avg = if count > 0 { sum / count as f32 } else { 0.0 };

            // Equalize: boost higher frequency ranges dynamically since human hearing perceives them differently.
            let boost = 1.0 + (x as f32 / num_bands as f32) * 4.0;
            let compressed = avg.sqrt();
            let scaled = compressed * boost * 2.5; // Multiplier tuned for normalized & compressed avg.
            let target = scaled.clamp(0.0, 1.0);

            // 4. Gravity peak decay physics.
            let current = self.visualizer_peaks[x];
            if target > current {
                self.visualizer_peaks[x] = target; // Fast rise.
            } else {
                self.visualizer_peaks[x] = (current - 0.08).max(target).max(0.0);
                // Smooth fall.
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Complex {
    re: f32,
    im: f32,
}

impl Complex {
    fn new(re: f32, im: f32) -> Self {
        Self { re, im }
    }

    fn zero() -> Self {
        Self { re: 0.0, im: 0.0 }
    }

    fn from_real(re: f32) -> Self {
        Self { re, im: 0.0 }
    }

    fn add(self, other: Self) -> Self {
        Self {
            re: self.re + other.re,
            im: self.im + other.im,
        }
    }

    fn sub(self, other: Self) -> Self {
        Self {
            re: self.re - other.re,
            im: self.im - other.im,
        }
    }

    fn mul(self, other: Self) -> Self {
        Self {
            re: self.re * other.re - self.im * other.im,
            im: self.re * other.im + self.im * other.re,
        }
    }

    fn norm(self) -> f32 {
        (self.re * self.re + self.im * self.im).sqrt()
    }
}

fn fft_rec(input: &[Complex], output: &mut [Complex]) {
    let n = input.len();
    if n <= 1 {
        if n == 1 {
            output[0] = input[0];
        }
        return;
    }

    let mut even = vec![Complex::zero(); n / 2];
    let mut odd = vec![Complex::zero(); n / 2];
    for i in 0..n / 2 {
        even[i] = input[2 * i];
        odd[i] = input[2 * i + 1];
    }

    let mut even_fft = vec![Complex::zero(); n / 2];
    let mut odd_fft = vec![Complex::zero(); n / 2];
    fft_rec(&even, &mut even_fft);
    fft_rec(&odd, &mut odd_fft);

    for k in 0..n / 2 {
        let angle = -2.0 * std::f32::consts::PI * (k as f32) / (n as f32);
        let twiddle = Complex::new(angle.cos(), angle.sin());
        let t = twiddle.mul(odd_fft[k]);
        output[k] = even_fft[k].add(t);
        output[k + n / 2] = even_fft[k].sub(t);
    }
}
''')


def update_docs_and_changelog() -> None:
    write(DOC, r'''
# App architecture

DriftFM routes input through a small action pipeline:

- `src/action.rs` defines user/application actions.
- `src/event.rs` maps terminal events to actions.
- `src/app.rs` owns the public `App` type and re-exports public app state enums.
- `src/app/update.rs` routes actions to focused reducer modules.

## App modules

- `src/app/types.rs` owns public app enums and app-level constants.
- `src/app/lifecycle.rs` owns app construction, notices, and audio status polling.
- `src/app/selectors.rs` owns read-only station selectors and currently-playing lookup.
- `src/app/search.rs` owns search mode, debounce state, stale-response handling, and search confirmation.
- `src/app/playback.rs` owns playback commands and volume/mute state.
- `src/app/settings.rs` owns settings overlay behavior and action blocking while settings are open.
- `src/app/library.rs` owns library removal, genre navigation, and persistence notices.
- `src/app/recording.rs` owns recording toggle state.
- `src/app/overlays.rs` owns help/settings visibility, layout cycling, deck page cycling, and visualizer mode cycling.
- `src/app/visualizer.rs` owns FFT and visualizer peak updates.
- `src/app/idle.rs` owns platform-specific user idle detection for notifications.

## Refactor rules

- Keep `crate::app::App` as the public UI-facing state root.
- Keep `crate::app::{InputMode, SearchStatus, PlaybackState, RecordingState, LayoutMode, AppNotice}` re-exported.
- Keep `Action` and terminal event mapping separate from app reducers.
- Preserve settings overlay action blocking. This prevents background actions from leaking through settings.
- Keep behavior changes out of mechanical module-splitting PRs.
''')

    text = CHANGELOG.read_text(encoding="utf-8")
    if "App State Module Split" not in text:
        text = replace_once(
            text,
            "### Improved\n",
            "### Improved\n*   **App State Module Split**: Split the monolithic app reducer into focused modules for types, lifecycle, selectors, search, playback, settings, library, recording, overlays, visualizer, and platform idle helpers while preserving the public app API.\n*   **App Reducer Contract Tests**: Added state-level tests covering settings action blocking, search confirmation, playback state updates, overlay toggles, library removal, genre navigation, and recording toggles.\n",
        )
        CHANGELOG.write_text(text, encoding="utf-8")


def main() -> None:
    if not APP_RS.exists():
        raise SystemExit("src/app.rs not found")

    write_app_root()
    write_types()
    write_lifecycle()
    write_idle()
    write_selectors()
    write_update()
    write_playback()
    write_search()
    write_settings()
    write_library()
    write_recording()
    write_overlays()
    write_visualizer()
    update_docs_and_changelog()
    THIS_SCRIPT.unlink(missing_ok=True)

    print("Applied Phase 3 app state refactor.")


if __name__ == "__main__":
    main()
