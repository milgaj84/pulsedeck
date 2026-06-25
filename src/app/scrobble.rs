use super::*;
use crate::scrobble::tracker::PendingScrobble;
use std::time::{SystemTime, UNIX_EPOCH};

const RETRY_DRAIN_INTERVAL_SECS: u32 = 60;

impl App {
    /// Tick the scrobble tracker with the current Unix timestamp.
    /// Emitted events are logged (no-op dispatch for now).
    pub(super) fn tick_scrobble_tracker(&mut self) {
        let timestamp = unix_timestamp_now();
        if let Some(_event) = self.scrobble_tracker.tick(timestamp) {
            // Placeholder: dispatch to ScrobbleClient will be wired in a later task.
        }
        self.maybe_drain_retries();
    }

    /// Periodically drain retry queue. Increments counter each tick;
    /// at 60 ticks, drains and dispatches retries. Skips when disabled.
    pub(super) fn maybe_drain_retries(&mut self) {
        if !self.scrobble_tracker.is_enabled() {
            return;
        }
        self.retry_drain_counter += 1;
        if self.retry_drain_counter < RETRY_DRAIN_INTERVAL_SECS {
            return;
        }
        self.retry_drain_counter = 0;
        let retries = self.scrobble_tracker.drain_retries();
        for event in retries {
            self.dispatch_scrobble_retry(event);
        }
    }

    /// Dispatch a single retry event. On failure, re-enqueue.
    /// Currently a placeholder — real client dispatch will be wired later.
    fn dispatch_scrobble_retry(&mut self, event: crate::scrobble::tracker::ScrobbleEvent) {
        if let crate::scrobble::tracker::ScrobbleEvent::Retry(pending) = event {
            // Placeholder: when a ScrobbleClient is injected, call it here.
            // For now, treat as success (consume the entry).
            let _ = pending;
        }
    }

    /// Re-enqueue a failed retry back into the tracker.
    pub(super) fn on_retry_failed(&mut self, pending: PendingScrobble) {
        self.scrobble_tracker.on_scrobble_failed(pending);
    }
}

fn unix_timestamp_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::favorites::Library;
    use crate::radio::Station;
    use crate::scrobble::tracker::{PendingScrobble, ScrobbleTracker};
    use crate::scrobble::TrackMetadata;

    fn station(name: &str, url: &str) -> Station {
        Station::basic(name, url, "Synthwave", "US", 128)
    }

    fn test_app() -> App {
        App::new(Library::in_memory(vec![
            station("A", "http://a"),
            station("B", "http://b"),
        ]))
    }

    fn pending(artist: &str, title: &str, ts: u64) -> PendingScrobble {
        PendingScrobble {
            meta: TrackMetadata {
                artist: artist.to_string(),
                title: title.to_string(),
            },
            timestamp: ts,
        }
    }

    #[test]
    fn test_counter_increments_on_tick_when_enabled() {
        let mut app = test_app();
        app.scrobble_tracker = ScrobbleTracker::new(true);
        assert_eq!(app.retry_drain_counter, 0);

        app.maybe_drain_retries();

        assert_eq!(app.retry_drain_counter, 1);
    }

    #[test]
    fn test_counter_does_not_increment_when_disabled() {
        let mut app = test_app();
        app.scrobble_tracker = ScrobbleTracker::new(false);

        app.maybe_drain_retries();

        assert_eq!(app.retry_drain_counter, 0);
    }

    #[test]
    fn test_drain_fires_at_60_and_resets_counter() {
        let mut app = test_app();
        app.scrobble_tracker = ScrobbleTracker::new(true);

        for _ in 0..59 {
            app.maybe_drain_retries();
        }
        assert_eq!(app.retry_drain_counter, 59);

        app.maybe_drain_retries();
        assert_eq!(app.retry_drain_counter, 0);
    }

    #[test]
    fn test_drain_removes_successful_retries() {
        let mut app = test_app();
        app.scrobble_tracker = ScrobbleTracker::new(true);
        app.scrobble_tracker.on_scrobble_failed(pending("A", "Song", 100));
        assert_eq!(app.scrobble_tracker.retry_queue_len(), 1);

        // Advance counter to trigger drain
        app.retry_drain_counter = 59;
        app.maybe_drain_retries();

        // Default dispatch is success (placeholder), so queue is drained
        assert_eq!(app.scrobble_tracker.retry_queue_len(), 0);
    }

    #[test]
    fn test_on_retry_failed_re_enqueues() {
        let mut app = test_app();
        app.scrobble_tracker = ScrobbleTracker::new(true);

        let p = pending("Artist", "Title", 500);
        app.on_retry_failed(p);

        assert_eq!(app.scrobble_tracker.retry_queue_len(), 1);
    }

    #[test]
    fn test_scrobble_tracker_ticks_on_app_tick() {
        let mut app = test_app();
        app.scrobble_tracker = ScrobbleTracker::new(true);
        let meta = TrackMetadata {
            artist: "Artist".to_string(),
            title: "Song".to_string(),
        };
        app.scrobble_tracker.on_track_change(meta);

        // Tick the app — scrobble tracker elapsed should advance
        app.tick_scrobble_tracker();

        // After one tick, no scrobble event yet (need 30)
        // We verify indirectly: tick again 29 more times to reach threshold
        for _ in 0..29 {
            app.tick_scrobble_tracker();
        }
        // The tracker should have emitted a scrobble at tick 30,
        // but since tick_scrobble_tracker consumes it, we verify the
        // tracker no longer emits on subsequent ticks.
        let event = app.scrobble_tracker.tick(9999);
        assert_eq!(event, None); // Already scrobbled, no double-emit
    }

    #[test]
    fn test_track_change_forwarded_to_tracker() {
        let mut app = test_app();
        app.scrobble_tracker = ScrobbleTracker::new(true);
        app.playback.view.playing_url = Some("http://a".to_string());

        app.handle_track_changed("http://a".to_string(), "Artist - Song".to_string());

        // The tracker should now have a current track set.
        // We verify by ticking 30 times — a scrobble event should emit.
        for _ in 0..29 {
            app.tick_scrobble_tracker();
        }
        let timestamp = unix_timestamp_now();
        let event = app.scrobble_tracker.tick(timestamp);
        assert_eq!(
            event,
            Some(crate::scrobble::tracker::ScrobbleEvent::Scrobble {
                meta: TrackMetadata {
                    artist: "Artist".to_string(),
                    title: "Song".to_string(),
                },
                timestamp,
            })
        );
    }

    #[test]
    fn test_disabled_config_tracker_disabled() {
        let mut app = test_app();
        // Default: scrobble_tracker is initialized with enabled=false
        assert_eq!(app.scrobble_tracker.tick(1000), None);

        app.playback.view.playing_url = Some("http://a".to_string());
        app.handle_track_changed("http://a".to_string(), "Artist - Song".to_string());

        // Even after track change and ticks, no events from disabled tracker
        for _ in 0..35 {
            app.tick_scrobble_tracker();
        }
        assert_eq!(app.scrobble_tracker.tick(9999), None);
    }

    #[test]
    fn test_maybe_drain_retries_wired_into_tick() {
        let mut app = test_app();
        app.scrobble_tracker = ScrobbleTracker::new(true);

        // Calling tick_scrobble_tracker should also call maybe_drain_retries
        app.tick_scrobble_tracker();
        assert_eq!(app.retry_drain_counter, 1);
    }
}
