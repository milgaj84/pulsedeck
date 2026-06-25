use crate::radio::health_classifier::{classify_health, confidence_label, HealthLevel};
use crate::ui::model::UiModel;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use super::{critical, theme};

const MIN_DETAILS_WIDTH: u16 = 56;
const MIN_DETAILS_HEIGHT: u16 = 12;

#[derive(Debug, Clone, PartialEq, Eq)]
struct DetailSection {
    title: &'static str,
    title_prefix: Option<(&'static str, Style)>,
    rows: Vec<DetailRow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DetailRow {
    label: &'static str,
    value: String,
}

pub fn render(frame: &mut Frame, area: Rect, app: &UiModel<'_>) {
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

fn station_detail_lines(app: &UiModel<'_>) -> Vec<Line<'static>> {
    if app.selected_station().is_none() {
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
    }

    section_lines(station_detail_sections(app))
}

fn station_detail_sections(app: &UiModel<'_>) -> Vec<DetailSection> {
    let Some(station) = app.selected_station() else {
        return Vec::new();
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
    let last_check = match station.last_check_ok {
        Some(true) => "OK",
        Some(false) => "Failing",
        None => "Unknown",
    };
    let station_uuid = station.station_uuid.as_deref().unwrap_or("N/A");

    vec![
        DetailSection {
            title: "Identity",
            title_prefix: None,
            rows: vec![
                detail_data("Name", station.name.as_str()),
                detail_data("UUID", station_uuid),
                detail_data("Saved", saved),
            ],
        },
        DetailSection {
            title: "Playback",
            title_prefix: None,
            rows: {
                let codec_label = codec_detail(station);
                vec![
                    detail_data("Stream", station.url.as_str()),
                    detail_data("Codec", codec_label.as_str()),
                    detail_data("Bitrate", bitrate_label(station.bitrate).as_str()),
                    detail_data("Now playing", now_playing),
                ]
            },
        },
        DetailSection {
            title: "Catalog",
            title_prefix: None,
            rows: vec![
                detail_data("Genre", fallback(station.genre.as_str(), "Other")),
                detail_data("Tags", metadata_list(&station.tags, "N/A").as_str()),
                detail_data("Country", fallback(station.country.as_str(), "??")),
                detail_data("Country ID", fallback(station.country_code.as_str(), "N/A")),
                detail_data("Language", fallback(station.language.as_str(), "N/A")),
                detail_data("Homepage", fallback(station.homepage.as_str(), "N/A")),
            ],
        },
        DetailSection {
            title: "Health",
            title_prefix: health_dot_prefix(station),
            rows: vec![
                detail_data("Last check", last_check),
                detail_data("Local health", local_health_label(station).as_str()),
                detail_data("Confidence", health_confidence_label(station).as_str()),
                detail_data("Votes", option_count_label(station.votes).as_str()),
                detail_data("Clicks", option_count_label(station.click_count).as_str()),
            ],
        },
    ]
}

fn section_lines(sections: Vec<DetailSection>) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for (idx, section) in sections.into_iter().enumerate() {
        if idx > 0 {
            lines.push(Line::from(""));
        }
        lines.push(section_title_line(&section));
        lines.extend(
            section
                .rows
                .into_iter()
                .map(|row| detail_row(row.label, row.value.as_str())),
        );
    }
    lines.push(Line::from(""));
    lines.push(close_hint());
    lines
}

fn health_dot_prefix(station: &crate::radio::Station) -> Option<(&'static str, Style)> {
    let now = now_timestamp_string();
    health_dot_prefix_at(station, &now)
}

fn health_dot_prefix_at(
    station: &crate::radio::Station,
    now: &str,
) -> Option<(&'static str, Style)> {
    match classify_health(&station.health, now) {
        Some(HealthLevel::Healthy) => Some(("● ", theme::health_healthy())),
        Some(HealthLevel::Flaky) => Some(("● ", theme::health_flaky())),
        Some(HealthLevel::Failed) => Some(("● ", theme::health_failed())),
        None => None,
    }
}

fn now_timestamp_string() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    secs.to_string()
}

fn section_title_line(section: &DetailSection) -> Line<'static> {
    match &section.title_prefix {
        Some((text, style)) => Line::from(vec![
            Span::styled(*text, *style),
            Span::styled(section.title, theme::title()),
        ]),
        None => Line::from(Span::styled(section.title, theme::title())),
    }
}

fn codec_detail(station: &crate::radio::Station) -> String {
    use crate::audio::PlaybackCapability;

    let codec = fallback(station.codec.as_str(), "N/A");
    let capability = crate::audio::codec_capability(&station.codec);

    match capability.capability {
        PlaybackCapability::Supported => format!("{codec} · playable"),
        PlaybackCapability::Unknown => format!("{codec} · playback will try"),
        PlaybackCapability::Unsupported => format!("{codec} · not playable yet"),
    }
}

fn detail_data(label: &'static str, value: &str) -> DetailRow {
    DetailRow {
        label,
        value: value.trim().to_string(),
    }
}

fn detail_row(label: &'static str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:>11}: "), theme::dim()),
        Span::styled(compact_detail_value(value), theme::text()),
    ])
}

fn compact_detail_value(value: &str) -> String {
    const MAX_CHARS: usize = 96;
    let value = value.trim();
    let mut chars = value.chars();
    let compact = chars.by_ref().take(MAX_CHARS).collect::<String>();
    if chars.next().is_some() {
        format!("{compact}…")
    } else {
        compact
    }
}

fn metadata_list(values: &[String], fallback: &str) -> String {
    if values.is_empty() {
        fallback.to_string()
    } else {
        values.join(", ")
    }
}

fn option_count_label(value: Option<u32>) -> String {
    value
        .map(|count| count.to_string())
        .unwrap_or_else(|| "N/A".to_string())
}

fn local_health_label(station: &crate::radio::Station) -> String {
    let health = &station.health;
    let failure_count = health.failure_count.unwrap_or(0);
    match (
        health.last_success_at.as_deref(),
        health.last_failure_at.as_deref(),
        failure_count,
    ) {
        (None, None, 0) => "N/A".to_string(),
        (Some(success), Some(failure), count)
            if failure_is_after_success(success, failure) && count > 0 =>
        {
            format!(
                "Last failed {failure} ({count}x): {}",
                fallback(&health.last_error_summary, "N/A")
            )
        }
        (Some(success), _, _) => format!("Last played {success}"),
        (None, Some(failure), count) if count > 0 => {
            format!(
                "Last failed {failure} ({count}x): {}",
                fallback(&health.last_error_summary, "N/A")
            )
        }
        _ => "N/A".to_string(),
    }
}

fn health_confidence_label(station: &crate::radio::Station) -> String {
    use crate::radio::health_classifier::calculate_confidence;
    let confidence = calculate_confidence(&station.health);
    if confidence == 0.0 {
        return "N/A".to_string();
    }
    format!("{:.0}% ({})", confidence * 100.0, confidence_label(confidence))
}

fn failure_is_after_success(success: &str, failure: &str) -> bool {
    match (success.parse::<u64>(), failure.parse::<u64>()) {
        (Ok(success), Ok(failure)) => failure > success,
        _ => failure > success,
    }
}

fn close_hint() -> Line<'static> {
    Line::from(vec![
        Span::styled(" i ", theme::cyan()),
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
    use crate::app::{App, PlaybackState};
    use crate::favorites::Library;
    use crate::radio::Station;

    fn station(name: &str, url: &str) -> Station {
        Station::basic(name, url, "Synthwave", "US", 128)
    }

    fn test_app_with(station: Station) -> App {
        App::new(Library::in_memory(vec![station]))
    }

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

    #[test]
    fn metadata_list_uses_fallback_or_joined_values() {
        assert_eq!(metadata_list(&[], "N/A"), "N/A");
        assert_eq!(
            metadata_list(&["jazz".to_string(), "live".to_string()], "N/A"),
            "jazz, live"
        );
    }

    #[test]
    fn option_count_label_formats_known_and_unknown_counts() {
        assert_eq!(option_count_label(None), "N/A");
        assert_eq!(option_count_label(Some(42)), "42");
    }

    #[test]
    fn compact_detail_value_truncates_long_metadata() {
        assert_eq!(compact_detail_value("  short  "), "short");
        assert!(compact_detail_value(&"x".repeat(120)).ends_with('…'));
    }

    #[test]
    fn detail_sections_group_expected_fields() {
        let mut station = station("A", "http://a");
        station.station_uuid = Some("uuid-123".to_string());
        station.codec = "MP3".to_string();
        station.tags = vec!["synthwave".to_string(), "night".to_string()];
        station.last_check_ok = Some(true);
        let mut app = test_app_with(station);
        app.playback.view.playing_url = Some("http://a".to_string());
        app.playback.view.state = PlaybackState::Playing;
        app.playback.view.current_track = Some("Artist - Track".to_string());

        let model = UiModel::from(&app);
        let sections = station_detail_sections(&model);

        assert_eq!(
            sections
                .iter()
                .map(|section| section.title)
                .collect::<Vec<_>>(),
            vec!["Identity", "Playback", "Catalog", "Health"]
        );
        assert!(sections[0].rows.iter().any(|row| row.label == "UUID"));
        assert!(sections[1]
            .rows
            .iter()
            .any(|row| row.label == "Now playing" && row.value == "Artist - Track"));
        assert!(sections[2].rows.iter().any(|row| row.label == "Tags"));
        assert!(sections[3]
            .rows
            .iter()
            .any(|row| row.label == "Local health"));
    }

    #[test]
    fn detail_sections_use_missing_metadata_fallbacks() {
        let mut station = station("A", "http://a");
        station.bitrate = 0;
        station.country.clear();
        let app = test_app_with(station);
        let model = UiModel::from(&app);

        let sections = station_detail_sections(&model);

        assert!(sections[1]
            .rows
            .iter()
            .any(|row| row.label == "Bitrate" && row.value == "Unknown"));
        assert!(sections[2]
            .rows
            .iter()
            .any(|row| row.label == "Country" && row.value == "??"));
        assert!(sections[3]
            .rows
            .iter()
            .any(|row| row.label == "Local health" && row.value == "N/A"));
    }

    #[test]
    fn local_health_prefers_newer_numeric_failure_over_older_success() {
        let mut station = station("A", "http://a");
        station.health.last_success_at = Some("99".to_string());
        station.health.last_failure_at = Some("100".to_string());
        station.health.failure_count = Some(2);
        station.health.last_error_summary = "timeout".to_string();

        assert_eq!(
            local_health_label(&station),
            "Last failed 100 (2x): timeout"
        );

        station.health.last_success_at = Some("101".to_string());
        assert_eq!(local_health_label(&station), "Last played 101");
    }

    #[test]
    fn health_dot_present_when_station_has_health_data() {
        let mut station = station("A", "http://a");
        station.health.last_success_at = Some("1700000000".to_string());
        station.health.failure_count = Some(0);

        let now = "1700000100"; // recent
        let prefix = health_dot_prefix_at(&station, now);
        assert!(
            prefix.is_some(),
            "dot should be present for healthy station"
        );
        assert_eq!(prefix.unwrap().0, "● ");
    }

    #[test]
    fn health_dot_absent_when_no_health_data() {
        let station = station("A", "http://a");
        // Default station has empty health (no success/failure timestamps)

        let now = "1700000000";
        let prefix = health_dot_prefix_at(&station, now);
        assert!(prefix.is_none(), "dot should be absent when no health data");
    }
}
