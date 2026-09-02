use ratatui::prelude::*;

use crate::app::InputMode;
use crate::radio::health_classifier::{classify_health, HealthLevel};
use crate::ui::theme;

/// Return the number of decimal digits needed to display `n`.
/// Returns 1 for n == 0.
pub(super) fn digit_count(n: usize) -> usize {
    if n == 0 {
        return 1;
    }
    let mut count = 0;
    let mut value = n;
    while value > 0 {
        count += 1;
        value /= 10;
    }
    count
}

pub(super) fn station_cursor(is_playing: bool, is_selected: bool) -> &'static str {
    match (is_playing, is_selected) {
        (true, true) => "▶ ",
        (false, true) => "▸ ",
        (true, false) => "● ",
        (false, false) => "  ",
    }
}

/// Return a styled Span for the health dot indicator.
/// Healthy → green ●, Flaky → yellow ●, Failed → red ●, None → "  " for alignment.
pub(super) fn health_dot_span(station: &crate::radio::Station) -> Span<'static> {
    let now = now_timestamp_string();
    health_dot_span_at(station, &now)
}

pub(super) fn health_dot_span_at(station: &crate::radio::Station, now: &str) -> Span<'static> {
    match classify_health(&station.health, now) {
        Some(HealthLevel::Healthy) => Span::styled("● ", theme::health_healthy()),
        Some(HealthLevel::Flaky) => Span::styled("● ", theme::health_flaky()),
        Some(HealthLevel::Failed) => Span::styled("● ", theme::health_failed()),
        None => Span::raw("  "),
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

pub(super) fn station_name_style(is_playing: bool, is_selected: bool, idx: usize) -> Style {
    if is_playing {
        theme::playing()
    } else if is_selected {
        theme::selected()
    } else if idx.is_multiple_of(2) {
        theme::dim_row_even()
    } else {
        theme::dim_row_odd()
    }
}

pub(super) fn station_meta_label(
    input_mode: &InputMode,
    station: &crate::radio::Station,
) -> String {
    if *input_mode == InputMode::Search {
        search_station_meta_label(station)
    } else {
        library_station_meta_label(station)
    }
}

fn search_station_meta_label(station: &crate::radio::Station) -> String {
    join_non_empty([
        empty_fallback(&station.genre, "Other").to_string(),
        station_country_chip(station),
        bitrate_chip(station.bitrate),
        codec_chip(station),
        check_chip(station),
    ])
}

fn library_station_meta_label(station: &crate::radio::Station) -> String {
    join_non_empty([
        station_health_chip(station),
        station_country_chip(station),
        bitrate_chip(station.bitrate),
    ])
}

pub(super) fn station_health_chip(station: &crate::radio::Station) -> String {
    let failure_count = station.health.failure_count.unwrap_or(0);
    if failure_count > 0
        && station_health_failure_is_current(
            station.health.last_success_at.as_deref(),
            station.health.last_failure_at.as_deref(),
        )
    {
        return format!("⚠{failure_count}x");
    }
    if station.health.last_success_at.is_some() {
        return "✓".to_string();
    }
    String::new()
}

fn station_health_failure_is_current(success: Option<&str>, failure: Option<&str>) -> bool {
    match (success, failure) {
        (_, None) => false,
        (None, Some(_)) => true,
        (Some(success), Some(failure)) => match (success.parse::<u64>(), failure.parse::<u64>()) {
            (Ok(success), Ok(failure)) => failure > success,
            _ => failure > success,
        },
    }
}

fn station_country_chip(station: &crate::radio::Station) -> String {
    if !station.country_code.trim().is_empty() {
        station.country_code.trim().to_ascii_uppercase()
    } else {
        empty_fallback(&station.country, "??").to_string()
    }
}

fn bitrate_chip(bitrate: u32) -> String {
    if bitrate == 0 {
        "?k".to_string()
    } else {
        format!("{bitrate}k")
    }
}

pub(super) fn codec_chip(station: &crate::radio::Station) -> String {
    use crate::audio::PlaybackCapability;

    let codec = station.codec.trim();
    if codec.is_empty() {
        return "codec ?".to_string();
    }

    let capability = crate::audio::codec_capability(codec);
    match capability.capability {
        PlaybackCapability::Supported => codec.to_ascii_uppercase(),
        PlaybackCapability::Unknown => format!("{} ?", codec.to_ascii_uppercase()),
        PlaybackCapability::Unsupported => format!("{} !", codec.to_ascii_uppercase()),
    }
}

fn check_chip(station: &crate::radio::Station) -> String {
    match station.last_check_ok {
        Some(true) => "OK".to_string(),
        Some(false) => "down?".to_string(),
        None => String::new(),
    }
}

fn join_non_empty<const N: usize>(parts: [String; N]) -> String {
    parts
        .into_iter()
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>()
        .join(" · ")
}

fn empty_fallback<'a>(value: &'a str, fallback: &'a str) -> &'a str {
    if value.trim().is_empty() {
        fallback
    } else {
        value.trim()
    }
}
