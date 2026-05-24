use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::app::App;
use super::theme;

/// Render the search input bar.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let result_count = app.search_results.len();
    let trimmed_query_len = app.search_query.trim().chars().count();
    let selected_saved = app.search_results
        .get(app.selected)
        .map(|station| app.library.contains(&station.url))
        .unwrap_or(false);

    let api_indicator = if trimmed_query_len < 2 {
        Span::styled("  Type 2+ chars to search", theme::dim())
    } else if app.searching_api {
        Span::styled("  ◌ searching...", Style::default().fg(theme::warm()))
    } else if selected_saved {
        Span::styled("  ★ Saved to library", Style::default().fg(theme::warm()))
    } else if result_count > 0 {
        Span::styled(format!("  {} found", result_count), theme::dim())
    } else {
        Span::styled("  No results", theme::dim())
    };

    let spans = vec![
        Span::styled(" 🔍 ", theme::neon()),
        Span::styled(&app.search_query, theme::cyan()),
        Span::styled("█", Style::default().fg(theme::highlight())),
        api_indicator,
    ];

    let line = Line::from(spans);

    let search_bar = Paragraph::new(vec![line])
        .style(Style::default().bg(theme::surface_color()));

    frame.render_widget(search_bar, area);
}
