/// All possible actions in DriftFM.
/// These flow from event handlers → app.update() to drive state changes.
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
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
    ToggleFavoritesView,
    NextGenre,
    PrevGenre,

    /// Station management
    RefreshStations,

    /// Dynamic TUI Modules
    ToggleRightPanel,
    ToggleHelp,
    NextDeckPage,
    ToggleSettings,
    ToggleSettingOption,
    ToggleRecording,

    /// App lifecycle
    Tick,
    Quit,
}
