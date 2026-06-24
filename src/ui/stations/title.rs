use crate::app::InputMode;
use crate::ui::model::UiModel;

pub(super) fn station_list_title(app: &UiModel<'_>, visible_count: usize) -> String {
    if app.input_mode == InputMode::Search {
        search_title(app, visible_count)
    } else if app.library_filter_active {
        library_filter_title(app.library_filter_query, visible_count)
    } else if visible_count == 0 {
        " ◇ Empty Library — press / to search ".to_string()
    } else {
        normal_title(app, visible_count)
    }
}

fn search_title(app: &UiModel<'_>, visible_count: usize) -> String {
    if app.search.query.is_empty() {
        " 🔍 Search the airwaves · Space previews · Enter saves ".to_string()
    } else if app.search.searching_api {
        format!(" 🔍 Tuning {}... ", search_title_label(&app.search.query))
    } else if visible_count == 0 {
        format!(
            " 🔍 No signal for {} ",
            search_title_label(&app.search.query)
        )
    } else {
        format!(
            " 🔍 Search Results ({}) · {} · Space preview · Enter save ",
            visible_count,
            search_title_label(&app.search.query)
        )
    }
}

fn normal_title(app: &UiModel<'_>, visible_count: usize) -> String {
    let genre_name = app
        .library
        .available_genres
        .get(app.nav.selected_genre_idx)
        .map(|s| s.as_str())
        .unwrap_or("All");
    let base = format!(" ◇ Library / {} ({}) ", genre_name, visible_count);
    append_number_jump_indicator(&base, app)
}

fn append_number_jump_indicator(base: &str, app: &UiModel<'_>) -> String {
    if app.number_jump_active {
        format!("{}│ → {} ", base, app.number_jump_display)
    } else {
        base.to_string()
    }
}

pub(super) fn library_filter_title(query: &str, visible_count: usize) -> String {
    if query.is_empty() {
        " ◇ Library Filter: ▎ ".to_string()
    } else if visible_count == 0 {
        format!(" ◇ Library Filter: {} — no matches ", query)
    } else {
        format!(" ◇ Library Filter: {}▎ ", query)
    }
}

fn search_title_label(raw_query: &str) -> String {
    crate::text::truncate_with_ellipsis(
        &crate::radio::StationSearchQuery::parse(raw_query).display_label(),
        32,
    )
}
