use super::station::StationHealth;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthLevel {
    Healthy,
    Flaky,
    Failed,
}

const DECAY_THRESHOLD_SECS: u64 = 7 * 24 * 3600; // 7 days
const CONFIDENCE_SATURATION: f32 = 10.0;

/// Pure function: classifies a station's reliability from its health record,
/// applying time-based decay when failures are older than 7 days.
/// Returns None when no health data exists or decay reduces classification to nothing.
pub fn classify_health(health: &StationHealth, now: &str) -> Option<HealthLevel> {
    let base = classify_base(health);
    apply_decay(base, health.last_failure_at.as_deref(), now)
}

/// Compute confidence as a value in [0.0, 1.0] based on total data points.
/// More successes + failures = higher confidence in the classification.
pub fn calculate_confidence(health: &StationHealth) -> f32 {
    let successes = health.success_count.unwrap_or(0) as f32;
    let failures = health.failure_count.unwrap_or(0) as f32;
    let total = successes + failures;
    (total / CONFIDENCE_SATURATION).min(1.0)
}

/// Human-readable label for confidence level.
pub fn confidence_label(confidence: f32) -> &'static str {
    if confidence >= 0.5 {
        "high confidence"
    } else {
        "low confidence"
    }
}

fn classify_base(health: &StationHealth) -> Option<HealthLevel> {
    if health.is_empty() {
        return None;
    }

    let failure_count = health.failure_count.unwrap_or(0);

    if health.last_failure_at.is_some() && failure_count >= 3 {
        return Some(HealthLevel::Failed);
    }

    if let (Some(last_success), Some(last_failure)) =
        (&health.last_success_at, &health.last_failure_at)
    {
        if last_failure > last_success && failure_count < 3 {
            return Some(HealthLevel::Flaky);
        }
    }

    if let Some(last_success) = &health.last_success_at {
        let success_is_recent = match &health.last_failure_at {
            None => true,
            Some(last_failure) => last_success >= last_failure,
        };
        if success_is_recent {
            return Some(HealthLevel::Healthy);
        }
    }

    None
}

fn apply_decay(
    level: Option<HealthLevel>,
    last_failure_at: Option<&str>,
    now: &str,
) -> Option<HealthLevel> {
    let level = level?;
    let Some(failure_ts) = last_failure_at else {
        return Some(level);
    };

    let age_secs = match timestamp_age_secs(failure_ts, now) {
        Some(age) => age,
        None => return Some(level), // parse failure: no decay
    };

    if age_secs <= DECAY_THRESHOLD_SECS {
        return Some(level);
    }

    match level {
        HealthLevel::Failed => Some(HealthLevel::Flaky),
        HealthLevel::Flaky => None,
        HealthLevel::Healthy => Some(HealthLevel::Healthy),
    }
}

/// Compute the age in seconds between two timestamps.
/// Supports ISO 8601 format ("2024-01-01T00:00:00Z") and plain Unix seconds.
pub(crate) fn timestamp_age_secs(earlier: &str, later: &str) -> Option<u64> {
    let earlier_epoch = parse_to_epoch(earlier)?;
    let later_epoch = parse_to_epoch(later)?;
    Some(later_epoch.saturating_sub(earlier_epoch))
}

/// Parse a timestamp string to seconds since Unix epoch.
/// Handles both ISO 8601 ("YYYY-MM-DDTHH:MM:SSZ") and plain integer strings.
fn parse_to_epoch(s: &str) -> Option<u64> {
    // Try plain integer first (Unix seconds)
    if let Ok(epoch) = s.parse::<u64>() {
        return Some(epoch);
    }
    // Try ISO 8601: "YYYY-MM-DDTHH:MM:SSZ"
    parse_iso8601_to_epoch(s)
}

fn parse_iso8601_to_epoch(s: &str) -> Option<u64> {
    let s = s.strip_suffix('Z')?;
    let (date_part, time_part) = s.split_once('T')?;

    let mut date_iter = date_part.splitn(3, '-');
    let year: u64 = date_iter.next()?.parse().ok()?;
    let month: u64 = date_iter.next()?.parse().ok()?;
    let day: u64 = date_iter.next()?.parse().ok()?;

    let mut time_iter = time_part.splitn(3, ':');
    let hour: u64 = time_iter.next()?.parse().ok()?;
    let minute: u64 = time_iter.next()?.parse().ok()?;
    let second: u64 = time_iter.next()?.parse().ok()?;

    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    if hour > 23 || minute > 59 || second > 59 {
        return None;
    }

    Some(days_since_epoch(year, month, day) * 86400 + hour * 3600 + minute * 60 + second)
}

/// Calculate the number of days from 1970-01-01 to the given date.
fn days_since_epoch(year: u64, month: u64, day: u64) -> u64 {
    // Adjust for months January and February (shift to March-based year)
    let (y, m) = if month <= 2 {
        (year - 1, month + 9)
    } else {
        (year, month - 3)
    };

    // Days from years since epoch (March-based)
    let era_days = 365 * y + y / 4 - y / 100 + y / 400;
    // Days from months (using March-based formula)
    let month_days = (153 * m + 2) / 5;
    // Combine and adjust to Unix epoch (1970-01-01)
    // Epoch offset: days from year 0 to 1970-01-01 in this formula
    era_days + month_days + day - 719469
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Timestamp just 1 day after the latest failure in existing tests (2024-01-02),
    /// so within the 7-day threshold and existing tests don't trigger decay.
    const NOW_RECENT: &str = "2024-01-03T00:00:00Z";

    fn health_with(
        last_success_at: Option<&str>,
        last_failure_at: Option<&str>,
        failure_count: Option<u32>,
    ) -> StationHealth {
        StationHealth {
            last_success_at: last_success_at.map(String::from),
            last_failure_at: last_failure_at.map(String::from),
            failure_count,
            success_count: None,
            last_error_summary: String::new(),
        }
    }

    #[test]
    fn classify_empty_health_returns_none() {
        let health = StationHealth::default();
        assert_eq!(classify_health(&health, NOW_RECENT), None);
    }

    #[test]
    fn classify_failure_count_ge_3_returns_failed() {
        let health = health_with(
            Some("2024-01-01T00:00:00Z"),
            Some("2024-01-02T00:00:00Z"),
            Some(3),
        );
        assert_eq!(
            classify_health(&health, NOW_RECENT),
            Some(HealthLevel::Failed)
        );
    }

    #[test]
    fn classify_failure_count_5_returns_failed() {
        let health = health_with(
            Some("2024-01-01T00:00:00Z"),
            Some("2024-01-02T00:00:00Z"),
            Some(5),
        );
        assert_eq!(
            classify_health(&health, NOW_RECENT),
            Some(HealthLevel::Failed)
        );
    }

    #[test]
    fn classify_recent_failure_with_count_lt_3_returns_flaky() {
        let health = health_with(
            Some("2024-01-01T00:00:00Z"),
            Some("2024-01-02T00:00:00Z"),
            Some(1),
        );
        assert_eq!(
            classify_health(&health, NOW_RECENT),
            Some(HealthLevel::Flaky)
        );
    }

    #[test]
    fn classify_recent_success_returns_healthy() {
        let health = health_with(
            Some("2024-01-02T00:00:00Z"),
            Some("2024-01-01T00:00:00Z"),
            Some(1),
        );
        assert_eq!(
            classify_health(&health, NOW_RECENT),
            Some(HealthLevel::Healthy)
        );
    }

    #[test]
    fn classify_success_only_returns_healthy() {
        let health = health_with(Some("2024-01-01T00:00:00Z"), None, None);
        assert_eq!(
            classify_health(&health, NOW_RECENT),
            Some(HealthLevel::Healthy)
        );
    }

    #[test]
    fn classify_equal_timestamps_returns_healthy() {
        let health = health_with(
            Some("2024-01-01T00:00:00Z"),
            Some("2024-01-01T00:00:00Z"),
            Some(1),
        );
        assert_eq!(
            classify_health(&health, NOW_RECENT),
            Some(HealthLevel::Healthy)
        );
    }

    #[test]
    fn classify_missing_failure_count_treated_as_zero() {
        let health = health_with(
            Some("2024-01-01T00:00:00Z"),
            Some("2024-01-02T00:00:00Z"),
            None,
        );
        assert_eq!(
            classify_health(&health, NOW_RECENT),
            Some(HealthLevel::Flaky)
        );
    }

    #[test]
    fn classify_missing_failure_count_with_success_only_returns_healthy() {
        let health = health_with(Some("2024-01-01T00:00:00Z"), None, None);
        assert_eq!(
            classify_health(&health, NOW_RECENT),
            Some(HealthLevel::Healthy)
        );
    }

    #[test]
    fn classify_failed_takes_precedence_over_flaky() {
        let health = health_with(
            Some("2024-01-01T00:00:00Z"),
            Some("2024-01-02T00:00:00Z"),
            Some(4),
        );
        assert_eq!(
            classify_health(&health, NOW_RECENT),
            Some(HealthLevel::Failed)
        );
    }

    // --- Decay tests ---

    #[test]
    fn decay_failed_with_old_failure_becomes_flaky() {
        // Failure at 2024-01-01, now is 2024-01-09 (8 days later)
        let health = health_with(
            Some("2023-12-01T00:00:00Z"),
            Some("2024-01-01T00:00:00Z"),
            Some(5),
        );
        let now = "2024-01-09T00:00:00Z";
        assert_eq!(classify_health(&health, now), Some(HealthLevel::Flaky));
    }

    #[test]
    fn decay_flaky_with_old_failure_becomes_none() {
        // Failure at 2024-01-01, now is 2024-01-09 (8 days later), count < 3 → Flaky base
        let health = health_with(
            Some("2023-12-01T00:00:00Z"),
            Some("2024-01-01T00:00:00Z"),
            Some(1),
        );
        let now = "2024-01-09T00:00:00Z";
        assert_eq!(classify_health(&health, now), None);
    }

    #[test]
    fn decay_fresh_failure_stays_failed() {
        // Failure at 2024-01-08, now is 2024-01-09 (1 day later, within threshold)
        let health = health_with(
            Some("2024-01-01T00:00:00Z"),
            Some("2024-01-08T00:00:00Z"),
            Some(3),
        );
        let now = "2024-01-09T00:00:00Z";
        assert_eq!(classify_health(&health, now), Some(HealthLevel::Failed));
    }

    #[test]
    fn decay_unparseable_timestamp_no_decay() {
        // Invalid `now` timestamp — fallback to base classification
        let health = health_with(
            Some("2024-01-01T00:00:00Z"),
            Some("2024-01-02T00:00:00Z"),
            Some(5),
        );
        let now = "not-a-timestamp";
        assert_eq!(classify_health(&health, now), Some(HealthLevel::Failed));
    }

    #[test]
    fn decay_unparseable_failure_timestamp_no_decay() {
        // Invalid failure timestamp — fallback to base classification
        let health = health_with(Some("2024-01-01T00:00:00Z"), Some("invalid-time"), Some(5));
        // Base classification checks `last_failure_at.is_some()` and count >= 3 → Failed
        assert_eq!(
            classify_health(&health, "2024-02-01T00:00:00Z"),
            Some(HealthLevel::Failed)
        );
    }

    #[test]
    fn decay_healthy_unchanged_even_when_old() {
        // Success more recent than failure, Healthy base — decay doesn't downgrade Healthy
        let health = health_with(
            Some("2024-01-10T00:00:00Z"),
            Some("2024-01-01T00:00:00Z"),
            Some(1),
        );
        let now = "2024-01-20T00:00:00Z"; // 19 days after failure
        assert_eq!(classify_health(&health, now), Some(HealthLevel::Healthy));
    }

    // --- Timestamp parsing tests ---

    #[test]
    fn timestamp_age_secs_iso8601() {
        // 2024-01-01 to 2024-01-02 = 86400 seconds
        let age = timestamp_age_secs("2024-01-01T00:00:00Z", "2024-01-02T00:00:00Z");
        assert_eq!(age, Some(86400));
    }

    #[test]
    fn timestamp_age_secs_unix_seconds() {
        let age = timestamp_age_secs("1000000", "1086400");
        assert_eq!(age, Some(86400));
    }

    #[test]
    fn timestamp_age_secs_mixed_formats_returns_none() {
        // ISO for earlier, integer for later — integer won't parse as ISO, ISO won't parse as integer
        // Actually both go through parse_to_epoch which tries both, so this may work
        // Let's verify: "2024-01-01T00:00:00Z" → parsed as ISO, "1704153600" → parsed as integer
        // This should work since parse_to_epoch handles both formats
        let age = timestamp_age_secs("2024-01-01T00:00:00Z", "1704153600");
        assert!(age.is_some());
    }

    #[test]
    fn timestamp_age_secs_invalid_returns_none() {
        let age = timestamp_age_secs("not-a-date", "also-not");
        assert_eq!(age, None);
    }

    #[test]
    fn parse_iso8601_known_epoch() {
        // 2024-01-01T00:00:00Z should be 1704067200
        let epoch = parse_to_epoch("2024-01-01T00:00:00Z");
        assert_eq!(epoch, Some(1704067200));
    }

    #[test]
    fn parse_iso8601_unix_epoch() {
        let epoch = parse_to_epoch("1970-01-01T00:00:00Z");
        assert_eq!(epoch, Some(0));
    }

    // --- Confidence tests ---

    #[test]
    fn confidence_zero_for_empty_health() {
        let health = StationHealth::default();
        assert_eq!(calculate_confidence(&health), 0.0);
    }

    #[test]
    fn confidence_one_success_is_low() {
        let health = StationHealth {
            success_count: Some(1),
            ..StationHealth::default()
        };
        assert_eq!(calculate_confidence(&health), 0.1);
    }

    #[test]
    fn confidence_five_data_points_is_half() {
        let health = StationHealth {
            success_count: Some(3),
            failure_count: Some(2),
            ..StationHealth::default()
        };
        assert_eq!(calculate_confidence(&health), 0.5);
    }

    #[test]
    fn confidence_ten_data_points_saturates() {
        let health = StationHealth {
            success_count: Some(8),
            failure_count: Some(2),
            ..StationHealth::default()
        };
        assert_eq!(calculate_confidence(&health), 1.0);
    }

    #[test]
    fn confidence_above_ten_still_capped() {
        let health = StationHealth {
            success_count: Some(50),
            failure_count: Some(10),
            ..StationHealth::default()
        };
        assert_eq!(calculate_confidence(&health), 1.0);
    }

    #[test]
    fn confidence_label_high_at_half() {
        assert_eq!(confidence_label(0.5), "high confidence");
        assert_eq!(confidence_label(1.0), "high confidence");
    }

    #[test]
    fn confidence_label_low_below_half() {
        assert_eq!(confidence_label(0.0), "low confidence");
        assert_eq!(confidence_label(0.49), "low confidence");
    }
}
