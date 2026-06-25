// Shared mtime-based change detection with configurable debounce and optional startup cooldown.

use std::time::{Duration, Instant, SystemTime};

/// Tracks file modification time for change detection with debounce and optional startup cooldown.
///
/// Used by both `ConfigWatcher` and `KeybindingWatcher` to avoid duplicating mtime+debounce logic.
pub struct MtimeDebounce {
    last_mtime: Option<SystemTime>,
    pending_since: Option<Instant>,
    debounce: Duration,
    cooldown_until: Option<Instant>,
}

impl MtimeDebounce {
    /// Create a new debounce tracker.
    ///
    /// - `debounce`: how long mtime must be stable before signaling readiness.
    /// - `cooldown`: optional startup cooldown during which all signals are suppressed.
    /// - `now`: the current instant (for testability).
    pub fn new(debounce: Duration, cooldown: Option<Duration>, now: Instant) -> Self {
        Self {
            last_mtime: None,
            pending_since: None,
            debounce,
            cooldown_until: cooldown.map(|c| now + c),
        }
    }

    /// Check whether a stabilized mtime change is ready for processing.
    ///
    /// Returns `true` only when:
    /// - The cooldown period has elapsed (if configured)
    /// - The mtime changed and then remained stable for the full debounce duration
    ///
    /// Returns `false` for `None` mtime (file missing/unreadable).
    pub fn check(&mut self, current_mtime: Option<SystemTime>, now: Instant) -> bool {
        if self.is_in_cooldown(now) {
            return false;
        }

        if self.mtime_changed(current_mtime) {
            self.last_mtime = current_mtime;
            self.pending_since = Some(now);
            return false;
        }

        self.is_debounce_elapsed(now)
    }

    fn is_in_cooldown(&self, now: Instant) -> bool {
        match self.cooldown_until {
            Some(until) => now < until,
            None => false,
        }
    }

    fn mtime_changed(&self, current: Option<SystemTime>) -> bool {
        match (current, self.last_mtime) {
            (Some(cur), Some(last)) => cur != last,
            (Some(_), None) => true,
            _ => false,
        }
    }

    fn is_debounce_elapsed(&mut self, now: Instant) -> bool {
        let pending = match self.pending_since {
            Some(t) => t,
            None => return false,
        };

        if now.duration_since(pending) < self.debounce {
            return false;
        }

        self.pending_since = None;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant, SystemTime};

    fn base_time() -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000)
    }

    #[test]
    fn test_cooldown_suppresses_all_signals() {
        let now = Instant::now();
        let mut debounce = MtimeDebounce::new(
            Duration::from_millis(100),
            Some(Duration::from_millis(500)),
            now,
        );

        let mtime = base_time();

        // During cooldown, even with mtime changes, returns false
        assert!(!debounce.check(Some(mtime), now + Duration::from_millis(50)));
        assert!(!debounce.check(Some(mtime), now + Duration::from_millis(200)));
        assert!(!debounce.check(Some(mtime), now + Duration::from_millis(499)));
    }

    #[test]
    fn test_debounce_prevents_premature_fire() {
        let now = Instant::now();
        let mut debounce = MtimeDebounce::new(Duration::from_millis(500), None, now);

        let mtime = base_time();

        // Detect change
        assert!(!debounce.check(Some(mtime), now));

        // Before debounce elapses — still false
        assert!(!debounce.check(Some(mtime), now + Duration::from_millis(100)));
        assert!(!debounce.check(Some(mtime), now + Duration::from_millis(200)));
        assert!(!debounce.check(Some(mtime), now + Duration::from_millis(499)));
    }

    #[test]
    fn test_normal_change_detection_after_cooldown() {
        let now = Instant::now();
        let mut debounce = MtimeDebounce::new(
            Duration::from_millis(100),
            Some(Duration::from_millis(200)),
            now,
        );

        let mtime = base_time();

        // During cooldown — suppressed
        assert!(!debounce.check(Some(mtime), now + Duration::from_millis(100)));

        // After cooldown, detect change
        let after_cooldown = now + Duration::from_millis(200);
        assert!(!debounce.check(Some(mtime), after_cooldown));

        // After debounce elapses — fires
        let after_debounce = after_cooldown + Duration::from_millis(100);
        assert!(debounce.check(Some(mtime), after_debounce));
    }

    #[test]
    fn test_none_mtime_returns_false() {
        let now = Instant::now();
        let mut debounce = MtimeDebounce::new(Duration::from_millis(100), None, now);

        // None mtime never triggers
        assert!(!debounce.check(None, now));
        assert!(!debounce.check(None, now + Duration::from_secs(10)));
    }

    #[test]
    fn test_rapid_mtime_changes_reset_debounce_timer() {
        let now = Instant::now();
        let mut debounce = MtimeDebounce::new(Duration::from_millis(500), None, now);

        let mtime1 = base_time();
        let mtime2 = base_time() + Duration::from_secs(1);

        // First change
        assert!(!debounce.check(Some(mtime1), now));

        // Second change at 200ms — resets debounce
        let t1 = now + Duration::from_millis(200);
        assert!(!debounce.check(Some(mtime2), t1));

        // 500ms from first change is NOT enough (timer reset at t1)
        let t2 = now + Duration::from_millis(500);
        assert!(!debounce.check(Some(mtime2), t2));

        // 500ms from t1 — fires
        let t3 = t1 + Duration::from_millis(500);
        assert!(debounce.check(Some(mtime2), t3));
    }

    #[test]
    fn test_fires_only_once_per_change() {
        let now = Instant::now();
        let mut debounce = MtimeDebounce::new(Duration::from_millis(100), None, now);

        let mtime = base_time();

        // Detect change
        assert!(!debounce.check(Some(mtime), now));

        // Fire after debounce
        assert!(debounce.check(Some(mtime), now + Duration::from_millis(100)));

        // No repeat firing without new change
        assert!(!debounce.check(Some(mtime), now + Duration::from_millis(200)));
        assert!(!debounce.check(Some(mtime), now + Duration::from_millis(1000)));
    }

    #[test]
    fn test_new_without_cooldown() {
        let now = Instant::now();
        let debounce = MtimeDebounce::new(Duration::from_millis(500), None, now);

        assert!(debounce.last_mtime.is_none());
        assert!(debounce.pending_since.is_none());
        assert!(debounce.cooldown_until.is_none());
        assert_eq!(debounce.debounce, Duration::from_millis(500));
    }

    #[test]
    fn test_new_with_cooldown() {
        let now = Instant::now();
        let debounce = MtimeDebounce::new(
            Duration::from_millis(500),
            Some(Duration::from_millis(300)),
            now,
        );

        assert_eq!(debounce.cooldown_until, Some(now + Duration::from_millis(300)));
    }
}
