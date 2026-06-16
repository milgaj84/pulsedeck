use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::radio::Station;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportSummary {
    pub added: usize,
    pub skipped: usize,
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

        let (stations, settings) = match path.as_ref() {
            Some(p) if p.exists() => match fs::read_to_string(p) {
                Ok(contents) => match parse_library_file(&contents) {
                    Ok((stations, settings, _warning)) => (stations, settings),
                    Err(_) => (Vec::new(), Settings::default()),
                },
                Err(_) => (Vec::new(), Settings::default()),
            },
            _ => (Vec::new(), Settings::default()),
        };

        let mut lib = Self {
            stations,
            available_genres: Vec::new(),
            settings,
            path,
            load_warnings: Vec::new(),
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

    /// Add a station to the library (deduplicates by URL).
    /// Returns true if actually added (not a duplicate).
    pub fn add(&mut self, station: Station) -> anyhow::Result<bool> {
        if self.stations.iter().any(|s| s.url == station.url) {
            return Ok(false);
        }
        self.stations.push(station);
        self.rebuild_genres();
        Ok(true)
    }

    /// Import a list of stations, merging by URL.
    pub fn import_stations(&mut self, stations: Vec<Station>) -> anyhow::Result<ImportSummary> {
        let mut added = 0;
        let mut skipped = 0;
        for s in stations {
            if self.contains(&s.url) {
                skipped += 1;
            } else {
                self.stations.push(s);
                added += 1;
            }
        }
        if added > 0 {
            self.rebuild_genres();
        }
        Ok(ImportSummary { added, skipped })
    }

    /// Remove a station by URL. Returns true if removed.
    pub fn remove(&mut self, url: &str) -> anyhow::Result<bool> {
        let before = self.stations.len();
        self.stations.retain(|s| s.url != url);
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
    crate::config::config_path(LIBRARY_FILE)
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
            load_warnings: Vec::new(),
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
            load_warnings: Vec::new(),
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
            load_warnings: Vec::new(),
        };
        lib.rebuild_genres();
        assert_eq!(lib.available_genres[0], "All");
        assert!(lib.available_genres.contains(&"Synthwave".to_string()));
        assert!(lib.available_genres.contains(&"Ambient".to_string()));
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

    #[test]
    fn test_import_stations() {
        let mut lib = Library::in_memory(vec![Station {
            name: "Test A".to_string(),
            url: "http://a".to_string(),
            genre: "Synthwave".to_string(),
            country: "US".to_string(),
            bitrate: 128,
        }]);

        let to_import = vec![
            Station {
                name: "Test A".to_string(),
                url: "http://a".to_string(),
                genre: "Synthwave".to_string(),
                country: "US".to_string(),
                bitrate: 128,
            },
            Station {
                name: "Test B".to_string(),
                url: "http://b".to_string(),
                genre: "Ambient".to_string(),
                country: "UK".to_string(),
                bitrate: 96,
            },
        ];

        let summary = lib.import_stations(to_import).unwrap();
        assert_eq!(summary.added, 1);
        assert_eq!(summary.skipped, 1);
        assert_eq!(lib.stations.len(), 2);
    }
}
