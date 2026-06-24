//! Mini mode renderer — compact 1-2 line display for constrained terminals.

use std::borrow::Cow;

use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use super::theme;
use crate::app::PlaybackState;
use crate::ui::model::UiModel;

const SEPARATOR: &str = " ";
const ELLIPSIS: char = '…';

/// Minimum terminal width for showing elapsed time in mini mode.
const MINI_ELAPSED_MIN_WIDTH: u16 = 40;

/// Render the mini mode compact display.
/// Single-line when height < 3 rows; two-line when height >= 3.
pub fn render(frame: &mut Frame, area: Rect, app: &UiModel<'_>) {
    let state_char = state_indicator(&app.player.state);
    let station_name = station_name_text(app);
    let track_title = track_title_text(app);
    let volume_text = volume_display(app);
    let elapsed_text = elapsed_display(app, area.width);
    let volume_flash_active = app.volume_flash_active;

    if area.height < 3 {
        let line = build_line(
            state_char,
            &station_name,
            track_title.as_deref(),
            elapsed_text.as_deref(),
            &volume_text,
            area.width as usize,
            volume_flash_active,
        );
        let paragraph = Paragraph::new(vec![line]).style(Style::default().bg(theme::bg()));
        frame.render_widget(paragraph, area);
    } else {
        render_two_line(
            frame,
            area,
            &TwoLineParams {
                indicator: state_char,
                station: &station_name,
                track: track_title.as_deref(),
                elapsed: elapsed_text.as_deref(),
                volume: &volume_text,
                volume_flash_active,
            },
        );
    }
}

/// Map PlaybackState to a single-character indicator.
pub fn state_indicator(state: &PlaybackState) -> char {
    match state {
        PlaybackState::Playing => '▶',
        PlaybackState::Paused => '⏸',
        PlaybackState::Stopped => '■',
        PlaybackState::Connecting => '◌',
        PlaybackState::FadingOut { .. } => '▶',
        PlaybackState::Error(_) => '✗',
    }
}

fn station_name_text(app: &UiModel<'_>) -> String {
    app.now_playing()
        .map(|s| s.name.clone())
        .unwrap_or_default()
}

fn track_title_text(app: &UiModel<'_>) -> Option<String> {
    match app.player.state {
        PlaybackState::Playing | PlaybackState::FadingOut { .. } | PlaybackState::Paused => {
            app.player.current_track.clone()
        }
        _ => None,
    }
}

fn volume_display(app: &UiModel<'_>) -> String {
    match app.player.state {
        PlaybackState::Playing | PlaybackState::FadingOut { .. } | PlaybackState::Paused => {
            format!("{}%", app.volume)
        }
        _ => String::new(),
    }
}

/// Return elapsed display text only if width >= threshold and elapsed data is available.
fn elapsed_display(app: &UiModel<'_>, width: u16) -> Option<String> {
    if width >= MINI_ELAPSED_MIN_WIDTH {
        app.elapsed_display.clone()
    } else {
        None
    }
}

/// Build the single-line layout with truncation.
fn build_line<'a>(
    indicator: char,
    station: &str,
    track: Option<&str>,
    elapsed: Option<&str>,
    volume: &str,
    max_width: usize,
    volume_flash_active: bool,
) -> Line<'a> {
    let parts = compose_parts(indicator, station, track, elapsed, volume, max_width);
    Line::from(styled_spans(&parts, volume_flash_active))
}

/// Internal data structure for a composed mini-mode line.
struct LineParts<'a> {
    indicator: char,
    station: Cow<'a, str>,
    track: Option<Cow<'a, str>>,
    elapsed: Option<Cow<'a, str>>,
    volume: Cow<'a, str>,
}

/// Compose and truncate parts to fit within max_width.
fn compose_parts<'a>(
    indicator: char,
    station: &'a str,
    track: Option<&'a str>,
    elapsed: Option<&'a str>,
    volume: &'a str,
    max_width: usize,
) -> LineParts<'a> {
    let indicator_width = 1 + SEPARATOR.len();
    let volume_width = if volume.is_empty() {
        0
    } else {
        SEPARATOR.len() + volume.len()
    };
    let elapsed_width = elapsed.map(|e| SEPARATOR.len() + e.len()).unwrap_or(0);
    let fixed_width = indicator_width + volume_width + elapsed_width;

    if max_width <= fixed_width {
        return LineParts {
            indicator,
            station: Cow::Borrowed(""),
            track: None,
            elapsed: elapsed.map(Cow::Borrowed),
            volume: Cow::Borrowed(volume),
        };
    }

    let available = max_width - fixed_width;
    let (station_cow, track_cow) = compose_station_track(station, track, available);

    LineParts {
        indicator,
        station: station_cow,
        track: track_cow,
        elapsed: elapsed.map(Cow::Borrowed),
        volume: Cow::Borrowed(volume),
    }
}

fn compose_station_track<'a>(
    station: &'a str,
    track: Option<&'a str>,
    available: usize,
) -> (Cow<'a, str>, Option<Cow<'a, str>>) {
    match track {
        Some(t) if !t.is_empty() => {
            let station_and_track_sep = SEPARATOR.len();
            if available <= station_and_track_sep + 1 {
                (cow_truncate(station, available), None)
            } else {
                let total_content = station.len() + station_and_track_sep + t.len();
                if total_content <= available {
                    (Cow::Borrowed(station), Some(Cow::Borrowed(t)))
                } else {
                    truncate_station_and_track(station, t, available, station_and_track_sep)
                }
            }
        }
        _ => (cow_truncate(station, available), None),
    }
}

fn truncate_station_and_track<'a>(
    station: &'a str,
    track: &'a str,
    available: usize,
    sep_width: usize,
) -> (Cow<'a, str>, Option<Cow<'a, str>>) {
    let station_space = station.len().min(available - sep_width - 1);
    let track_space = available - station_space - sep_width;
    if track_space <= 1 {
        (cow_truncate(station, available), None)
    } else {
        (
            cow_truncate(station, station_space),
            Some(cow_truncate(track, track_space)),
        )
    }
}

/// Truncate a string to fit within `max_chars` Unicode scalar positions.
fn truncate_str(s: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let char_count = s.chars().count();
    if char_count <= max_chars {
        s.to_string()
    } else if max_chars == 1 {
        ELLIPSIS.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars - 1).collect();
        format!("{truncated}{ELLIPSIS}")
    }
}

/// Return a `Cow::Borrowed` when the string fits, `Cow::Owned` when truncation occurs.
fn cow_truncate<'a>(s: &'a str, max_chars: usize) -> Cow<'a, str> {
    if max_chars == 0 {
        return Cow::Borrowed("");
    }
    let char_count = s.chars().count();
    if char_count <= max_chars {
        Cow::Borrowed(s)
    } else {
        Cow::Owned(truncate_str(s, max_chars))
    }
}

fn styled_spans(parts: &LineParts, volume_flash_active: bool) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    spans.push(Span::styled(parts.indicator.to_string(), theme::playing()));
    if !parts.station.is_empty() {
        spans.push(Span::styled(SEPARATOR.to_string(), theme::dim()));
        spans.push(Span::styled(parts.station.to_string(), theme::cyan()));
    }
    if let Some(ref track) = parts.track {
        spans.push(Span::styled(SEPARATOR.to_string(), theme::dim()));
        spans.push(Span::styled(track.to_string(), theme::dim()));
    }
    if let Some(ref elapsed) = parts.elapsed {
        spans.push(Span::styled(SEPARATOR.to_string(), theme::dim()));
        spans.push(Span::styled(elapsed.to_string(), theme::dim()));
    }
    if !parts.volume.is_empty() {
        let volume_style = if volume_flash_active {
            theme::cyan()
        } else {
            theme::text()
        };
        spans.push(Span::styled(SEPARATOR.to_string(), theme::dim()));
        spans.push(Span::styled(parts.volume.to_string(), volume_style));
    }
    spans
}

/// Parameters for the two-line mini-mode render.
struct TwoLineParams<'a> {
    indicator: char,
    station: &'a str,
    track: Option<&'a str>,
    elapsed: Option<&'a str>,
    volume: &'a str,
    volume_flash_active: bool,
}

/// Two-line layout: line 1 = indicator + station + elapsed + volume, line 2 = track title.
fn render_two_line(frame: &mut Frame, area: Rect, params: &TwoLineParams<'_>) {
    let width = area.width as usize;
    let line1 = build_line(
        params.indicator,
        params.station,
        None,
        params.elapsed,
        params.volume,
        width,
        params.volume_flash_active,
    );
    let line2 = match params.track {
        Some(t) if !t.is_empty() => {
            let truncated = truncate_str(t, width);
            Line::from(Span::styled(truncated, theme::dim()))
        }
        _ => Line::from(""),
    };

    let paragraph = Paragraph::new(vec![line1, line2]).style(Style::default().bg(theme::bg()));
    frame.render_widget(paragraph, area);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Compute the display width of a composed line (for testing).
    fn composed_line_width(
        indicator: char,
        station: &str,
        track: Option<&str>,
        elapsed: Option<&str>,
        volume: &str,
        max_width: usize,
    ) -> usize {
        let parts = compose_parts(indicator, station, track, elapsed, volume, max_width);
        let mut width = 1; // indicator
        if !parts.station.is_empty() {
            width += SEPARATOR.len() + parts.station.chars().count();
        }
        if let Some(ref track) = parts.track {
            width += SEPARATOR.len() + track.chars().count();
        }
        if let Some(ref elapsed) = parts.elapsed {
            width += SEPARATOR.len() + elapsed.len();
        }
        if !parts.volume.is_empty() {
            width += SEPARATOR.len() + parts.volume.len();
        }
        width
    }

    /// Check if the composed output contains the volume string (for testing).
    fn composed_contains_volume(
        indicator: char,
        station: &str,
        track: Option<&str>,
        elapsed: Option<&str>,
        volume: &str,
        max_width: usize,
    ) -> bool {
        let parts = compose_parts(indicator, station, track, elapsed, volume, max_width);
        parts.volume == volume
    }

    /// Check if the composed output contains the indicator char (for testing).
    fn composed_contains_indicator(
        indicator: char,
        station: &str,
        track: Option<&str>,
        elapsed: Option<&str>,
        volume: &str,
        max_width: usize,
    ) -> bool {
        let parts = compose_parts(indicator, station, track, elapsed, volume, max_width);
        parts.indicator == indicator
    }

    #[test]
    fn test_state_indicator_playing_returns_play_symbol() {
        assert_eq!(state_indicator(&PlaybackState::Playing), '▶');
    }

    #[test]
    fn test_state_indicator_paused_returns_pause_symbol() {
        assert_eq!(state_indicator(&PlaybackState::Paused), '⏸');
    }

    #[test]
    fn test_state_indicator_stopped_returns_stop_symbol() {
        assert_eq!(state_indicator(&PlaybackState::Stopped), '■');
    }

    #[test]
    fn test_state_indicator_connecting_returns_connecting_symbol() {
        assert_eq!(state_indicator(&PlaybackState::Connecting), '◌');
    }

    #[test]
    fn test_state_indicator_fading_out_returns_play_symbol() {
        assert_eq!(
            state_indicator(&PlaybackState::FadingOut {
                current_volume: 0.5
            }),
            '▶'
        );
    }

    #[test]
    fn test_state_indicator_error_returns_error_symbol() {
        assert_eq!(
            state_indicator(&PlaybackState::Error("fail".to_string())),
            '✗'
        );
    }

    #[test]
    fn test_truncation_preserves_indicator_and_volume() {
        let width = composed_line_width(
            '▶',
            "Very Long Station Name",
            Some("Very Long Track"),
            None,
            "80%",
            20,
        );
        assert!(width <= 20);
        assert!(composed_contains_indicator(
            '▶',
            "Very Long Station Name",
            Some("Very Long Track"),
            None,
            "80%",
            20
        ));
        assert!(composed_contains_volume(
            '▶',
            "Very Long Station Name",
            Some("Very Long Track"),
            None,
            "80%",
            20
        ));
    }

    #[test]
    fn test_truncation_track_truncated_first() {
        let parts = compose_parts(
            '▶',
            "Station",
            Some("A Really Long Track Title"),
            None,
            "80%",
            30,
        );
        assert_eq!(parts.station, "Station");
        assert!(parts.track.is_some());
        let track = parts.track.unwrap();
        assert!(track.ends_with('…') || track.len() <= 25);
    }

    #[test]
    fn test_truncation_station_truncated_when_track_gone() {
        let parts = compose_parts('▶', "Very Long Station", Some("Track"), None, "80%", 10);
        assert!(parts.station.chars().count() <= 4);
        assert!(parts.track.is_none() || parts.station.ends_with('…'));
    }

    #[test]
    fn test_compose_everything_fits() {
        let parts = compose_parts('▶', "FM", Some("Song"), None, "50%", 80);
        assert_eq!(parts.station, "FM");
        assert_eq!(parts.track.as_deref(), Some("Song"));
        assert_eq!(parts.volume, "50%");
    }

    #[test]
    fn test_compose_no_track() {
        let parts = compose_parts('■', "Station", None, None, "", 40);
        assert_eq!(parts.station, "Station");
        assert_eq!(parts.track, None);
        assert_eq!(parts.volume, "");
    }

    #[test]
    fn test_truncate_str_no_truncation_needed() {
        assert_eq!(truncate_str("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_str_exact_fit() {
        assert_eq!(truncate_str("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_str_truncates_with_ellipsis() {
        assert_eq!(truncate_str("hello world", 6), "hello…");
    }

    #[test]
    fn test_truncate_str_single_char_becomes_ellipsis() {
        assert_eq!(truncate_str("hello", 1), "…");
    }

    #[test]
    fn test_truncate_str_zero_width_returns_empty() {
        assert_eq!(truncate_str("hello", 0), "");
    }

    #[test]
    fn test_composed_line_never_exceeds_width() {
        let widths = [10, 15, 20, 30, 50, 80, 120];
        for w in widths {
            let actual = composed_line_width(
                '▶',
                "A Fairly Long Station Name Here",
                Some("Some Track Title That Is Also Quite Long"),
                None,
                "100%",
                w,
            );
            assert!(actual <= w, "Width {actual} exceeded max {w}");
        }
    }

    #[test]
    fn test_volume_always_present_when_provided() {
        let widths = [10, 15, 20, 30, 50, 80];
        for w in widths {
            assert!(
                composed_contains_volume(
                    '▶',
                    "Long Station Name",
                    Some("Long Track Title"),
                    None,
                    "80%",
                    w,
                ),
                "Volume missing at width {w}"
            );
        }
    }

    #[test]
    fn test_indicator_always_present() {
        let widths = [10, 15, 20, 30, 50, 80];
        for w in widths {
            assert!(
                composed_contains_indicator(
                    '▶',
                    "Long Station Name",
                    Some("Long Track Title"),
                    None,
                    "80%",
                    w,
                ),
                "Indicator missing at width {w}"
            );
        }
    }

    #[test]
    fn test_elapsed_included_when_width_at_least_60() {
        let parts = compose_parts('▶', "Station", Some("Track"), Some("03:45"), "80%", 80);
        assert_eq!(parts.elapsed.as_deref(), Some("03:45"));
    }

    #[test]
    fn test_elapsed_included_in_line_width_calculation() {
        let width_without = composed_line_width('▶', "FM", Some("Song"), None, "80%", 80);
        let width_with = composed_line_width('▶', "FM", Some("Song"), Some("03:45"), "80%", 80);
        assert_eq!(width_with - width_without, 6);
    }

    #[test]
    fn test_elapsed_none_has_no_effect_on_layout() {
        let parts = compose_parts('▶', "Station", Some("Track"), None, "80%", 60);
        assert_eq!(parts.elapsed, None);
    }

    #[test]
    fn test_composed_line_with_elapsed_never_exceeds_width() {
        let widths = [20, 30, 50, 60, 80, 120];
        for w in widths {
            let actual = composed_line_width(
                '▶',
                "A Fairly Long Station Name",
                Some("Long Track Title"),
                Some("1:23:45"),
                "100%",
                w,
            );
            assert!(actual <= w, "Width {actual} exceeded max {w} with elapsed");
        }
    }

    #[test]
    fn test_mini_elapsed_width_threshold_constant() {
        assert_eq!(MINI_ELAPSED_MIN_WIDTH, 40);
    }

    #[test]
    fn test_volume_style_highlighted_when_flash_active() {
        let parts = compose_parts('▶', "Station", None, None, "80%", 40);
        let spans_flash = styled_spans(&parts, true);
        let spans_normal = styled_spans(&parts, false);

        let volume_span_flash = spans_flash.last().unwrap();
        let volume_span_normal = spans_normal.last().unwrap();

        assert_eq!(volume_span_flash.style, theme::cyan());
        assert_eq!(volume_span_normal.style, theme::text());
        assert_ne!(volume_span_flash.style, volume_span_normal.style);
    }
}
