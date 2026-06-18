use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use super::theme;
use crate::app::{App, SearchStatus};

const SEARCH_DEBOUNCE_FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Render the search input bar.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let result_count = app.search.results.len();
    let selected_saved = app
        .search
        .results
        .get(app.nav.selected)
        .map(|station| app.library.contains_station(station))
        .unwrap_or(false);

    let api_indicator = match &app.search.status {
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
        SearchStatus::Empty { query } => Span::styled(empty_search_hint(query), theme::dim()),
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
        SearchStatus::StaleResponseDiscarded {
            query,
            received_stale,
        } => Span::styled(
            stale_response_text(query, received_stale),
            Style::default().fg(theme::warm()),
        ),
    };

    let spans = vec![
        Span::styled(" 🔍 ", theme::neon()),
        Span::styled(&app.search.query, theme::cyan()),
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

fn stale_response_text(query: &str, received_stale: &str) -> String {
    format!(
        "  ⊘ discarded stale {}; {} is current",
        compact_search_label(received_stale),
        compact_search_label(query)
    )
}

fn empty_search_hint(query: &str) -> String {
    if query.contains(':') {
        format!(
            "  No results for {}; try a broader value",
            compact_search_label(query)
        )
    } else {
        "  No results; try tag:ambient, country:BA, lang:english, or a shorter name".to_string()
    }
}

fn compact_search_label(value: &str) -> String {
    const MAX_CHARS: usize = 24;
    let mut chars = value.chars();

    let compact = chars.by_ref().take(MAX_CHARS).collect::<String>();
    if chars.next().is_some() {
        format!("{compact}…")
    } else {
        compact
    }
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

    #[test]
    fn stale_response_text_reports_discarded_query() {
        let text = stale_response_text("jazz", "synth");

        assert_eq!(text, "  ⊘ discarded stale synth; jazz is current");
    }

    #[test]
    fn compact_search_label_truncates_long_queries() {
        assert_eq!(
            compact_search_label("abcdefghijklmnopqrstuvwxyz"),
            "abcdefghijklmnopqrstuvwx…"
        );
    }

    #[test]
    fn empty_search_hint_suggests_prefixes_for_plain_query() {
        assert_eq!(
            empty_search_hint("zzzz"),
            "  No results; try tag:ambient, country:BA, lang:english, or a shorter name"
        );
    }

    #[test]
    fn empty_search_hint_suggests_broadening_prefixed_query() {
        assert_eq!(
            empty_search_hint("tag:nope"),
            "  No results for tag:nope; try a broader value"
        );
    }
}
