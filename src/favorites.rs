use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::radio::Station;

const NEW_CONFIG_DIR: &str = "pulsedeck";
const OLD_CONFIG_DIR: &str = "driftfm";
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

impl Library {
    /// Load library from disk.
    /// On first launch (no file), seeds with starter stations.
    pub fn load(seed_stations: Vec<Station>) -> Self {
        let path = config_path();

        let (stations, settings) = if let Some(ref p) = path {
            if p.exists() {
                match fs::read_to_string(p) {
                    Ok(contents) => match serde_json::from_str::<LibraryFile>(&contents) {
                        Ok(file) => (
                            file.stations.into_iter().map(Station::from).collect(),
                            file.settings,
                        ),
                        Err(_) => (seed_stations, Settings::default()), // corrupt file → use seeds
                    },
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
                // TODO: return load warnings to App so first-launch seed save failures can be shown.
                let _ = lib.save();
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

    /// Create an in-memory library for tests without touching the user's config file.
    #[cfg(test)]
    pub fn in_memory(stations: Vec<Station>) -> Self {
        let mut lib = Self {
            stations,
            available_genres: Vec::new(),
            settings: Settings::default(),
            path: None,
        };
        lib.rebuild_genres();
        lib
    }

    /// Add a station to the library (deduplicates by URL).
    /// Returns true if actually added (not a duplicate).
    pub fn add(&mut self, station: Station) -> anyhow::Result<bool> {
        if self.stations.iter().any(|s| s.url == station.url) {
            return Ok(false);
        }
        self.stations.push(station);
        self.rebuild_genres();
        self.save()?;
        Ok(true)
    }

    /// Remove a station by URL. Returns true if removed.
    pub fn remove(&mut self, url: &str) -> anyhow::Result<bool> {
        let before = self.stations.len();
        self.stations.retain(|s| s.url != url);
        let removed = self.stations.len() < before;
        if removed {
            self.rebuild_genres();
            self.save()?;
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

    /// Check if a station URL is in the library.
    pub fn contains(&self, url: &str) -> bool {
        self.stations.iter().any(|s| s.url == url)
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
    dirs::config_dir().map(|base| {
        let new_path = library_path(&base, NEW_CONFIG_DIR);
        let old_path = library_path(&base, OLD_CONFIG_DIR);
        migrate_file_if_needed(&old_path, &new_path);
        new_path
    })
}

fn library_path(base: &Path, config_dir: &str) -> PathBuf {
    base.join(config_dir).join(LIBRARY_FILE)
}

fn migrate_file_if_needed(old_path: &Path, new_path: &Path) {
    if new_path.exists() || !old_path.exists() {
        return;
    }

    if let Some(parent) = new_path.parent() {
        if fs::create_dir_all(parent).is_err() {
            return;
        }
    }

    let _ = fs::copy(old_path, new_path);
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_library_add_deduplicates() {
        let mut lib = Library {
            stations: vec![],
            available_genres: vec![],
            settings: Settings::default(),
            path: None,
        };
        let station = Station {
            name: "Test".to_string(),
            url: "http://test".to_string(),
            genre: "Synthwave".to_string(),
            country: "US".to_string(),
            bitrate: 128,
        };
        assert!(lib.add(station.clone()).unwrap());
        assert!(!lib.add(station).unwrap());
        assert_eq!(lib.stations.len(), 1);
    }

    #[test]
    fn test_library_remove() {
        let mut lib = Library {
            stations: vec![Station {
                name: "Test".to_string(),
                url: "http://test".to_string(),
                genre: "Synthwave".to_string(),
                country: "US".to_string(),
                bitrate: 128,
            }],
            available_genres: vec![],
            settings: Settings::default(),
            path: None,
        };
        assert!(lib.remove("http://test").unwrap());
        assert!(!lib.remove("http://missing").unwrap());
        assert!(lib.stations.is_empty());
    }

    #[test]
    fn test_library_contains() {
        let lib = Library {
            stations: vec![Station {
                name: "Test".to_string(),
                url: "http://test".to_string(),
                genre: "Synthwave".to_string(),
                country: "US".to_string(),
                bitrate: 128,
            }],
            available_genres: vec![],
            settings: Settings::default(),
            path: None,
        };
        assert!(lib.contains("http://test"));
        assert!(!lib.contains("http://missing"));
    }

    #[test]
    fn test_rebuild_genres() {
        let mut lib = Library {
            stations: vec![
                Station {
                    name: "A".to_string(),
                    url: "http://a".to_string(),
                    genre: "Synthwave".to_string(),
                    country: "US".to_string(),
                    bitrate: 128,
                },
                Station {
                    name: "B".to_string(),
                    url: "http://b".to_string(),
                    genre: "Ambient".to_string(),
                    country: "US".to_string(),
                    bitrate: 128,
                },
            ],
            available_genres: vec![],
            settings: Settings::default(),
            path: None,
        };
        lib.rebuild_genres();
        assert_eq!(lib.available_genres[0], "All");
        assert!(lib.available_genres.contains(&"Synthwave".to_string()));
        assert!(lib.available_genres.contains(&"Ambient".to_string()));
    }

    #[test]
    fn library_path_uses_requested_config_dir() {
        let base = PathBuf::from("/tmp/config");

        assert_eq!(
            library_path(&base, NEW_CONFIG_DIR),
            PathBuf::from("/tmp/config/pulsedeck/library.json")
        );
        assert_eq!(
            library_path(&base, OLD_CONFIG_DIR),
            PathBuf::from("/tmp/config/driftfm/library.json")
        );
    }

    #[test]
    fn test_in_memory_library_rebuilds_genres_without_path() {
        let lib = Library::in_memory(vec![Station {
            name: "Test".to_string(),
            url: "http://test".to_string(),
            genre: "Synthwave".to_string(),
            country: "US".to_string(),
            bitrate: 128,
        }]);
        assert!(lib.path.is_none());
        assert!(lib.available_genres.contains(&"Synthwave".to_string()));
    }
}
