use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::Path;

const MAX_ENTRIES: usize = 500;
const HISTORY_FILE: &str = "history.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub title: String,
    pub station: String,
    pub at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct History {
    #[serde(default = "default_version")]
    version: u32,
    #[serde(default)]
    entries: VecDeque<HistoryEntry>,
}

fn default_version() -> u32 {
    1
}

impl Default for History {
    fn default() -> Self {
        Self {
            version: default_version(),
            entries: VecDeque::new(),
        }
    }
}

impl History {
    #[allow(dead_code)]
    pub fn load() -> Self {
        Self::load_with_warning().0
    }

    pub fn load_with_warning() -> (Self, Option<String>) {
        let Some(path) = crate::config::config_path(HISTORY_FILE) else {
            return (Self::default(), None);
        };
        Self::load_from_path(&path)
    }

    fn load_from_path(path: &Path) -> (Self, Option<String>) {
        let (history, warning) =
            crate::config::load_json_from_path_with_warning::<Self>(path, HISTORY_FILE);
        (history.sanitized(), warning)
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let Some(path) = crate::config::config_path(HISTORY_FILE) else {
            return Ok(());
        };
        self.save_to_path(&path)
    }

    fn save_to_path(&self, path: &Path) -> anyhow::Result<()> {
        crate::config::save_json_to_path(path, &self.clone().sanitized())
    }

    fn sanitized(mut self) -> Self {
        while self.entries.len() > MAX_ENTRIES {
            self.entries.pop_front();
        }
        self
    }

    pub fn record(&mut self, title: String, station: String) {
        let at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs().to_string())
            .unwrap_or_default();
        self.entries.push_back(HistoryEntry { title, station, at });
        while self.entries.len() > MAX_ENTRIES {
            self.entries.pop_front();
        }
    }

    pub fn recent(&self, limit: usize) -> impl Iterator<Item = &HistoryEntry> {
        self.entries.iter().rev().take(limit)
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path(name: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir()
            .join(format!("pulsedeck-history-{}-{nanos}", std::process::id()))
            .join(name)
    }

    #[test]
    fn record_caps_at_500_and_recent_is_newest_first() {
        let mut history = History::default();
        for index in 0..600 {
            history.record(format!("Title {index}"), "Station".to_string());
        }
        assert_eq!(history.entries.len(), MAX_ENTRIES);
        assert_eq!(history.entries.front().unwrap().title, "Title 100");
        assert_eq!(history.recent(1).next().unwrap().title, "Title 599");
    }

    #[test]
    fn real_persistence_round_trips_and_sanitizes() {
        let path = temp_path(HISTORY_FILE);
        let mut history = History::default();
        for index in 0..550 {
            history.entries.push_back(HistoryEntry {
                title: format!("T{index}"),
                station: "S".to_string(),
                at: "0".to_string(),
            });
        }
        history.save_to_path(&path).unwrap();
        let (loaded, warning) = History::load_from_path(&path);
        assert!(warning.is_none());
        assert_eq!(loaded.entries.len(), MAX_ENTRIES);
        assert_eq!(loaded.entries.front().unwrap().title, "T50");
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn replacement_preserves_previous_history_as_backup() {
        let path = temp_path(HISTORY_FILE);
        let mut first = History::default();
        first.record("First".to_string(), "A".to_string());
        first.save_to_path(&path).unwrap();
        let mut second = first.clone();
        second.record("Second".to_string(), "B".to_string());
        second.save_to_path(&path).unwrap();
        let backup = crate::persistence::backup_path(&path);
        let (previous, warning) = History::load_from_path(&backup);
        assert!(warning.is_none());
        assert_eq!(previous.entries.len(), 1);
        assert_eq!(previous.entries.front().unwrap().title, "First");
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn malformed_history_returns_default_and_warning() {
        let path = temp_path(HISTORY_FILE);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "{broken").unwrap();
        let (history, warning) = History::load_from_path(&path);
        assert!(history.is_empty());
        assert!(warning.unwrap().contains("Could not parse history.json"));
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }
}
