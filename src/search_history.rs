use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

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

        // Dedup: remove existing match
        if let Some(pos) = self.entries.iter().position(|e| e == trimmed) {
            self.entries.remove(pos);
        }

        self.entries.push_back(trimmed.to_string());

        // Cap at MAX_ENTRIES by removing from front (oldest)
        while self.entries.len() > MAX_ENTRIES {
            self.entries.pop_front();
        }

        true
    }

    pub fn get(&self, index: usize) -> Option<&str> {
        self.entries.get(index).map(|s| s.as_str())
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn iter_recent(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().rev().map(|s| s.as_str())
    }

    pub fn entries(&self) -> &VecDeque<String> {
        &self.entries
    }

    pub fn save(&self, path: &std::path::Path) -> Result<(), std::io::Error> {
        let file = SearchHistoryFile {
            version: 1,
            entries: self.entries.iter().cloned().collect(),
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(&file)
            .map_err(std::io::Error::other)?;
        std::fs::write(path, json)
    }

    pub fn load(path: &std::path::Path) -> Self {
        let contents = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return Self::new(),
        };
        let file: SearchHistoryFile = match serde_json::from_str(&contents) {
            Ok(f) => f,
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
            // Dedup
            if let Some(pos) = ring.entries.iter().position(|e| e == &trimmed) {
                ring.entries.remove(pos);
            }
            ring.entries.push_back(trimmed);
        }
        // Keep only the last MAX_ENTRIES
        while ring.entries.len() > MAX_ENTRIES {
            ring.entries.pop_front();
        }
        ring
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_and_get_basic() {
        let mut ring = SearchHistoryRing::new();
        assert!(ring.push("jazz"));
        assert_eq!(ring.get(0), Some("jazz"));
    }

    #[test]
    fn test_push_rejects_short_query() {
        let mut ring = SearchHistoryRing::new();
        assert!(!ring.push("a"));
        assert_eq!(ring.len(), 0);
    }

    #[test]
    fn test_push_rejects_long_query() {
        let mut ring = SearchHistoryRing::new();
        let long_query = "x".repeat(201);
        assert!(!ring.push(&long_query));
        assert_eq!(ring.len(), 0);
    }

    #[test]
    fn test_push_trims_whitespace() {
        let mut ring = SearchHistoryRing::new();
        assert!(ring.push("  jazz  "));
        assert_eq!(ring.get(0), Some("jazz"));
    }

    #[test]
    fn test_push_dedup_moves_to_back() {
        let mut ring = SearchHistoryRing::new();
        ring.push("aa");
        ring.push("bb");
        ring.push("aa");
        assert_eq!(ring.len(), 2);
        assert_eq!(ring.get(0), Some("bb"));
        assert_eq!(ring.get(1), Some("aa"));
    }

    #[test]
    fn test_push_capacity_cap() {
        let mut ring = SearchHistoryRing::new();
        for i in 0..11 {
            ring.push(&format!("query_{:02}", i));
        }
        assert_eq!(ring.len(), 10);
        // Oldest (query_00) should be gone
        assert_eq!(ring.get(0), Some("query_01"));
        assert_eq!(ring.get(9), Some("query_10"));
    }

    #[test]
    fn test_iter_recent_order() {
        let mut ring = SearchHistoryRing::new();
        ring.push("aa");
        ring.push("bb");
        ring.push("cc");
        let recent: Vec<&str> = ring.iter_recent().collect();
        assert_eq!(recent, vec!["cc", "bb", "aa"]);
    }

    #[test]
    fn test_from_entries_deduplicates() {
        let entries = vec![
            "jazz".to_string(),
            "lofi".to_string(),
            "jazz".to_string(),
        ];
        let ring = SearchHistoryRing::from_entries(entries);
        assert_eq!(ring.len(), 2);
        assert_eq!(ring.get(0), Some("lofi"));
        assert_eq!(ring.get(1), Some("jazz"));
    }

    #[test]
    fn test_from_entries_caps_at_10() {
        let entries: Vec<String> = (0..15).map(|i| format!("entry_{:02}", i)).collect();
        let ring = SearchHistoryRing::from_entries(entries);
        assert_eq!(ring.len(), 10);
        // Should keep the last 10 (entry_05 through entry_14)
        assert_eq!(ring.get(0), Some("entry_05"));
        assert_eq!(ring.get(9), Some("entry_14"));
    }

    #[test]
    fn test_empty_ring_operations() {
        let ring = SearchHistoryRing::new();
        assert_eq!(ring.len(), 0);
        assert!(ring.is_empty());
        assert_eq!(ring.get(0), None);
        assert_eq!(ring.iter_recent().count(), 0);
    }

    #[test]
    fn test_save_and_load_round_trip() {
        let dir = std::env::temp_dir().join("pulsedeck_test_search_history_roundtrip");
        let path = dir.join("search_history.json");
        let _ = std::fs::remove_file(&path);

        let mut ring = SearchHistoryRing::new();
        ring.push("jazz");
        ring.push("lofi beats");
        ring.push("synthwave");
        ring.save(&path).unwrap();

        let loaded = SearchHistoryRing::load(&path);
        assert_eq!(loaded.len(), 3);
        assert_eq!(loaded.get(0), Some("jazz"));
        assert_eq!(loaded.get(1), Some("lofi beats"));
        assert_eq!(loaded.get(2), Some("synthwave"));

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn test_load_missing_file_returns_empty() {
        let path = std::path::PathBuf::from("/tmp/pulsedeck_nonexistent_file_xyz.json");
        let ring = SearchHistoryRing::load(&path);
        assert!(ring.is_empty());
    }

    #[test]
    fn test_load_invalid_json_returns_empty() {
        let dir = std::env::temp_dir().join("pulsedeck_test_search_history_invalid");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("search_history.json");
        std::fs::write(&path, "not valid json {{{").unwrap();

        let ring = SearchHistoryRing::load(&path);
        assert!(ring.is_empty());

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;
    use std::collections::HashSet;

    // Feature: v090-features, Property 8: Search history ring capacity and uniqueness invariant
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Validates: Requirements 4.1, 4.7**
        #[test]
        fn ring_capacity_and_uniqueness(
            queries in prop::collection::vec(".{1,250}", 0..=30),
        ) {
            let mut ring = SearchHistoryRing::new();
            for q in &queries {
                ring.push(q);
            }
            prop_assert!(ring.len() <= 10, "ring exceeded capacity: {}", ring.len());

            let entries: Vec<&str> = ring.entries().iter().map(|s| s.as_str()).collect();
            let unique: HashSet<&str> = entries.iter().copied().collect();
            prop_assert_eq!(
                entries.len(),
                unique.len(),
                "ring contains duplicates: {:?}",
                entries
            );
        }
    }

    // Feature: v090-features, Property 9: Search history ring push acceptance filter
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Validates: Requirements 4.2**
        #[test]
        fn push_acceptance_filter(
            query in ".{0,250}",
        ) {
            let mut ring = SearchHistoryRing::new();
            let len_before = ring.len();
            let result = ring.push(&query);
            let trimmed = query.trim();

            if trimmed.len() >= 2 && trimmed.len() <= 200 {
                prop_assert!(result, "push should accept valid query: {:?}", trimmed);
                // From empty ring, push always increases len by 1
                prop_assert_eq!(ring.len(), len_before + 1);
            } else {
                prop_assert!(!result, "push should reject invalid query: {:?}", trimmed);
                prop_assert_eq!(ring.len(), len_before, "ring should remain unchanged");
            }
        }
    }
}
