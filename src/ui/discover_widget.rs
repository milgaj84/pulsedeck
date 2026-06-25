use crate::radio::Station;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem};

use super::theme;

const OVERLAY_WIDTH_PCT: u16 = 60;
const OVERLAY_HEIGHT_PCT: u16 = 70;
const TITLE: &str = " Discover — Similar Stations ";

/// Render the discover overlay on top of the main frame.
/// Only renders when `results` is non-empty.
pub fn render_discover_overlay(frame: &mut Frame, area: Rect, results: &[Station], cursor: usize) {
    if results.is_empty() {
        return;
    }

    let popup_area = super::centered_rect(OVERLAY_WIDTH_PCT, OVERLAY_HEIGHT_PCT, area);
    frame.render_widget(Clear, popup_area);

    let block = overlay_block();
    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let visible_height = inner.height as usize;
    let offset = scroll_offset(cursor, visible_height);

    let items = build_list_items(results, cursor, offset, visible_height);
    let list = List::new(items);
    frame.render_widget(list, inner);
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
    results: &[Station],
    cursor: usize,
    offset: usize,
    visible_height: usize,
) -> Vec<ListItem<'static>> {
    results
        .iter()
        .enumerate()
        .skip(offset)
        .take(visible_height)
        .map(|(i, station)| format_row(station, i == cursor))
        .collect()
}

fn format_row(station: &Station, is_selected: bool) -> ListItem<'static> {
    let text = format_station_line(station);
    let style = if is_selected {
        Style::default()
            .fg(theme::highlight())
            .add_modifier(Modifier::BOLD)
    } else {
        theme::text()
    };
    ListItem::new(Span::styled(text, style))
}

fn format_station_line(station: &Station) -> String {
    let country = if station.country_code.is_empty() {
        &station.country
    } else {
        &station.country_code
    };
    format!("{}  {}  {}", station.name, station.genre, country)
}

/// Compute scroll offset to keep `cursor` visible within viewport of `visible_height`.
/// Returns the first visible index (offset <= cursor < offset + visible_height).
pub fn scroll_offset(cursor: usize, visible_height: usize) -> usize {
    if visible_height == 0 {
        return 0;
    }
    cursor.saturating_sub(visible_height - 1)
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
        assert_eq!(format_station_line(&station), "Jazz FM  Jazz  US");
    }

    #[test]
    fn format_station_line_falls_back_to_country_when_no_code() {
        let station = station_with("Rock Radio", "Rock", "Germany", "");
        assert_eq!(format_station_line(&station), "Rock Radio  Rock  Germany");
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
                render_discover_overlay(frame, area, &[], 0);
            })
            .unwrap();

        // No panic = success; overlay not rendered for empty results
    }

    #[test]
    fn render_produces_output_for_non_empty_results() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        let results = vec![
            station_with("Station A", "Pop", "US", "US"),
            station_with("Station B", "Rock", "UK", "GB"),
        ];

        terminal
            .draw(|frame| {
                let area = frame.area();
                render_discover_overlay(frame, area, &results, 0);
            })
            .unwrap();

        let buffer = terminal.backend().buffer().clone();
        let content = buffer_to_string(&buffer);
        assert!(content.contains("Discover"), "Title should be present");
        assert!(content.contains("Station A"), "First station should render");
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
