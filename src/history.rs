use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[cfg(not(test))]
use std::fs;
#[cfg(not(test))]
use std::path::{Path, PathBuf};

const MAX_ENTRIES: usize = 500;
#[cfg(not(test))]
const NEW_CONFIG_DIR: &str = "pulsedeck";
#[cfg(not(test))]
const OLD_CONFIG_DIR: &str = "driftfm";
#[cfg(not(test))]
const HISTORY_FILE: &str = "history.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub title: String,
    pub station: String,
    pub at: String, // Unix seconds since epoch as string
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
            version: 1,
            entries: VecDeque::new(),
        }
    }
}

impl History {
    #[cfg(not(test))]
    pub fn load() -> Self {
        let Some(path) = history_path() else {
            return Self::default();
        };

        fs::read_to_string(path)
            .ok()
            .and_then(|contents| serde_json::from_str::<Self>(&contents).ok())
            .map(Self::sanitized)
            .unwrap_or_default()
    }

    #[cfg(test)]
    pub fn load() -> Self {
        Self::default()
    }

    #[cfg(not(test))]
    pub fn save(&self) -> anyhow::Result<()> {
        let Some(path) = history_path() else {
            return Ok(());
        };

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let json = serde_json::to_string_pretty(&self.clone().sanitized())?;
        fs::write(path, json)?;
        Ok(())
    }

    #[cfg(test)]
    pub fn save(&self) -> anyhow::Result<()> {
        Ok(())
    }

    fn sanitized(mut self) -> Self {
        while self.entries.len() > MAX_ENTRIES {
            self.entries.pop_front();
        }
        self
    }

    pub fn record(&mut self, title: String, station: String) {
        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs().to_string())
            .unwrap_or_default();

        self.entries.push_back(HistoryEntry {
            title,
            station,
            at: now_secs,
        });

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

#[cfg(not(test))]
fn history_path() -> Option<PathBuf> {
    dirs::config_dir().map(|base| {
        let new_path = history_path_for(&base, NEW_CONFIG_DIR);
        let old_path = history_path_for(&base, OLD_CONFIG_DIR);
        migrate_file_if_needed(&old_path, &new_path);
        new_path
    })
}

#[cfg(not(test))]
fn history_path_for(base: &Path, config_dir: &str) -> PathBuf {
    base.join(config_dir).join(HISTORY_FILE)
}

#[cfg(not(test))]
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
    fn test_record_caps_at_500() {
        let mut history = History::default();
        for i in 0..600 {
            history.record(format!("Title {}", i), "Station".to_string());
        }
        assert_eq!(history.entries.len(), 500);
        assert_eq!(history.entries[0].title, "Title 100");
        assert_eq!(history.entries[499].title, "Title 599");
    }

    #[test]
    fn test_recent_ordering() {
        let mut history = History::default();
        history.record("A".to_string(), "S".to_string());
        history.record("B".to_string(), "S".to_string());
        let recent: Vec<_> = history.recent(10).collect();
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].title, "B");
        assert_eq!(recent[1].title, "A");
    }

    #[test]
    fn test_sanitized() {
        let mut history = History::default();
        for i in 0..550 {
            history.entries.push_back(HistoryEntry {
                title: format!("T{}", i),
                station: "S".to_string(),
                at: "0".to_string(),
            });
        }
        assert_eq!(history.entries.len(), 550);
        history = history.sanitized();
        assert_eq!(history.entries.len(), 500);
        assert_eq!(history.entries[0].title, "T50");
    }
}
