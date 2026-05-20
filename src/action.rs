/// All possible actions in DriftFM.
/// These flow from event handlers → app.update() to drive state changes.
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

    /// Favorites
    ToggleFavorite,
    NextGenre,
    PrevGenre,

    /// Dynamic TUI Modules
    ToggleRightPanel,
    ToggleHelp,
    NextDeckPage,
    ToggleSettings,
    ToggleRecording,

    /// App lifecycle
    Tick,
    Quit,
}
