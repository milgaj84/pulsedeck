//! Radio Browser API availability tracking.
//! First-notice-only suppression: shows a degradation notice on the first failure,
//! suppresses subsequent failures until the service recovers.

/// Tracks whether the Radio Browser API is currently unavailable and whether
/// the user has already been notified in this session.
///
/// Key invariant: once `notice_shown` is true, `mark_unavailable()` returns
/// false (suppresses repeated notices) until `mark_available()` resets state.
#[derive(Debug, Clone)]
pub struct RadioBrowserStatus {
    unavailable: bool,
    notice_shown: bool,
}

impl RadioBrowserStatus {
    pub fn new() -> Self {
        Self {
            unavailable: false,
            notice_shown: false,
        }
    }

    /// Mark the API as unavailable. Returns `true` if a notice should be
    /// shown to the user (first time only). Subsequent calls return `false`.
    pub fn mark_unavailable(&mut self) -> bool {
        self.unavailable = true;
        if self.notice_shown {
            false
        } else {
            self.notice_shown = true;
            true
        }
    }

    /// Mark the API as available again. Resets all state so a future
    /// failure will produce a fresh notice.
    pub fn mark_available(&mut self) {
        self.unavailable = false;
        self.notice_shown = false;
    }

    /// Whether the API is currently considered unavailable.
    pub fn is_unavailable(&self) -> bool {
        self.unavailable
    }
}

impl Default for RadioBrowserStatus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_state_is_available() {
        let status = RadioBrowserStatus::new();
        assert!(!status.is_unavailable());
    }

    #[test]
    fn test_first_mark_unavailable_returns_true() {
        let mut status = RadioBrowserStatus::new();
        assert!(status.mark_unavailable());
        assert!(status.is_unavailable());
    }

    #[test]
    fn test_second_mark_unavailable_returns_false() {
        let mut status = RadioBrowserStatus::new();
        assert!(status.mark_unavailable());
        assert!(!status.mark_unavailable());
        assert!(status.is_unavailable());
    }

    #[test]
    fn test_mark_available_resets_state() {
        let mut status = RadioBrowserStatus::new();
        status.mark_unavailable();
        status.mark_available();
        assert!(!status.is_unavailable());
    }

    #[test]
    fn test_cycle_available_unavailable_shows_notice_again() {
        let mut status = RadioBrowserStatus::new();

        // First failure cycle
        assert!(status.mark_unavailable());
        assert!(!status.mark_unavailable());

        // Recovery
        status.mark_available();

        // Second failure cycle — notice shown again
        assert!(status.mark_unavailable());
        assert!(!status.mark_unavailable());
    }

    #[test]
    fn test_many_unavailable_calls_only_first_returns_true() {
        let mut status = RadioBrowserStatus::new();
        assert!(status.mark_unavailable());
        for _ in 0..100 {
            assert!(!status.mark_unavailable());
        }
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// Property 9: Only the first mark_unavailable() call returns true until mark_available() resets.
        #[test]
        fn prop_notice_suppression(call_count in 1..100usize) {
            let mut status = RadioBrowserStatus::new();
            let first = status.mark_unavailable();
            prop_assert!(first, "first call should return true");
            for _ in 1..call_count {
                let result = status.mark_unavailable();
                prop_assert!(!result, "subsequent calls should return false");
            }
        }
    }
}
