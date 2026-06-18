use crate::app::App;
use crate::radio::Station;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use super::{critical, theme};

const MIN_DETAILS_WIDTH: u16 = 64;
const MIN_DETAILS_HEIGHT: u16 = 16;

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
        .title(Span::styled(" Station Details ", theme::title()))
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
        Paragraph::new(station_detail_lines(app)).wrap(ratatui::widgets::Wrap { trim: true });
    frame.render_widget(paragraph, content_area);

    if let Some(alert_area) = alert_area {
        critical::render_engine_fault_banner(frame, alert_area, &app.player.state);
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
            Line::from(Span::styled(
                "Press / to search for stations or switch categories with Tab.",
                theme::dim(),
            )),
            Line::from(""),
            close_hint(),
        ];
    };

    let saved = if app.library.contains_station(station) {
        "Yes"
    } else {
        "No"
    };
    let now_playing = app
        .player
        .current_track
        .as_deref()
        .filter(|_| app.player.playing_url.as_ref() == Some(&station.url))
        .unwrap_or("N/A");

    let tags = tags_label(station);
    let country = country_label(station);
    let codec = codec_label(station);
    let checked = check_label(station);
    let popularity = popularity_label(station);
    let homepage = homepage_label(station);
    let uuid = uuid_label(station);
    let bitrate = bitrate_label(station.bitrate);

    vec![
        detail_row("Name", station.name.as_str()),
        detail_row("Tags", tags.as_str()),
        detail_row("Country", country.as_str()),
        detail_row("Language", fallback(station.language.as_str(), "Unknown")),
        detail_row("Codec", codec.as_str()),
        detail_row("Bitrate", bitrate.as_str()),
        detail_row("Checked", checked.as_str()),
        detail_row("Popularity", popularity.as_str()),
        detail_row("Saved", saved),
        detail_row("Now playing", now_playing),
        detail_row("Homepage", homepage.as_str()),
        detail_row("UUID", uuid.as_str()),
        detail_row("Stream", station.url.as_str()),
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
        Span::styled("closes this panel", theme::dim()),
    ])
}

fn tags_label(station: &Station) -> String {
    if !station.tags.is_empty() {
        station
            .tags
            .iter()
            .take(4)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ")
    } else {
        fallback(station.genre.as_str(), "Other").to_string()
    }
}

fn country_label(station: &Station) -> String {
    let country = fallback(station.country.as_str(), "Unknown");
    if station.country_code.trim().is_empty() {
        country.to_string()
    } else {
        format!(
            "{} ({})",
            country,
            station.country_code.trim().to_ascii_uppercase()
        )
    }
}

fn codec_label(station: &Station) -> String {
    fallback(station.codec.as_str(), "Unknown").to_string()
}

fn check_label(station: &Station) -> String {
    match station.last_check_ok {
        Some(true) => "Online at last check".to_string(),
        Some(false) => "Failed last check".to_string(),
        None => "Unknown".to_string(),
    }
}

fn popularity_label(station: &Station) -> String {
    match (station.votes, station.click_count) {
        (Some(votes), Some(clicks)) => format!("{votes} votes · {clicks} recent clicks"),
        (Some(votes), None) => format!("{votes} votes"),
        (None, Some(clicks)) => format!("{clicks} recent clicks"),
        (None, None) => "Unknown".to_string(),
    }
}

fn homepage_label(station: &Station) -> String {
    fallback(station.homepage.as_str(), "Unknown").to_string()
}

fn uuid_label(station: &Station) -> String {
    station
        .station_uuid
        .as_deref()
        .filter(|uuid| !uuid.trim().is_empty())
        .unwrap_or("Unknown")
        .to_string()
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
        assert!(details_area_is_compact(Rect::new(0, 0, 63, 16)));
        assert!(details_area_is_compact(Rect::new(0, 0, 64, 15)));
    }

    #[test]
    fn details_overlay_accepts_minimum_area() {
        assert!(!details_area_is_compact(Rect::new(0, 0, 64, 16)));
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

    #[test]
    fn country_label_includes_country_code_when_available() {
        let mut station = Station::basic("A", "http://a", "Radio", "Bosnia", 128);
        station.country_code = "ba".to_string();

        assert_eq!(country_label(&station), "Bosnia (BA)");
    }

    #[test]
    fn tags_label_uses_full_tags_when_available() {
        let mut station = Station::basic("A", "http://a", "Radio", "US", 128);
        station.tags = vec!["jazz".to_string(), "soul".to_string()];

        assert_eq!(tags_label(&station), "jazz, soul");
    }

    #[test]
    fn popularity_label_handles_votes_and_clicks() {
        let mut station = Station::basic("A", "http://a", "Radio", "US", 128);
        station.votes = Some(5);
        station.click_count = Some(20);

        assert_eq!(popularity_label(&station), "5 votes · 20 recent clicks");
    }
}
