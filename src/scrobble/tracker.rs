// Scrobble tracker — state machine for now-playing and scrobble threshold logic.

use std::collections::VecDeque;

use super::TrackMetadata;

const SCROBBLE_THRESHOLD_SECONDS: u32 = 30;
const MAX_RETRY_QUEUE: usize = 50;

/// A failed scrobble awaiting retry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingScrobble {
    pub meta: TrackMetadata,
    pub timestamp: u64,
}

/// Events emitted by the ScrobbleTracker for the application layer to dispatch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScrobbleEvent {
    NowPlaying(TrackMetadata),
    Scrobble { meta: TrackMetadata, timestamp: u64 },
    Retry(PendingScrobble),
}

/// Domain-level scrobble state machine.
///
/// Ticked by the application layer each second. Emits `ScrobbleEvent` variants
/// that the application layer dispatches to the injected `ScrobbleClient`.
pub struct ScrobbleTracker {
    enabled: bool,
    current_track: Option<TrackMetadata>,
    elapsed_seconds: u32,
    last_scrobbled: Option<TrackMetadata>,
    retry_queue: VecDeque<PendingScrobble>,
}

impl ScrobbleTracker {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            current_track: None,
            elapsed_seconds: 0,
            last_scrobbled: None,
            retry_queue: VecDeque::new(),
        }
    }

    /// Handle a track change. Emits `NowPlaying` if the new title differs
    /// from the last sent now-playing title.
    pub fn on_track_change(&mut self, meta: TrackMetadata) -> Option<ScrobbleEvent> {
        if !self.enabled {
            return None;
        }
        let dominated_by_current = self
            .current_track
            .as_ref()
            .map_or(false, |current| current.title == meta.title);
        if dominated_by_current {
            return None;
        }
        self.current_track = Some(meta.clone());
        self.elapsed_seconds = 0;
        self.last_scrobbled = None;
        Some(ScrobbleEvent::NowPlaying(meta))
    }

    /// Tick the tracker (called once per second). Emits `Scrobble` when the
    /// threshold is reached for the current track.
    pub fn tick(&mut self, timestamp: u64) -> Option<ScrobbleEvent> {
        if !self.enabled {
            return None;
        }
        let track = self.current_track.as_ref()?;
        self.elapsed_seconds += 1;
        if self.elapsed_seconds == SCROBBLE_THRESHOLD_SECONDS && self.last_scrobbled.is_none() {
            let meta = track.clone();
            self.last_scrobbled = Some(meta.clone());
            return Some(ScrobbleEvent::Scrobble { meta, timestamp });
        }
        None
    }

    /// Enqueue a failed scrobble for retry. Drops the oldest if the queue
    /// exceeds MAX_RETRY_QUEUE.
    pub fn on_scrobble_failed(&mut self, pending: PendingScrobble) {
        if !self.enabled {
            return;
        }
        if self.retry_queue.len() >= MAX_RETRY_QUEUE {
            self.retry_queue.pop_front();
        }
        self.retry_queue.push_back(pending);
    }

    /// Drain all pending retries as events.
    pub fn drain_retries(&mut self) -> Vec<ScrobbleEvent> {
        self.retry_queue
            .drain(..)
            .map(ScrobbleEvent::Retry)
            .collect()
    }

    pub fn retry_queue_len(&self) -> usize {
        self.retry_queue.len()
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(artist: &str, title: &str) -> TrackMetadata {
        TrackMetadata {
            artist: artist.to_string(),
            title: title.to_string(),
        }
    }

    #[test]
    fn test_track_change_emits_now_playing() {
        let mut tracker = ScrobbleTracker::new(true);
        let event = tracker.on_track_change(meta("Artist", "Song"));
        assert_eq!(event, Some(ScrobbleEvent::NowPlaying(meta("Artist", "Song"))));
    }

    #[test]
    fn test_track_change_same_title_no_event() {
        let mut tracker = ScrobbleTracker::new(true);
        tracker.on_track_change(meta("Artist", "Song"));
        let event = tracker.on_track_change(meta("Artist", "Song"));
        assert_eq!(event, None);
    }

    #[test]
    fn test_track_change_different_title_emits_now_playing() {
        let mut tracker = ScrobbleTracker::new(true);
        tracker.on_track_change(meta("Artist", "Song A"));
        let event = tracker.on_track_change(meta("Artist", "Song B"));
        assert_eq!(
            event,
            Some(ScrobbleEvent::NowPlaying(meta("Artist", "Song B")))
        );
    }

    #[test]
    fn test_tick_emits_scrobble_at_30s() {
        let mut tracker = ScrobbleTracker::new(true);
        tracker.on_track_change(meta("Artist", "Song"));

        // Tick 29 times — no scrobble yet
        for i in 1..30 {
            assert_eq!(tracker.tick(1000 + i), None);
        }
        // 30th tick → scrobble
        let event = tracker.tick(1030);
        assert_eq!(
            event,
            Some(ScrobbleEvent::Scrobble {
                meta: meta("Artist", "Song"),
                timestamp: 1030
            })
        );
    }

    #[test]
    fn test_tick_no_double_scrobble() {
        let mut tracker = ScrobbleTracker::new(true);
        tracker.on_track_change(meta("Artist", "Song"));

        for i in 1..=30 {
            tracker.tick(1000 + i);
        }
        // Further ticks should not emit another scrobble
        assert_eq!(tracker.tick(1031), None);
        assert_eq!(tracker.tick(1032), None);
    }

    #[test]
    fn test_early_track_change_discards_pending_scrobble() {
        let mut tracker = ScrobbleTracker::new(true);
        tracker.on_track_change(meta("Artist", "Song A"));

        // Tick 15 times (not yet at threshold)
        for i in 1..=15 {
            tracker.tick(1000 + i);
        }
        // Track changes before threshold
        tracker.on_track_change(meta("Artist", "Song B"));

        // Tick 30 times for new track
        for i in 1..=30 {
            let event = tracker.tick(2000 + i);
            if i == 30 {
                assert_eq!(
                    event,
                    Some(ScrobbleEvent::Scrobble {
                        meta: meta("Artist", "Song B"),
                        timestamp: 2030
                    })
                );
            } else {
                assert_eq!(event, None);
            }
        }
    }

    #[test]
    fn test_retry_enqueue() {
        let mut tracker = ScrobbleTracker::new(true);
        let pending = PendingScrobble {
            meta: meta("Artist", "Song"),
            timestamp: 1000,
        };
        tracker.on_scrobble_failed(pending.clone());
        assert_eq!(tracker.retry_queue_len(), 1);

        let retries = tracker.drain_retries();
        assert_eq!(retries, vec![ScrobbleEvent::Retry(pending)]);
    }

    #[test]
    fn test_retry_queue_drops_oldest_when_full() {
        let mut tracker = ScrobbleTracker::new(true);

        // Fill the queue to MAX_RETRY_QUEUE
        for i in 0..MAX_RETRY_QUEUE {
            tracker.on_scrobble_failed(PendingScrobble {
                meta: meta("Artist", &format!("Song {i}")),
                timestamp: i as u64,
            });
        }
        assert_eq!(tracker.retry_queue_len(), MAX_RETRY_QUEUE);

        // Add one more — oldest (Song 0) should be dropped
        tracker.on_scrobble_failed(PendingScrobble {
            meta: meta("Artist", "New Song"),
            timestamp: 999,
        });
        assert_eq!(tracker.retry_queue_len(), MAX_RETRY_QUEUE);

        let retries = tracker.drain_retries();
        // First entry should be "Song 1" (Song 0 was dropped)
        assert_eq!(retries[0], ScrobbleEvent::Retry(PendingScrobble {
            meta: meta("Artist", "Song 1"),
            timestamp: 1,
        }));
        // Last entry should be the new one
        assert_eq!(retries[MAX_RETRY_QUEUE - 1], ScrobbleEvent::Retry(PendingScrobble {
            meta: meta("Artist", "New Song"),
            timestamp: 999,
        }));
    }

    #[test]
    fn test_disabled_track_change_no_event() {
        let mut tracker = ScrobbleTracker::new(false);
        let event = tracker.on_track_change(meta("Artist", "Song"));
        assert_eq!(event, None);
    }

    #[test]
    fn test_disabled_tick_no_event() {
        let mut tracker = ScrobbleTracker::new(false);
        // Even if we somehow had a current_track, disabled means no events
        let event = tracker.tick(1000);
        assert_eq!(event, None);
    }

    #[test]
    fn test_disabled_scrobble_failed_no_enqueue() {
        let mut tracker = ScrobbleTracker::new(false);
        tracker.on_scrobble_failed(PendingScrobble {
            meta: meta("Artist", "Song"),
            timestamp: 1000,
        });
        assert_eq!(tracker.retry_queue_len(), 0);
    }

    #[test]
    fn test_tick_with_no_current_track_no_event() {
        let mut tracker = ScrobbleTracker::new(true);
        let event = tracker.tick(1000);
        assert_eq!(event, None);
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    // Feature: v080-features, Property 5: Scrobble now-playing on track change

    fn arb_track_metadata() -> impl Strategy<Value = TrackMetadata> {
        ("[a-z]{1,5}", "[a-z]{1,5}").prop_map(|(artist, title)| TrackMetadata { artist, title })
    }

    /// Events that can be delivered to a ScrobbleTracker.
    #[derive(Debug, Clone)]
    enum TrackerEvent {
        TrackChange(TrackMetadata),
        Tick(u64),
    }

    fn arb_tracker_event() -> impl Strategy<Value = TrackerEvent> {
        prop_oneof![
            arb_track_metadata().prop_map(TrackerEvent::TrackChange),
            (0u64..10000).prop_map(TrackerEvent::Tick),
        ]
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Validates: Requirements 3.1**
        #[test]
        fn now_playing_emitted_once_per_distinct_title_transition(
            tracks in proptest::collection::vec(arb_track_metadata(), 1..50),
        ) {
            let mut tracker = ScrobbleTracker::new(true);
            let mut now_playing_count = 0u32;
            let mut last_now_playing_title: Option<String> = None;
            let mut expected_count = 0u32;

            for meta in &tracks {
                let event = tracker.on_track_change(meta.clone());

                // Compute expected: NowPlaying emits when title differs from last sent
                let should_emit = last_now_playing_title
                    .as_ref()
                    .map_or(true, |prev| prev != &meta.title);

                if should_emit {
                    expected_count += 1;
                    last_now_playing_title = Some(meta.title.clone());
                }

                if let Some(ScrobbleEvent::NowPlaying(_)) = &event {
                    now_playing_count += 1;
                }
            }

            prop_assert_eq!(
                now_playing_count, expected_count,
                "NowPlaying emitted {} times, expected {} for {} track changes",
                now_playing_count, expected_count, tracks.len()
            );
        }

        // Feature: v080-features, Property 7: Retry queue bounded at 50
        /// **Validates: Requirements 3.4**
        #[test]
        fn retry_queue_never_exceeds_50(
            num_failures in 1usize..=200,
        ) {
            let mut tracker = ScrobbleTracker::new(true);

            for i in 0..num_failures {
                let pending = PendingScrobble {
                    meta: TrackMetadata {
                        artist: format!("artist_{i}"),
                        title: format!("title_{i}"),
                    },
                    timestamp: i as u64,
                };
                tracker.on_scrobble_failed(pending);
                // Verify invariant holds after every insertion
                prop_assert!(
                    tracker.retry_queue_len() <= 50,
                    "Queue length {} exceeded 50 after {} calls",
                    tracker.retry_queue_len(), i + 1
                );
            }

            prop_assert!(
                tracker.retry_queue_len() <= 50,
                "Final queue length {} exceeded 50 after {} total calls",
                tracker.retry_queue_len(), num_failures
            );
        }

        // Feature: v080-features, Property 6: Scrobble 30-second threshold
        /// **Validates: Requirements 3.2, 3.3**
        #[test]
        fn scrobble_emitted_iff_track_unchanged_for_30_ticks(
            ticks in 1u32..=60,
        ) {
            let mut tracker = ScrobbleTracker::new(true);
            let track = TrackMetadata {
                artist: "artist".to_string(),
                title: "title".to_string(),
            };
            tracker.on_track_change(track);

            let mut scrobble_count = 0u32;
            for t in 1..=ticks {
                if let Some(ScrobbleEvent::Scrobble { .. }) = tracker.tick(t as u64) {
                    scrobble_count += 1;
                }
            }

            if ticks >= 30 {
                prop_assert_eq!(scrobble_count, 1,
                    "Expected 1 scrobble after {} ticks (>= 30), got {}",
                    ticks, scrobble_count);
            } else {
                prop_assert_eq!(scrobble_count, 0,
                    "Expected 0 scrobbles after {} ticks (< 30), got {}",
                    ticks, scrobble_count);
            }
        }

        /// **Validates: Requirements 3.2, 3.3**
        #[test]
        fn early_track_change_discards_pending_scrobble(
            first_ticks in 1u32..30,
        ) {
            let mut tracker = ScrobbleTracker::new(true);
            let track_a = TrackMetadata {
                artist: "a".to_string(),
                title: "song_a".to_string(),
            };
            let track_b = TrackMetadata {
                artist: "b".to_string(),
                title: "song_b".to_string(),
            };

            tracker.on_track_change(track_a.clone());

            // Tick less than 30 times — no scrobble yet
            let mut scrobble_for_a = 0u32;
            for t in 1..=first_ticks {
                if let Some(ScrobbleEvent::Scrobble { meta, .. }) = tracker.tick(t as u64) {
                    if meta == track_a {
                        scrobble_for_a += 1;
                    }
                }
            }

            // Track changes before threshold — pending scrobble discarded
            tracker.on_track_change(track_b.clone());

            // Tick 30 more for new track
            let mut scrobble_for_b = 0u32;
            for t in 1..=30 {
                if let Some(ScrobbleEvent::Scrobble { meta, .. }) =
                    tracker.tick(1000 + t as u64)
                {
                    if meta == track_b {
                        scrobble_for_b += 1;
                    }
                }
            }

            prop_assert_eq!(scrobble_for_a, 0,
                "Track A should NOT have been scrobbled after only {} ticks",
                first_ticks);
            prop_assert_eq!(scrobble_for_b, 1,
                "Track B should have been scrobbled after 30 ticks");
        }

        // Feature: v080-features, Property 8: Disabled scrobbler produces no events
        /// **Validates: Requirements 4.4**
        #[test]
        fn disabled_scrobbler_produces_no_events(
            events in proptest::collection::vec(arb_tracker_event(), 1..100),
        ) {
            let mut tracker = ScrobbleTracker::new(false);

            for event in &events {
                match event {
                    TrackerEvent::TrackChange(meta) => {
                        let result = tracker.on_track_change(meta.clone());
                        prop_assert_eq!(result, None, "Disabled tracker emitted event on track change");
                    }
                    TrackerEvent::Tick(ts) => {
                        let result = tracker.tick(*ts);
                        prop_assert_eq!(result, None, "Disabled tracker emitted event on tick");
                    }
                }
            }

            // Verify retry queue stays empty after failed scrobble attempts
            tracker.on_scrobble_failed(PendingScrobble {
                meta: TrackMetadata { artist: "x".into(), title: "y".into() },
                timestamp: 0,
            });
            prop_assert_eq!(tracker.retry_queue_len(), 0, "Disabled tracker accepted retry entry");
        }
    }
}
