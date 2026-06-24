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
mod scrobble;
mod search;
mod selectors;
mod settings;
mod sleep_timer;
mod types;
mod ui_runtime;
mod ui_state;
mod update;
mod visualizer;
pub mod visualizer_mode;

use crate::favorites::Library;
use crate::keybindings::KeybindingRegistry;
use crate::number_jump::NumberJump;
use crate::radio::Station;
use crate::scrobble::tracker::ScrobbleTracker;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

pub use command_palette::{command_label, CommandPaletteState, PaletteCommand};
pub use lifecycle::NoticeState;
pub use nav::Navigation;
pub use overlays::{ActiveOverlay, Overlays};
pub use playback::PlaybackView;
pub use playback_error::playback_error_action_hint;
#[cfg(test)]
pub(crate) use playback_error::{classify_playback_error, PlaybackErrorKind};
pub use playback_runtime::PlaybackRuntime;
pub use reconnect::Reconnect;
pub use search::SearchState;
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

    /// Custom keybinding registry (loaded from keybindings.json).
    pub keybinding_registry: KeybindingRegistry,

    /// Cooldown state for notification rate-limiting.
    pub(crate) notification_cooldown: lifecycle::NotificationCooldown,

    /// Discovery results from the recommendation engine.
    pub discover_results: Vec<Station>,

    /// Scrobble state machine — ticked each app tick, receives track changes.
    pub scrobble_tracker: ScrobbleTracker,

    /// Unified TOML configuration loaded at startup.
    pub config: crate::config_toml::AppConfig,

    /// Preserved unknown TOML keys for round-trip save operations.
    pub config_preserved: toml::Value,

    /// Test-only counter for how many notifications were dispatched.
    #[cfg(test)]
    pub(crate) notification_count: u32,
}
