/// All possible actions in PulseDeck.
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
    SearchAudition,

    /// Library management
    RemoveLibrarySelection,
    UndoRemoveLibrarySelection,
    NextGenre,
    PrevGenre,

    /// Dynamic TUI Modules
    CycleLayout,
    ToggleHelp,
    StepSettingForward,
    StepSettingBackward,
    NextDeckPage,
    ToggleSettings,
    ToggleRecording,
    ToggleVisualizerMode,
    KeepRecordingRecovery,
    TrashRecordingRecovery,
    DismissRecordingRecovery,

    /// Local tape archive
    RefreshTapeArchive,
    OpenSelectedTapeFolder,
    EnterTapeFilter,
    ExitTapeFilter,
    TapeFilterInput(char),
    TapeFilterBackspace,
    DeleteSelectedTape,
    ConfirmDeleteTape,
    CancelDeleteTape,

    /// App lifecycle
    Tick,
    Quit,
}
