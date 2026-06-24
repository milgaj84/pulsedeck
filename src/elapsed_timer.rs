use std::time::Duration;

pub struct ElapsedTimer {
    accumulated: Duration,
    running: bool,
}

impl Default for ElapsedTimer {
    fn default() -> Self {
        Self::new()
    }
}

impl ElapsedTimer {
    pub fn new() -> Self {
        Self {
            accumulated: Duration::ZERO,
            running: false,
        }
    }

    pub fn reset(&mut self) {
        self.accumulated = Duration::ZERO;
        self.running = false;
    }

    pub fn start(&mut self) {
        self.running = true;
    }

    pub fn pause(&mut self) {
        self.running = false;
    }

    pub fn tick(&mut self, delta: Duration) {
        if self.running && !delta.is_zero() {
            self.accumulated += delta;
        }
    }

    pub fn elapsed(&self) -> Duration {
        self.accumulated
    }

    #[allow(dead_code)]
    pub fn is_running(&self) -> bool {
        self.running
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_starts_at_zero() {
        let timer = ElapsedTimer::new();
        assert_eq!(timer.elapsed(), Duration::ZERO);
        assert!(!timer.is_running());
    }

    #[test]
    fn test_start_enables_ticking() {
        let mut timer = ElapsedTimer::new();
        timer.start();
        assert!(timer.is_running());

        timer.tick(Duration::from_secs(5));
        assert_eq!(timer.elapsed(), Duration::from_secs(5));
    }

    #[test]
    fn test_pause_freezes_elapsed() {
        let mut timer = ElapsedTimer::new();
        timer.start();
        timer.tick(Duration::from_secs(10));
        timer.pause();

        assert!(!timer.is_running());
        assert_eq!(timer.elapsed(), Duration::from_secs(10));

        timer.tick(Duration::from_secs(5));
        assert_eq!(timer.elapsed(), Duration::from_secs(10));
    }

    #[test]
    fn test_tick_while_paused_is_noop() {
        let mut timer = ElapsedTimer::new();
        timer.tick(Duration::from_secs(100));
        assert_eq!(timer.elapsed(), Duration::ZERO);
    }

    #[test]
    fn test_reset_clears_accumulated() {
        let mut timer = ElapsedTimer::new();
        timer.start();
        timer.tick(Duration::from_secs(30));
        assert_eq!(timer.elapsed(), Duration::from_secs(30));

        timer.reset();
        assert_eq!(timer.elapsed(), Duration::ZERO);
        assert!(!timer.is_running());
    }

    #[test]
    fn test_zero_delta_is_noop() {
        let mut timer = ElapsedTimer::new();
        timer.start();
        timer.tick(Duration::ZERO);
        assert_eq!(timer.elapsed(), Duration::ZERO);
    }

    #[test]
    fn test_elapsed_returns_accumulated_value() {
        let mut timer = ElapsedTimer::new();
        timer.start();
        timer.tick(Duration::from_secs(3));
        timer.tick(Duration::from_secs(7));
        assert_eq!(timer.elapsed(), Duration::from_secs(10));
    }

    #[test]
    fn test_resume_continues_from_accumulated() {
        let mut timer = ElapsedTimer::new();
        timer.start();
        timer.tick(Duration::from_secs(5));
        timer.pause();
        timer.start();
        timer.tick(Duration::from_secs(3));
        assert_eq!(timer.elapsed(), Duration::from_secs(8));
    }
}
