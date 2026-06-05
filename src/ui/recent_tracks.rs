use crate::app::App;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use super::{critical, theme};

const MIN_RECENT_WIDTH: u16 = 56;
const MIN_RECENT_HEIGHT: u16 = 12;
const MAX_VISIBLE_TRACKS: usize = 10;

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let popup_area = super::centered_rect(62, 52, area);

    if recent_area_is_compact(popup_area) {
        frame.render_widget(Clear, popup_area);
        super::render_boundary_warning(
            frame,
            popup_area,
            "Recent Tracks Too Compact",
            format!(
                "Expand terminal or close recent tracks (overlay: {}x{})",
                popup_area.width, popup_area.height
            ),
        );
        return;
    }

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(Span::styled(" ✦ Recent Tracks ✦ ", theme::title()))
        .borders(Borders::ALL)
        .border_style(
            Style::default()
                .fg(theme::accent_secondary())
                .add_modifier(Modifier::BOLD),
        )
        .border_type(ratatui::widgets::BorderType::Rounded)
        .style(theme::clear());

    let inner_area = block.inner(popup_area);
    let (content_area, alert_area) = critical::split_overlay_alert_area(inner_area, &app.playback);
    frame.render_widget(block, popup_area);

    let paragraph = Paragraph::new(recent_track_lines(app)).wrap(ratatui::widgets::Wrap { trim: true });
    frame.render_widget(paragraph, content_area);

    if let Some(alert_area) = alert_area {
        critical::render_engine_fault_banner(frame, alert_area, &app.playback);
    }
}

fn recent_area_is_compact(area: Rect) -> bool {
    area.width < MIN_RECENT_WIDTH || area.height < MIN_RECENT_HEIGHT
}

fn recent_track_lines(app: &App) -> Vec<Line<'static>> {
    if app.song_history.is_empty() {
        return vec![
            Line::from(Span::styled("No track titles heard yet", theme::title())),
            Line::from(""),
            Line::from(Span::styled(
                "PulseDeck will list stream-provided track titles here while you listen.",
                theme::dim(),
            )),
            Line::from(""),
            close_hint(),
        ];
    }

    let mut lines = Vec::new();
    for (idx, track) in app
        .song_history
        .iter()
        .rev()
        .take(MAX_VISIBLE_TRACKS)
        .enumerate()
    {
        lines.push(Line::from(vec![
            Span::styled(format!("{:>2}. ", idx + 1), theme::dim()),
            Span::styled(track.clone(), theme::text()),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Session-only list; nothing is recorded or archived.",
        theme::dim(),
    )));
    lines.push(close_hint());
    lines
}

fn close_hint() -> Line<'static> {
    Line::from(vec![
        Span::styled(" g ", theme::cyan()),
        Span::styled("or", theme::dim()),
        Span::styled(" Esc/q ", theme::cyan()),
        Span::styled("closes this panel", theme::dim()),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_overlay_rejects_tiny_area() {
        assert!(recent_area_is_compact(Rect::new(0, 0, 55, 12)));
        assert!(recent_area_is_compact(Rect::new(0, 0, 56, 11)));
    }

    #[test]
    fn recent_overlay_accepts_minimum_area() {
        assert!(!recent_area_is_compact(Rect::new(0, 0, 56, 12)));
    }
}
