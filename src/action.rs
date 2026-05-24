/// All possible actions in DriftFM.
/// These flow from event handlers → app.update() to drive state changes.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// Navigation
    NextStation,
    PrevStation,

    /// Playback
    PlaySelected,
    TogglePause,
    Stop,

    /// Volume
    VolumeUp,
    VolumeDown,
    ToggleMute,

    /// Search
    EnterSearch,
    ExitSearch,
    SearchInput(char),
    SearchBackspace,
    SearchConfirm,

    /// Library management
    ToggleFavorite,
    NextGenre,
    PrevGenre,

    /// Dynamic TUI Modules
    CycleLayout,
    ToggleHelp,
    NextDeckPage,
    ToggleSettings,
    ToggleRecording,
    ToggleVisualizerMode,

    /// App lifecycle
    Tick,
    Quit,
}
