use serde::{Deserialize, Serialize};

use crate::radio::normalized_station_url;

/// Number of station preset slots (Alt+1 through Alt+5).
pub const STATION_SLOTS_COUNT: usize = 5;

/// Fixed station preset slots. Each slot holds an optional normalized URL.
/// Slots are user-assigned (Ctrl+1–5) and recalled (Alt+1–5).
/// Unlike a recency ring, positions never shift — a slot keeps its station
/// until the user explicitly reassigns it.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct StationSlots {
    slots: [Option<String>; STATION_SLOTS_COUNT],
}

impl StationSlots {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {
            slots: Default::default(),
        }
    }

    /// Assign a station URL to a slot (1-indexed).
    /// Returns false if index is out of range (0 or >5).
    pub fn assign(&mut self, index: usize, url: &str) -> bool {
        if index == 0 || index > STATION_SLOTS_COUNT {
            return false;
        }
        self.slots[index - 1] = Some(normalized_station_url(url));
        true
    }

    /// Get the station URL assigned to a slot (1-indexed).
    /// Returns None if index is out of range or slot is empty.
    pub fn get(&self, index: usize) -> Option<&str> {
        if index == 0 || index > STATION_SLOTS_COUNT {
            return None;
        }
        self.slots[index - 1].as_deref()
    }

    /// Clear a slot (1-indexed).
    #[allow(dead_code)]
    pub fn clear(&mut self, index: usize) {
        if index > 0 && index <= STATION_SLOTS_COUNT {
            self.slots[index - 1] = None;
        }
    }

    /// Check if a slot is occupied (1-indexed).
    #[allow(dead_code)]
    pub fn is_assigned(&self, index: usize) -> bool {
        self.get(index).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_slots_are_all_empty() {
        let slots = StationSlots::new();
        for i in 1..=5 {
            assert_eq!(slots.get(i), None);
            assert!(!slots.is_assigned(i));
        }
    }

    #[test]
    fn assign_and_get_slot() {
        let mut slots = StationSlots::new();
        slots.assign(1, "http://example.com/stream");
        assert_eq!(slots.get(1), Some("http://example.com/stream"));
        assert!(slots.is_assigned(1));
    }

    #[test]
    fn assign_normalizes_url() {
        let mut slots = StationSlots::new();
        slots.assign(2, "  HTTP://Example.COM/Stream/  ");
        assert_eq!(slots.get(2), Some("http://example.com/stream"));
    }

    #[test]
    fn assign_replaces_existing() {
        let mut slots = StationSlots::new();
        slots.assign(1, "http://old.com");
        slots.assign(1, "http://new.com");
        assert_eq!(slots.get(1), Some("http://new.com"));
    }

    #[test]
    fn assign_returns_false_for_zero_index() {
        let mut slots = StationSlots::new();
        assert!(!slots.assign(0, "http://a.com"));
    }

    #[test]
    fn assign_returns_false_for_out_of_range() {
        let mut slots = StationSlots::new();
        assert!(!slots.assign(6, "http://a.com"));
        assert!(!slots.assign(100, "http://a.com"));
    }

    #[test]
    fn get_returns_none_for_zero_index() {
        let slots = StationSlots::new();
        assert_eq!(slots.get(0), None);
    }

    #[test]
    fn get_returns_none_for_out_of_range() {
        let slots = StationSlots::new();
        assert_eq!(slots.get(6), None);
    }

    #[test]
    fn clear_removes_assignment() {
        let mut slots = StationSlots::new();
        slots.assign(3, "http://radio.com");
        slots.clear(3);
        assert_eq!(slots.get(3), None);
        assert!(!slots.is_assigned(3));
    }

    #[test]
    fn slots_are_independent() {
        let mut slots = StationSlots::new();
        slots.assign(1, "http://a.com");
        slots.assign(3, "http://c.com");
        slots.assign(5, "http://e.com");

        assert_eq!(slots.get(1), Some("http://a.com"));
        assert_eq!(slots.get(2), None);
        assert_eq!(slots.get(3), Some("http://c.com"));
        assert_eq!(slots.get(4), None);
        assert_eq!(slots.get(5), Some("http://e.com"));
    }

    #[test]
    fn serde_round_trip() {
        let mut slots = StationSlots::new();
        slots.assign(1, "http://a.com");
        slots.assign(3, "http://c.com");

        let json = serde_json::to_string(&slots).unwrap();
        let deserialized: StationSlots = serde_json::from_str(&json).unwrap();
        assert_eq!(slots, deserialized);
    }

    #[test]
    fn serde_default_produces_empty_slots() {
        #[derive(Deserialize)]
        struct Container {
            #[serde(default)]
            station_slots: StationSlots,
        }

        let container: Container = serde_json::from_str("{}").unwrap();
        for i in 1..=5 {
            assert_eq!(container.station_slots.get(i), None);
        }
    }

    #[test]
    fn backward_compat_old_recent_ring_field_ignored() {
        // If library.json has the old `recent_ring` field, StationSlots
        // is still constructed from its own field with default.
        #[derive(Deserialize)]
        struct Container {
            #[serde(default)]
            station_slots: StationSlots,
        }

        let json = r#"{"recent_ring": {"entries": ["http://a.com"]}}"#;
        let container: Container = serde_json::from_str(json).unwrap();
        for i in 1..=5 {
            assert_eq!(container.station_slots.get(i), None);
        }
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn assign_get_roundtrip(
            index in 1usize..=5,
            url in "[a-z]{5,20}",
        ) {
            let mut slots = StationSlots::new();
            slots.assign(index, &url);
            // get returns the normalized URL, so compare normalized
            let stored = slots.get(index).unwrap();
            prop_assert!(!stored.is_empty());
        }

        #[test]
        fn out_of_range_assign_always_fails(
            index in prop::sample::select(vec![0usize, 6, 7, 10, 100, 999]),
            url in "[a-z]{5,20}",
        ) {
            let mut slots = StationSlots::new();
            prop_assert!(!slots.assign(index, &url));
            prop_assert_eq!(slots.get(index), None);
        }
    }
}
