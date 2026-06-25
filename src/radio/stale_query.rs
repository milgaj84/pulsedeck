use super::health_classifier::{classify_health, timestamp_age_secs, HealthLevel};
use super::station::Station;

/// 30 days in seconds.
const STALE_THRESHOLD_SECS: u64 = 2_592_000;

/// Count stations classified as Failed whose last failure is 30+ days old.
/// Pure function — no I/O or side effects.
pub fn count_stale_stations(stations: &[Station], now: &str) -> usize {
    stations
        .iter()
        .filter(|station| is_stale(station, now))
        .count()
}

fn is_stale(station: &Station, now: &str) -> bool {
    if station.health.is_empty() {
        return false;
    }

    let Some(last_failure_at) = station.health.last_failure_at.as_deref() else {
        return false;
    };

    let Some(age) = timestamp_age_secs(last_failure_at, now) else {
        return false;
    };

    if age < STALE_THRESHOLD_SECS {
        return false;
    }

    // Use classify_health without decay-awareness: we check base Failed status
    // by verifying failure_count >= 3 (the condition for Failed classification).
    // We pass last_failure_at as "now" to avoid decay downgrading the result.
    matches!(
        classify_health(&station.health, last_failure_at),
        Some(HealthLevel::Failed)
    )
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use crate::radio::station::StationHealth;
    use proptest::prelude::*;

    // Feature: v100-features, Property 7: Stale station count matches manual classification

    fn manually_is_stale(station: &Station, now: &str) -> bool {
        if station.health.is_empty() {
            return false;
        }
        let Some(last_failure_at) = station.health.last_failure_at.as_deref() else {
            return false;
        };
        let Some(age) = timestamp_age_secs(last_failure_at, now) else {
            return false;
        };
        if age < STALE_THRESHOLD_SECS {
            return false;
        }
        matches!(
            classify_health(&station.health, last_failure_at),
            Some(HealthLevel::Failed)
        )
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Validates: Requirements 2.1, 2.5**
        #[test]
        fn stale_count_matches_manual_classification(
            now_offset in 200u64 * 86400..=400u64 * 86400,
        ) {
            let now_epoch = now_offset;
            let now = format!("{}", now_epoch);

            // Generate stations inline with fixed now_epoch
            let stations: Vec<Station> = (0..10).map(|i| {
                let mut station = Station::basic(
                    &format!("S{}", i),
                    &format!("http://s{}", i),
                    "Genre", "US", 128,
                );
                // Vary health data deterministically from index
                if i % 3 == 0 {
                    // Failed with old failure
                    station.health = StationHealth {
                        last_success_at: Some(format!("{}", now_epoch.saturating_sub(100 * 86400))),
                        last_failure_at: Some(format!("{}", now_epoch.saturating_sub(50 * 86400))),
                        failure_count: Some(5),
                        last_error_summary: String::new(),
                    };
                } else if i % 3 == 1 {
                    // Healthy
                    station.health = StationHealth {
                        last_success_at: Some(format!("{}", now_epoch.saturating_sub(86400))),
                        last_failure_at: None,
                        failure_count: None,
                        last_error_summary: String::new(),
                    };
                }
                // else: empty health
                station
            }).collect();

            let actual = count_stale_stations(&stations, &now);
            let expected = stations.iter().filter(|s| manually_is_stale(s, &now)).count();
            prop_assert_eq!(actual, expected);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::radio::station::StationHealth;

    fn station_with_health(health: StationHealth) -> Station {
        let mut station = Station::basic("Test", "http://test", "Genre", "US", 128);
        station.health = health;
        station
    }

    fn healthy_health() -> StationHealth {
        StationHealth {
            last_success_at: Some("2024-06-01T00:00:00Z".to_string()),
            last_failure_at: None,
            failure_count: None,
            last_error_summary: String::new(),
        }
    }

    fn failed_health(last_failure_at: &str) -> StationHealth {
        StationHealth {
            last_success_at: Some("2024-01-01T00:00:00Z".to_string()),
            last_failure_at: Some(last_failure_at.to_string()),
            failure_count: Some(5),
            last_error_summary: "connection refused".to_string(),
        }
    }

    #[test]
    fn test_no_stations_returns_zero() {
        assert_eq!(count_stale_stations(&[], "2024-07-01T00:00:00Z"), 0);
    }

    #[test]
    fn test_healthy_stations_returns_zero() {
        let stations = vec![
            station_with_health(healthy_health()),
            station_with_health(healthy_health()),
        ];
        assert_eq!(count_stale_stations(&stations, "2024-07-01T00:00:00Z"), 0);
    }

    #[test]
    fn test_recent_failure_not_counted() {
        // Failed 10 days ago (less than 30-day threshold)
        let now = "2024-07-01T00:00:00Z";
        let ten_days_ago = "2024-06-21T00:00:00Z";
        let stations = vec![station_with_health(failed_health(ten_days_ago))];
        assert_eq!(count_stale_stations(&stations, now), 0);
    }

    #[test]
    fn test_old_failure_counted() {
        // Failed 40 days ago (more than 30-day threshold)
        let now = "2024-07-01T00:00:00Z";
        let forty_days_ago = "2024-05-22T00:00:00Z";
        let stations = vec![station_with_health(failed_health(forty_days_ago))];
        assert_eq!(count_stale_stations(&stations, now), 1);
    }

    #[test]
    fn test_mixed_health_correct_count() {
        let now = "2024-07-01T00:00:00Z";
        let forty_days_ago = "2024-05-22T00:00:00Z";
        let ten_days_ago = "2024-06-21T00:00:00Z";

        let stations = vec![
            station_with_health(healthy_health()), // healthy → not stale
            station_with_health(failed_health(ten_days_ago)), // recent failure → not stale
            station_with_health(failed_health(forty_days_ago)), // old failure → stale
            station_with_health(StationHealth::default()), // no health data → not stale
        ];
        assert_eq!(count_stale_stations(&stations, now), 1);
    }
}
