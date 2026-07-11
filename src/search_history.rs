use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::Path;

const MAX_ENTRIES: usize = 10;

#[derive(Debug, Serialize, Deserialize)]
struct SearchHistoryFile {
    #[serde(default = "default_version")]
    version: u32,
    #[serde(default)]
    entries: Vec<String>,
}

fn default_version() -> u32 {
    1
}

#[derive(Debug, Clone)]
pub struct SearchHistoryRing {
    entries: VecDeque<String>,
}

impl Default for SearchHistoryRing {
    fn default() -> Self {
        Self::new()
    }
}

impl SearchHistoryRing {
    pub fn new() -> Self {
        Self {
            entries: VecDeque::new(),
        }
    }

    pub fn push(&mut self, query: &str) -> bool {
        let trimmed = query.trim();
        if trimmed.len() < 2 || trimmed.len() > 200 {
            return false;
        }

        if let Some(position) = self.entries.iter().position(|entry| entry == trimmed) {
            self.entries.remove(position);
        }
        self.entries.push_back(trimmed.to_string());

        while self.entries.len() > MAX_ENTRIES {
            self.entries.pop_front();
        }
        true
    }

    pub fn get(&self, index: usize) -> Option<&str> {
        self.entries.get(index).map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn iter_recent(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().rev().map(String::as_str)
    }

    pub fn entries(&self) -> &VecDeque<String> {
        &self.entries
    }

    pub fn save(&self, path: &Path) -> Result<(), std::io::Error> {
        let file = SearchHistoryFile {
            version: default_version(),
            entries: self.entries.iter().cloned().collect(),
        };
        let bytes = serde_json::to_vec_pretty(&file).map_err(std::io::Error::other)?;
        crate::persistence::atomic_write(path, &bytes)
    }

    pub fn load(path: &Path) -> Self {
        let contents = match std::fs::read_to_string(path) {
            Ok(contents) => contents,
            Err(_) => return Self::new(),
        };
        let file = match serde_json::from_str::<SearchHistoryFile>(&contents) {
            Ok(file) => file,
            Err(_) => return Self::new(),
        };
        Self::from_entries(file.entries)
    }

    pub fn from_entries(entries: Vec<String>) -> Self {
        let mut ring = Self::new();
        for entry in entries {
            let trimmed = entry.trim().to_string();
            if trimmed.len() < 2 || trimmed.len() > 200 {
                continue;
            }
            if let Some(position) = ring.entries.iter().position(|existing| existing == &trimmed) {
                ring.entries.remove(position);
            }
            ring.entries.push_back(trimmed);
        }
        while ring.entries.len() > MAX_ENTRIES {
            ring.entries.pop_front();
        }
        ring
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_path(name: &str) -> std::path::PathBuf {
        let sequence = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir()
            .join(format!(
                "pulsedeck-search-history-{name}-{}-{sequence}",
                std::process::id()
            ))
            .join("search_history.json")
    }

    #[test]
    fn push_and_get_basic() {
        let mut ring = SearchHistoryRing::new();
        assert!(ring.push("jazz"));
        assert_eq!(ring.get(0), Some("jazz"));
    }

    #[test]
    fn push_rejects_invalid_lengths() {
        let mut ring = SearchHistoryRing::new();
        assert!(!ring.push("a"));
        assert!(!ring.push(&"x".repeat(201)));
        assert!(ring.is_empty());
    }

    #[test]
    fn push_trims_and_deduplicates_to_most_recent() {
        let mut ring = SearchHistoryRing::new();
        assert!(ring.push("  jazz  "));
        assert!(ring.push("lofi"));
        assert!(ring.push("jazz"));
        assert_eq!(ring.entries().iter().cloned().collect::<Vec<_>>(), vec!["lofi", "jazz"]);
    }

    #[test]
    fn capacity_keeps_ten_most_recent_queries() {
        let mut ring = SearchHistoryRing::new();
        for index in 0..11 {
            ring.push(&format!("query_{index:02}"));
        }
        assert_eq!(ring.len(), MAX_ENTRIES);
        assert_eq!(ring.get(0), Some("query_01"));
        assert_eq!(ring.get(9), Some("query_10"));
    }

    #[test]
    fn iter_recent_returns_newest_first() {
        let ring = SearchHistoryRing::from_entries(vec![
            "aa".to_string(),
            "bb".to_string(),
            "cc".to_string(),
        ]);
        assert_eq!(ring.iter_recent().collect::<Vec<_>>(), vec!["cc", "bb", "aa"]);
    }

    #[test]
    fn from_entries_filters_deduplicates_and_caps() {
        let mut entries = vec!["x".to_string(), "jazz".to_string(), "jazz".to_string()];
        entries.extend((0..15).map(|index| format!("entry_{index:02}")));
        let ring = SearchHistoryRing::from_entries(entries);
        assert_eq!(ring.len(), MAX_ENTRIES);
        assert_eq!(ring.get(0), Some("entry_05"));
        assert_eq!(ring.get(9), Some("entry_14"));
    }

    #[test]
    fn real_persistence_round_trips() {
        let path = temp_path("roundtrip");
        let mut ring = SearchHistoryRing::new();
        ring.push("jazz");
        ring.push("lofi beats");
        ring.push("synthwave");
        ring.save(&path).unwrap();

        let loaded = SearchHistoryRing::load(&path);
        assert_eq!(loaded.entries().iter().cloned().collect::<Vec<_>>(), vec![
            "jazz",
            "lofi beats",
            "synthwave",
        ]);
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn replacement_preserves_previous_search_history_as_backup() {
        let path = temp_path("backup");
        let first = SearchHistoryRing::from_entries(vec!["jazz".to_string()]);
        let second = SearchHistoryRing::from_entries(vec!["jazz".to_string(), "rock".to_string()]);
        first.save(&path).unwrap();
        second.save(&path).unwrap();

        let backup = crate::persistence::backup_path(&path);
        let previous = SearchHistoryRing::load(&backup);
        assert_eq!(previous.entries().iter().cloned().collect::<Vec<_>>(), vec!["jazz"]);
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn missing_or_invalid_file_returns_empty() {
        let missing = temp_path("missing");
        assert!(SearchHistoryRing::load(&missing).is_empty());

        let invalid = temp_path("invalid");
        fs::create_dir_all(invalid.parent().unwrap()).unwrap();
        fs::write(&invalid, "not valid json {{{").unwrap();
        assert!(SearchHistoryRing::load(&invalid).is_empty());
        let _ = fs::remove_dir_all(invalid.parent().unwrap());
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;
    use std::collections::HashSet;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn ring_capacity_and_uniqueness(
            queries in prop::collection::vec(".{1,250}", 0..=30),
        ) {
            let mut ring = SearchHistoryRing::new();
            for query in &queries {
                ring.push(query);
            }
            prop_assert!(ring.len() <= MAX_ENTRIES);
            let entries: Vec<&str> = ring.entries().iter().map(String::as_str).collect();
            let unique: HashSet<&str> = entries.iter().copied().collect();
            prop_assert_eq!(entries.len(), unique.len());
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn push_acceptance_filter(query in ".{0,250}") {
            let mut ring = SearchHistoryRing::new();
            let length_before = ring.len();
            let result = ring.push(&query);
            let trimmed = query.trim();

            if (2..=200).contains(&trimmed.len()) {
                prop_assert!(result);
                prop_assert_eq!(ring.len(), length_before + 1);
            } else {
                prop_assert!(!result);
                prop_assert_eq!(ring.len(), length_before);
            }
        }
    }
}
