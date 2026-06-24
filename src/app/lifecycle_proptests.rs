use super::*;
use proptest::prelude::*;
use std::time::{Duration, Instant};

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
