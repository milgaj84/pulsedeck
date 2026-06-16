use std::collections::HashMap;

#[derive(Default)]
pub struct Navigation {
    pub selected: usize,
    pub normal_selected_snapshot: usize,
    pub search_selected_snapshot: usize,
    pub genre_selection_memory: HashMap<String, usize>,
    pub selected_genre_idx: usize,
}
