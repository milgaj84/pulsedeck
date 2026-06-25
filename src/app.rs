mod command_palette;
mod discover;
mod favorites_actions;
mod idle;
mod library;
mod library_filter;
mod lifecycle;
pub mod mini_mode;
mod nav;
mod notifier;
mod number_jump_handler;
mod overlays;
mod persist;
mod playback;
mod playback_error;
mod playback_runtime;
mod recent;
mod reconnect;
mod search;
mod selectors;
mod settings;
mod settings_undo;
mod sleep_timer;
mod types;
mod ui_runtime;
mod ui_state;
mod update;
mod visualizer;
pub mod visualizer_mode;

use crate::config_toml::hot_reload::ConfigWatcher;
use crate::favorites::Library;
use crate::keybindings::watcher::KeybindingWatcher;
use crate::keybindings::KeybindingRegistry;
use crate::library_sort::SortMode;
use crate::number_jump::NumberJump;
use crate::radio::Station;
use crate::recommend::ScoredStation;
use crate::search_history::SearchHistoryRing;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub use command_palette::{command_label, CommandPaletteState, PaletteCommand};
pub use discover::DiscoverFetchRequest;
pub use lifecycle::NoticeState;
pub use nav::Navigation;
pub use overlays::{ActiveOverlay, Overlays};
pub use playback::PlaybackView;
pub use playback_error::playback_error_action_hint;
#[cfg(test)]
pub(crate) use playback_error::{classify_playback_error, PlaybackErrorKind};
pub use playback_runtime::{PlaybackOptions, PlaybackRuntime};
pub use reconnect::Reconnect;
pub use search::SearchState;
pub use settings_undo::SettingsUndoStack;
pub use sleep_timer::{SleepTimer, SLEEP_MAX_MINUTES, SLEEP_PRESETS, SLEEP_STEP_MINUTES};
pub use types::{
    AppNotice, DecoderState, DisplayMode, InputMode, LayoutMode, PlaybackDiagnostics,
    PlaybackState, SearchStatus, SettingRow,
};
pub use ui_runtime::UiRuntimeState;
pub use visualizer_mode::VisualizerMode;

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

    /// Library filter query text (active when InputMode::LibraryFilter).
    pub library_filter_query: String,

    /// Number jump accumulator.
    pub number_jump: NumberJump,

    metadata_refresh_pending: bool,
    metadata_refresh_running: bool,
    persist: persist::PersistFlags,

    /// Current library sort mode.
    pub sort_mode: SortMode,

    /// Custom keybinding registry (loaded from keybindings.json).
    pub keybinding_registry: KeybindingRegistry,

    /// Cooldown state for notification rate-limiting.
    pub(crate) notification_cooldown: lifecycle::NotificationCooldown,

    /// Discovery results from the recommendation engine.
    pub discover_results: Vec<ScoredStation>,

    /// Selection cursor index into `discover_results`.
    pub discover_cursor: usize,

    /// Pending discover fetch request (consumed by the runtime driver).
    pub discover_fetch_pending: Option<DiscoverFetchRequest>,

    /// Unified TOML configuration loaded at startup.
    pub config: crate::config_toml::AppConfig,

    /// Preserved unknown TOML keys for round-trip save operations.
    pub config_preserved: toml::Value,

    /// Resolved config directory for persisting settings to TOML.
    pub config_dir: Option<PathBuf>,

    /// Watches config file mtime for hot-reload on tick.
    pub config_watcher: ConfigWatcher,

    /// Watches keybindings JSON file for hot-reload on tick.
    pub keybinding_watcher: KeybindingWatcher,

    /// Search history ring for query recall via Up/Down arrows.
    pub search_history: SearchHistoryRing,

    /// Per-row undo buffer for the settings overlay.
    pub settings_undo: SettingsUndoStack,

    /// Test-only counter for how many notifications were dispatched.
    #[cfg(test)]
    pub(crate) notification_count: u32,
}
