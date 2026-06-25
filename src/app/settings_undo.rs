use std::collections::HashMap;

/// Captures the state of a single setting before a change.
#[derive(Debug, Clone, PartialEq)]
pub enum SettingSnapshot {
    Bool(bool),
    String(String),
    OptionalString(Option<String>),
}

/// Per-row single-level undo buffer.
/// Stores at most one previous value per SettingRow.
#[derive(Debug, Default)]
pub struct SettingsUndoStack {
    entries: HashMap<usize, SettingSnapshot>,
}

impl SettingsUndoStack {
    pub fn new() -> Self {
        Self::default()
    }

    /// Stores snapshot for row, overwrites existing.
    pub fn capture(&mut self, row_index: usize, snapshot: SettingSnapshot) {
        self.entries.insert(row_index, snapshot);
    }

    /// Removes and returns entry for row.
    pub fn take(&mut self, row_index: usize) -> Option<SettingSnapshot> {
        self.entries.remove(&row_index)
    }

    /// Removes all entries.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Checks if entry exists for row.
    #[allow(dead_code)]
    pub fn has_entry(&self, row_index: usize) -> bool {
        self.entries.contains_key(&row_index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capture_and_take_round_trip() {
        let mut stack = SettingsUndoStack::new();
        stack.capture(0, SettingSnapshot::Bool(true));
        assert_eq!(stack.take(0), Some(SettingSnapshot::Bool(true)));
    }

    #[test]
    fn test_overwrite_replaces_previous() {
        let mut stack = SettingsUndoStack::new();
        stack.capture(1, SettingSnapshot::Bool(false));
        stack.capture(1, SettingSnapshot::String("theme-b".to_string()));
        assert_eq!(
            stack.take(1),
            Some(SettingSnapshot::String("theme-b".to_string()))
        );
    }

    #[test]
    fn test_take_consumes_entry() {
        let mut stack = SettingsUndoStack::new();
        stack.capture(2, SettingSnapshot::Bool(true));
        let _ = stack.take(2);
        assert!(!stack.has_entry(2));
    }

    #[test]
    fn test_clear_empties_all() {
        let mut stack = SettingsUndoStack::new();
        stack.capture(0, SettingSnapshot::Bool(true));
        stack.capture(1, SettingSnapshot::String("x".to_string()));
        stack.capture(2, SettingSnapshot::OptionalString(Some("y".to_string())));
        stack.clear();
        assert!(!stack.has_entry(0));
        assert!(!stack.has_entry(1));
        assert!(!stack.has_entry(2));
    }

    #[test]
    fn test_has_entry_reports_correctly() {
        let mut stack = SettingsUndoStack::new();
        stack.capture(3, SettingSnapshot::Bool(false));
        assert!(stack.has_entry(3));
        let _ = stack.take(3);
        assert!(!stack.has_entry(3));
    }

    #[test]
    fn test_take_nonexistent_returns_none() {
        let mut stack = SettingsUndoStack::new();
        assert_eq!(stack.take(99), None);
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    // Feature: v100-features, Property 9: Settings undo single-level overwrite and consume

    fn arb_snapshot() -> impl Strategy<Value = SettingSnapshot> {
        prop_oneof![
            any::<bool>().prop_map(SettingSnapshot::Bool),
            "[a-z]{1,10}".prop_map(SettingSnapshot::String),
            any::<Option<bool>>()
                .prop_map(|opt| { SettingSnapshot::OptionalString(opt.map(|b| format!("{}", b))) }),
        ]
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Validates: Requirements 5.1, 5.2, 5.4**
        #[test]
        fn undo_single_level_overwrite_and_consume(
            ops in prop::collection::vec((0usize..6, arb_snapshot()), 1..=20),
        ) {
            let mut stack = SettingsUndoStack::new();

            // Track the last snapshot captured per row
            let mut expected: std::collections::HashMap<usize, SettingSnapshot> =
                std::collections::HashMap::new();

            for (row, snapshot) in &ops {
                stack.capture(*row, snapshot.clone());
                expected.insert(*row, snapshot.clone());
            }

            // For each row that was captured, take() returns the most recent
            for (row, snapshot) in &expected {
                prop_assert!(stack.has_entry(*row));
                let taken = stack.take(*row);
                prop_assert_eq!(taken.as_ref(), Some(snapshot));
                // After take, has_entry is false
                prop_assert!(!stack.has_entry(*row));
            }
        }
    }
}
