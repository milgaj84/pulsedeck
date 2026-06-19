pub(super) const SEARCH_MIN_CHARS: usize = 2;

/// Input mode determines how keyboard events are routed.
#[derive(Debug, Clone, PartialEq)]
pub enum InputMode {
    Normal,
    Search,
    /// Searchable action launcher; keys route through an isolated table so
    /// normal shortcuts never leak through while typing a command.
    CommandPalette,
    /// Modal sleep-timer overlay; keys route through an isolated table so they
    /// can never collide with Normal or Search bindings.
    SleepTimer,
}

/// Explicit search state for UI messages and stale-response handling.
#[derive(Debug, Clone, PartialEq)]
pub enum SearchStatus {
    WaitingForInput,
    Debouncing {
        query: String,
    },
    Searching {
        query: String,
    },
    Ready {
        query: String,
    },
    Empty {
        query: String,
    },
    Error {
        query: String,
        message: String,
    },
    StaleResponseDiscarded {
        query: String,
        received_stale: String,
    },
}

/// Playback state visible to the UI.
#[derive(Debug, Clone, PartialEq)]
pub enum PlaybackState {
    Stopped,
    Connecting,
    Playing,
    FadingOut { current_volume: f32 },
    Paused,
    Error(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PlaybackDiagnostics {
    pub output_device: String,
    pub metadata_enabled: bool,
    pub reconnect_attempts: u8,
    pub reconnect_limit: u8,
    pub buffer_percent: u8,
    pub buffer_seconds: u32,
    pub last_event: Option<String>,
    pub last_error: Option<String>,
    pub last_recovery: Option<String>,
    pub decoder_state: DecoderState,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum DecoderState {
    #[default]
    Idle,
    Connecting,
    Probing,
    Playing,
    Ended,
    Failed,
}

/// TUI Dashboard layout configurations.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum LayoutMode {
    Split,     // Mode 0: Station list on left (55%), Signal Deck on right (45%)
    LeftOnly,  // Mode 1: Closed Bento, Station list full width (100%)
    RightOnly, // Mode 2: Signal Deck full width (100%)
}

/// Rows shown in the settings overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingRow {
    Notifications,
    AutoplayLast,
    OutputDevice,
    Theme,
    StreamMetadata,
    SaveHistory,
}

impl SettingRow {
    pub const ALL: [Self; 6] = [
        Self::Notifications,
        Self::AutoplayLast,
        Self::OutputDevice,
        Self::Theme,
        Self::StreamMetadata,
        Self::SaveHistory,
    ];

    pub const COUNT: usize = Self::ALL.len();

    pub fn from_index(index: usize) -> Option<Self> {
        Self::ALL.get(index).copied()
    }

    pub fn index(self) -> usize {
        match self {
            Self::Notifications => 0,
            Self::AutoplayLast => 1,
            Self::OutputDevice => 2,
            Self::Theme => 3,
            Self::StreamMetadata => 4,
            Self::SaveHistory => 5,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AppNotice {
    Info(String),
    Error(String),
}
