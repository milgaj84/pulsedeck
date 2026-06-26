use std::time::{Duration, Instant};

pub(super) const NOTIFICATION_COOLDOWN: Duration = Duration::from_secs(5);

pub(crate) struct NotificationCooldown {
    last_notified: Option<Instant>,
}

impl NotificationCooldown {
    pub fn new() -> Self {
        Self {
            last_notified: None,
        }
    }

    pub fn may_notify(&self, now: Instant) -> bool {
        match self.last_notified {
            None => true,
            Some(last) => now.duration_since(last) >= NOTIFICATION_COOLDOWN,
        }
    }

    pub fn record_notification(&mut self, now: Instant) {
        self.last_notified = Some(now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn test_notification_cooldown_new_has_no_last_notified() {
        let cooldown = NotificationCooldown::new();
        let now = Instant::now();
        assert!(cooldown.may_notify(now));
    }

    #[test]
    fn test_may_notify_true_when_no_previous_notification() {
        let cooldown = NotificationCooldown::new();
        let now = Instant::now();
        assert!(cooldown.may_notify(now));
    }

    #[test]
    fn test_may_notify_true_when_elapsed_gte_cooldown() {
        let mut cooldown = NotificationCooldown::new();
        let t1 = Instant::now();
        cooldown.record_notification(t1);

        let t2 = t1 + Duration::from_secs(5);
        assert!(cooldown.may_notify(t2));
    }

    #[test]
    fn test_may_notify_false_when_elapsed_lt_cooldown() {
        let mut cooldown = NotificationCooldown::new();
        let t1 = Instant::now();
        cooldown.record_notification(t1);

        let t2 = t1 + Duration::from_millis(4999);
        assert!(!cooldown.may_notify(t2));
    }

    #[test]
    fn test_record_notification_updates_timestamp() {
        let mut cooldown = NotificationCooldown::new();
        let t1 = Instant::now();
        cooldown.record_notification(t1);

        let t2 = t1 + Duration::from_secs(2);
        assert!(!cooldown.may_notify(t2));

        let t3 = t1 + Duration::from_secs(6);
        assert!(cooldown.may_notify(t3));
    }

    #[test]
    fn test_may_notify_boundary_exactly_5000ms_returns_true() {
        let mut cooldown = NotificationCooldown::new();
        let t1 = Instant::now();
        cooldown.record_notification(t1);

        let t2 = t1 + Duration::from_millis(5000);
        assert!(cooldown.may_notify(t2));
    }

    #[test]
    fn test_may_notify_boundary_4999ms_returns_false() {
        let mut cooldown = NotificationCooldown::new();
        let t1 = Instant::now();
        cooldown.record_notification(t1);

        let t2 = t1 + Duration::from_millis(4999);
        assert!(!cooldown.may_notify(t2));
    }

    #[test]
    fn test_notification_cooldown_second_record_resets_window() {
        let mut cooldown = NotificationCooldown::new();
        let t0 = Instant::now();
        let t1 = t0 + Duration::from_secs(3);
        let t2 = t0 + Duration::from_secs(6);

        cooldown.record_notification(t0);
        assert!(!cooldown.may_notify(t1));

        cooldown.record_notification(t1);
        assert!(!cooldown.may_notify(t2));

        let t3 = t1 + Duration::from_secs(5);
        assert!(cooldown.may_notify(t3));
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    // **Validates: Requirements 2.1**
    //
    // Property: for any elapsed duration >= 5s, may_notify returns true
    // after a prior record_notification.
    proptest! {
        #[test]
        fn may_notify_true_when_elapsed_gte_5s(elapsed_ms in 5000u64..=600_000u64) {
            let mut cooldown = NotificationCooldown::new();
            let t1 = Instant::now();
            cooldown.record_notification(t1);

            let t2 = t1 + Duration::from_millis(elapsed_ms);
            prop_assert!(cooldown.may_notify(t2),
                "Expected may_notify=true for elapsed={}ms", elapsed_ms);
        }
    }

    // **Validates: Requirements 2.1**
    //
    // Property: for any elapsed duration < 5s, may_notify returns false
    // after a prior record_notification.
    proptest! {
        #[test]
        fn may_notify_false_when_elapsed_lt_5s(elapsed_ms in 0u64..5000u64) {
            let mut cooldown = NotificationCooldown::new();
            let t1 = Instant::now();
            cooldown.record_notification(t1);

            let t2 = t1 + Duration::from_millis(elapsed_ms);
            prop_assert!(!cooldown.may_notify(t2),
                "Expected may_notify=false for elapsed={}ms", elapsed_ms);
        }
    }

    // **Validates: Requirements 2.1**
    //
    // Property: a fresh NotificationCooldown always allows notification
    // regardless of what Instant is provided.
    proptest! {
        #[test]
        fn may_notify_always_true_when_fresh(offset_ms in 0u64..=1_000_000u64) {
            let cooldown = NotificationCooldown::new();
            let now = Instant::now() + Duration::from_millis(offset_ms);
            prop_assert!(cooldown.may_notify(now),
                "Fresh cooldown should always allow notification");
        }
    }

    // **Validates: Requirements 2.1**
    //
    // Property: record_notification always updates so that subsequent
    // may_notify within cooldown returns false.
    proptest! {
        #[test]
        fn record_then_immediate_check_returns_false(
            first_offset_ms in 0u64..=100_000u64,
            gap_ms in 0u64..5000u64,
        ) {
            let mut cooldown = NotificationCooldown::new();
            let t1 = Instant::now() + Duration::from_millis(first_offset_ms);
            cooldown.record_notification(t1);

            let t2 = t1 + Duration::from_millis(gap_ms);
            prop_assert!(!cooldown.may_notify(t2),
                "After record at t1, may_notify at t1+{}ms should be false", gap_ms);
        }
    }
}
