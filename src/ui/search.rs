use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::app::App;
use super::theme;

/// Render the search input bar.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let result_count = app.search_results.len();

    let api_indicator = if app.searching_api {
        Span::styled("  ◌ searching...", Style::default().fg(theme::SUNSET_ORANGE))
    } else if result_count > 0 {
        Span::styled(format!("  {} found", result_count), theme::dim())
    } else {
        Span::styled("", theme::dim())
    };

    let spans = vec![
        Span::styled(" 🔍 ", theme::neon()),
        Span::styled(&app.search_query, theme::cyan()),
        Span::styled("█", Style::default().fg(theme::NEON_CYAN)),
        api_indicator,
    ];

    let line = Line::from(spans);

    let search_bar = Paragraph::new(vec![line])
        .style(Style::default().bg(Color::Rgb(15, 10, 30)));

    frame.render_widget(search_bar, area);
}
