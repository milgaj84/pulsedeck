use crate::radio::Station;
use crate::recommend::ScoredStation;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};

use super::theme;

const OVERLAY_WIDTH_PCT: u16 = 60;
const OVERLAY_HEIGHT_PCT: u16 = 70;
const TITLE: &str = " Discover — Similar Stations ";

/// Render the discover overlay on top of the main frame.
/// Only renders when `results` is non-empty.
/// `explanation` is the pre-computed hint for the highlighted station.
pub fn render_discover_overlay(
    frame: &mut Frame,
    area: Rect,
    results: &[ScoredStation],
    cursor: usize,
    explanation: &str,
) {
    if results.is_empty() {
        return;
    }

    let popup_area = super::centered_rect(OVERLAY_WIDTH_PCT, OVERLAY_HEIGHT_PCT, area);
    frame.render_widget(Clear, popup_area);

    let block = overlay_block();
    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let list_area = Rect {
        height: inner.height.saturating_sub(1),
        ..inner
    };
    let hint_area = Rect {
        y: inner.y + list_area.height,
        height: 1,
        ..inner
    };

    let visible_height = list_area.height as usize;
    let offset = scroll_offset(cursor, visible_height);

    let items = build_list_items(results, cursor, offset, visible_height);
    let list = List::new(items);
    frame.render_widget(list, list_area);

    let hint = Paragraph::new(Span::styled(explanation, theme::dim()));
    frame.render_widget(hint, hint_area);
}

fn overlay_block() -> Block<'static> {
    Block::default()
        .title(Span::styled(TITLE, theme::title()))
        .borders(Borders::ALL)
        .border_style(
            Style::default()
                .fg(theme::accent_secondary())
                .add_modifier(Modifier::BOLD),
        )
        .border_type(ratatui::widgets::BorderType::Rounded)
        .style(theme::clear())
}

fn build_list_items(
    results: &[ScoredStation],
    cursor: usize,
    offset: usize,
    visible_height: usize,
) -> Vec<ListItem<'static>> {
    results
        .iter()
        .enumerate()
        .skip(offset)
        .take(visible_height)
        .map(|(i, scored)| format_row(&scored.station, scored.score, i == cursor))
        .collect()
}

fn format_row(station: &Station, score: u32, is_selected: bool) -> ListItem<'static> {
    let text = format_station_line(station, score);
    let style = if is_selected {
        Style::default()
            .fg(theme::highlight())
            .add_modifier(Modifier::BOLD)
    } else {
        theme::text()
    };
    ListItem::new(Span::styled(text, style))
}

fn format_station_line(station: &Station, score: u32) -> String {
    let country = if station.country_code.is_empty() {
        &station.country
    } else {
        &station.country_code
    };
    format!(
        "{}  {}  {}  ⚡{}",
        station.name, station.genre, country, score
    )
}

/// Compute scroll offset to keep `cursor` visible within viewport of `visible_height`.
/// Returns the first visible index (offset <= cursor < offset + visible_height).
pub fn scroll_offset(cursor: usize, visible_height: usize) -> usize {
    if visible_height == 0 {
        return 0;
    }
    cursor.saturating_sub(visible_height - 1)
}

/// Format a `ScoreExplanation` into a one-line hint string.
/// Returns `""` when no contributing factors exist.
pub fn format_explanation(explanation: &crate::recommend::ScoreExplanation) -> String {
    let parts: Vec<&str> = explanation
        .genres
        .iter()
        .chain(explanation.tags.iter())
        .chain(explanation.countries.iter())
        .map(|s| s.as_str())
        .collect();
    if parts.is_empty() {
        String::new()
    } else {
        format!("matches: {}", parts.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scroll_offset_zero_when_cursor_fits() {
        assert_eq!(scroll_offset(0, 10), 0);
        assert_eq!(scroll_offset(5, 10), 0);
        assert_eq!(scroll_offset(9, 10), 0);
    }

    #[test]
    fn scroll_offset_advances_when_cursor_exceeds_viewport() {
        assert_eq!(scroll_offset(10, 10), 1);
        assert_eq!(scroll_offset(15, 10), 6);
        assert_eq!(scroll_offset(20, 5), 16);
    }

    #[test]
    fn scroll_offset_zero_height_returns_zero() {
        assert_eq!(scroll_offset(5, 0), 0);
        assert_eq!(scroll_offset(0, 0), 0);
    }

    #[test]
    fn scroll_offset_height_one_equals_cursor() {
        assert_eq!(scroll_offset(0, 1), 0);
        assert_eq!(scroll_offset(3, 1), 3);
        assert_eq!(scroll_offset(99, 1), 99);
    }

    #[test]
    fn format_station_line_uses_country_code_when_available() {
        let station = station_with("Jazz FM", "Jazz", "United States", "US");
        assert_eq!(format_station_line(&station, 5), "Jazz FM  Jazz  US  ⚡5");
    }

    #[test]
    fn format_station_line_falls_back_to_country_when_no_code() {
        let station = station_with("Rock Radio", "Rock", "Germany", "");
        assert_eq!(
            format_station_line(&station, 3),
            "Rock Radio  Rock  Germany  ⚡3"
        );
    }

    #[test]
    fn format_station_line_includes_lightning_prefix_with_score() {
        let station = station_with("Test FM", "Pop", "France", "FR");
        let line = format_station_line(&station, 7);
        assert!(line.contains("⚡7"));
        assert_eq!(line, "Test FM  Pop  FR  ⚡7");
    }

    #[test]
    fn render_does_nothing_when_results_empty() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                let area = frame.area();
                render_discover_overlay(frame, area, &[], 0, "");
            })
            .unwrap();

        // No panic = success; overlay not rendered for empty results
    }

    #[test]
    fn render_produces_output_for_non_empty_results() {
        use crate::recommend::ScoredStation;
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        let results = vec![
            ScoredStation {
                station: station_with("Station A", "Pop", "US", "US"),
                score: 4,
            },
            ScoredStation {
                station: station_with("Station B", "Rock", "UK", "GB"),
                score: 3,
            },
        ];

        terminal
            .draw(|frame| {
                let area = frame.area();
                render_discover_overlay(frame, area, &results, 0, "matches: pop, US");
            })
            .unwrap();

        let buffer = terminal.backend().buffer().clone();
        let content = buffer_to_string(&buffer);
        assert!(content.contains("Discover"), "Title should be present");
        assert!(content.contains("Station A"), "First station should render");
    }

    #[test]
    fn format_explanation_with_all_factors() {
        use crate::recommend::ScoreExplanation;
        let explanation = ScoreExplanation {
            genres: vec!["jazz".to_string()],
            tags: vec!["smooth".to_string(), "chill".to_string()],
            countries: vec!["DE".to_string()],
        };
        assert_eq!(
            format_explanation(&explanation),
            "matches: jazz, smooth, chill, DE"
        );
    }

    #[test]
    fn format_explanation_empty_when_no_factors() {
        use crate::recommend::ScoreExplanation;
        let explanation = ScoreExplanation {
            genres: vec![],
            tags: vec![],
            countries: vec![],
        };
        assert_eq!(format_explanation(&explanation), "");
    }

    #[test]
    fn format_explanation_genres_only() {
        use crate::recommend::ScoreExplanation;
        let explanation = ScoreExplanation {
            genres: vec!["rock".to_string()],
            tags: vec![],
            countries: vec![],
        };
        assert_eq!(format_explanation(&explanation), "matches: rock");
    }

    #[test]
    fn render_displays_explanation_line() {
        use crate::recommend::ScoredStation;
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        let results = vec![ScoredStation {
            station: station_with("Station A", "Jazz", "DE", "DE"),
            score: 5,
        }];

        terminal
            .draw(|frame| {
                let area = frame.area();
                render_discover_overlay(frame, area, &results, 0, "matches: jazz, DE");
            })
            .unwrap();

        let buffer = terminal.backend().buffer().clone();
        let content = buffer_to_string(&buffer);
        assert!(
            content.contains("matches: jazz, DE"),
            "Explanation line should be visible"
        );
    }

    #[test]
    fn render_shows_empty_hint_when_no_explanation() {
        use crate::recommend::ScoredStation;
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        let results = vec![ScoredStation {
            station: station_with("Station A", "Jazz", "DE", "DE"),
            score: 5,
        }];

        terminal
            .draw(|frame| {
                let area = frame.area();
                render_discover_overlay(frame, area, &results, 0, "");
            })
            .unwrap();

        // No panic, empty explanation line doesn't crash
        let buffer = terminal.backend().buffer().clone();
        let content = buffer_to_string(&buffer);
        assert!(content.contains("Station A"));
    }

    // -- Helpers --

    fn station_with(name: &str, genre: &str, country: &str, code: &str) -> Station {
        let mut s = Station::basic(name, "http://test", genre, country, 128);
        s.country_code = code.to_string();
        s
    }

    fn buffer_to_string(buf: &ratatui::buffer::Buffer) -> String {
        let area = buf.area;
        let mut output = String::new();
        for y in area.y..area.y + area.height {
            for x in area.x..area.x + area.width {
                output.push_str(buf.cell((x, y)).map_or("", |c| c.symbol()));
            }
        }
        output
    }
}
