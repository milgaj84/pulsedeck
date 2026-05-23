use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Tabs};

use crate::app::{App, InputMode};
use super::theme;

/// Render the station list.
/// Normal mode: your library. Search mode: API search results.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let visible = app.visible_stations();

    // ── Layout Split for normal mode genre folders ────────────────
    let (list_area, tabs_area) = if app.input_mode == InputMode::Normal && !app.library.available_genres.is_empty() {
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
                .title(Span::styled(" 📂 Categories ", theme::title())),
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
    let items: Vec<ListItem> = visible
        .iter()
        .enumerate()
        .map(|(idx, station)| {
            let is_playing = app.playing_url.as_ref() == Some(&station.url);
            let is_selected = app.selected == idx;

            // Cursor indicator
            let cursor = if is_playing && is_selected {
                Span::styled("▸ ", theme::playing())
            } else if is_selected {
                Span::styled("▸ ", theme::cyan())
            } else if is_playing {
                Span::styled("♫ ", theme::playing())
            } else {
                Span::styled("  ", theme::dim())
            };

            let name_style = if is_playing {
                theme::playing()
            } else if is_selected {
                theme::selected()
            } else if idx % 2 == 0 {
                theme::text()
            } else {
                theme::scanline()
            };

            let genre_style = if is_selected {
                Style::default().fg(theme::accent_secondary()).add_modifier(Modifier::ITALIC)
            } else {
                theme::dim()
            };

            let mut spans = vec![cursor];

            // In search mode, show ★ for stations already in your library
            if app.input_mode == InputMode::Search
                && app.library.contains(&station.url)
            {
                spans.push(Span::styled("★ ", Style::default().fg(theme::warm())));
            }

            let name_len = station.name.chars().count();
            let genre_len = station.genre.chars().count() + 4;
            let prefix_len = if app.input_mode == InputMode::Search && app.library.contains(&station.url) {
                4
            } else {
                2
            };

            let padding = (list_area.width as usize)
                .saturating_sub(name_len + genre_len + prefix_len + 4);

            let pad_str = " ".repeat(padding);

            spans.push(Span::styled(station.name.as_str(), name_style));
            spans.push(Span::raw(pad_str));
            spans.push(Span::styled(format!("[ {} ]", station.genre), genre_style));

            let line = Line::from(spans);
            ListItem::new(line)
        })
        .collect();

    let title_text = match app.input_mode {
        InputMode::Search => {
            if app.search_query.is_empty() {
                " 🔍 Type to search... ".to_string()
            } else if app.searching_api {
                format!(" 🔍 Searching \"{}\"... ", app.search_query)
            } else if visible.is_empty() {
                format!(" 🔍 No results for \"{}\" ", app.search_query)
            } else {
                format!(" 🔍 Results ({}) ", visible.len())
            }
        }
        InputMode::Normal => {
            if visible.is_empty() {
                " ⚡ No stations — press / to search ".to_string()
            } else {
                let genre_name = app.library.available_genres.get(app.selected_genre_idx)
                    .map(|s| s.as_str())
                    .unwrap_or("All");
                format!(" ⚡ Stations [Category: {}] ({}) ", genre_name, visible.len())
            }
        }
    };

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
        .highlight_symbol("▸ ");

    let mut state = ListState::default();
    if !visible.is_empty() {
        state.select(Some(app.selected));
    }

    frame.render_stateful_widget(list, list_area, &mut state);
}
