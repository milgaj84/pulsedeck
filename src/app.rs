mod idle;
mod library;
mod lifecycle;
mod notifier;
mod overlays;
mod playback;
mod recording;
mod search;
mod selectors;
mod settings;
mod tape_archive;
mod types;
mod ui_state;
mod update;
mod visualizer;

use crate::audio::AudioEngine;
use crate::favorites::Library;
use crate::radio::Station;
use crate::tape_archive::TapeArchive;
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

pub use types::{
    AppNotice, InputMode, LayoutMode, PlaybackState, RecordingState, SearchStatus, SettingRow,
    TapePlaybackMode,
};

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
    normal_selected_snapshot: usize,
    search_selected_snapshot: usize,
    genre_selection_memory: HashMap<String, usize>,
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
    pub tape_archive: TapeArchive,
    pub tape_archive_scan_requested: bool,
    pub tape_archive_scan_inflight: bool,
    pub local_playback_path: Option<PathBuf>,
    pub pending_tape_delete: Option<PathBuf>,
    pub tape_playback_mode: TapePlaybackMode,

    pub show_settings: bool,
    pub selected_setting_idx: usize,

    pub recording_state: RecordingState,
    pub active_record_filepath: Option<String>,
    pub recording_started_at: Option<SystemTime>,
    pub recording_station_name: Option<String>,
    pub recording_station_url: Option<String>,
    pub recording_category: Option<String>,
    pub recording_recovery: Option<crate::recording_journal::RecordingRecovery>,
    pub recording_recovery_notice: Option<String>,
    pub buffer_percent: u8,
    pub buffer_seconds: u32,

    pub undo_history: VecDeque<(Station, usize, String)>,

    audio: AudioEngine,
    pub sample_buffer: Arc<Mutex<VecDeque<f32>>>,
    pub visualizer_mode: usize, // 0 = Spectrum, 1 = Oscilloscope, 2 = Simulated
    pub visualizer_peaks: Vec<f32>,
}
