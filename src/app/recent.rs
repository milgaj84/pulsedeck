use super::*;
use crate::audio::AudioCommand;
use crate::radio::{find_station_by_url, station_url_matches};

impl App {
    /// Assign the currently playing station to a preset slot (1-indexed).
    pub(super) fn assign_slot(&mut self, index: u8) {
        let Some(ref url) = self.playback.view.playing_url else {
            self.set_info_notice("No station playing to assign".to_string());
            return;
        };

        let url = url.clone();
        self.library
            .settings
            .station_slots
            .assign(index as usize, &url);
        self.mark_library_dirty();

        let station_name = self
            .now_playing()
            .map(|s| s.name.clone())
            .unwrap_or_else(|| "Station".to_string());
        self.set_info_notice(format!("Slot {index}: {station_name}"));
    }

    /// Play the station assigned to a preset slot (1-indexed).
    pub(super) fn play_slot(&mut self, index: u8) {
        let Some(url) = self
            .library
            .settings
            .station_slots
            .get(index as usize)
            .map(|s| s.to_string())
        else {
            self.set_info_notice(format!("Slot {index} is empty — assign with Ctrl+{index}"));
            return;
        };

        // Same-station no-op: if already playing this URL, do nothing.
        if self
            .playback
            .view
            .playing_url
            .as_deref()
            .is_some_and(|playing| station_url_matches(playing, &url))
        {
            return;
        }

        // Find the station in the library for codec validation.
        let station = find_station_by_url(&self.library.stations, &url).cloned();

        let Some(station) = station else {
            self.set_info_notice("Station no longer in library".to_string());
            return;
        };

        // Codec capability gate (same as play_selected).
        if !self.validate_station_playback_capability(&station) {
            return;
        }

        self.playback.reconnect.disarm();
        let next_playback = self.playback_after_play_command_for_slot();
        self.playback.view.playing_url = Some(station.url.clone());
        self.playback.view.state = next_playback;

        self.library.settings.last_played_url = Some(station.url.clone());
        self.mark_library_dirty();

        if self.send_audio_command(AudioCommand::Play(station.url)) {
            self.sync_volume();
        }

        self.select_playing();
    }

    /// Determine playback state transition for slot switch.
    fn playback_after_play_command_for_slot(&self) -> PlaybackState {
        if matches!(
            &self.playback.view.state,
            PlaybackState::Playing | PlaybackState::Paused | PlaybackState::FadingOut { .. }
        ) {
            PlaybackState::FadingOut {
                current_volume: self.playback.output_volume_fraction(),
            }
        } else {
            PlaybackState::Connecting
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::favorites::Library;
    use crate::radio::Station;

    fn station(name: &str, url: &str) -> Station {
        Station::basic(name, url, "Synthwave", "US", 128)
    }

    fn test_app() -> App {
        App::new(Library::in_memory(vec![
            station("A", "http://a"),
            station("B", "http://b"),
            station("C", "http://c"),
        ]))
    }

    fn notice_text(app: &App) -> Option<&str> {
        match app.ui.notice.current.as_ref() {
            Some(AppNotice::Info(msg)) | Some(AppNotice::Error(msg)) => Some(msg.as_str()),
            None => None,
        }
    }

    #[test]
    fn assign_slot_stores_playing_url() {
        let mut app = test_app();
        app.playback.view.playing_url = Some("http://a".to_string());

        app.assign_slot(1);

        assert_eq!(app.library.settings.station_slots.get(1), Some("http://a"));
    }

    #[test]
    fn assign_slot_shows_confirmation_notice() {
        let mut app = test_app();
        app.playback.view.playing_url = Some("http://a".to_string());

        app.assign_slot(3);

        assert!(notice_text(&app).unwrap().contains("Slot 3"));
    }

    #[test]
    fn assign_slot_noop_when_nothing_playing() {
        let mut app = test_app();

        app.assign_slot(1);

        assert_eq!(notice_text(&app), Some("No station playing to assign"));
        assert_eq!(app.library.settings.station_slots.get(1), None);
    }

    #[test]
    fn play_slot_starts_playback() {
        let mut app = test_app();
        app.library.settings.station_slots.assign(1, "http://b");

        app.play_slot(1);

        assert_eq!(app.playback.view.playing_url.as_deref(), Some("http://b"));
    }

    #[test]
    fn play_slot_empty_shows_notice() {
        let mut app = test_app();

        app.play_slot(2);

        assert!(notice_text(&app).unwrap().contains("Slot 2 is empty"));
    }

    #[test]
    fn play_slot_same_station_is_noop() {
        let mut app = test_app();
        app.library.settings.station_slots.assign(1, "http://a");
        app.playback.view.playing_url = Some("http://a".to_string());
        app.playback.view.state = PlaybackState::Playing;

        app.play_slot(1);

        // No change — same station
        assert_eq!(app.playback.view.state, PlaybackState::Playing);
        assert_eq!(app.ui.notice.current, None);
    }

    #[test]
    fn play_slot_station_not_in_library_shows_notice() {
        let mut app = test_app();
        app.library
            .settings
            .station_slots
            .assign(1, "http://removed");

        app.play_slot(1);

        assert_eq!(notice_text(&app), Some("Station no longer in library"));
    }

    #[test]
    fn play_slot_updates_selection_cursor() {
        let mut app = test_app();
        app.library.settings.station_slots.assign(1, "http://c");
        app.ui.nav.selected = 0;

        app.play_slot(1);

        assert_eq!(app.ui.nav.selected, 2);
    }

    #[test]
    fn play_slot_codec_failure_shows_error() {
        let mut st = station("HLS Radio", "http://hls");
        st.codec = "HLS".to_string();
        let mut app = App::new(Library::in_memory(vec![st]));
        app.library.settings.station_slots.assign(1, "http://hls");

        app.play_slot(1);

        assert_eq!(app.playback.view.playing_url, None);
        assert!(matches!(app.playback.view.state, PlaybackState::Error(_)));
    }

    #[test]
    fn slots_are_stable_after_playback() {
        let mut app = test_app();
        app.library.settings.station_slots.assign(1, "http://a");
        app.library.settings.station_slots.assign(2, "http://b");

        // Play slot 1, then slot 2 — slots should not shift
        app.play_slot(1);
        app.play_slot(2);

        assert_eq!(app.library.settings.station_slots.get(1), Some("http://a"));
        assert_eq!(app.library.settings.station_slots.get(2), Some("http://b"));
    }
}
