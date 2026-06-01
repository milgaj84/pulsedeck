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

/// Rows shown in the settings overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingRow {
    Notifications,
    AutoplayLast,
    RecordingDir,
    KeepSnippets,
    MinSongDuration,
    Theme,
}

impl SettingRow {
    pub const ALL: [Self; 6] = [
        Self::Notifications,
        Self::AutoplayLast,
        Self::RecordingDir,
        Self::KeepSnippets,
        Self::MinSongDuration,
        Self::Theme,
    ];

    pub const COUNT: usize = Self::ALL.len();

    pub fn from_index(index: usize) -> Option<Self> {
        Self::ALL.get(index).copied()
    }

    pub fn index(self) -> usize {
        match self {
            Self::Notifications => 0,
            Self::AutoplayLast => 1,
            Self::RecordingDir => 2,
            Self::KeepSnippets => 3,
            Self::MinSongDuration => 4,
            Self::Theme => 5,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AppNotice {
    Info(String),
    Error(String),
}
