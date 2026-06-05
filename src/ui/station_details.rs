use crate::app::App;
use crate::radio::Station;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use super::{critical, theme};

const MIN_DETAILS_WIDTH: u16 = 56;
const MIN_DETAILS_HEIGHT: u16 = 12;

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let popup_area = super::centered_rect(62, 48, area);

    if details_area_is_compact(popup_area) {
        frame.render_widget(Clear, popup_area);
        super::render_boundary_warning(
            frame,
            popup_area,
            "Station Details Too Compact",
            format!(
                "Expand terminal or close details (overlay: {}x{})",
                popup_area.width, popup_area.height
            ),
        );
        return;
    }

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(Span::styled(" ✦ Station Details ✦ ", theme::title()))
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

    let lines = station_detail_lines(app);
    let paragraph = Paragraph::new(lines).wrap(ratatui::widgets::Wrap { trim: true });
    frame.render_widget(paragraph, content_area);

    if let Some(alert_area) = alert_area {
        critical::render_engine_fault_banner(frame, alert_area, &app.playback);
    }
}

fn details_area_is_compact(area: Rect) -> bool {
    area.width < MIN_DETAILS_WIDTH || area.height < MIN_DETAILS_HEIGHT
}

fn station_detail_lines(app: &App) -> Vec<Line<'static>> {
    let Some(station) = app.selected_station() else {
        return vec![
            Line::from(Span::styled("No station selected", theme::title())),
            Line::from(""),
            Line::from(Span::styled("Press / to search for stations or switch categories with Tab.", theme::dim())),
            Line::from(""),
            close_hint(),
        ];
    };

    let saved = if app.library.contains(&station.url) { "Yes" } else { "No" };
    let now_playing = app
        .current_track
        .as_deref()
        .filter(|_| app.playing_url.as_ref() == Some(&station.url))
        .unwrap_or("N/A");

    vec![
        detail_row("Name", station.name.as_str()),
        detail_row("Genre", fallback(station.genre.as_str(), "Other")),
        detail_row("Country", fallback(station.country.as_str(), "??")),
        detail_row("Bitrate", bitrate_label(station.bitrate).as_str()),
        detail_row("Saved", saved),
        detail_row("Now playing", now_playing),
        detail_row("URL", station.url.as_str()),
        Line::from(""),
        close_hint(),
    ]
}

fn detail_row(label: &'static str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:>11}: "), theme::dim()),
        Span::styled(value.to_string(), theme::text()),
    ])
}

fn close_hint() -> Line<'static> {
    Line::from(vec![
        Span::styled(" i ", theme::cyan()),
        Span::styled("or", theme::dim()),
        Span::styled(" Esc/q ", theme::cyan()),
        Span::styled("closes this panel", theme::dim()),
    ])
}

fn bitrate_label(bitrate: u32) -> String {
    if bitrate == 0 {
        "Unknown".to_string()
    } else {
        format!("{bitrate}k")
    }
}

fn fallback<'a>(value: &'a str, fallback: &'a str) -> &'a str {
    if value.trim().is_empty() {
        fallback
    } else {
        value.trim()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn details_overlay_rejects_tiny_area() {
        assert!(details_area_is_compact(Rect::new(0, 0, 55, 12)));
        assert!(details_area_is_compact(Rect::new(0, 0, 56, 11)));
    }

    #[test]
    fn details_overlay_accepts_minimum_area() {
        assert!(!details_area_is_compact(Rect::new(0, 0, 56, 12)));
    }

    #[test]
    fn bitrate_label_handles_zero_as_unknown() {
        assert_eq!(bitrate_label(0), "Unknown");
        assert_eq!(bitrate_label(128), "128k");
    }

    #[test]
    fn fallback_trims_blank_values() {
        assert_eq!(fallback("", "Other"), "Other");
        assert_eq!(fallback(" Synthwave ", "Other"), "Synthwave");
    }
}
