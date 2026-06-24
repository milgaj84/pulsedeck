use super::helpers::{
    codec_chip, digit_count, health_dot_span, health_dot_span_at, station_health_chip,
    station_meta_label,
};
use super::render::should_render_empty_library_onboarding;
use super::title::{library_filter_title, station_list_title};
use super::truncation::truncate_station_name;
use crate::app::{App, InputMode};
use crate::favorites::Library;
use crate::ui::model::UiModel;
use crate::ui::theme;

#[test]
fn truncation_keeps_short_names_unchanged() {
    assert_eq!(
        crate::text::truncate_with_ellipsis("Nightride FM", 20),
        "Nightride FM"
    );
}

#[test]
fn truncation_adds_ellipsis_for_long_names() {
    assert_eq!(
        crate::text::truncate_with_ellipsis("SomaFM Deep Space One", 10),
        "SomaFM De…"
    );
}

#[test]
fn codec_chip_marks_unsupported_codec() {
    let mut station = crate::radio::Station::basic("HLS", "http://hls", "Pop", "US", 128);
    station.codec = "HLS".to_string();
    assert_eq!(codec_chip(&station), "HLS !");
}

#[test]
fn codec_chip_marks_aac_as_supported() {
    let mut station = crate::radio::Station::basic("AAC", "http://aac", "Pop", "US", 128);
    station.codec = "AAC".to_string();
    assert_eq!(codec_chip(&station), "AAC");
}

#[test]
fn codec_chip_marks_supported_mp3() {
    let mut station = crate::radio::Station::basic("MP3", "http://mp3", "Pop", "US", 128);
    station.codec = "MP3".to_string();
    assert_eq!(codec_chip(&station), "MP3");
}

#[test]
fn codec_chip_marks_empty_as_unknown() {
    let mut station = crate::radio::Station::basic("X", "http://x", "Pop", "US", 128);
    station.codec = String::new();
    assert_eq!(codec_chip(&station), "codec ?");
}

#[test]
fn codec_chip_marks_unrecognized_as_unknown() {
    let mut station = crate::radio::Station::basic("X", "http://x", "Pop", "US", 128);
    station.codec = "WEIRD".to_string();
    assert_eq!(codec_chip(&station), "WEIRD ?");
}

#[test]
fn station_meta_search_includes_genre_country_and_bitrate() {
    let station = crate::radio::Station::basic("A", "http://a", "Synthwave", "US", 128);
    assert_eq!(
        station_meta_label(&InputMode::Search, &station),
        "Synthwave · US · 128k · codec ?"
    );
}

#[test]
fn station_meta_normal_keeps_library_rows_compact() {
    let station = crate::radio::Station::basic("A", "http://a", "Synthwave", "US", 128);
    assert_eq!(
        station_meta_label(&InputMode::Normal, &station),
        "US · 128k"
    );
}

#[test]
fn station_meta_normal_includes_health_badge() {
    let mut station = crate::radio::Station::basic("A", "http://a", "Synthwave", "US", 128);
    station.health.last_failure_at = Some("20".to_string());
    station.health.failure_count = Some(2);

    assert_eq!(
        station_meta_label(&InputMode::Normal, &station),
        "⚠2x · US · 128k"
    );

    station.health.last_success_at = Some("21".to_string());
    assert_eq!(station_health_chip(&station), "✓");
}

#[test]
fn station_health_badge_compares_numeric_timestamps() {
    let mut station = crate::radio::Station::basic("A", "http://a", "Synthwave", "US", 128);
    station.health.last_success_at = Some("99".to_string());
    station.health.last_failure_at = Some("100".to_string());
    station.health.failure_count = Some(1);

    assert_eq!(station_health_chip(&station), "⚠1x");
}

#[test]
fn empty_library_onboarding_only_renders_for_empty_normal_mode() {
    let app = App::new(Library::in_memory(vec![]));
    let model = UiModel::from(&app);

    assert!(should_render_empty_library_onboarding(&model, 0));
    assert!(!should_render_empty_library_onboarding(&model, 1));
}

#[test]
fn search_title_explains_preview_and_save_actions() {
    let mut app = App::new(Library::in_memory(vec![]));
    app.ui.input_mode = InputMode::Search;
    let model = UiModel::from(&app);

    let title = station_list_title(&model, 3);

    assert!(title.contains("Space preview"));
    assert!(title.contains("Enter save"));
}

#[test]
fn search_truncation_keeps_matching_suffix_visible() {
    let truncated = truncate_station_name(
        "SomaFM Deep Space One Underground 80s",
        Some("Underground"),
        18,
    );

    assert!(truncated.starts_with('…'));
    assert!(truncated.contains("Underground"));
}

#[test]
fn search_truncation_keeps_matching_tail_visible() {
    let truncated = truncate_station_name("SomaFM Deep Space One", Some("Space One"), 12);

    assert!(truncated.starts_with('…'));
    assert!(truncated.contains("Space One"));
}

#[test]
fn search_truncation_falls_back_when_query_is_blank() {
    assert_eq!(
        truncate_station_name("SomaFM Deep Space One", Some("   "), 10),
        "SomaFM De…"
    );
}

#[test]
fn search_truncation_falls_back_when_query_is_missing() {
    assert_eq!(
        truncate_station_name("SomaFM Deep Space One", Some("jazz"), 10),
        "SomaFM De…"
    );
}

#[test]
fn search_truncation_handles_tiny_width() {
    assert_eq!(truncate_station_name("SomaFM", Some("fm"), 1), "…");
}

#[test]
fn search_truncation_is_unicode_safe() {
    let truncated = truncate_station_name("São Paulo Rádio Underground", Some("rádio"), 10);

    assert!(truncated.contains("Rádio"));
}

#[test]
fn library_filter_title_shows_cursor_when_query_empty() {
    let title = library_filter_title("", 5);
    assert_eq!(title, " ◇ Library Filter: ▎ ");
}

#[test]
fn library_filter_title_shows_query_with_cursor() {
    let title = library_filter_title("soma", 3);
    assert_eq!(title, " ◇ Library Filter: soma▎ ");
}

#[test]
fn library_filter_title_shows_no_matches_indicator() {
    let title = library_filter_title("zzz", 0);
    assert_eq!(title, " ◇ Library Filter: zzz — no matches ");
}

#[test]
fn station_list_title_uses_filter_title_when_filter_active() {
    let mut app = App::new(Library::in_memory(vec![crate::radio::Station::basic(
        "A",
        "http://a",
        "Synthwave",
        "US",
        128,
    )]));
    app.ui.input_mode = InputMode::LibraryFilter;
    app.library_filter_query = "synth".to_string();
    let model = UiModel::from(&app);

    let title = station_list_title(&model, 1);
    assert!(title.contains("Library Filter"));
    assert!(title.contains("synth"));
    assert!(title.contains("▎"));
}

#[test]
fn station_list_title_shows_normal_when_filter_inactive() {
    let app = App::new(Library::in_memory(vec![crate::radio::Station::basic(
        "A",
        "http://a",
        "Synthwave",
        "US",
        128,
    )]));
    let model = UiModel::from(&app);

    let title = station_list_title(&model, 1);
    assert!(title.contains("Library"));
    assert!(!title.contains("Filter:"));
}

#[test]
fn station_list_title_shows_number_jump_indicator_when_active() {
    let mut app = App::new(Library::in_memory(vec![crate::radio::Station::basic(
        "A",
        "http://a",
        "Synthwave",
        "US",
        128,
    )]));
    app.number_jump.push_digit('4');
    app.number_jump.push_digit('2');
    let model = UiModel::from(&app);

    let title = station_list_title(&model, 1);
    assert!(title.contains("│ → 42"));
}

#[test]
fn station_list_title_hides_number_jump_indicator_when_inactive() {
    let app = App::new(Library::in_memory(vec![crate::radio::Station::basic(
        "A",
        "http://a",
        "Synthwave",
        "US",
        128,
    )]));
    let model = UiModel::from(&app);

    let title = station_list_title(&model, 1);
    assert!(!title.contains("│ →"));
}

#[test]
fn digit_count_returns_correct_width() {
    assert_eq!(digit_count(0), 1);
    assert_eq!(digit_count(1), 1);
    assert_eq!(digit_count(9), 1);
    assert_eq!(digit_count(10), 2);
    assert_eq!(digit_count(99), 2);
    assert_eq!(digit_count(100), 3);
    assert_eq!(digit_count(999), 3);
    assert_eq!(digit_count(1000), 4);
}

#[test]
fn favorite_indicator_shown_for_favorited_station() {
    let mut app = App::new(Library::in_memory(vec![
        crate::radio::Station::basic("A", "http://a", "Synthwave", "US", 128),
        crate::radio::Station::basic("B", "http://b", "Synthwave", "US", 128),
    ]));
    app.library.settings.favorites.toggle("http://a");
    let model = UiModel::from(&app);

    assert!(model.favorites.contains("http://a"));
    assert!(!model.favorites.contains("http://b"));
}

#[test]
fn row_number_format_single_digit_list() {
    let width = digit_count(5);
    assert_eq!(width, 1);
    let row_str = format!("{:>width$} ", 1, width = width);
    assert_eq!(row_str, "1 ");
    let row_str = format!("{:>width$} ", 5, width = width);
    assert_eq!(row_str, "5 ");
}

#[test]
fn row_number_format_double_digit_list() {
    let width = digit_count(12);
    assert_eq!(width, 2);
    let row_str = format!("{:>width$} ", 1, width = width);
    assert_eq!(row_str, " 1 ");
    let row_str = format!("{:>width$} ", 12, width = width);
    assert_eq!(row_str, "12 ");
}

#[test]
fn row_number_not_shown_in_search_mode() {
    let mut app = App::new(Library::in_memory(vec![crate::radio::Station::basic(
        "A",
        "http://a",
        "Synthwave",
        "US",
        128,
    )]));
    app.ui.input_mode = InputMode::Search;
    let model = UiModel::from(&app);

    let is_library_mode =
        model.input_mode == InputMode::Normal || model.input_mode == InputMode::LibraryFilter;
    assert!(!is_library_mode);
}

#[test]
fn row_number_shown_in_library_filter_mode() {
    let mut app = App::new(Library::in_memory(vec![crate::radio::Station::basic(
        "A",
        "http://a",
        "Synthwave",
        "US",
        128,
    )]));
    app.ui.input_mode = InputMode::LibraryFilter;
    let model = UiModel::from(&app);

    let is_library_mode =
        model.input_mode == InputMode::Normal || model.input_mode == InputMode::LibraryFilter;
    assert!(is_library_mode);
}

#[test]
fn health_dot_healthy_renders_green_dot() {
    let mut station = crate::radio::Station::basic("A", "http://a", "Pop", "US", 128);
    station.health.last_success_at = Some("2024-01-02T00:00:00Z".to_string());

    let span = health_dot_span_at(&station, "2024-01-03T00:00:00Z");

    assert_eq!(span.content, "● ");
    assert_eq!(span.style, theme::health_healthy());
}

#[test]
fn health_dot_flaky_renders_yellow_dot() {
    let mut station = crate::radio::Station::basic("A", "http://a", "Pop", "US", 128);
    station.health.last_success_at = Some("2024-01-01T00:00:00Z".to_string());
    station.health.last_failure_at = Some("2024-01-02T00:00:00Z".to_string());
    station.health.failure_count = Some(1);

    let span = health_dot_span_at(&station, "2024-01-03T00:00:00Z");

    assert_eq!(span.content, "● ");
    assert_eq!(span.style, theme::health_flaky());
}

#[test]
fn health_dot_failed_renders_red_dot() {
    let mut station = crate::radio::Station::basic("A", "http://a", "Pop", "US", 128);
    station.health.last_failure_at = Some("2024-01-02T00:00:00Z".to_string());
    station.health.failure_count = Some(3);

    let span = health_dot_span_at(&station, "2024-01-03T00:00:00Z");

    assert_eq!(span.content, "● ");
    assert_eq!(span.style, theme::health_failed());
}

#[test]
fn health_dot_none_renders_space_for_alignment() {
    let station = crate::radio::Station::basic("A", "http://a", "Pop", "US", 128);

    let span = health_dot_span_at(&station, "2024-01-03T00:00:00Z");

    assert_eq!(span.content, "  ");
}

#[test]
fn health_dot_alignment_consistent_across_all_levels() {
    let empty_station = crate::radio::Station::basic("A", "http://a", "Pop", "US", 128);
    let mut healthy_station = crate::radio::Station::basic("B", "http://b", "Pop", "US", 128);
    healthy_station.health.last_success_at = Some("2024-01-01T00:00:00Z".to_string());

    let empty_span = health_dot_span(&empty_station);
    let healthy_span = health_dot_span(&healthy_station);

    // Both are 2 chars wide for consistent alignment
    assert_eq!(empty_span.content.chars().count(), 2);
    assert_eq!(healthy_span.content.chars().count(), 2);
}
