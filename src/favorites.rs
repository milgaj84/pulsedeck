use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::radio::{
    clean_tag_values, normalize_codec, normalize_country_code, normalize_station_uuid,
    sanitize_bitrate, station_identity_matches, station_url_matches, Station, StationHealth,
};

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
    stations: Vec<SavedStation>,
    #[serde(default)]
    settings: Settings,
}

fn default_library_version() -> u32 {
    1
}

/// Serializable station (mirrors Station but with serde).
#[derive(Serialize, Deserialize)]
struct SavedStation {
    name: String,
    url: String,
    genre: String,
    country: String,
    bitrate: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    station_uuid: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    country_code: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    tags: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    language: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    codec: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    homepage: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_check_ok: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    votes: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    click_count: Option<u32>,
    #[serde(default, skip_serializing_if = "StationHealth::is_empty")]
    health: StationHealth,
}

impl From<&Station> for SavedStation {
    fn from(s: &Station) -> Self {
        Self {
            name: s.name.clone(),
            url: s.url.clone(),
            genre: s.genre.clone(),
            country: s.country.clone(),
            bitrate: s.bitrate,
            station_uuid: s.station_uuid.clone(),
            country_code: s.country_code.clone(),
            tags: s.tags.clone(),
            language: s.language.clone(),
            codec: s.codec.clone(),
            homepage: s.homepage.clone(),
            last_check_ok: s.last_check_ok,
            votes: s.votes,
            click_count: s.click_count,
            health: s.health.clone(),
        }
    }
}

impl From<SavedStation> for Station {
    fn from(s: SavedStation) -> Self {
        Self {
            name: s.name,
            url: s.url,
            genre: s.genre,
            country: s.country.trim().to_string(),
            bitrate: sanitize_bitrate(s.bitrate),
            station_uuid: s.station_uuid.and_then(normalize_station_uuid),
            country_code: normalize_country_code(&s.country_code),
            tags: clean_tag_values(s.tags),
            language: s.language.trim().to_string(),
            codec: normalize_codec(&s.codec),
            homepage: s.homepage.trim().to_string(),
            last_check_ok: s.last_check_ok,
            votes: s.votes,
            click_count: s.click_count,
            health: s.health,
        }
    }
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
        let path = config_path();

        let mut load_warnings = Vec::new();

        let (stations, settings) = if let Some(ref p) = path {
            if p.exists() {
                match fs::read_to_string(p) {
                    Ok(contents) => match parse_library_file(&contents) {
                        Ok((stations, settings, warning)) => {
                            if let Some(warning) = warning {
                                load_warnings.push(warning);
                            }
                            (stations, settings)
                        }
                        Err(err) => {
                            load_warnings.push(format!(
                                "Could not parse library.json; using starter stations: {err}"
                            ));
                            (seed_stations, Settings::default())
                        }
                    },
                    Err(err) => {
                        load_warnings.push(format!(
                            "Could not read library.json; using starter stations: {err}"
                        ));
                        (seed_stations, Settings::default())
                    }
                }
            } else {
                // First launch — seed and save
                let mut lib = Self {
                    stations: seed_stations,
                    available_genres: Vec::new(),
                    settings: Settings::default(),
                    path: path.clone(),
                    load_warnings: Vec::new(),
                };
                lib.rebuild_genres();
                if let Err(err) = lib.save() {
                    lib.load_warnings
                        .push(format!("Could not save starter library: {err}"));
                }
                return lib;
            }
        } else {
            (seed_stations, Settings::default())
        };

        let mut lib = Self {
            stations,
            available_genres: Vec::new(),
            settings,
            path,
            load_warnings,
        };
        lib.rebuild_genres();
        lib
    }

    /// Load the library for read-only/CLI use without seeding starters or writing a file.
    /// Keeps the resolved path so a later import can still persist.
    pub fn load_existing() -> Self {
        let path = config_path();

        let mut load_warnings = Vec::new();
        let (stations, settings) = match path.as_ref() {
            Some(p) if p.exists() => match fs::read_to_string(p) {
                Ok(contents) => match parse_library_file(&contents) {
                    Ok((stations, settings, warning)) => {
                        if let Some(warning) = warning {
                            load_warnings.push(warning);
                        }
                        (stations, settings)
                    }
                    Err(err) => {
                        load_warnings.push(format!(
                            "Could not parse library.json; using empty library: {err}"
                        ));
                        (Vec::new(), Settings::default())
                    }
                },
                Err(err) => {
                    load_warnings.push(format!(
                        "Could not read library.json; using empty library: {err}"
                    ));
                    (Vec::new(), Settings::default())
                }
            },
            _ => (Vec::new(), Settings::default()),
        };

        let mut lib = Self {
            stations,
            available_genres: Vec::new(),
            settings,
            path,
            load_warnings,
        };
        lib.rebuild_genres();
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
            station.health.failure_count = Some(
                station
                    .health
                    .failure_count
                    .unwrap_or(0)
                    .saturating_add(1),
            );
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
            stations: self.stations.iter().map(SavedStation::from).collect(),
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

fn compact_error_summary(error: &str) -> String {
    crate::ui::text::truncate_with_ellipsis(error.trim(), 96)
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
        file.stations.into_iter().map(Station::from).collect(),
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
        let mut lib = Library::in_memory(vec![station(
            "Test A",
            "http://a",
            "Synthwave",
            "US",
            128,
        )]);

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

        assert_eq!(import_skip_reason(&station), Some("missing station name".to_string()));
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
        assert_eq!(summary.notice(), "Metadata refresh: 3 checked, 1 enriched, 1 unchanged, 1 failed");
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
        let mut lib = Library::in_memory(vec![station(
            "Test A",
            "http://a/",
            "Synthwave",
            "US",
            128,
        )]);

        assert!(lib.mark_station_failure(" HTTP://A ", "10".to_string(), "network timeout"));
        assert_eq!(lib.stations[0].health.failure_count, Some(1));
        assert_eq!(lib.stations[0].health.last_failure_at.as_deref(), Some("10"));
        assert_eq!(lib.stations[0].health.last_error_summary, "network timeout");

        assert!(lib.mark_station_success("http://a", "11".to_string()));
        assert_eq!(lib.stations[0].health.last_success_at.as_deref(), Some("11"));
        assert!(lib.stations[0].health.last_error_summary.is_empty());
    }
}

