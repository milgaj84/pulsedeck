use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::app::{App, SearchStatus};
use super::theme;

/// Render the search input bar.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let result_count = app.search_results.len();
    let selected_saved = app.search_results
        .get(app.selected)
        .map(|station| app.library.contains(&station.url))
        .unwrap_or(false);

    let api_indicator = match &app.search_status {
        SearchStatus::WaitingForInput => {
            Span::styled("  Type 2+ chars to search", theme::dim())
        }
        SearchStatus::Debouncing { query } => {
            Span::styled(format!("  Searching soon: {}", query), theme::dim())
        }
        SearchStatus::Searching { query } => {
            Span::styled(format!("  ◌ searching {}...", query), Style::default().fg(theme::warm()))
        }
        SearchStatus::Ready { .. } if selected_saved => {
            Span::styled("  ★ Saved to library", Style::default().fg(theme::warm()))
        }
        SearchStatus::Ready { .. } => {
            Span::styled(format!("  {} found", result_count), theme::dim())
        }
        SearchStatus::Empty { query } => {
            Span::styled(format!("  No results for {}", query), theme::dim())
        }
        SearchStatus::Error { .. } => {
            Span::styled("  Search failed. Check connection.", theme::error())
        }
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
