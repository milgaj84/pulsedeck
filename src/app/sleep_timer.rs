use crate::action::Action;
use crate::app::PlaybackState;
use std::time::{Duration, Instant};

/// Fine-adjust granularity, in minutes.
pub const SLEEP_STEP_MINUTES: u32 = 5;
/// Largest selectable duration, in minutes.
pub const SLEEP_MAX_MINUTES: u32 = 120;
/// Quick-pick presets surfaced on the overlay number keys 1..=6.
pub const SLEEP_PRESETS: [u32; 6] = [15, 30, 45, 60, 90, 120];

/// Sleep-timer state.
///
/// `minutes == 0` means the timer is off. Any armed value is always a multiple
/// of [`SLEEP_STEP_MINUTES`] (presets are multiples of the step too), so the
/// fine controls stay on a clean grid.
#[derive(Debug, Default)]
pub struct SleepTimer {
    minutes: u32,
    deadline: Option<Instant>,
    remaining: Option<Duration>,
}

impl SleepTimer {
    /// Selected duration in minutes (0 means off).
    pub fn minutes(&self) -> u32 {
        self.minutes
    }

    /// Whether a countdown is currently armed.
    pub fn is_armed(&self) -> bool {
        self.minutes > 0
    }

    pub fn is_waiting_for_playback(&self) -> bool {
        self.is_armed() && self.deadline.is_none()
    }

    /// Set an absolute duration in minutes, clamped to `[0, SLEEP_MAX_MINUTES]`,
    /// recomputing the deadline from `now`.
    pub fn set_minutes(&mut self, minutes: u32, now: Instant) {
        self.minutes = minutes.min(SLEEP_MAX_MINUTES);
        self.deadline = if self.minutes == 0 {
            None
        } else {
            Some(now + Duration::from_secs(self.minutes as u64 * 60))
        };
        self.remaining = None;
    }

    /// Arm the timer without starting the countdown yet.
    pub fn set_minutes_waiting(&mut self, minutes: u32) {
        self.minutes = minutes.min(SLEEP_MAX_MINUTES);
        self.deadline = None;
        self.remaining = if self.minutes == 0 {
            None
        } else {
            Some(Duration::from_secs(self.minutes as u64 * 60))
        };
    }

    /// Increase by one step. Stepping past the maximum wraps back to off.
    pub fn increase(&mut self, now: Instant, should_run: bool) {
        let next = self.minutes + SLEEP_STEP_MINUTES;
        let next = if next > SLEEP_MAX_MINUTES { 0 } else { next };
        self.set_minutes_for_playback(next, now, should_run);
    }

    /// Decrease by one step. Stepping below off wraps to the maximum.
    pub fn decrease(&mut self, now: Instant, should_run: bool) {
        let next = if self.minutes == 0 {
            SLEEP_MAX_MINUTES
        } else {
            self.minutes.saturating_sub(SLEEP_STEP_MINUTES)
        };
        self.set_minutes_for_playback(next, now, should_run);
    }

    /// Turn the timer off.
    pub fn clear(&mut self) {
        self.minutes = 0;
        self.deadline = None;
        self.remaining = None;
    }

    /// Human-readable label for the current selection.
    pub fn label(&self) -> String {
        if self.minutes == 0 {
            "off".to_string()
        } else {
            format!("{} min", self.minutes)
        }
    }

    /// Returns true exactly once when the deadline passes, resetting to off.
    pub fn expired(&mut self, now: Instant) -> bool {
        match self.deadline {
            Some(deadline) if now >= deadline => {
                self.clear();
                true
            }
            _ => false,
        }
    }

    /// Time left until the timer fires, if armed.
    pub fn remaining(&self, now: Instant) -> Option<Duration> {
        self.deadline
            .map(|deadline| deadline.saturating_duration_since(now))
            .or(self.remaining)
    }

    fn set_minutes_for_playback(&mut self, minutes: u32, now: Instant, should_run: bool) {
        if should_run {
            self.set_minutes(minutes, now);
        } else {
            self.set_minutes_waiting(minutes);
        }
    }

    fn start(&mut self, now: Instant) {
        if self.minutes == 0 || self.deadline.is_some() {
            return;
        }

        let remaining = self
            .remaining
            .unwrap_or_else(|| Duration::from_secs(self.minutes as u64 * 60));
        self.deadline = Some(now + remaining);
        self.remaining = None;
    }

    fn pause(&mut self, now: Instant) {
        if let Some(deadline) = self.deadline.take() {
            self.remaining = Some(deadline.saturating_duration_since(now));
        }
    }
}

impl super::App {
    /// Open or close the sleep-timer overlay. Opening switches into the
    /// dedicated input mode and closes any other overlay first.
    pub(super) fn toggle_sleep_timer(&mut self) {
        self.toggle_sleep_timer_overlay();
    }

    /// Handle actions while the sleep-timer overlay is open. Changes apply live.
    pub(super) fn handle_sleep_timer_action(&mut self, action: Action) {
        let now = Instant::now();
        match action {
            Action::SleepTimerIncrease => {
                self.playback
                    .sleep_timer
                    .increase(now, self.sleep_timer_should_run());
                self.announce_sleep_timer();
            }
            Action::SleepTimerDecrease => {
                self.playback
                    .sleep_timer
                    .decrease(now, self.sleep_timer_should_run());
                self.announce_sleep_timer();
            }
            Action::SleepTimerPreset(minutes) => {
                self.set_sleep_timer_minutes(minutes as u32, now);
                self.announce_sleep_timer();
            }
            Action::SleepTimerClear => {
                self.playback.sleep_timer.clear();
                self.set_info_notice("Sleep timer off");
            }
            Action::ToggleSleepTimer => self.toggle_sleep_timer(),
            Action::Quit => {
                self.stop_audio_before_quit();
                self.ui.should_quit = true;
            }
            Action::Tick => self.tick(),
            _ => {
                // Ignore everything else while the overlay is open.
            }
        }
    }

    fn announce_sleep_timer(&mut self) {
        if self.playback.sleep_timer.is_waiting_for_playback() {
            let label = self.playback.sleep_timer.label();
            self.set_info_notice(format!("Sleep timer: {label} when playback starts"));
        } else if self.playback.sleep_timer.is_armed() {
            let label = self.playback.sleep_timer.label();
            self.set_info_notice(format!("Sleep timer: {label}"));
        } else {
            self.set_info_notice("Sleep timer off");
        }
    }

    pub(super) fn check_sleep_timer(&mut self, now: Instant) {
        if self.sleep_timer_should_run() {
            self.playback.sleep_timer.start(now);
        } else {
            self.playback.sleep_timer.pause(now);
        }

        if self.playback.sleep_timer.expired(now) {
            self.playback.view.intentional_stop = true;
            self.stop_playback();
            self.set_info_notice("Sleep timer ended playback");
        }
    }

    fn set_sleep_timer_minutes(&mut self, minutes: u32, now: Instant) {
        if self.sleep_timer_should_run() {
            self.playback.sleep_timer.set_minutes(minutes, now);
        } else {
            self.playback.sleep_timer.set_minutes_waiting(minutes);
        }
    }

    fn sleep_timer_should_run(&self) -> bool {
        matches!(
            self.playback.view.state,
            PlaybackState::Playing | PlaybackState::Connecting | PlaybackState::FadingOut { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn increase_steps_then_wraps_to_off() {
        let now = Instant::now();
        let mut timer = SleepTimer::default();
        assert_eq!(timer.minutes(), 0);

        timer.increase(now, true);
        assert_eq!(timer.minutes(), SLEEP_STEP_MINUTES);
        assert!(timer.is_armed());

        while timer.minutes() < SLEEP_MAX_MINUTES {
            timer.increase(now, true);
        }
        assert_eq!(timer.minutes(), SLEEP_MAX_MINUTES);
        assert!(timer.remaining(now).is_some());

        timer.increase(now, true);
        assert_eq!(timer.minutes(), 0);
        assert!(timer.remaining(now).is_none());
    }

    #[test]
    fn decrease_from_off_wraps_to_max() {
        let now = Instant::now();
        let mut timer = SleepTimer::default();
        timer.decrease(now, true);
        assert_eq!(timer.minutes(), SLEEP_MAX_MINUTES);
        timer.decrease(now, true);
        assert_eq!(timer.minutes(), SLEEP_MAX_MINUTES - SLEEP_STEP_MINUTES);
    }

    #[test]
    fn decrease_to_zero_turns_off() {
        let now = Instant::now();
        let mut timer = SleepTimer::default();
        timer.set_minutes(SLEEP_STEP_MINUTES, now);
        timer.decrease(now, true);
        assert_eq!(timer.minutes(), 0);
        assert!(timer.remaining(now).is_none());
    }

    #[test]
    fn preset_sets_absolute_clamped_minutes() {
        let now = Instant::now();
        let mut timer = SleepTimer::default();
        timer.set_minutes(30, now);
        assert_eq!(timer.minutes(), 30);
        assert_eq!(timer.remaining(now).unwrap().as_secs(), 30 * 60);
        timer.set_minutes(9999, now);
        assert_eq!(timer.minutes(), SLEEP_MAX_MINUTES);
    }

    #[test]
    fn waiting_timer_does_not_spend_time_before_playback_starts() {
        let now = Instant::now();
        let mut timer = SleepTimer::default();

        timer.set_minutes_waiting(15);
        assert_eq!(
            timer
                .remaining(now + Duration::from_secs(5 * 60))
                .unwrap()
                .as_secs(),
            15 * 60
        );

        timer.start(now + Duration::from_secs(5 * 60));
        assert_eq!(
            timer
                .remaining(now + Duration::from_secs(5 * 60))
                .unwrap()
                .as_secs(),
            15 * 60
        );
    }

    #[test]
    fn timer_pauses_when_playback_is_not_running() {
        let now = Instant::now();
        let mut timer = SleepTimer::default();

        timer.set_minutes(15, now);
        timer.pause(now + Duration::from_secs(2 * 60));

        assert_eq!(
            timer
                .remaining(now + Duration::from_secs(10 * 60))
                .unwrap()
                .as_secs(),
            13 * 60
        );
    }

    #[test]
    fn expiration_fires_once_and_resets() {
        let now = Instant::now();
        let mut timer = SleepTimer::default();
        timer.set_minutes(15, now);
        assert!(!timer.expired(now + Duration::from_secs(15 * 60 - 1)));
        assert!(timer.expired(now + Duration::from_secs(15 * 60)));
        assert_eq!(timer.minutes(), 0);
        assert!(!timer.expired(now + Duration::from_secs(15 * 60 + 1)));
    }

    #[test]
    fn label_reflects_state() {
        let now = Instant::now();
        let mut timer = SleepTimer::default();
        assert_eq!(timer.label(), "off");
        timer.set_minutes(45, now);
        assert_eq!(timer.label(), "45 min");
    }

    #[test]
    fn presets_within_bounds_and_sorted() {
        assert!(SLEEP_PRESETS.iter().all(|m| *m <= SLEEP_MAX_MINUTES));
        assert!(SLEEP_PRESETS.windows(2).all(|w| w[0] < w[1]));
    }
}
