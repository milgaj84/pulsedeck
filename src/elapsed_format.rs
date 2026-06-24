//! Pure formatting functions for elapsed listening duration.
//!
//! Converts a `Duration` to a human-readable "MM:SS" or "H:MM:SS" string,
//! and provides a parser for round-trip testing.

use std::time::Duration;

const SECONDS_PER_MINUTE: u64 = 60;
const SECONDS_PER_HOUR: u64 = 3600;
const MAX_DISPLAY_SECONDS: u64 = 359_999;

/// Format a Duration as a human-readable elapsed time string.
///
/// - < 3600s: "MM:SS" (zero-padded)
/// - 3600..359999s: "H:MM:SS" (H unpadded, MM:SS zero-padded)
/// - >= 360000s: "99:59:59" (clamped max)
pub fn format_elapsed(duration: Duration) -> String {
    let total_secs = duration.as_secs().min(MAX_DISPLAY_SECONDS);

    if total_secs < SECONDS_PER_HOUR {
        let minutes = total_secs / SECONDS_PER_MINUTE;
        let seconds = total_secs % SECONDS_PER_MINUTE;
        format!("{minutes:02}:{seconds:02}")
    } else {
        let hours = total_secs / SECONDS_PER_HOUR;
        let remaining = total_secs % SECONDS_PER_HOUR;
        let minutes = remaining / SECONDS_PER_MINUTE;
        let seconds = remaining % SECONDS_PER_MINUTE;
        format!("{hours}:{minutes:02}:{seconds:02}")
    }
}

/// Parse an elapsed format string back to total seconds.
///
/// Accepts "MM:SS" or "H:MM:SS" formats. Returns `None` for invalid input.
#[cfg(test)]
pub fn parse_elapsed(s: &str) -> Option<u64> {
    let parts: Vec<&str> = s.split(':').collect();

    match parts.len() {
        2 => {
            let minutes: u64 = parts[0].parse().ok()?;
            let seconds: u64 = parts[1].parse().ok()?;
            if seconds >= SECONDS_PER_MINUTE {
                return None;
            }
            Some(minutes * SECONDS_PER_MINUTE + seconds)
        }
        3 => {
            let hours: u64 = parts[0].parse().ok()?;
            let minutes: u64 = parts[1].parse().ok()?;
            let seconds: u64 = parts[2].parse().ok()?;
            if minutes >= SECONDS_PER_MINUTE || seconds >= SECONDS_PER_MINUTE {
                return None;
            }
            Some(hours * SECONDS_PER_HOUR + minutes * SECONDS_PER_MINUTE + seconds)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_elapsed_zero_seconds_returns_zero_display() {
        assert_eq!(format_elapsed(Duration::from_secs(0)), "00:00");
    }

    #[test]
    fn test_format_elapsed_67_seconds_returns_minutes_and_seconds() {
        assert_eq!(format_elapsed(Duration::from_secs(67)), "01:07");
    }

    #[test]
    fn test_format_elapsed_3661_seconds_returns_hour_format() {
        assert_eq!(format_elapsed(Duration::from_secs(3661)), "1:01:01");
    }

    #[test]
    fn test_format_elapsed_max_clamped() {
        assert_eq!(format_elapsed(Duration::from_secs(360_000)), "99:59:59");
    }

    #[test]
    fn test_format_elapsed_just_under_one_hour() {
        assert_eq!(format_elapsed(Duration::from_secs(3599)), "59:59");
    }

    #[test]
    fn test_format_elapsed_exactly_one_hour() {
        assert_eq!(format_elapsed(Duration::from_secs(3600)), "1:00:00");
    }

    #[test]
    fn test_format_elapsed_max_displayable() {
        assert_eq!(
            format_elapsed(Duration::from_secs(MAX_DISPLAY_SECONDS)),
            "99:59:59"
        );
    }

    #[test]
    fn test_parse_elapsed_zero() {
        assert_eq!(parse_elapsed("00:00"), Some(0));
    }

    #[test]
    fn test_parse_elapsed_minutes_seconds() {
        assert_eq!(parse_elapsed("01:07"), Some(67));
    }

    #[test]
    fn test_parse_elapsed_hour_format() {
        assert_eq!(parse_elapsed("1:01:01"), Some(3661));
    }

    #[test]
    fn test_parse_elapsed_max_display() {
        assert_eq!(parse_elapsed("99:59:59"), Some(359_999));
    }

    #[test]
    fn test_parse_elapsed_invalid_input_returns_none() {
        assert_eq!(parse_elapsed(""), None);
        assert_eq!(parse_elapsed("abc"), None);
        assert_eq!(parse_elapsed("1:2:3:4"), None);
    }

    #[test]
    fn test_parse_elapsed_invalid_seconds_returns_none() {
        assert_eq!(parse_elapsed("01:60"), None);
        assert_eq!(parse_elapsed("1:00:60"), None);
    }

    #[test]
    fn test_round_trip_known_values() {
        let cases = [0, 67, 3599, 3600, 3661, 7200, 359_999];
        for secs in cases {
            let formatted = format_elapsed(Duration::from_secs(secs));
            let parsed = parse_elapsed(&formatted);
            assert_eq!(
                parsed,
                Some(secs),
                "Round-trip failed for {secs}s → {formatted}"
            );
        }
    }

    #[test]
    fn test_round_trip_clamped_value() {
        let formatted = format_elapsed(Duration::from_secs(400_000));
        let parsed = parse_elapsed(&formatted);
        assert_eq!(parsed, Some(MAX_DISPLAY_SECONDS));
    }
}
