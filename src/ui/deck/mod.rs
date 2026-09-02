use crate::app::LayoutMode;
use crate::ui::model::UiModel;
use crate::ui::theme;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};

mod cassette;
mod meta;
mod visualizer;

pub fn render(frame: &mut Frame, area: Rect, app: &UiModel<'_>) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::border())
        .border_type(ratatui::widgets::BorderType::Rounded)
        .padding(ratatui::widgets::Padding::horizontal(1))
        .title(Span::styled(" 📡 Signal Deck ", theme::title()));

    let inner_area = block.inner(area);
    frame.render_widget(block, area);

    let full_deck = app.layout_mode == LayoutMode::RightOnly;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(cassette::DECK_ART_HEIGHT),
            Constraint::Length(3), // now-playing hero
            Constraint::Length(1), // divider
            Constraint::Length(5),
            Constraint::Min(0),
        ])
        .split(inner_area);

    cassette::render_cassette(frame, chunks[0], app);
    render_now_playing_hero(frame, chunks[1], app);
    render_hero_divider(frame, chunks[2]);
    meta::render_meta_details(frame, chunks[3], app, full_deck);
    visualizer::render_oscilloscope(frame, chunks[4], app);
}

/// Render the now-playing hero section with bold station name and track title.
fn render_now_playing_hero(frame: &mut Frame, area: Rect, app: &UiModel<'_>) {
    let palette = theme::active_palette();
    let lines = match app.now_playing() {
        Some(station) => {
            let name_width = area.width.saturating_sub(2) as usize;
            let name = truncate_with_ellipsis(&station.name, name_width);
            let name_line = Line::from(Span::styled(
                format!(" {}", name),
                Style::default()
                    .fg(palette.accent)
                    .add_modifier(Modifier::BOLD),
            ));
            let track_line = match &app.player.current_track {
                Some(track) if !track.is_empty() => Line::from(Span::styled(
                    format!(" 🎵 {}", track),
                    Style::default().fg(palette.highlight),
                )),
                _ => Line::from(Span::styled(
                    " No track info",
                    Style::default().fg(palette.text_dim),
                )),
            };
            vec![name_line, track_line]
        }
        None => {
            vec![Line::from(Span::styled(
                " No station selected",
                Style::default().fg(palette.text_dim),
            ))]
        }
    };
    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, area);
}

/// Render a horizontal divider between the hero section and the visualizer.
fn render_hero_divider(frame: &mut Frame, area: Rect) {
    let width = area.width as usize;
    let divider = "─".repeat(width);
    let line = Paragraph::new(Line::from(Span::styled(divider, theme::dim())));
    frame.render_widget(line, area);
}

fn truncate_with_ellipsis(s: &str, max_width: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_width {
        s.to_string()
    } else if max_width > 0 {
        let truncated: String = chars[..max_width - 1].iter().collect();
        format!("{}…", truncated)
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_with_ellipsis_short_unchanged() {
        assert_eq!(truncate_with_ellipsis("Hello", 10), "Hello");
    }

    #[test]
    fn truncate_with_ellipsis_exact_length_unchanged() {
        assert_eq!(truncate_with_ellipsis("Hello", 5), "Hello");
    }

    #[test]
    fn truncate_with_ellipsis_over_length_truncated() {
        assert_eq!(truncate_with_ellipsis("Hello World", 5), "Hell…");
    }

    #[test]
    fn truncate_with_ellipsis_zero_width() {
        assert_eq!(truncate_with_ellipsis("Hello", 0), "");
    }

    #[test]
    fn truncate_with_ellipsis_one_width() {
        assert_eq!(truncate_with_ellipsis("Hello", 1), "…");
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// Property 8: Truncated name never exceeds available width, ends with '…' when truncated.
        #[test]
        fn prop_name_truncation(name in ".{0,200}", width in 0..200usize) {
            let result = truncate_with_ellipsis(&name, width);
            let result_chars: Vec<char> = result.chars().collect();
            prop_assert!(result_chars.len() <= width,
                "result {} chars exceeds width {}", result_chars.len(), width);
            if name.chars().count() > width && width > 0 {
                prop_assert!(result.ends_with('…'),
                    "truncated result should end with ellipsis");
            }
        }
    }
}
