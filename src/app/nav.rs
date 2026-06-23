use std::collections::HashMap;

#[derive(Default)]
pub struct Navigation {
    pub selected: usize,
    pub normal_selected_snapshot: usize,
    pub search_selected_snapshot: usize,
    pub genre_selection_memory: HashMap<String, usize>,
    pub selected_genre_idx: usize,
    /// Snapshot of selection index before entering library filter mode.
    pub library_filter_selected_snapshot: usize,
    /// Snapshot of genre index before entering library filter mode.
    pub library_filter_genre_snapshot: usize,
    /// Active tab index in the help overlay.
    pub help_tab_index: usize,
}
