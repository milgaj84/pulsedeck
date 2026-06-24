use super::*;

fn station(name: &str, url: &str, genre: &str, country: &str, bitrate: u32) -> Station {
    Station::basic(name, url, genre, country, bitrate)
}

#[test]
fn test_resolve_parent_genre_synthwave_variants() {
    assert_eq!(resolve_parent_genre("Synthwave"), "Synthwave");
    assert_eq!(resolve_parent_genre("chillsynth"), "Synthwave");
    assert_eq!(resolve_parent_genre("darksynth"), "Synthwave");
    assert_eq!(resolve_parent_genre("Retrowave"), "Synthwave");
    assert_eq!(resolve_parent_genre("Neon Synthwave Beats"), "Synthwave");
}

#[test]
fn test_resolve_parent_genre_ambient_variants() {
    assert_eq!(resolve_parent_genre("Ambient"), "Ambient");
    assert_eq!(resolve_parent_genre("chillout"), "Ambient");
    assert_eq!(resolve_parent_genre("drone"), "Ambient");
    assert_eq!(resolve_parent_genre("space music"), "Ambient");
    assert_eq!(resolve_parent_genre("Drone Ambient"), "Ambient");
}

#[test]
fn test_resolve_parent_genre_rock() {
    assert_eq!(resolve_parent_genre("rock"), "Rock");
    assert_eq!(resolve_parent_genre("Metal"), "Rock");
    assert_eq!(resolve_parent_genre("guitar solo"), "Rock");
}

#[test]
fn test_resolve_parent_genre_vaporwave() {
    assert_eq!(resolve_parent_genre("vaporwave"), "Vaporwave");
    assert_eq!(resolve_parent_genre("plaza"), "Vaporwave");
    assert_eq!(resolve_parent_genre("synthpop"), "Vaporwave");
}

#[test]
fn test_resolve_parent_genre_other() {
    assert_eq!(resolve_parent_genre("classical"), "Other");
    assert_eq!(resolve_parent_genre("jazz"), "Other");
    assert_eq!(resolve_parent_genre(""), "Other");
}

#[test]
fn test_resolve_parent_genre_case_insensitive() {
    assert_eq!(resolve_parent_genre("SYNTHWAVE"), "Synthwave");
    assert_eq!(resolve_parent_genre("AMBIENT"), "Ambient");
    assert_eq!(resolve_parent_genre("ROCK"), "Rock");
    assert_eq!(resolve_parent_genre("VAPORWAVE"), "Vaporwave");
}

#[test]
fn settings_default_uses_default_audio_output() {
    assert_eq!(Settings::default().output_device_name, None);
}

#[test]
fn settings_deserializes_missing_audio_output_as_default() {
    let json = r#"{
            "notifications_enabled": true,
            "autoplay_last": false,
            "theme": "Retrowave"
        }"#;

    let settings: Settings = serde_json::from_str(json).unwrap();

    assert_eq!(settings.output_device_name, None);
}

#[test]
fn settings_deserializes_missing_station_slots_and_favorites_as_defaults() {
    let json = r#"{
            "notifications_enabled": true,
            "autoplay_last": false,
            "theme": "Retrowave"
        }"#;

    let settings: Settings = serde_json::from_str(json).unwrap();

    assert_eq!(settings.station_slots.get(1), None);
    assert!(settings.favorites.is_empty());
}

#[test]
fn newer_library_version_loads_with_warning() {
    let json = r#"{
            "version": 2,
            "stations": [{
                "name": "Future FM",
                "url": "http://future",
                "genre": "Synthwave",
                "country": "US",
                "bitrate": 128
            }],
            "settings": {
                "theme": "Terminal"
            }
        }"#;

    let (stations, settings, warning) = parse_library_file(json).unwrap();

    assert_eq!(stations.len(), 1);
    assert_eq!(stations[0].name, "Future FM");
    assert_eq!(settings.theme, "Terminal");
    assert!(warning
        .as_deref()
        .is_some_and(|message| message.contains("version 2")));
}

#[test]
fn library_file_load_normalizes_station_metadata_without_saved_station_mirror() {
    let json = r#"{
            "version": 1,
            "stations": [{
                "name": "Saved FM",
                "url": " HTTP://Saved/ ",
                "genre": "Radio",
                "country": " US ",
                "bitrate": 5000,
                "station_uuid": " uuid-1 ",
                "country_code": "ba",
                "tags": [" ambient ", "Ambient", "drone"],
                "language": " english ",
                "codec": " audio/mpeg ",
                "homepage": " https://example.com "
            }],
            "settings": {}
        }"#;

    let (stations, _, _) = parse_library_file(json).unwrap();
    let station = &stations[0];

    assert_eq!(station.name, "Saved FM");
    assert_eq!(station.url, " HTTP://Saved/ ");
    assert_eq!(station.genre, "Radio");
    assert_eq!(station.country, "US");
    assert_eq!(station.bitrate, 0);
    assert_eq!(station.station_uuid.as_deref(), Some("uuid-1"));
    assert_eq!(station.country_code, "BA");
    assert_eq!(
        station.tags,
        vec!["ambient".to_string(), "drone".to_string()]
    );
    assert_eq!(station.language, "english");
    assert_eq!(station.codec, "MP3");
    assert_eq!(station.homepage, "https://example.com");
}

#[test]
fn library_file_serializes_station_directly_and_omits_empty_optional_fields() {
    let file = LibraryFile {
        version: 1,
        stations: vec![station("Saved FM", "http://saved", "Radio", "US", 128)],
        settings: Settings::default(),
    };

    let json = serde_json::to_string(&file).unwrap();

    assert!(json.contains("\"stations\""));
    assert!(json.contains("\"Saved FM\""));
    assert!(!json.contains("station_uuid"));
    assert!(!json.contains("country_code"));
    assert!(!json.contains("health"));
}

#[test]
fn fallback_for_missing_library_preserves_seed_and_empty_policies() {
    let seed = vec![station("Seed", "http://seed", "Radio", "US", 128)];

    let (seed_stations, _, should_save_seed) =
        fallback_for_missing_library(&MissingLibraryPolicy::SeedAndSave(seed));
    let (empty_stations, _, should_save_empty) =
        fallback_for_missing_library(&MissingLibraryPolicy::Empty);

    assert_eq!(seed_stations.len(), 1);
    assert!(should_save_seed);
    assert!(empty_stations.is_empty());
    assert!(!should_save_empty);
}

#[test]
fn test_library_add_deduplicates() {
    let mut lib = Library {
        stations: vec![],
        available_genres: vec![],
        settings: Settings::default(),
        path: None,
        load_warnings: Vec::new(),
    };
    let station = station("Test", "http://test", "Synthwave", "US", 128);
    assert!(lib.add(station.clone()).unwrap());
    assert!(!lib.add(station).unwrap());
    assert_eq!(lib.stations.len(), 1);
}

#[test]
fn test_library_remove() {
    let mut lib = Library {
        stations: vec![station("Test", "http://test", "Synthwave", "US", 128)],
        available_genres: vec![],
        settings: Settings::default(),
        path: None,
        load_warnings: Vec::new(),
    };
    assert!(lib.remove("http://test").unwrap());
    assert!(!lib.remove("http://missing").unwrap());
    assert!(lib.stations.is_empty());
}

#[test]
fn test_library_contains() {
    let lib = Library {
        stations: vec![station("Test", "http://test", "Synthwave", "US", 128)],
        available_genres: vec![],
        settings: Settings::default(),
        path: None,
        load_warnings: Vec::new(),
    };
    assert!(lib.contains("http://test"));
    assert!(!lib.contains("http://missing"));
}

#[test]
fn contains_matches_normalized_station_url() {
    let lib = Library::in_memory(vec![station(
        "Test",
        " HTTP://STREAM/ ",
        "Synthwave",
        "US",
        128,
    )]);

    assert!(lib.contains("http://stream"));
}

#[test]
fn remove_matches_normalized_station_url() {
    let mut lib = Library::in_memory(vec![station(
        "Test",
        " HTTP://STREAM/ ",
        "Synthwave",
        "US",
        128,
    )]);

    assert!(lib.remove("http://stream").unwrap());
    assert!(lib.stations.is_empty());
}

#[test]
fn test_rebuild_genres() {
    let mut lib = Library {
        stations: vec![
            station("A", "http://a", "Synthwave", "US", 128),
            station("B", "http://b", "Ambient", "US", 128),
        ],
        available_genres: vec![],
        settings: Settings::default(),
        path: None,
        load_warnings: Vec::new(),
    };
    lib.rebuild_genres();
    assert_eq!(lib.available_genres[0], "All");
    assert!(lib.available_genres.contains(&"Synthwave".to_string()));
    assert!(lib.available_genres.contains(&"Ambient".to_string()));
}

#[test]
fn test_in_memory_library_rebuilds_genres_without_path() {
    let lib = Library::in_memory(vec![station("Test", "http://test", "Synthwave", "US", 128)]);
    assert!(lib.path.is_none());
    assert!(lib.available_genres.contains(&"Synthwave".to_string()));
}

#[test]
fn test_import_stations() {
    let mut lib = Library::in_memory(vec![station("Test A", "http://a", "Synthwave", "US", 128)]);

    let to_import = vec![
        station("Test A", "http://a", "Synthwave", "US", 128),
        station("Test B", "http://b", "Ambient", "UK", 96),
    ];

    let summary = lib.import_stations(to_import).unwrap();
    assert_eq!(summary.added, 1);
    assert_eq!(summary.skipped, 1);
    assert_eq!(summary.enriched, 0);
    assert_eq!(lib.stations.len(), 2);
}

#[test]
fn preview_import_classifies_new_duplicate_enrichment_and_skip() {
    let mut saved = station("Saved", "http://saved", "Synthwave", "US", 0);
    saved.station_uuid = Some("uuid-saved".to_string());
    let lib = Library::in_memory(vec![saved]);

    let duplicate = station("Saved", "http://saved", "Synthwave", "US", 0);
    let mut enrichment = station("Saved Rich", "http://saved", "Synthwave", "US", 128);
    enrichment.station_uuid = Some("uuid-saved".to_string());
    enrichment.codec = "MP3".to_string();
    let new_station = station("New", "http://new", "Ambient", "UK", 96);
    let broken = station("Broken", "", "Ambient", "UK", 96);

    let preview = lib.preview_import(vec![duplicate, enrichment, new_station, broken]);

    assert_eq!(preview.duplicates.len(), 1);
    assert_eq!(preview.enrichments.len(), 1);
    assert_eq!(preview.new_stations.len(), 1);
    assert_eq!(preview.skipped.len(), 1);
    assert_eq!(preview.skipped[0].reason, "missing stream URL");
}

#[test]
fn empty_import_preview_reports_empty() {
    let lib = Library::in_memory(vec![]);

    assert!(lib.preview_import(vec![]).is_empty());
}

#[test]
fn preview_import_does_not_write_library() {
    let lib = Library::in_memory(vec![station(
        "Saved",
        "http://saved",
        "Synthwave",
        "US",
        128,
    )]);

    let preview = lib.preview_import(vec![station("New", "http://new", "Ambient", "UK", 96)]);

    assert_eq!(preview.new_stations.len(), 1);
    assert_eq!(lib.stations.len(), 1);
}

#[test]
fn apply_import_preview_enrich_only_does_not_add_new_stations() {
    let mut saved = station("Saved", "http://saved", "Synthwave", "US", 0);
    saved.station_uuid = Some("uuid-saved".to_string());
    let mut lib = Library::in_memory(vec![saved]);

    let mut enrichment = station("Saved Rich", "http://saved", "Synthwave", "US", 128);
    enrichment.station_uuid = Some("uuid-saved".to_string());
    enrichment.codec = "MP3".to_string();
    let preview = lib.preview_import(vec![
        enrichment,
        station("New", "http://new", "Ambient", "UK", 96),
    ]);

    let summary = lib
        .apply_import_preview(preview, ImportMode::EnrichExistingOnly)
        .unwrap();

    assert_eq!(summary.added, 0);
    assert_eq!(summary.enriched, 1);
    assert_eq!(summary.skipped, 1);
    assert_eq!(lib.stations.len(), 1);
    assert_eq!(lib.stations[0].bitrate, 128);
    assert_eq!(lib.stations[0].codec, "MP3");
}

#[test]
fn apply_import_preview_all_adds_new_and_preserves_saved_identity_fields() {
    let mut saved = station("Saved Name", "http://saved", "Synthwave", "US", 0);
    saved.station_uuid = Some("uuid-saved".to_string());
    let mut lib = Library::in_memory(vec![saved]);

    let mut enrichment = station("Incoming Name", "http://saved", "Ambient", "CA", 128);
    enrichment.station_uuid = Some("uuid-saved".to_string());
    enrichment.codec = "AAC".to_string();
    let preview = lib.preview_import(vec![
        enrichment,
        station("New", "http://new", "Ambient", "UK", 96),
    ]);

    let summary = lib.apply_import_preview(preview, ImportMode::All).unwrap();

    assert_eq!(summary.added, 1);
    assert_eq!(summary.enriched, 1);
    assert_eq!(summary.skipped, 0);
    assert_eq!(lib.stations.len(), 2);
    assert_eq!(lib.stations[0].name, "Saved Name");
    assert_eq!(lib.stations[0].url, "http://saved");
    assert_eq!(lib.stations[0].genre, "Synthwave");
    assert_eq!(lib.stations[0].codec, "AAC");
}

#[test]
fn preview_import_deduplicates_incoming_stations_against_each_other() {
    let lib = Library::in_memory(vec![]);

    let preview = lib.preview_import(vec![
        station("A", "http://same", "Synthwave", "US", 128),
        station("A Copy", "http://same/", "Ambient", "UK", 96),
    ]);

    assert_eq!(preview.new_stations.len(), 1);
    assert_eq!(preview.duplicates.len(), 1);
}

#[test]
fn import_skip_name_uses_stable_fallback_for_blank_names() {
    let station = station("   ", "http://blank-name", "Ambient", "UK", 96);

    assert_eq!(
        import_skip_reason(&station),
        Some("missing station name".to_string())
    );
    assert_eq!(import_skip_name(&station), "Unnamed station");
}

#[test]
fn metadata_refresh_summary_counts_changed_unchanged_and_failed() {
    let mut saved = station("Saved", "http://saved", "Synthwave", "US", 0);
    saved.station_uuid = Some("uuid-saved".to_string());
    let mut lib = Library::in_memory(vec![saved]);

    let mut changed = station("Incoming", "http://saved", "Ambient", "CA", 128);
    changed.station_uuid = Some("uuid-saved".to_string());
    changed.codec = "MP3".to_string();
    let unchanged = station("Missing Match", "http://missing", "Ambient", "UK", 96);

    let summary = lib.apply_metadata_refresh_results(3, vec![changed, unchanged], 1);

    assert_eq!(summary.checked, 3);
    assert_eq!(summary.enriched, 1);
    assert_eq!(summary.unchanged, 1);
    assert_eq!(summary.failed, 1);
    assert_eq!(
        summary.notice(),
        "Metadata refresh: 3 checked, 1 enriched, 1 unchanged, 1 failed"
    );
    assert_eq!(lib.stations[0].name, "Saved");
    assert_eq!(lib.stations[0].url, "http://saved");
    assert_eq!(lib.stations[0].genre, "Synthwave");
    assert_eq!(lib.stations[0].codec, "MP3");
}

#[test]
fn metadata_refresh_summary_counts_no_match_as_unchanged() {
    let mut lib = Library::in_memory(vec![station(
        "Saved",
        "http://saved",
        "Synthwave",
        "US",
        128,
    )]);

    let summary = lib.apply_metadata_refresh_results(1, Vec::new(), 0);

    assert_eq!(summary.enriched, 0);
    assert_eq!(summary.unchanged, 1);
    assert_eq!(summary.failed, 0);
}

#[test]
fn station_health_deserializes_missing_fields() {
    let json = r#"{
            "version": 1,
            "stations": [{
                "name": "Old FM",
                "url": "http://old",
                "genre": "Radio",
                "country": "US",
                "bitrate": 128
            }],
            "settings": {}
        }"#;

    let (stations, _, _) = parse_library_file(json).unwrap();

    assert!(stations[0].health.is_empty());
}

#[test]
fn mark_station_success_and_failure_update_saved_health() {
    let mut lib = Library::in_memory(vec![station("Test A", "http://a/", "Synthwave", "US", 128)]);

    assert!(lib.mark_station_failure(" HTTP://A ", "10".to_string(), "network timeout"));
    assert_eq!(lib.stations[0].health.failure_count, Some(1));
    assert_eq!(
        lib.stations[0].health.last_failure_at.as_deref(),
        Some("10")
    );
    assert_eq!(lib.stations[0].health.last_error_summary, "network timeout");

    assert!(lib.mark_station_success("http://a", "11".to_string()));
    assert_eq!(
        lib.stations[0].health.last_success_at.as_deref(),
        Some("11")
    );
    assert!(lib.stations[0].health.last_error_summary.is_empty());
}

#[test]
fn metadata_refresh_notice_contains_values_in_order() {
    let notice = MetadataRefreshSummary {
        checked: 10,
        enriched: 3,
        unchanged: 6,
        failed: 1,
    }
    .notice();

    let pos_10 = notice.find("10").expect("should contain '10'");
    let pos_3 = notice[pos_10..]
        .find('3')
        .map(|p| p + pos_10)
        .expect("should contain '3' after '10'");
    let pos_6 = notice[pos_3..]
        .find('6')
        .map(|p| p + pos_3)
        .expect("should contain '6' after '3'");
    let pos_1 = notice[pos_6..]
        .find('1')
        .map(|p| p + pos_6)
        .expect("should contain '1' after '6'");
    assert!(pos_10 < pos_3);
    assert!(pos_3 < pos_6);
    assert!(pos_6 < pos_1);
}

#[test]
fn metadata_refresh_notice_all_zero_contains_four_zeros() {
    let notice = MetadataRefreshSummary {
        checked: 0,
        enriched: 0,
        unchanged: 0,
        failed: 0,
    }
    .notice();

    assert!(!notice.is_empty());
    let zero_count = notice.matches('0').count();
    assert!(
        zero_count >= 4,
        "expected at least 4 occurrences of '0', got {zero_count} in: {notice}"
    );
}

#[test]
fn metadata_refresh_notice_contains_field_labels() {
    let notice = MetadataRefreshSummary {
        checked: 10,
        enriched: 3,
        unchanged: 6,
        failed: 1,
    }
    .notice();

    assert!(notice.contains("checked"), "missing 'checked' in: {notice}");
    assert!(
        notice.contains("enriched"),
        "missing 'enriched' in: {notice}"
    );
    assert!(
        notice.contains("unchanged"),
        "missing 'unchanged' in: {notice}"
    );
    assert!(notice.contains("failed"), "missing 'failed' in: {notice}");
}

#[test]
fn mark_station_success_returns_true_and_updates_health_for_matching_url() {
    let mut lib = Library::in_memory(vec![Station::basic(
        "Test FM",
        "http://stream.example.com/live",
        "Synthwave",
        "US",
        128,
    )]);

    let result = lib.mark_station_success(
        "http://stream.example.com/live",
        "2024-01-15T10:00:00Z".to_string(),
    );

    assert!(result);
    assert_eq!(
        lib.stations[0].health.last_success_at.as_deref(),
        Some("2024-01-15T10:00:00Z")
    );
    assert!(lib.stations[0].health.last_error_summary.is_empty());
}

#[test]
fn mark_station_success_returns_false_for_non_matching_url() {
    let mut lib = Library::in_memory(vec![Station::basic(
        "Test FM",
        "http://stream.example.com/live",
        "Synthwave",
        "US",
        128,
    )]);

    let result = lib.mark_station_success(
        "http://other.example.com/stream",
        "2024-01-15T10:00:00Z".to_string(),
    );

    assert!(!result);
    assert!(lib.stations[0].health.last_success_at.is_none());
    assert!(lib.stations[0].health.last_failure_at.is_none());
    assert_eq!(lib.stations[0].health.failure_count, None);
    assert!(lib.stations[0].health.last_error_summary.is_empty());
}

#[test]
fn mark_station_failure_returns_true_and_updates_health_for_matching_url() {
    let mut lib = Library::in_memory(vec![Station::basic(
        "Test FM",
        "http://stream.example.com/live",
        "Synthwave",
        "US",
        128,
    )]);

    let long_error = "a".repeat(200);
    let result = lib.mark_station_failure(
        "http://stream.example.com/live",
        "2024-01-15T10:05:00Z".to_string(),
        &long_error,
    );

    assert!(result);
    assert_eq!(
        lib.stations[0].health.last_failure_at.as_deref(),
        Some("2024-01-15T10:05:00Z")
    );
    assert_eq!(lib.stations[0].health.failure_count, Some(1));
    // Error summary is truncated to 96 characters
    assert!(lib.stations[0].health.last_error_summary.chars().count() <= 96);
    assert!(!lib.stations[0].health.last_error_summary.is_empty());
}

#[test]
fn mark_station_failure_consecutive_calls_increment_failure_count() {
    let mut lib = Library::in_memory(vec![Station::basic(
        "Test FM",
        "http://stream.example.com/live",
        "Synthwave",
        "US",
        128,
    )]);

    lib.mark_station_failure("http://stream.example.com/live", "t1".to_string(), "err1");
    lib.mark_station_failure("http://stream.example.com/live", "t2".to_string(), "err2");
    lib.mark_station_failure("http://stream.example.com/live", "t3".to_string(), "err3");

    assert_eq!(lib.stations[0].health.failure_count, Some(3));
}

#[test]
fn mark_station_failure_returns_false_for_non_matching_url() {
    let mut lib = Library::in_memory(vec![Station::basic(
        "Test FM",
        "http://stream.example.com/live",
        "Synthwave",
        "US",
        128,
    )]);

    let result = lib.mark_station_failure(
        "http://other.example.com/stream",
        "2024-01-15T10:05:00Z".to_string(),
        "connection refused",
    );

    assert!(!result);
    assert!(lib.stations[0].health.last_success_at.is_none());
    assert!(lib.stations[0].health.last_failure_at.is_none());
    assert_eq!(lib.stations[0].health.failure_count, None);
    assert!(lib.stations[0].health.last_error_summary.is_empty());
}

#[test]
fn mark_station_success_after_failures_clears_error_but_preserves_failure_history() {
    let mut lib = Library::in_memory(vec![Station::basic(
        "Test FM",
        "http://stream.example.com/live",
        "Synthwave",
        "US",
        128,
    )]);

    // Record some failures first
    lib.mark_station_failure(
        "http://stream.example.com/live",
        "t1".to_string(),
        "timeout",
    );
    lib.mark_station_failure(
        "http://stream.example.com/live",
        "t2".to_string(),
        "timeout",
    );

    assert_eq!(lib.stations[0].health.failure_count, Some(2));
    assert_eq!(
        lib.stations[0].health.last_failure_at.as_deref(),
        Some("t2")
    );
    assert!(!lib.stations[0].health.last_error_summary.is_empty());

    // Now mark success
    lib.mark_station_success("http://stream.example.com/live", "t3".to_string());

    // last_error_summary is cleared
    assert!(lib.stations[0].health.last_error_summary.is_empty());
    // failure_count and last_failure_at are preserved
    assert_eq!(lib.stations[0].health.failure_count, Some(2));
    assert_eq!(
        lib.stations[0].health.last_failure_at.as_deref(),
        Some("t2")
    );
    // last_success_at is set
    assert_eq!(
        lib.stations[0].health.last_success_at.as_deref(),
        Some("t3")
    );
}

#[test]
fn persistence_round_trip_station_slots_and_favorites() {
    use std::fs;

    let dir = std::env::temp_dir().join("pulsedeck_test_persistence_round_trip");
    let _ = fs::create_dir_all(&dir);
    let path = dir.join("library.json");

    let mut lib = Library {
        stations: vec![station(
            "Test FM",
            "http://test.fm/stream",
            "Synthwave",
            "US",
            128,
        )],
        available_genres: vec![],
        settings: Settings::default(),
        path: Some(path.clone()),
        load_warnings: Vec::new(),
    };
    lib.rebuild_genres();

    // Populate station slots
    lib.settings.station_slots.assign(1, "http://a.com/stream");
    lib.settings.station_slots.assign(3, "http://c.com/live");

    // Populate favorites
    lib.settings.favorites.toggle("http://test.fm/stream");

    // Save to disk
    lib.save().unwrap();

    // Read file back and parse
    let contents = fs::read_to_string(&path).unwrap();
    let (stations, settings, warning) = parse_library_file(&contents).unwrap();

    assert_eq!(stations.len(), 1);
    assert_eq!(stations[0].name, "Test FM");

    // Verify station_slots round-tripped
    assert_eq!(settings.station_slots.get(1), Some("http://a.com/stream"));
    assert_eq!(settings.station_slots.get(2), None);
    assert_eq!(settings.station_slots.get(3), Some("http://c.com/live"));

    // Verify favorites round-tripped
    assert!(settings.favorites.contains("http://test.fm/stream"));

    assert!(warning.is_none());

    let _ = fs::remove_file(&path);
    let _ = fs::remove_dir(&dir);
}

#[test]
fn backward_compatibility_missing_station_slots_and_favorites() {
    // A library.json from an older version without station_slots or favorites
    let json = r#"{
            "version": 1,
            "stations": [
                {
                    "name": "Old FM",
                    "url": "http://old.fm/stream",
                    "genre": "Ambient",
                    "country": "DE",
                    "bitrate": 192
                }
            ],
            "settings": {
                "notifications_enabled": true,
                "autoplay_last": false,
                "theme": "Retrowave"
            }
        }"#;

    let (stations, settings, warning) = parse_library_file(json).unwrap();

    assert_eq!(stations.len(), 1);
    assert_eq!(stations[0].name, "Old FM");
    assert_eq!(settings.station_slots.get(1), None);
    assert!(settings.favorites.is_empty());
    assert!(warning.is_none());
}
