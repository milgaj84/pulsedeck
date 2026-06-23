use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::radio::normalized_station_url;

/// A set of favorited station URLs, stored in normalized form.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct FavoritesSet {
    urls: HashSet<String>,
}

impl FavoritesSet {
    /// Toggle favorite status for a URL.
    /// Returns `true` if the URL is now favorited, `false` if it was removed.
    /// Returns `false` for empty/whitespace-only URLs without modifying the set.
    pub fn toggle(&mut self, url: &str) -> bool {
        let Some(normalized) = Self::normalize(url) else {
            return false;
        };
        if self.urls.remove(&normalized) {
            false
        } else {
            self.urls.insert(normalized);
            true
        }
    }

    /// Check if a URL is in the favorites set.
    /// Returns `false` for empty/whitespace-only URLs.
    pub fn contains(&self, url: &str) -> bool {
        let Some(normalized) = Self::normalize(url) else {
            return false;
        };
        self.urls.contains(&normalized)
    }

    /// Return the number of favorites.
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.urls.len()
    }

    /// Check if the favorites set is empty.
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.urls.is_empty()
    }

    /// Normalize a URL for storage/lookup.
    /// Returns `None` for empty or whitespace-only URLs.
    fn normalize(url: &str) -> Option<String> {
        let trimmed = url.trim();
        if trimmed.is_empty() {
            return None;
        }
        Some(normalized_station_url(url))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_adds_url_to_empty_set() {
        let mut set = FavoritesSet::default();
        let result = set.toggle("http://example.com/stream");
        assert!(result);
        assert!(set.contains("http://example.com/stream"));
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn toggle_removes_url_when_already_favorited() {
        let mut set = FavoritesSet::default();
        set.toggle("http://example.com/stream");
        let result = set.toggle("http://example.com/stream");
        assert!(!result);
        assert!(!set.contains("http://example.com/stream"));
        assert_eq!(set.len(), 0);
    }

    #[test]
    fn contains_returns_true_after_toggle_on() {
        let mut set = FavoritesSet::default();
        set.toggle("http://example.com/stream");
        assert!(set.contains("http://example.com/stream"));
    }

    #[test]
    fn contains_returns_false_for_empty_url() {
        let mut set = FavoritesSet::default();
        set.toggle("http://example.com/stream");
        assert!(!set.contains(""));
    }

    #[test]
    fn contains_returns_false_for_whitespace_only_url() {
        let mut set = FavoritesSet::default();
        set.toggle("http://example.com/stream");
        assert!(!set.contains("   "));
        assert!(!set.contains("\t\n"));
    }

    #[test]
    fn toggle_returns_false_for_empty_url() {
        let mut set = FavoritesSet::default();
        assert!(!set.toggle(""));
        assert!(set.is_empty());
    }

    #[test]
    fn toggle_returns_false_for_whitespace_only_url() {
        let mut set = FavoritesSet::default();
        assert!(!set.toggle("   "));
        assert!(set.is_empty());
    }

    #[test]
    fn url_normalization_equivalence_trailing_slash() {
        let mut set = FavoritesSet::default();
        set.toggle("http://example.com/stream/");
        assert!(set.contains("http://example.com/stream"));
        assert!(set.contains("http://example.com/stream/"));
    }

    #[test]
    fn url_normalization_equivalence_case() {
        let mut set = FavoritesSet::default();
        set.toggle("HTTP://Example.COM/Stream");
        assert!(set.contains("http://example.com/stream"));
        assert!(set.contains("HTTP://EXAMPLE.COM/STREAM"));
    }

    #[test]
    fn url_normalization_equivalence_whitespace() {
        let mut set = FavoritesSet::default();
        set.toggle("  http://example.com/stream  ");
        assert!(set.contains("http://example.com/stream"));
        assert!(set.contains("  http://example.com/stream  "));
    }

    #[test]
    fn toggle_self_inverse() {
        let mut set = FavoritesSet::default();
        let url = "http://example.com/stream";

        // Initially not contained
        assert!(!set.contains(url));

        // First toggle adds
        set.toggle(url);
        assert!(set.contains(url));

        // Second toggle removes — back to original state
        set.toggle(url);
        assert!(!set.contains(url));
    }

    #[test]
    fn toggle_self_inverse_with_normalized_variants() {
        let mut set = FavoritesSet::default();

        // Toggle with one variant
        set.toggle("http://example.com/stream/");
        assert!(set.contains("http://example.com/stream"));

        // Toggle off with a different variant of the same URL
        set.toggle("  HTTP://Example.COM/stream  ");
        assert!(!set.contains("http://example.com/stream"));
    }

    #[test]
    fn len_and_is_empty() {
        let mut set = FavoritesSet::default();
        assert!(set.is_empty());
        assert_eq!(set.len(), 0);

        set.toggle("http://a.com");
        assert!(!set.is_empty());
        assert_eq!(set.len(), 1);

        set.toggle("http://b.com");
        assert_eq!(set.len(), 2);

        set.toggle("http://a.com");
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn serde_round_trip() {
        let mut set = FavoritesSet::default();
        set.toggle("http://example.com/stream");
        set.toggle("http://other.com/radio");

        let json = serde_json::to_string(&set).unwrap();
        let deserialized: FavoritesSet = serde_json::from_str(&json).unwrap();

        assert_eq!(set, deserialized);
    }

    #[test]
    fn serde_default_on_missing_field() {
        #[derive(Deserialize)]
        struct Container {
            #[serde(default)]
            favorites: FavoritesSet,
        }

        let json = r#"{}"#;
        let container: Container = serde_json::from_str(json).unwrap();
        assert!(container.favorites.is_empty());
    }
}
