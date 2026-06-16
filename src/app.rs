mod idle;
mod library;
mod lifecycle;
mod nav;
mod notifier;
mod overlays;
mod persist;
mod playback;
mod reconnect;
mod search;
mod selectors;
mod settings;
mod sleep_timer;
mod types;
mod ui_state;
mod update;
mod visualizer;

use crate::audio::AudioEngine;
use crate::favorites::Library;
use crate::radio::Station;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

pub use lifecycle::NoticeState;
pub use nav::Navigation;
pub use overlays::{ActiveOverlay, Overlays};
pub use playback::PlaybackView;
pub use reconnect::Reconnect;
pub use search::SearchState;
pub use sleep_timer::{SleepTimer, SLEEP_MAX_MINUTES, SLEEP_PRESETS, SLEEP_STEP_MINUTES};
pub use types::{AppNotice, InputMode, LayoutMode, PlaybackState, SearchStatus, SettingRow};

/// Core application state.
pub struct App {
    // Your station library (persisted to disk)
    pub library: Library,

    pub nav: Navigation,
    pub search: SearchState,
    pub player: PlaybackView,
    pub volume: u8, // 0-100
    pub muted: bool,
    pub should_quit: bool,
    pub notice: NoticeState,

    // Input mode
    pub input_mode: InputMode,

    pub tick_count: u64,

    pub layout_mode: LayoutMode,
    pub overlays: Overlays,
    pub song_history: VecDeque<String>,

    pub undo_history: VecDeque<(Station, usize, String)>,

    pub reconnect: Reconnect,
    pub sleep_timer: SleepTimer,
    pub history: crate::history::History,
    persist: persist::PersistFlags,
    audio: AudioEngine,
    pub sample_buffer: Arc<Mutex<VecDeque<f32>>>,
    pub visualizer_mode: usize, // 0 = Spectrum, 1 = Oscilloscope, 2 = Simulated
    pub visualizer_peaks: Vec<f32>,
}
