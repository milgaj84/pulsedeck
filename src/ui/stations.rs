use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Tabs};

use super::theme;
use crate::app::{App, InputMode};

/// Render the station list.
/// Normal mode: your library. Search mode: API search results.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let visible = app.visible_stations();

    // ── Layout Split for normal mode genre folders ────────────────
    let (list_area, tabs_area) =
        if app.input_mode == InputMode::Normal && !app.library.available_genres.is_empty() {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Min(0)])
                .split(area);
            (chunks[1], Some(chunks[0]))
        } else {
            (area, None)
        };

    // ── Render Tabs (Genre folders) if present ────────────────────
    if let Some(t_area) = tabs_area {
        let tabs = Tabs::new(
            app.library
                .available_genres
                .iter()
                .map(|g| Span::raw(format!(" {} ", g)))
                .collect::<Vec<Span>>(),
        )
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(theme::border())
                .border_type(ratatui::widgets::BorderType::Rounded)
                .title(Span::styled(" ◇ Library Categories ", theme::title())),
        )
        .select(app.selected_genre_idx)
        .style(theme::dim())
        .highlight_style(
            Style::default()
                .fg(theme::highlight())
                .add_modifier(Modifier::BOLD),
        );

        frame.render_widget(tabs, t_area);
    }

    // ── Render Station List ───────────────────────────────────────
    let row_width = list_area.width.saturating_sub(4) as usize;
    let items: Vec<ListItem> = visible
        .iter()
        .enumerate()
        .map(|(idx, station)| {
            let is_playing = app.playing_url.as_ref() == Some(&station.url);
            let is_selected = app.selected == idx;
            let is_saved_search_result =
                app.input_mode == InputMode::Search && app.library.contains(&station.url);

            let cursor = station_cursor(is_playing, is_selected);
            let cursor_style = if is_playing {
                theme::playing()
            } else if is_selected {
                theme::cyan()
            } else {
                theme::dim()
            };

            let save_marker = if is_saved_search_result { "★ " } else { "  " };
            let save_style = if is_saved_search_result {
                Style::default().fg(theme::warm())
            } else {
                theme::dim()
            };

            let name_style = station_name_style(is_playing, is_selected, idx);
            let meta_style = if is_selected {
                Style::default()
                    .fg(theme::accent_secondary())
                    .add_modifier(Modifier::ITALIC)
            } else {
                theme::dim()
            };

            let meta = station_meta_label(
                &app.input_mode,
                station.genre.as_str(),
                station.country.as_str(),
                station.bitrate,
            );
            let meta_chip = format!(" {} ", meta);
            let fixed_width =
                visible_len(cursor) + visible_len(save_marker) + visible_len(&meta_chip) + 2;
            let name_width = row_width.saturating_sub(fixed_width).max(8);
            let name = truncate_with_ellipsis(station.name.as_str(), name_width);
            let padding = row_width.saturating_sub(
                visible_len(cursor)
                    + visible_len(save_marker)
                    + visible_len(&name)
                    + visible_len(&meta_chip),
            );

            ListItem::new(Line::from(vec![
                Span::styled(cursor, cursor_style),
                Span::styled(save_marker, save_style),
                Span::styled(name, name_style),
                Span::raw(" ".repeat(padding)),
                Span::styled(meta_chip, meta_style),
            ]))
        })
        .collect();

    let title_text = station_list_title(app, visible.len());

    let list = List::new(items)
        .block(
            Block::default()
                .title(Span::styled(title_text, theme::title()))
                .borders(Borders::ALL)
                .border_style(theme::border())
                .border_type(ratatui::widgets::BorderType::Rounded)
                .style(Style::default().bg(theme::bg())),
        )
        .highlight_style(theme::selected())
        .highlight_symbol("");

    let mut state = ListState::default();
    if !visible.is_empty() {
        state.select(Some(app.selected));
    }

    frame.render_stateful_widget(list, list_area, &mut state);
}

fn station_cursor(is_playing: bool, is_selected: bool) -> &'static str {
    match (is_playing, is_selected) {
        (true, true) => "▶ ",
        (false, true) => "▸ ",
        (true, false) => "● ",
        (false, false) => "  ",
    }
}

fn station_name_style(is_playing: bool, is_selected: bool, idx: usize) -> Style {
    if is_playing {
        theme::playing()
    } else if is_selected {
        theme::selected()
    } else if idx % 2 == 0 {
        theme::text()
    } else {
        theme::scanline()
    }
}

fn station_meta_label(input_mode: &InputMode, genre: &str, country: &str, bitrate: u32) -> String {
    let genre = empty_fallback(genre, "Other");
    let country = empty_fallback(country, "??");

    match input_mode {
        InputMode::Search => format!("{} · {} · {}k", genre, country, bitrate),
        InputMode::Normal => format!("{} · {}k", country, bitrate),
    }
}

fn station_list_title(app: &App, visible_count: usize) -> String {
    match app.input_mode {
        InputMode::Search => {
            if app.search_query.is_empty() {
                " 🔍 Search the airwaves ".to_string()
            } else if app.searching_api {
                format!(" 🔍 Tuning query \"{}\"... ", app.search_query)
            } else if visible_count == 0 {
                format!(" 🔍 No signal for \"{}\" ", app.search_query)
            } else {
                format!(" 🔍 Search Results ({}) ", visible_count)
            }
        }
        InputMode::Normal => {
            if visible_count == 0 {
                " ◇ Empty Library — press / to search ".to_string()
            } else {
                let genre_name = app
                    .library
                    .available_genres
                    .get(app.selected_genre_idx)
                    .map(|s| s.as_str())
                    .unwrap_or("All");
                format!(" ◇ Library / {} ({}) ", genre_name, visible_count)
            }
        }
    }
}

fn empty_fallback<'a>(value: &'a str, fallback: &'a str) -> &'a str {
    if value.trim().is_empty() {
        fallback
    } else {
        value.trim()
    }
}

fn truncate_with_ellipsis(value: &str, max_chars: usize) -> String {
    let value_len = visible_len(value);
    if value_len <= max_chars {
        return value.to_string();
    }

    if max_chars <= 1 {
        return "…".to_string();
    }

    let mut truncated = value.chars().take(max_chars - 1).collect::<String>();
    truncated.push('…');
    truncated
}

fn visible_len(value: &str) -> usize {
    value.chars().count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncation_keeps_short_names_unchanged() {
        assert_eq!(truncate_with_ellipsis("Nightride FM", 20), "Nightride FM");
    }

    #[test]
    fn truncation_adds_ellipsis_for_long_names() {
        assert_eq!(
            truncate_with_ellipsis("SomaFM Deep Space One", 10),
            "SomaFM De…"
        );
    }

    #[test]
    fn station_meta_search_includes_genre_country_and_bitrate() {
        assert_eq!(
            station_meta_label(&InputMode::Search, "Synthwave", "US", 128),
            "Synthwave · US · 128k"
        );
    }

    #[test]
    fn station_meta_normal_keeps_library_rows_compact() {
        assert_eq!(
            station_meta_label(&InputMode::Normal, "Synthwave", "US", 128),
            "US · 128k"
        );
    }
}
