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

include!("library_impl.rs");
include!("persistence.rs");

#[cfg(test)]
mod tests;
