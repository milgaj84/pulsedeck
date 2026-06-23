use crate::app::{playback_error_action_hint, PlaybackState};
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

const FAULT_MESSAGE_MAX_CHARS: usize = 96;

/// Reserve one bottom row inside an overlay for critical engine faults.
///
/// Normal overlays keep their full content area. When playback is in an error
/// state, the overlay content shrinks by one row and the returned alert area
/// can be used to render a visible fault banner.
pub fn split_overlay_alert_area(area: Rect, playback: &PlaybackState) -> (Rect, Option<Rect>) {
    if engine_fault_message(playback).is_none() || area.height < 2 {
        return (area, None);
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(area);

    (chunks[0], Some(chunks[1]))
}

/// Render a critical engine fault banner into a previously reserved overlay row.
pub fn render_engine_fault_banner(frame: &mut Frame, area: Rect, playback: &PlaybackState) {
    if let Some(message) = engine_fault_message(playback) {
        let banner = Paragraph::new(engine_fault_line(message))
            .style(Style::default().bg(Color::Rgb(40, 0, 0)));
        frame.render_widget(banner, area);
    }
}

fn engine_fault_message(playback: &PlaybackState) -> Option<&str> {
    match playback {
        PlaybackState::Error(message) => Some(message.as_str()),
        _ => None,
    }
}

fn engine_fault_line(message: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            " ⚠ ENGINE CRITICAL DISCONNECT: ",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            compact_fault_message(message),
            Style::default().fg(Color::White),
        ),
        Span::styled(
            format!(" | {}", playback_error_action_hint(message)),
            Style::default().fg(Color::Yellow),
        ),
    ])
}

fn compact_fault_message(message: &str) -> String {
    let mut chars = message.chars();
    let compact = chars
        .by_ref()
        .take(FAULT_MESSAGE_MAX_CHARS)
        .collect::<String>();

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
    fn split_overlay_alert_area_reserves_row_for_engine_error() {
        let area = Rect::new(0, 0, 80, 20);
        let (content, alert) =
            split_overlay_alert_area(area, &PlaybackState::Error("HTTP 404".to_string()));

        assert_eq!(content.height, 19);
        assert_eq!(alert.unwrap().height, 1);
    }

    #[test]
    fn split_overlay_alert_area_keeps_full_area_without_error() {
        let area = Rect::new(0, 0, 80, 20);
        let (content, alert) = split_overlay_alert_area(area, &PlaybackState::Playing);

        assert_eq!(content, area);
        assert!(alert.is_none());
    }

    #[test]
    fn split_overlay_alert_area_keeps_tiny_area_full_height() {
        let area = Rect::new(0, 0, 80, 1);
        let (content, alert) =
            split_overlay_alert_area(area, &PlaybackState::Error("HTTP 404".to_string()));

        assert_eq!(content, area);
        assert!(alert.is_none());
    }

    #[test]
    fn compact_fault_message_truncates_long_errors() {
        let long = "x".repeat(FAULT_MESSAGE_MAX_CHARS + 4);

        assert_eq!(
            compact_fault_message(&long),
            format!("{}…", "x".repeat(FAULT_MESSAGE_MAX_CHARS))
        );
    }
}
