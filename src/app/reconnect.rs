use crate::app::PlaybackState;
use std::time::{Duration, Instant};

#[derive(Debug)]
pub struct Reconnect {
    attempts: u8,
    max_attempts: u8,
    backoff_seconds: Vec<u64>,
    next_attempt_at: Option<Instant>,
    armed_url: Option<String>,
}

impl Reconnect {
    pub fn new(max_attempts: u8, backoff_seconds: Vec<u64>) -> Self {
        Self {
            attempts: 0,
            max_attempts,
            backoff_seconds,
            next_attempt_at: None,
            armed_url: None,
        }
    }

    pub fn arm(&mut self, url: String, now: Instant) {
        if self.armed_url.as_deref() != Some(url.as_str()) {
            self.attempts = 0;
        }
        self.armed_url = Some(url);
        let index = (self.attempts as usize).min(self.backoff_seconds.len() - 1);
        let backoff = self.backoff_seconds[index];
        self.next_attempt_at = Some(now + Duration::from_secs(backoff));
    }

    pub fn disarm(&mut self) {
        self.attempts = 0;
        self.next_attempt_at = None;
        self.armed_url = None;
    }

    /// Returns the url to retry when it is time and attempts remain.
    pub fn take_due(&mut self, now: Instant) -> Option<String> {
        let due = self.next_attempt_at.is_some_and(|t| now >= t);
        if due && self.attempts < self.max_attempts {
            self.attempts += 1;
            self.next_attempt_at = None;
            self.armed_url.clone()
        } else {
            None
        }
    }

    pub fn exhausted(&self) -> bool {
        self.attempts >= self.max_attempts
    }

    pub fn attempt(&self) -> u8 {
        self.attempts
    }

    pub fn max(&self) -> u8 {
        self.max_attempts
    }

    /// Update max_attempts and backoff_seconds without resetting in-flight state.
    /// Preserves `armed_url`, `attempts`, and `next_attempt_at`.
    pub fn update_params(&mut self, max_attempts: u8, backoff_seconds: Vec<u64>) {
        self.max_attempts = max_attempts;
        self.backoff_seconds = backoff_seconds;
    }
}

impl Default for Reconnect {
    fn default() -> Self {
        Self::new(3, vec![3, 6, 12])
    }
}

impl super::App {
    pub(super) fn drive_reconnect(&mut self, now: Instant) {
        if let Some(url) = self.playback.reconnect.take_due(now) {
            self.playback.elapsed_timer.reset();
            let (n, max) = (
                self.playback.reconnect.attempt(),
                self.playback.reconnect.max(),
            );
            self.set_info_notice(format!("Reconnecting ({n}/{max})"));
            self.playback.view.state = PlaybackState::Connecting;
            if self.send_audio_command(crate::audio::AudioCommand::Play(url)) {
                self.sync_volume();
            }
        } else if self.playback.reconnect.exhausted()
            && matches!(self.playback.view.state, PlaybackState::Connecting)
        {
            self.playback.view.state = PlaybackState::Error("Reconnect failed".into());
            self.playback.reconnect.disarm();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reconnect_backoff_and_attempts() {
        let mut rec = Reconnect::default();
        let now = Instant::now();
        let url = "http://radio".to_string();

        // Initial arm
        rec.arm(url.clone(), now);
        assert_eq!(rec.attempts, 0);
        assert_eq!(rec.armed_url.as_ref().unwrap(), &url);
        assert!(!rec.exhausted());

        // Not due yet
        assert!(rec.take_due(now).is_none());

        // Due after 3s (first backoff)
        let due_time = now + Duration::from_secs(3);
        assert_eq!(rec.take_due(due_time), Some(url.clone()));
        assert_eq!(rec.attempts, 1);

        // Arm again (represents attempt failure)
        rec.arm(url.clone(), due_time);
        assert_eq!(rec.attempts, 1); // persists attempt count for same url

        // Due after 6s (second backoff)
        let due_time_2 = due_time + Duration::from_secs(6);
        assert_eq!(rec.take_due(due_time_2), Some(url.clone()));
        assert_eq!(rec.attempts, 2);

        // Arm again
        rec.arm(url.clone(), due_time_2);

        // Due after 12s (third backoff)
        let due_time_3 = due_time_2 + Duration::from_secs(12);
        assert_eq!(rec.take_due(due_time_3), Some(url.clone()));
        assert_eq!(rec.attempts, 3);
        assert!(rec.exhausted());

        // Arm again - should be exhausted
        rec.arm(url.clone(), due_time_3);
        assert!(rec.exhausted());
        assert!(rec.take_due(due_time_3 + Duration::from_secs(12)).is_none());

        // Disarm resets everything
        rec.disarm();
        assert_eq!(rec.attempts, 0);
        assert!(rec.armed_url.is_none());
        assert!(!rec.exhausted());
    }

    #[test]
    fn test_reconnect_different_url_resets() {
        let mut rec = Reconnect::default();
        let now = Instant::now();

        rec.arm("http://a".to_string(), now);
        rec.take_due(now + Duration::from_secs(3));
        assert_eq!(rec.attempts, 1);

        // Arming a different URL resets attempts to 0
        rec.arm("http://b".to_string(), now);
        assert_eq!(rec.attempts, 0);
    }

    #[test]
    fn test_take_due_at_exact_deadline_returns_url() {
        let mut rec = Reconnect::default();
        let now = Instant::now();
        let url = "http://exact-boundary".to_string();

        rec.arm(url.clone(), now);

        // First backoff is 3 seconds; calling at exactly now + 3s should return the URL.
        let deadline = now + Duration::from_secs(3);
        assert_eq!(rec.take_due(deadline), Some(url));
    }

    #[test]
    fn test_take_due_one_nanosecond_before_deadline_returns_none() {
        let mut rec = Reconnect::default();
        let now = Instant::now();
        let url = "http://one-ns-early".to_string();

        rec.arm(url, now);

        // 1 nanosecond before the 3-second deadline
        let before_deadline = now + Duration::from_secs(3) - Duration::from_nanos(1);
        assert_eq!(rec.take_due(before_deadline), None);
    }

    #[test]
    fn test_exhaustion_persists_until_disarm() {
        let mut rec = Reconnect::default();
        let now = Instant::now();
        let url = "http://exhaust-me".to_string();

        // Consume all 3 attempts
        rec.arm(url.clone(), now);
        let t1 = now + Duration::from_secs(3);
        assert!(rec.take_due(t1).is_some());

        rec.arm(url.clone(), t1);
        let t2 = t1 + Duration::from_secs(6);
        assert!(rec.take_due(t2).is_some());

        rec.arm(url.clone(), t2);
        let t3 = t2 + Duration::from_secs(12);
        assert!(rec.take_due(t3).is_some());
        assert!(rec.exhausted());

        // Re-arm same URL after exhaustion — take_due must return None
        rec.arm(url.clone(), t3);
        let t4 = t3 + Duration::from_secs(100);
        assert_eq!(rec.take_due(t4), None);

        // Disarm clears exhaustion, allowing re-use
        rec.disarm();
        assert!(!rec.exhausted());
        rec.arm(url.clone(), t4);
        let t5 = t4 + Duration::from_secs(3);
        assert_eq!(rec.take_due(t5), Some(url));
    }

    #[test]
    fn test_different_url_after_exhaustion_resets_and_fires() {
        let mut rec = Reconnect::default();
        let now = Instant::now();
        let url_a = "http://old-url".to_string();
        let url_b = "http://new-url".to_string();

        // Exhaust attempts on url_a
        rec.arm(url_a.clone(), now);
        let t1 = now + Duration::from_secs(3);
        assert!(rec.take_due(t1).is_some());

        rec.arm(url_a.clone(), t1);
        let t2 = t1 + Duration::from_secs(6);
        assert!(rec.take_due(t2).is_some());

        rec.arm(url_a.clone(), t2);
        let t3 = t2 + Duration::from_secs(12);
        assert!(rec.take_due(t3).is_some());
        assert!(rec.exhausted());

        // Arm a different URL — attempts reset to 0
        rec.arm(url_b.clone(), t3);
        assert_eq!(rec.attempts, 0);
        assert!(!rec.exhausted());

        // After the new backoff (3s), the new URL is returned
        let t4 = t3 + Duration::from_secs(3);
        assert_eq!(rec.take_due(t4), Some(url_b));
    }

    #[test]
    fn test_custom_max_attempts_respected() {
        let mut rec = Reconnect::new(5, vec![1, 2, 4]);
        let now = Instant::now();
        let url = "http://custom".to_string();

        assert_eq!(rec.max(), 5);

        let mut t = now;
        for i in 0..5 {
            rec.arm(url.clone(), t);
            let backoff = [1, 2, 4][i.min(2)];
            t = t + Duration::from_secs(backoff);
            assert_eq!(rec.take_due(t), Some(url.clone()));
        }
        assert!(rec.exhausted());
        assert_eq!(rec.attempts, 5);
    }

    #[test]
    fn test_custom_backoff_list_used() {
        let mut rec = Reconnect::new(3, vec![1, 2, 4]);
        let now = Instant::now();
        let url = "http://backoff-test".to_string();

        // Attempt 0 → backoff 1s
        rec.arm(url.clone(), now);
        assert!(rec.take_due(now + Duration::from_secs(1)).is_some());

        // Attempt 1 → backoff 2s
        let t1 = now + Duration::from_secs(1);
        rec.arm(url.clone(), t1);
        assert!(rec.take_due(t1 + Duration::from_secs(2)).is_some());

        // Attempt 2 → backoff 4s
        let t2 = t1 + Duration::from_secs(2);
        rec.arm(url.clone(), t2);
        assert!(rec.take_due(t2 + Duration::from_secs(4)).is_some());
        assert!(rec.exhausted());
    }

    #[test]
    fn test_backoff_last_element_repeats_for_overflow() {
        let mut rec = Reconnect::new(5, vec![2, 5]);
        let now = Instant::now();
        let url = "http://overflow".to_string();

        // Attempt 0 → backoff[0] = 2s
        rec.arm(url.clone(), now);
        let t1 = now + Duration::from_secs(2);
        assert_eq!(rec.take_due(t1), Some(url.clone()));

        // Attempt 1 → backoff[1] = 5s
        rec.arm(url.clone(), t1);
        let t2 = t1 + Duration::from_secs(5);
        assert_eq!(rec.take_due(t2), Some(url.clone()));

        // Attempt 2 → backoff[min(2, 1)] = backoff[1] = 5s (last repeats)
        rec.arm(url.clone(), t2);
        let t3 = t2 + Duration::from_secs(5);
        assert_eq!(rec.take_due(t3), Some(url.clone()));

        // Attempt 3 → also 5s (still overflow)
        rec.arm(url.clone(), t3);
        let t4 = t3 + Duration::from_secs(5);
        assert_eq!(rec.take_due(t4), Some(url.clone()));

        // Attempt 4 → also 5s
        rec.arm(url.clone(), t4);
        let t5 = t4 + Duration::from_secs(5);
        assert_eq!(rec.take_due(t5), Some(url.clone()));

        assert!(rec.exhausted());
        assert_eq!(rec.attempts, 5);
    }

    /// **Validates: Requirements 2.4**
    #[test]
    fn test_update_params_changes_max_and_backoff() {
        let mut rec = Reconnect::new(3, vec![3, 6, 12]);
        assert_eq!(rec.max(), 3);

        rec.update_params(5, vec![1, 2, 4]);

        assert_eq!(rec.max(), 5);

        // Verify new backoff is used: first backoff should be 1s
        let now = Instant::now();
        rec.arm("http://test".to_string(), now);
        let t1 = now + Duration::from_secs(1);
        assert_eq!(rec.take_due(t1), Some("http://test".to_string()));
    }

    /// **Validates: Requirements 2.4**
    #[test]
    fn test_update_params_preserves_armed_url_and_attempts() {
        let mut rec = Reconnect::new(3, vec![3, 6, 12]);
        let now = Instant::now();
        let url = "http://armed".to_string();

        // Arm a URL and make one attempt
        rec.arm(url.clone(), now);
        let t1 = now + Duration::from_secs(3);
        assert_eq!(rec.take_due(t1), Some(url.clone()));
        assert_eq!(rec.attempt(), 1);

        // Update params
        rec.update_params(5, vec![1, 2, 4]);

        // armed_url and attempts should be preserved
        assert_eq!(rec.attempt(), 1);
        // Re-arm same URL to verify armed_url was preserved (attempts won't reset)
        rec.arm(url.clone(), t1);
        assert_eq!(rec.attempt(), 1); // still 1, same URL means no reset
    }

    /// **Validates: Requirements 3.1, 3.2**
    #[test]
    fn test_drive_reconnect_resets_elapsed_timer() {
        use super::super::startup::AppParts;
        use super::super::ui_state::UiState;
        use super::super::{App, DisplayMode, LayoutMode, VisualizerMode};
        use crate::audio::MockAudioSink;
        use crate::favorites::Library;
        use crate::radio::Station;
        use std::collections::VecDeque;
        use std::sync::{Arc, Mutex};

        let library = Library::in_memory(vec![Station::basic(
            "Test",
            "http://test-stream",
            "Radio",
            "US",
            128,
        )]);
        let parts = AppParts {
            library,
            ui_state: UiState::from_app_values(
                50,
                false,
                LayoutMode::Split,
                VisualizerMode::RealOscilloscope,
                DisplayMode::Normal,
                None,
            ),
            ui_state_warning: None,
            history: crate::history::History::default(),
            history_warning: None,
            audio: Box::new(MockAudioSink::disconnected()),
            sample_buffer: Arc::new(Mutex::new(VecDeque::new())),
            config: crate::config_toml::AppConfig::default(),
            config_preserved: toml::Value::Table(toml::map::Map::new()),
            config_warnings: Vec::new(),
            config_loaded_from_file: false,
        };
        let mut app = App::from_parts(parts);

        // Simulate accumulated elapsed time
        app.playback.elapsed_timer.start();
        app.playback.elapsed_timer.tick(Duration::from_secs(120));
        assert_eq!(
            app.playback.elapsed_timer.elapsed(),
            Duration::from_secs(120)
        );
        assert!(app.playback.elapsed_timer.is_running());

        // Arm a reconnect
        let now = Instant::now();
        app.playback
            .reconnect
            .arm("http://test-stream".to_string(), now);

        // Drive reconnect after backoff elapses
        let after_backoff = now + Duration::from_secs(3);
        app.drive_reconnect(after_backoff);

        // Timer should be reset to zero and not running
        assert_eq!(
            app.playback.elapsed_timer.elapsed(),
            Duration::ZERO,
            "elapsed_timer should be zero after drive_reconnect fires"
        );
        assert!(
            !app.playback.elapsed_timer.is_running(),
            "elapsed_timer should not be running after drive_reconnect fires"
        );
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        // Feature: v090-features, Property 3: Backoff index selection
        /// For any valid backoff list of length L and any attempt index I (zero-based),
        /// the selected backoff duration SHALL equal `backoff_seconds[min(I, L-1)]`.
        ///
        /// **Validates: Requirements 1.7**
        #[test]
        fn backoff_index_selection(
            backoff_list in proptest::collection::vec(1u64..=60, 1..=10),
            attempt_index in 0u8..=20,
        ) {
            let max_attempts = 21u8; // high max so we don't exhaust
            let mut rec = Reconnect::new(max_attempts, backoff_list.clone());
            let url = "http://test".to_string();
            let mut now = Instant::now();

            for i in 0..=attempt_index {
                rec.arm(url.clone(), now);

                let expected_backoff = backoff_list[((i as usize)).min(backoff_list.len() - 1)];
                let expected_delay = Duration::from_secs(expected_backoff);

                // 1 nanosecond before expected delay → not due yet
                let before = now + expected_delay - Duration::from_nanos(1);
                prop_assert_eq!(
                    rec.take_due(before),
                    None,
                    "attempt {} should NOT be due 1ns before expected backoff of {}s",
                    i,
                    expected_backoff,
                );

                // At exactly the expected delay → due
                let at_deadline = now + expected_delay;
                prop_assert_eq!(
                    rec.take_due(at_deadline),
                    Some(url.clone()),
                    "attempt {} should be due at expected backoff of {}s",
                    i,
                    expected_backoff,
                );

                now = at_deadline;
            }
        }
    }
}
