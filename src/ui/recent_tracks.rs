use crate::ui::model::UiModel;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use super::{critical, theme};

const MIN_RECENT_WIDTH: u16 = 56;
const MIN_RECENT_HEIGHT: u16 = 12;
const MAX_VISIBLE_TRACKS: usize = 10;

pub fn render(frame: &mut Frame, area: Rect, app: &UiModel<'_>) {
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
        .title(Span::styled(
            recent_panel_title(app.library.settings.save_history),
            theme::title(),
        ))
        .borders(Borders::ALL)
        .border_style(
            Style::default()
                .fg(theme::accent_secondary())
                .add_modifier(Modifier::BOLD),
        )
        .border_type(ratatui::widgets::BorderType::Rounded)
        .style(theme::clear());

    let inner_area = block.inner(popup_area);
    let (content_area, alert_area) =
        critical::split_overlay_alert_area(inner_area, &app.player.state);
    frame.render_widget(block, popup_area);

    let paragraph =
        Paragraph::new(recent_track_lines(app)).wrap(ratatui::widgets::Wrap { trim: true });
    frame.render_widget(paragraph, content_area);

    if let Some(alert_area) = alert_area {
        critical::render_engine_fault_banner(frame, alert_area, &app.player.state);
    }
}

fn recent_area_is_compact(area: Rect) -> bool {
    area.width < MIN_RECENT_WIDTH || area.height < MIN_RECENT_HEIGHT
}

fn recent_panel_title(save_history: bool) -> &'static str {
    if save_history {
        " ✦ Listening History ✦ "
    } else {
        " ✦ Recent Tracks ✦ "
    }
}

fn format_relative_time(entry_at_str: &str) -> String {
    let Ok(entry_at) = entry_at_str.parse::<u64>() else {
        return "unknown".to_string();
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if now <= entry_at {
        return "just now".to_string();
    }
    let diff = now - entry_at;
    if diff < 60 {
        "just now".to_string()
    } else if diff < 3600 {
        format!("{}m ago", diff / 60)
    } else if diff < 86400 {
        format!("{}h ago", diff / 3600)
    } else {
        format!("{}d ago", diff / 86400)
    }
}

fn recent_track_lines(app: &UiModel<'_>) -> Vec<Line<'static>> {
    if app.library.settings.save_history {
        if app.history.is_empty() {
            return vec![
                Line::from(Span::styled("No track titles archived yet", theme::title())),
                Line::from(""),
                Line::from(Span::styled(
                    "PulseDeck will persist played track titles here across runs.",
                    theme::dim(),
                )),
                Line::from(""),
                close_hint(),
            ];
        }

        let mut lines = Vec::new();
        for (idx, entry) in app.history.recent(MAX_VISIBLE_TRACKS).enumerate() {
            let relative = format_relative_time(&entry.at);
            lines.push(Line::from(vec![
                Span::styled(format!("{:>2}. ", idx + 1), theme::dim()),
                Span::styled(entry.title.clone(), theme::text()),
                Span::styled(format!(" ({}, {})", entry.station, relative), theme::dim()),
            ]));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Listening History enabled; saved to history.json.",
            theme::dim(),
        )));
        lines.push(close_hint());
        return lines;
    }

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
        "Recent Tracks is session-only; nothing is saved while history is off.",
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

    #[test]
    fn recent_title_reflects_history_persistence() {
        assert_eq!(recent_panel_title(false), " ✦ Recent Tracks ✦ ");
        assert_eq!(recent_panel_title(true), " ✦ Listening History ✦ ");
    }
}
