/// Input mode determines how keyboard events are routed.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum InputMode {
    Normal,
    Search,
    /// Searchable action launcher; keys route through an isolated table so
    /// normal shortcuts never leak through while typing a command.
    CommandPalette,
    /// Modal sleep-timer overlay; keys route through an isolated table so they
    /// can never collide with Normal or Search bindings.
    SleepTimer,
    /// In-library substring filter mode; the user types to filter their library
    /// stations by name, genre, or tag.
    LibraryFilter,
}
