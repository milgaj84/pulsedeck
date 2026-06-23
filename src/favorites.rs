use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::favorites_set::FavoritesSet;
use crate::radio::{
    clean_tag_values, normalize_codec, normalize_country_code, normalize_station_uuid,
    sanitize_bitrate, station_identity_matches, station_url_matches, Station,
};
use crate::recent_ring::StationSlots;

const LIBRARY_FILE: &str = "library.json";

/// Your personal station library, persisted to disk.
///
/// This IS the station list. No random default playlist.
/// First launch seeds with curated starter stations.
/// After that, you manage your own list via search + add/remove.
/// Application settings, serialized inside the main config.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Settings {
    #[serde(default = "default_true")]
    pub notifications_enabled: bool,
    #[serde(default)]
    pub autoplay_last: bool,
    #[serde(default)]
    pub last_played_url: Option<String>,
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default)]
    pub output_device_name: Option<String>,
    #[serde(default)]
    pub save_history: bool,
    #[serde(default = "default_true")]
    pub stream_metadata_enabled: bool,
    #[serde(default)]
    pub station_slots: StationSlots,
    #[serde(default)]
    pub favorites: FavoritesSet,
}

fn default_true() -> bool {
    true
}

fn default_theme() -> String {
    "Retrowave".to_string()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            notifications_enabled: true,
            autoplay_last: false,
            last_played_url: None,
            theme: "Retrowave".to_string(),
            output_device_name: None,
            save_history: false,
            stream_metadata_enabled: true,
            station_slots: StationSlots::default(),
            favorites: FavoritesSet::default(),
        }
    }
}

pub struct Library {
    pub stations: Vec<Station>,
    pub available_genres: Vec<String>,
    pub settings: Settings,
    pub(crate) path: Option<std::path::PathBuf>,
    pub load_warnings: Vec<String>,
}

/// On-disk JSON format — stores full station data.
#[derive(Serialize, Deserialize)]
struct LibraryFile {
    #[serde(default = "default_library_version")]
    version: u32,
    #[serde(default)]
    stations: Vec<Station>,
    #[serde(default)]
    settings: Settings,
}

fn default_library_version() -> u32 {
    1
}

fn normalize_loaded_station(mut station: Station) -> Station {
    station.country = station.country.trim().to_string();
    station.bitrate = sanitize_bitrate(station.bitrate);
    station.station_uuid = station.station_uuid.and_then(normalize_station_uuid);
    station.country_code = normalize_country_code(&station.country_code);
    station.tags = clean_tag_values(station.tags);
    station.language = station.language.trim().to_string();
    station.codec = normalize_codec(&station.codec);
    station.homepage = station.homepage.trim().to_string();
    station
}

/// Helper to map dynamic micro-genres to static parent categories.
pub fn resolve_parent_genre(subgenre: &str) -> &'static str {
    let s = subgenre.to_lowercase();
    if s.contains("synthwave")
        || s.contains("chillsynth")
        || s.contains("darksynth")
        || s.contains("retrowave")
    {
        "Synthwave"
    } else if s.contains("ambient")
        || s.contains("chillout")
        || s.contains("drone")
        || s.contains("space")
    {
        "Ambient"
    } else if s.contains("rock") || s.contains("metal") || s.contains("guitar") {
        "Rock"
    } else if s.contains("vaporwave") || s.contains("plaza") || s.contains("synthpop") {
        "Vaporwave"
    } else {
        "Other"
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportSummary {
    pub added: usize,
    pub skipped: usize,
    pub enriched: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MetadataRefreshSummary {
    pub checked: usize,
    pub enriched: usize,
    pub unchanged: usize,
    pub failed: usize,
}

impl MetadataRefreshSummary {
    pub fn notice(self) -> String {
        format!(
            "Metadata refresh: {} checked, {} enriched, {} unchanged, {} failed",
            self.checked, self.enriched, self.unchanged, self.failed
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportPreview {
    pub new_stations: Vec<Station>,
    pub duplicates: Vec<Station>,
    pub enrichments: Vec<Station>,
    pub skipped: Vec<ImportSkip>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportSkip {
    pub name: String,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportMode {
    All,
    EnrichExistingOnly,
}

#[derive(Debug, Clone)]
enum MissingLibraryPolicy {
    SeedAndSave(Vec<Station>),
    Empty,
}

impl ImportPreview {
    pub fn is_empty(&self) -> bool {
        self.new_stations.is_empty()
            && self.duplicates.is_empty()
            && self.enrichments.is_empty()
            && self.skipped.is_empty()
    }
}

impl Library {
    /// Load library from disk.
    /// On first launch (no file), seeds with starter stations.
    pub fn load(seed_stations: Vec<Station>) -> Self {
        Self::load_with_policy(MissingLibraryPolicy::SeedAndSave(seed_stations))
    }

    /// Load the library for read-only/CLI use without seeding starters or writing a file.
    /// Keeps the resolved path so a later import can still persist.
    pub fn load_existing() -> Self {
        Self::load_with_policy(MissingLibraryPolicy::Empty)
    }

    fn load_with_policy(policy: MissingLibraryPolicy) -> Self {
        let path = config_path();
        let mut load_warnings = Vec::new();

        let (stations, settings, should_save_seed) = match path.as_ref() {
            Some(path) if path.exists() => {
                read_library_file_or_fallback(path, &policy, &mut load_warnings)
            }
            _ => fallback_for_missing_library(&policy),
        };

        let mut lib = Self {
            stations,
            available_genres: Vec::new(),
            settings,
            path,
            load_warnings,
        };
        lib.rebuild_genres();

        if should_save_seed {
            if let Err(err) = lib.save() {
                lib.load_warnings
                    .push(format!("Could not save starter library: {err}"));
            }
        }

        lib
    }

    /// Create an in-memory library for tests without touching the user's config file.
    #[cfg(test)]
    pub fn in_memory(stations: Vec<Station>) -> Self {
        let mut lib = Self {
            stations,
            available_genres: Vec::new(),
            settings: Settings::default(),
            path: None,
            load_warnings: Vec::new(),
        };
        lib.rebuild_genres();
        lib
    }

    /// Add a station to the library (deduplicates by Radio Browser UUID when present, then URL).
    /// Returns true if actually added (not a duplicate).
    pub fn add(&mut self, station: Station) -> anyhow::Result<bool> {
        if self.contains_station(&station) {
            return Ok(false);
        }
        self.stations.push(station);
        self.rebuild_genres();
        Ok(true)
    }

    /// Import a list of stations, merging by Radio Browser UUID when present, then URL.
    pub fn import_stations(&mut self, stations: Vec<Station>) -> anyhow::Result<ImportSummary> {
        let preview = self.preview_import(stations);
        self.apply_import_preview(preview, ImportMode::All)
    }

    pub fn preview_import(&self, stations: Vec<Station>) -> ImportPreview {
        let mut preview = ImportPreview {
            new_stations: Vec::new(),
            duplicates: Vec::new(),
            enrichments: Vec::new(),
            skipped: Vec::new(),
        };
        let mut virtual_library = self.stations.clone();

        for station in stations {
            if let Some(reason) = import_skip_reason(&station) {
                preview.skipped.push(ImportSkip {
                    name: import_skip_name(&station),
                    reason,
                });
                continue;
            }

            if let Some(existing) = virtual_library
                .iter_mut()
                .find(|saved| station_identity_matches(saved, &station))
            {
                let mut enriched = existing.clone();
                if enriched.enrich_from(&station) {
                    *existing = enriched;
                    preview.enrichments.push(station);
                } else {
                    preview.duplicates.push(station);
                }
            } else {
                virtual_library.push(station.clone());
                preview.new_stations.push(station);
            }
        }

        preview
    }

    pub fn apply_import_preview(
        &mut self,
        preview: ImportPreview,
        mode: ImportMode,
    ) -> anyhow::Result<ImportSummary> {
        let mut added = 0;
        let mut skipped = preview.duplicates.len() + preview.skipped.len();
        let mut enriched = 0;

        for station in preview.enrichments {
            if self.enrich_matching_station(&station) {
                enriched += 1;
            } else if mode == ImportMode::All && !self.contains_station(&station) {
                self.stations.push(station);
                added += 1;
            } else {
                skipped += 1;
            }
        }

        if mode == ImportMode::All {
            for station in preview.new_stations {
                if self.contains_station(&station) {
                    skipped += 1;
                } else {
                    self.stations.push(station);
                    added += 1;
                }
            }
        } else {
            skipped += preview.new_stations.len();
        }

        if added > 0 || enriched > 0 {
            self.rebuild_genres();
        }

        Ok(ImportSummary {
            added,
            skipped,
            enriched,
        })
    }

    pub fn apply_metadata_refresh_results(
        &mut self,
        checked: usize,
        matches: Vec<Station>,
        failed: usize,
    ) -> MetadataRefreshSummary {
        let mut summary = MetadataRefreshSummary {
            checked,
            failed,
            ..MetadataRefreshSummary::default()
        };

        for station in matches {
            if self.enrich_matching_station(&station) {
                summary.enriched += 1;
            } else {
                summary.unchanged += 1;
            }
        }

        summary.unchanged += checked.saturating_sub(summary.enriched + summary.unchanged + failed);

        if summary.enriched > 0 {
            self.rebuild_genres();
        }

        summary
    }

    /// Remove a station by URL. Returns true if removed.
    pub fn remove(&mut self, url: &str) -> anyhow::Result<bool> {
        let before = self.stations.len();
        self.stations
            .retain(|station| !station_url_matches(&station.url, url));
        let removed = self.stations.len() < before;
        if removed {
            self.rebuild_genres();
        }
        Ok(removed)
    }

    /// Dynamically rebuild unique genres list, sorting them with "All" at the front.
    pub fn rebuild_genres(&mut self) {
        let genres: std::collections::HashSet<String> = self
            .stations
            .iter()
            .map(|s| resolve_parent_genre(&s.genre).to_string())
            .filter(|g| !g.is_empty())
            .collect();

        let mut sorted: Vec<String> = genres.into_iter().collect();
        sorted.sort_by_key(|a| a.to_lowercase());
        sorted.insert(0, "All".to_string());
        self.available_genres = sorted;
    }

    /// Check if a station identity is in the library, preferring Radio Browser UUID when available.
    pub fn contains_station(&self, station: &Station) -> bool {
        self.stations
            .iter()
            .any(|saved| station_identity_matches(saved, station))
    }

    /// Enrich an already-saved matching station with fresh Radio Browser metadata.
    pub fn enrich_matching_station(&mut self, station: &Station) -> bool {
        self.stations
            .iter_mut()
            .find(|saved| station_identity_matches(saved, station))
            .is_some_and(|saved| saved.enrich_from(station))
    }

    pub fn mark_station_success(&mut self, url: &str, now: String) -> bool {
        if let Some(station) = self
            .stations
            .iter_mut()
            .find(|station| station_url_matches(&station.url, url))
        {
            station.health.last_success_at = Some(now);
            station.health.last_error_summary.clear();
            return true;
        }
        false
    }

    pub fn mark_station_failure(&mut self, url: &str, now: String, error: &str) -> bool {
        if let Some(station) = self
            .stations
            .iter_mut()
            .find(|station| station_url_matches(&station.url, url))
        {
            station.health.last_failure_at = Some(now);
            station.health.failure_count =
                Some(station.health.failure_count.unwrap_or(0).saturating_add(1));
            station.health.last_error_summary = compact_error_summary(error);
            return true;
        }
        false
    }

    /// Check if a station URL is in the library.
    pub fn contains(&self, url: &str) -> bool {
        self.stations
            .iter()
            .any(|station| station_url_matches(&station.url, url))
    }

    /// Save library to disk (best-effort).
    pub fn save(&self) -> anyhow::Result<()> {
        let Some(ref path) = self.path else {
            return Ok(());
        };

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let file = LibraryFile {
            version: 1,
            stations: self.stations.clone(),
            settings: self.settings.clone(),
        };

        let json = serde_json::to_string_pretty(&file)?;
        fs::write(path, json)?;

        Ok(())
    }
}

/// Config file path: ~/.config/pulsedeck/library.json
///
/// If an existing DriftFM config exists and no PulseDeck config has been written yet,
/// copy the old file into the new config directory so users keep their library.
fn config_path() -> Option<PathBuf> {
    crate::config::config_path(LIBRARY_FILE)
}

fn read_library_file_or_fallback(
    path: &Path,
    policy: &MissingLibraryPolicy,
    load_warnings: &mut Vec<String>,
) -> (Vec<Station>, Settings, bool) {
    match fs::read_to_string(path) {
        Ok(contents) => match parse_library_file(&contents) {
            Ok((stations, settings, warning)) => {
                if let Some(warning) = warning {
                    load_warnings.push(warning);
                }
                (stations, settings, false)
            }
            Err(err) => {
                load_warnings.push(format!(
                    "Could not parse library.json; using {}: {err}",
                    fallback_label(policy)
                ));
                fallback_for_missing_library(policy)
            }
        },
        Err(err) => {
            load_warnings.push(format!(
                "Could not read library.json; using {}: {err}",
                fallback_label(policy)
            ));
            fallback_for_missing_library(policy)
        }
    }
}

fn fallback_for_missing_library(policy: &MissingLibraryPolicy) -> (Vec<Station>, Settings, bool) {
    match policy {
        MissingLibraryPolicy::SeedAndSave(stations) => {
            (stations.clone(), Settings::default(), true)
        }
        MissingLibraryPolicy::Empty => (Vec::new(), Settings::default(), false),
    }
}

fn fallback_label(policy: &MissingLibraryPolicy) -> &'static str {
    match policy {
        MissingLibraryPolicy::SeedAndSave(_) => "starter stations",
        MissingLibraryPolicy::Empty => "empty library",
    }
}

fn compact_error_summary(error: &str) -> String {
    crate::text::truncate_with_ellipsis(error.trim(), 96)
}

fn import_skip_reason(station: &Station) -> Option<String> {
    if station.url.trim().is_empty() {
        Some("missing stream URL".to_string())
    } else if station.name.trim().is_empty() {
        Some("missing station name".to_string())
    } else {
        None
    }
}

fn import_skip_name(station: &Station) -> String {
    let name = station.name.trim();
    if name.is_empty() {
        "Unnamed station".to_string()
    } else {
        name.to_string()
    }
}

fn parse_library_file(
    contents: &str,
) -> serde_json::Result<(Vec<Station>, Settings, Option<String>)> {
    let file = serde_json::from_str::<LibraryFile>(contents)?;
    let warning = if file.version == 1 {
        None
    } else {
        Some(format!(
            "Library file version {} is newer than supported version 1",
            file.version
        ))
    };
    Ok((
        file.stations
            .into_iter()
            .map(normalize_loaded_station)
            .collect(),
        file.settings,
        warning,
    ))
}

#[cfg(test)]
mod tests {
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
        let mut lib =
            Library::in_memory(vec![station("Test A", "http://a", "Synthwave", "US", 128)]);

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
        let mut lib =
            Library::in_memory(vec![station("Test A", "http://a/", "Synthwave", "US", 128)]);

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
}
