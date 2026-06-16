use crate::app::{App, SLEEP_MAX_MINUTES, SLEEP_PRESETS, SLEEP_STEP_MINUTES};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use super::{critical, theme};

const MIN_SLEEP_WIDTH: u16 = 56;
const MIN_SLEEP_HEIGHT: u16 = 14;

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let popup_area = super::centered_rect(58, 56, area);

    if sleep_area_is_compact(popup_area) {
        frame.render_widget(Clear, popup_area);
        super::render_boundary_warning(
            frame,
            popup_area,
            "Sleep Timer Too Compact",
            format!(
                "Expand terminal or close sleep timer (overlay: {}x{})",
                popup_area.width, popup_area.height
            ),
        );
        return;
    }

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(Span::styled(
            " \u{1F4A4} Sleep Timer \u{1F4A4} ",
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

    let paragraph = Paragraph::new(sleep_lines(app)).wrap(ratatui::widgets::Wrap { trim: true });
    frame.render_widget(paragraph, content_area);

    if let Some(alert_area) = alert_area {
        critical::render_engine_fault_banner(frame, alert_area, &app.player.state);
    }
}

fn sleep_area_is_compact(area: Rect) -> bool {
    area.width < MIN_SLEEP_WIDTH || area.height < MIN_SLEEP_HEIGHT
}

fn highlight_bold() -> Style {
    Style::default()
        .fg(theme::highlight())
        .add_modifier(Modifier::BOLD)
}

fn sleep_lines(app: &App) -> Vec<Line<'static>> {
    let now = std::time::Instant::now();
    let mut lines: Vec<Line<'static>> = Vec::new();

    let status = match app.sleep_timer.remaining(now) {
        Some(remaining) if app.sleep_timer.is_waiting_for_playback() => {
            let secs = remaining.as_secs();
            format!(
                "Armed for {} min  (starts with playback at {:02}:{:02})",
                app.sleep_timer.minutes(),
                secs / 60,
                secs % 60
            )
        }
        Some(remaining) => {
            let secs = remaining.as_secs();
            format!(
                "Armed for {} min  (stops in {:02}:{:02})",
                app.sleep_timer.minutes(),
                secs / 60,
                secs % 60
            )
        }
        None => "Off".to_string(),
    };
    lines.push(Line::from(vec![
        Span::styled("  Status: ", theme::cyan()),
        Span::styled(status, highlight_bold()),
    ]));
    lines.push(Line::from(""));

    lines.push(Line::from(Span::styled(
        format!("  \u{2191} / +    add {SLEEP_STEP_MINUTES} min"),
        theme::text(),
    )));
    lines.push(Line::from(Span::styled(
        format!("  \u{2193} / -    subtract {SLEEP_STEP_MINUTES} min"),
        theme::text(),
    )));
    lines.push(Line::from(Span::styled(
        format!("           wraps at {SLEEP_MAX_MINUTES} min back to off"),
        theme::dim(),
    )));
    lines.push(Line::from(""));

    lines.push(Line::from(Span::styled("  Presets:", theme::cyan())));
    let mut preset_spans: Vec<Span<'static>> = vec![Span::styled("  ", theme::dim())];
    for (idx, minutes) in SLEEP_PRESETS.iter().enumerate() {
        if idx > 0 {
            preset_spans.push(Span::styled("   ", theme::dim()));
        }
        preset_spans.push(Span::styled(format!("[{}] ", idx + 1), highlight_bold()));
        preset_spans.push(Span::styled(format!("{minutes}m"), theme::text()));
    }
    lines.push(Line::from(preset_spans));
    lines.push(Line::from(""));

    lines.push(Line::from(vec![
        Span::styled("  0 ", theme::cyan()),
        Span::styled("or", theme::dim()),
        Span::styled(" c ", theme::cyan()),
        Span::styled("turn off", theme::dim()),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  t ", theme::cyan()),
        Span::styled("/", theme::dim()),
        Span::styled(" Esc ", theme::cyan()),
        Span::styled("/", theme::dim()),
        Span::styled(" Enter ", theme::cyan()),
        Span::styled("close (changes apply live)", theme::dim()),
    ]));
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sleep_overlay_rejects_tiny_area() {
        assert!(sleep_area_is_compact(Rect::new(0, 0, 55, 14)));
        assert!(sleep_area_is_compact(Rect::new(0, 0, 56, 13)));
    }

    #[test]
    fn sleep_overlay_accepts_minimum_area() {
        assert!(!sleep_area_is_compact(Rect::new(0, 0, 56, 14)));
    }
}
