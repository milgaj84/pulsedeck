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

pub use types::{
    AppNotice, InputMode, LayoutMode, PlaybackState, RecordingState, SearchStatus, SettingRow,
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

    pub undo_removed_station: Option<(Station, usize, String)>,

    audio: AudioEngine,
    pub sample_buffer: Arc<Mutex<VecDeque<f32>>>,
    pub visualizer_mode: usize, // 0 = Spectrum, 1 = Oscilloscope, 2 = Simulated
    pub visualizer_peaks: Vec<f32>,
}
