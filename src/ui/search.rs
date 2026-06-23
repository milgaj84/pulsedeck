use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use super::theme;
use crate::app::SearchStatus;
use crate::radio::{
    explain_station_match, has_unknown_prefix, prefix_examples_inline, rank_explanation_label,
    SearchField, StationSearchQuery,
};
use crate::ui::model::UiModel;

const SEARCH_DEBOUNCE_FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Render the search input bar.
pub fn render(frame: &mut Frame, area: Rect, app: &UiModel<'_>) {
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
        SearchStatus::Ready { .. } => {
            let status = highlighted_result_explanation(app)
                .unwrap_or_else(|| format!("{} found", result_count));
            let style = if selected_saved {
                Style::default().fg(theme::warm())
            } else {
                theme::dim()
            };
            Span::styled(format!("  {status}"), style)
        }
        SearchStatus::Empty { query } => Span::styled(empty_search_hint(query), theme::dim()),
        SearchStatus::Error { message, .. } => Span::styled(
            format!("  Search failed: {}", public_search_error_message(message)),
            theme::error(),
        ),
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

fn highlighted_result_explanation(app: &UiModel<'_>) -> Option<String> {
    let station = app.search.results.get(app.nav.selected)?;
    let query = StationSearchQuery::parse(&app.search.query);
    let is_saved = app.library.contains_station(station);
    let explanation = explain_station_match(&query, station, is_saved);
    Some(compact_explanation_label(&rank_explanation_label(
        &explanation,
    )))
}

fn compact_explanation_label(value: &str) -> String {
    const MAX_CHARS: usize = 72;
    let mut chars = value.chars();
    let compact = chars.by_ref().take(MAX_CHARS).collect::<String>();
    if chars.next().is_some() {
        format!("{compact}…")
    } else {
        compact
    }
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
    let parsed = StationSearchQuery::parse(query);
    let value = compact_search_label(parsed.value());

    match parsed.field() {
        SearchField::Name if has_unknown_prefix(query) => format!(
            "  No results for {}; unknown prefix, treated as station name",
            compact_search_label(query)
        ),
        SearchField::Name => format!("  No results; {}", prefix_examples_inline()),
        SearchField::Tag => format!("  No tag results for {value}; try a broader genre"),
        SearchField::Country => {
            format!("  No country results for {value}; try a country code like country:BA")
        }
        SearchField::CountryCode => {
            format!("  No country results for {value}; try the full country name")
        }
        SearchField::Language => {
            format!("  No language results for {value}; try english, bosnian, or serbian")
        }
        SearchField::Codec => {
            format!("  No codec results for {value}; codec: filters metadata, playback is MP3-first")
        }
    }
}

fn public_search_error_message(message: &str) -> String {
    const MAX_CHARS: usize = 96;
    let trimmed = message.trim();
    let public = trimmed
        .split("Details:")
        .next()
        .unwrap_or(trimmed)
        .split('|')
        .next()
        .unwrap_or(trimmed)
        .trim();
    let mut chars = public.chars();
    let compact = chars.by_ref().take(MAX_CHARS).collect::<String>();
    if chars.next().is_some() {
        format!("{compact}…")
    } else {
        compact
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
    fn compact_explanation_label_truncates_safely() {
        let long = "Signal ".repeat(20);

        assert!(compact_explanation_label(&long).ends_with('…'));
        assert!(compact_explanation_label("Exact tag").contains("Exact tag"));
    }
}
