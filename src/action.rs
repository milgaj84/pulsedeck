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
    RetryStream,

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

    /// Command palette
    OpenCommandPalette,
    CommandPaletteConfirm,
    CommandPaletteClose,
    CommandPaletteInput(char),
    CommandPaletteBackspace,
    CommandPaletteNext,
    CommandPalettePrev,

    /// Library management
    RemoveLibrarySelection,
    UndoRemoveLibrarySelection,
    NextGenre,
    PrevGenre,

    /// Library filter
    EnterLibraryFilter,
    ExitLibraryFilter,
    LibraryFilterInput(char),
    LibraryFilterBackspace,
    LibraryFilterConfirm,

    /// Station preset slots
    PlaySlot(u8),
    AssignSlot(u8),

    /// Favorites
    ToggleFavorite,

    /// Number jump
    NumberJumpDigit(char),
    NumberJumpConfirm,
    #[allow(dead_code)]
    NumberJumpCancel,

    /// Dynamic TUI Modules
    CycleLayout,
    ToggleHelp,
    ToggleStationDetails,
    ToggleRecentTracks,
    TogglePlaybackDoctor,
    StepSettingForward,
    StepSettingBackward,
    ToggleSettings,
    CycleThemeSetting,
    ToggleStreamMetadata,
    RefreshLibraryMetadata,
    ToggleVisualizerMode,

    /// App lifecycle
    Tick,
    Quit,
    ToggleSleepTimer,
    SleepTimerIncrease,
    SleepTimerDecrease,
    SleepTimerPreset(u16),
    SleepTimerClear,
    ExportLibrary,
}
