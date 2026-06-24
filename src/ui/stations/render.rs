use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Tabs};

use super::helpers::{
    digit_count, health_dot_span, station_cursor, station_meta_label, station_name_style,
};
use super::title::station_list_title;
use super::truncation::truncate_station_name;
use crate::app::InputMode;
use crate::ui::model::UiModel;
use crate::ui::theme;

/// Render the station list.
/// Normal mode: your library. Search mode: API search results.
pub fn render(frame: &mut Frame, area: Rect, app: &UiModel<'_>) {
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
        render_genre_tabs(frame, t_area, app);
    }

    // ── Render Station List ───────────────────────────────────────
    render_station_list(frame, list_area, app, visible);
}

fn render_genre_tabs(frame: &mut Frame, t_area: Rect, app: &UiModel<'_>) {
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
    .select(app.nav.selected_genre_idx)
    .style(theme::dim())
    .highlight_style(
        Style::default()
            .fg(theme::highlight())
            .add_modifier(Modifier::BOLD),
    );

    frame.render_widget(tabs, t_area);
}

fn render_station_list(
    frame: &mut Frame,
    list_area: Rect,
    app: &UiModel<'_>,
    visible: &[&crate::radio::Station],
) {
    let row_width = list_area.width.saturating_sub(4) as usize;
    let is_library_mode =
        app.input_mode == InputMode::Normal || app.input_mode == InputMode::LibraryFilter;
    let row_number_width = if is_library_mode {
        digit_count(visible.len())
    } else {
        0
    };
    let items: Vec<ListItem> = visible
        .iter()
        .enumerate()
        .map(|(idx, station)| {
            build_station_item(
                app,
                station,
                idx,
                row_width,
                row_number_width,
                is_library_mode,
            )
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
                .style(theme::clear()),
        )
        .highlight_style(theme::selected())
        .highlight_symbol("");

    let mut state = ListState::default();
    if !visible.is_empty() {
        state.select(Some(app.nav.selected));
    }

    frame.render_stateful_widget(list, list_area, &mut state);

    if should_render_empty_library_onboarding(app, visible.len()) {
        render_empty_library_onboarding(frame, list_area);
    }
}

fn build_station_item<'a>(
    app: &UiModel<'_>,
    station: &crate::radio::Station,
    idx: usize,
    row_width: usize,
    row_number_width: usize,
    is_library_mode: bool,
) -> ListItem<'a> {
    let is_playing = app.player.playing_url.as_ref() == Some(&station.url);
    let is_selected = app.nav.selected == idx;

    let cursor = station_cursor(is_playing, is_selected);
    let cursor_style = if is_playing {
        theme::playing()
    } else if is_selected {
        theme::cyan()
    } else {
        theme::dim()
    };

    let (save_marker, save_style) = save_marker_for(app, station, is_library_mode);

    let row_number_str = if is_library_mode {
        format!("{:>width$} ", idx + 1, width = row_number_width)
    } else {
        String::new()
    };
    let row_number_style = theme::dim();

    let name_style = station_name_style(is_playing, is_selected, idx);
    let meta_style = if is_selected {
        Style::default()
            .fg(theme::accent_secondary())
            .add_modifier(Modifier::ITALIC)
    } else {
        theme::dim()
    };

    let meta = station_meta_label(&app.input_mode, station);
    let meta_chip = format!(" {} ", meta);
    let health_dot = health_dot_span(station);
    let health_dot_width = 2;
    let fixed_width = crate::text::visible_len(cursor)
        + health_dot_width
        + crate::text::visible_len(save_marker)
        + crate::text::visible_len(&row_number_str)
        + crate::text::visible_len(&meta_chip)
        + 2;
    let name_width = row_width.saturating_sub(fixed_width).max(8);
    let search_query = if app.input_mode == InputMode::Search {
        Some(app.search.query.as_str())
    } else {
        None
    };
    let name = truncate_station_name(station.name.as_str(), search_query, name_width);
    let padding = row_width.saturating_sub(
        crate::text::visible_len(cursor)
            + health_dot_width
            + crate::text::visible_len(save_marker)
            + crate::text::visible_len(&row_number_str)
            + crate::text::visible_len(&name)
            + crate::text::visible_len(&meta_chip),
    );

    ListItem::new(Line::from(vec![
        Span::styled(cursor, cursor_style),
        health_dot,
        Span::styled(save_marker, save_style),
        Span::styled(row_number_str, row_number_style),
        Span::styled(name, name_style),
        Span::raw(" ".repeat(padding)),
        Span::styled(meta_chip, meta_style),
    ]))
}

fn save_marker_for(
    app: &UiModel<'_>,
    station: &crate::radio::Station,
    is_library_mode: bool,
) -> (&'static str, Style) {
    if app.input_mode == InputMode::Search {
        let is_saved = app.library.contains_station(station);
        if is_saved {
            ("★ ", Style::default().fg(theme::warm()))
        } else {
            ("  ", theme::dim())
        }
    } else if is_library_mode && app.favorites.contains(&station.url) {
        ("★ ", Style::default().fg(theme::warm()))
    } else {
        ("  ", theme::dim())
    }
}

pub(super) fn should_render_empty_library_onboarding(
    app: &UiModel<'_>,
    visible_count: usize,
) -> bool {
    app.input_mode == InputMode::Normal && visible_count == 0
}

fn render_empty_library_onboarding(frame: &mut Frame, area: Rect) {
    if area.width < 36 || area.height < 8 {
        return;
    }

    let card_area = crate::ui::centered_rect(82, 58, area);
    let lines = vec![
        Line::from(Span::styled("No saved stations yet", theme::title())),
        Line::from(""),
        onboarding_hint("/", "Search worldwide radio"),
        onboarding_hint("Space", "Audition a search result"),
        onboarding_hint("Enter", "Save + play highlighted result"),
        onboarding_hint(",", "Configure theme and audio output"),
        onboarding_hint("h", "Open full help"),
    ];

    let card = Paragraph::new(lines).alignment(Alignment::Center).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(theme::border())
            .border_type(ratatui::widgets::BorderType::Rounded)
            .style(theme::clear()),
    );

    frame.render_widget(card, card_area);
}

fn onboarding_hint(key: &'static str, label: &'static str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{key:>7}"), theme::cyan()),
        Span::styled("  ", theme::dim()),
        Span::styled(label, theme::dim()),
    ])
}
