mod command_palette;
mod idle;
mod library;
mod lifecycle;
mod nav;
mod notifier;
mod overlays;
mod playback_error;
mod persist;
mod playback;
mod playback_runtime;
mod reconnect;
mod search;
mod selectors;
mod settings;
mod sleep_timer;
mod types;
mod ui_runtime;
mod ui_state;
mod update;
mod visualizer;

use crate::favorites::Library;
use crate::radio::Station;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

pub use command_palette::{command_label, CommandPaletteState, PaletteCommand};
pub use lifecycle::NoticeState;
pub use nav::Navigation;
pub use overlays::{ActiveOverlay, Overlays};
pub use playback::PlaybackView;
pub use playback_error::playback_error_action_hint;
pub use playback_runtime::PlaybackRuntime;
pub use reconnect::Reconnect;
pub use search::SearchState;
pub use sleep_timer::{SleepTimer, SLEEP_MAX_MINUTES, SLEEP_PRESETS, SLEEP_STEP_MINUTES};
pub use ui_runtime::UiRuntimeState;
pub use types::{
    AppNotice, DecoderState, InputMode, LayoutMode, PlaybackDiagnostics, PlaybackState,
    SearchStatus, SettingRow,
};

/// Core application state.
pub struct App {
    // Your station library (persisted to disk)
    pub library: Library,

    pub search: SearchState,
    pub history: crate::history::History,
    pub song_history: VecDeque<String>,
    pub undo_history: VecDeque<(Station, usize, String)>,

    pub ui: UiRuntimeState,
    pub playback: PlaybackRuntime,

    metadata_refresh_pending: bool,
    metadata_refresh_running: bool,
    persist: persist::PersistFlags,
}
