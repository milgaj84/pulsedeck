use super::*;
use std::time::{SystemTime, UNIX_EPOCH};

impl App {
    /// Tick the scrobble tracker with the current Unix timestamp.
    /// Emitted events are logged (no-op dispatch for now).
    pub(super) fn tick_scrobble_tracker(&mut self) {
        let timestamp = unix_timestamp_now();
        if let Some(_event) = self.scrobble_tracker.tick(timestamp) {
            // Placeholder: dispatch to ScrobbleClient will be wired in a later task.
        }
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
    use crate::scrobble::tracker::ScrobbleTracker;
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
}
