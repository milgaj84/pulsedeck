use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::radio::Station;

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
    #[serde(default = "default_recording_dir")]
    pub recording_dir: String,
    #[serde(default = "default_false")]
    pub keep_snippets: bool,
    #[serde(default = "default_min_duration")]
    pub min_song_duration_secs: u32,
    #[serde(default = "default_theme")]
    pub theme: String,
}

fn default_true() -> bool {
    true
}

fn default_recording_dir() -> String {
    "./recordings".to_string()
}

fn default_false() -> bool {
    false
}

fn default_min_duration() -> u32 {
    90
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
            recording_dir: "./recordings".to_string(),
            keep_snippets: false,
            min_song_duration_secs: 90,
            theme: "Retrowave".to_string(),
        }
    }
}

pub struct Library {
    pub stations: Vec<Station>,
    pub available_genres: Vec<String>,
    pub settings: Settings,
    path: Option<PathBuf>,
}

/// On-disk JSON format — stores full station data.
#[derive(Serialize, Deserialize)]
struct LibraryFile {
    version: u32,
    stations: Vec<SavedStation>,
    #[serde(default)]
    settings: Settings,
}

/// Serializable station (mirrors Station but with serde).
#[derive(Serialize, Deserialize)]
struct SavedStation {
    name: String,
    url: String,
    genre: String,
    country: String,
    bitrate: u32,
}

impl From<&Station> for SavedStation {
    fn from(s: &Station) -> Self {
        Self {
            name: s.name.clone(),
            url: s.url.clone(),
            genre: s.genre.clone(),
            country: s.country.clone(),
            bitrate: s.bitrate,
        }
    }
}

impl From<SavedStation> for Station {
    fn from(s: SavedStation) -> Self {
        Self {
            name: s.name,
            url: s.url,
            genre: s.genre,
            country: s.country,
            bitrate: s.bitrate,
        }
    }
}

/// Helper to map dynamic micro-genres to static parent categories.
pub fn resolve_parent_genre(subgenre: &str) -> &'static str {
    let s = subgenre.to_lowercase();
    if s.contains("synthwave") || s.contains("chillsynth") || s.contains("darksynth") || s.contains("retrowave") {
        "Synthwave"
    } else if s.contains("ambient") || s.contains("chillout") || s.contains("drone") || s.contains("space") {
        "Ambient"
    } else if s.contains("rock") || s.contains("metal") || s.contains("guitar") {
        "Rock"
    } else if s.contains("vaporwave") || s.contains("plaza") || s.contains("synthpop") {
        "Vaporwave"
    } else {
        "Other"
    }
}

impl Library {
    /// Load library from disk.
    /// On first launch (no file), seeds with starter stations.
    pub fn load(seed_stations: Vec<Station>) -> Self {
        let path = config_path();

        let (stations, settings) = if let Some(ref p) = path {
            if p.exists() {
                match fs::read_to_string(p) {
                    Ok(contents) => {
                        match serde_json::from_str::<LibraryFile>(&contents) {
                            Ok(file) => {
                                (file.stations.into_iter().map(Station::from).collect(), file.settings)
                            }
                            Err(_) => (seed_stations, Settings::default()), // corrupt file → use seeds
                        }
                    }
                    Err(_) => (seed_stations, Settings::default()),
                }
            } else {
                // First launch — seed and save
                let mut lib = Self {
                    stations: seed_stations,
                    available_genres: Vec::new(),
                    settings: Settings::default(),
                    path: path.clone(),
                };
                lib.rebuild_genres();
                lib.save();
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
        };
        lib.rebuild_genres();
        lib
    }

    /// Add a station to the library (deduplicates by URL).
    /// Returns true if actually added (not a duplicate).
    pub fn add(&mut self, station: Station) -> bool {
        if self.stations.iter().any(|s| s.url == station.url) {
            return false;
        }
        self.stations.push(station);
        self.rebuild_genres();
        self.save();
        true
    }

    /// Remove a station by URL. Returns true if removed.
    pub fn remove(&mut self, url: &str) -> bool {
        let before = self.stations.len();
        self.stations.retain(|s| s.url != url);
        let removed = self.stations.len() < before;
        if removed {
            self.rebuild_genres();
            self.save();
        }
        removed
    }

    /// Dynamically rebuild unique genres list, sorting them with "All" at the front.
    pub fn rebuild_genres(&mut self) {
        let genres: std::collections::HashSet<String> = self.stations
            .iter()
            .map(|s| resolve_parent_genre(&s.genre).to_string())
            .filter(|g| !g.is_empty())
            .collect();

        let mut sorted: Vec<String> = genres.into_iter().collect();
        sorted.sort_by_key(|a| a.to_lowercase());
        sorted.insert(0, "All".to_string());
        self.available_genres = sorted;
    }

    /// Check if a station URL is in the library.
    pub fn contains(&self, url: &str) -> bool {
        self.stations.iter().any(|s| s.url == url)
    }

    /// Save library to disk (best-effort).
    pub fn save(&self) {
        let Some(ref path) = self.path else { return };

        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let file = LibraryFile {
            version: 1,
            stations: self.stations.iter().map(SavedStation::from).collect(),
            settings: self.settings.clone(),
        };

        if let Ok(json) = serde_json::to_string_pretty(&file) {
            let _ = fs::write(path, json);
        }
    }
}

/// Config file path: ~/.config/driftfm/library.json
fn config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("driftfm").join("library.json"))
}
