use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use super::theme;
use crate::app::{App, SearchStatus};

const SEARCH_DEBOUNCE_FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Render the search input bar.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let result_count = app.search_results.len();
    let selected_saved = app
        .search_results
        .get(app.selected)
        .map(|station| app.library.contains(&station.url))
        .unwrap_or(false);

    let api_indicator = match &app.search_status {
        SearchStatus::WaitingForInput => Span::styled("  Type 2+ chars to search", theme::dim()),
        SearchStatus::Debouncing { query } => {
            Span::styled(debounce_indicator_text(query, app.tick_count), theme::dim())
        }
        SearchStatus::Searching { query } => Span::styled(
            format!("  ◌ searching {}...", query),
            Style::default().fg(theme::warm()),
        ),
        SearchStatus::Ready { .. } if selected_saved => {
            Span::styled("  ★ Saved to library", Style::default().fg(theme::warm()))
        }
        SearchStatus::Ready { .. } => {
            Span::styled(format!("  {} found", result_count), theme::dim())
        }
        SearchStatus::Empty { query } => {
            Span::styled(format!("  No results for {}", query), theme::dim())
        }
        SearchStatus::Error { message, .. } => {
            let message = message
                .split('|')
                .next()
                .unwrap_or(message)
                .chars()
                .take(96)
                .collect::<String>();
            Span::styled(format!("  Search failed: {}", message), theme::error())
        }
    };

    let spans = vec![
        Span::styled(" 🔍 ", theme::neon()),
        Span::styled(&app.search_query, theme::cyan()),
        Span::styled("█", Style::default().fg(theme::highlight())),
        api_indicator,
    ];

    let line = Line::from(spans);

    let search_bar = Paragraph::new(vec![line]).style(Style::default().bg(theme::surface_color()));

    frame.render_widget(search_bar, area);
}

fn debounce_indicator_text(query: &str, tick_count: u64) -> String {
    format!(
        "  {} initializing query for {}...",
        search_debounce_frame(tick_count),
        query
    )
}

fn search_debounce_frame(tick_count: u64) -> &'static str {
    SEARCH_DEBOUNCE_FRAMES[tick_count as usize % SEARCH_DEBOUNCE_FRAMES.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_debounce_frame_wraps_through_spinner_frames() {
        assert_eq!(search_debounce_frame(0), "⠋");
        assert_eq!(search_debounce_frame(1), "⠙");
        assert_eq!(search_debounce_frame(9), "⠏");
        assert_eq!(search_debounce_frame(10), "⠋");
    }

    #[test]
    fn debounce_indicator_text_feels_active_without_saying_soon() {
        let text = debounce_indicator_text("lofi", 2);

        assert_eq!(text, "  ⠹ initializing query for lofi...");
        assert!(!text.contains("soon"));
    }
}
