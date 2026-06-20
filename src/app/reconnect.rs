use crate::app::PlaybackState;
use std::time::{Duration, Instant};

const MAX_ATTEMPTS: u8 = 3;
const BACKOFFS: [u64; 3] = [3, 6, 12]; // seconds

#[derive(Debug, Default)]
pub struct Reconnect {
    attempts: u8,
    next_attempt_at: Option<Instant>,
    armed_url: Option<String>,
}

impl Reconnect {
    pub fn arm(&mut self, url: String, now: Instant) {
        // Keep counting across consecutive failures of the same url.
        if self.armed_url.as_deref() != Some(url.as_str()) {
            self.attempts = 0;
        }
        self.armed_url = Some(url);
        let backoff = BACKOFFS[(self.attempts as usize).min(BACKOFFS.len() - 1)];
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
        if due && self.attempts < MAX_ATTEMPTS {
            self.attempts += 1;
            self.next_attempt_at = None;
            self.armed_url.clone()
        } else {
            None
        }
    }

    pub fn exhausted(&self) -> bool {
        self.attempts >= MAX_ATTEMPTS
    }

    pub fn attempt(&self) -> u8 {
        self.attempts
    }

    pub fn max(&self) -> u8 {
        MAX_ATTEMPTS
    }
}

impl super::App {
    pub(super) fn drive_reconnect(&mut self, now: Instant) {
        if let Some(url) = self.reconnect.take_due(now) {
            let (n, max) = (self.reconnect.attempt(), self.reconnect.max());
            self.set_info_notice(format!("Reconnecting ({n}/{max})"));
            self.player.state = PlaybackState::Connecting;
            if self.send_audio_command(crate::audio::AudioCommand::Play(url)) {
                self.sync_volume();
            }
        } else if self.reconnect.exhausted()
            && matches!(self.player.state, PlaybackState::Connecting)
        {
            self.player.state = PlaybackState::Error("Reconnect failed".into());
            self.reconnect.disarm();
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
}
