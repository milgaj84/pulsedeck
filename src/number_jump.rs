use std::time::{Duration, Instant};

pub const NUMBER_JUMP_TIMEOUT_MS: u64 = 1500;
pub const NUMBER_JUMP_MAX_DIGITS: usize = 5;

#[derive(Debug, Clone)]
pub struct NumberJump {
    digits: String,
    last_input: Option<Instant>,
}

impl NumberJump {
    pub fn new() -> Self {
        Self {
            digits: String::new(),
            last_input: None,
        }
    }

    /// Append a digit. Returns true if accepted, false if non-digit or at max digits.
    pub fn push_digit(&mut self, digit: char) -> bool {
        if !digit.is_ascii_digit() {
            return false;
        }
        if self.digits.len() >= NUMBER_JUMP_MAX_DIGITS {
            return false;
        }
        self.digits.push(digit);
        self.last_input = Some(Instant::now());
        true
    }

    /// Check if the jump has timed out relative to `now`.
    pub fn is_expired(&self, now: Instant) -> bool {
        match self.last_input {
            Some(last) => now.duration_since(last) > Duration::from_millis(NUMBER_JUMP_TIMEOUT_MS),
            None => false,
        }
    }

    /// Get the accumulated number (0 if empty).
    pub fn target_row(&self) -> usize {
        if self.digits.is_empty() {
            return 0;
        }
        self.digits.parse::<usize>().unwrap_or(0)
    }

    /// Whether digits are currently being accumulated.
    pub fn is_active(&self) -> bool {
        !self.digits.is_empty()
    }

    /// Get the accumulated digits as a display string.
    pub fn display(&self) -> &str {
        &self.digits
    }

    /// Clear all accumulated state.
    pub fn clear(&mut self) {
        self.digits.clear();
        self.last_input = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_new_is_inactive() {
        let jump = NumberJump::new();
        assert!(!jump.is_active());
        assert_eq!(jump.target_row(), 0);
        assert_eq!(jump.display(), "");
    }

    #[test]
    fn test_push_single_digit() {
        let mut jump = NumberJump::new();
        assert!(jump.push_digit('5'));
        assert!(jump.is_active());
        assert_eq!(jump.display(), "5");
        assert_eq!(jump.target_row(), 5);
    }

    #[test]
    fn test_push_multiple_digits() {
        let mut jump = NumberJump::new();
        assert!(jump.push_digit('1'));
        assert!(jump.push_digit('2'));
        assert!(jump.push_digit('3'));
        assert_eq!(jump.display(), "123");
        assert_eq!(jump.target_row(), 123);
    }

    #[test]
    fn test_max_digits_rejection() {
        let mut jump = NumberJump::new();
        for c in ['1', '2', '3', '4', '5'] {
            assert!(jump.push_digit(c));
        }
        assert!(!jump.push_digit('6'));
        assert_eq!(jump.display(), "12345");
        assert_eq!(jump.target_row(), 12345);
    }

    #[test]
    fn test_target_row_parsing_zero() {
        let mut jump = NumberJump::new();
        jump.push_digit('0');
        assert_eq!(jump.target_row(), 0);
    }

    #[test]
    fn test_target_row_parsing_leading_zeros() {
        let mut jump = NumberJump::new();
        jump.push_digit('0');
        jump.push_digit('0');
        jump.push_digit('7');
        assert_eq!(jump.target_row(), 7);
    }

    #[test]
    fn test_is_active_after_push() {
        let mut jump = NumberJump::new();
        assert!(!jump.is_active());
        jump.push_digit('1');
        assert!(jump.is_active());
    }

    #[test]
    fn test_timeout_not_expired_immediately() {
        let mut jump = NumberJump::new();
        jump.push_digit('1');
        let now = Instant::now();
        assert!(!jump.is_expired(now));
    }

    #[test]
    fn test_timeout_expired_after_threshold() {
        let mut jump = NumberJump::new();
        jump.push_digit('1');
        let future = Instant::now() + Duration::from_millis(NUMBER_JUMP_TIMEOUT_MS + 1);
        assert!(jump.is_expired(future));
    }

    #[test]
    fn test_timeout_not_expired_just_before_threshold() {
        let mut jump = NumberJump::new();
        jump.push_digit('1');
        // Well within the threshold (half the timeout)
        let before_threshold = Instant::now() + Duration::from_millis(NUMBER_JUMP_TIMEOUT_MS / 2);
        assert!(!jump.is_expired(before_threshold));
    }

    #[test]
    fn test_is_expired_with_no_input() {
        let jump = NumberJump::new();
        let now = Instant::now();
        assert!(!jump.is_expired(now));
    }

    #[test]
    fn test_clear_resets_all_state() {
        let mut jump = NumberJump::new();
        jump.push_digit('4');
        jump.push_digit('2');
        assert!(jump.is_active());

        jump.clear();

        assert!(!jump.is_active());
        assert_eq!(jump.target_row(), 0);
        assert_eq!(jump.display(), "");
        // After clear, is_expired should return false (no last_input)
        let future = Instant::now() + Duration::from_secs(10);
        assert!(!jump.is_expired(future));
    }

    #[test]
    fn test_display_returns_accumulated_digits() {
        let mut jump = NumberJump::new();
        jump.push_digit('9');
        jump.push_digit('8');
        assert_eq!(jump.display(), "98");
    }

    #[test]
    fn test_push_non_digit_rejected() {
        let mut jump = NumberJump::new();
        assert!(!jump.push_digit('a'));
        assert!(!jump.push_digit(' '));
        assert!(!jump.push_digit('!'));
        assert!(!jump.is_active());
    }
}
