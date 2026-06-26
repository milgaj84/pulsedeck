/// All possible actions in PulseDeck.
/// These flow from event handlers → app.update() to drive state changes.
#[derive(Debug, Clone, PartialEq, Eq)]
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
    SearchHistoryUp,
    SearchHistoryDown,

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

    /// Library sort
    CycleSortMode,

    /// Display mode
    ToggleMiniMode,

    /// Keybindings
    ShowKeybindings,

    /// Discovery
    Discover,
    DiscoverNext,
    DiscoverPrev,
    DiscoverSelect,
    DiscoverDismiss,

    /// Settings undo
    UndoSetting,

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
