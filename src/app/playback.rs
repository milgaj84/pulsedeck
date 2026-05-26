use super::*;
use crate::audio::AudioCommand;

impl App {
    pub(super) fn play_selected(&mut self) {
        let station = self.visible_stations().get(self.selected).copied().cloned();
        if let Some(station) = station {
            self.playing_url = Some(station.url.clone());
            self.playback = PlaybackState::Connecting;

            // Persist last played station URL.
            self.library.settings.last_played_url = Some(station.url.clone());
            self.save_library_or_notice("last played station");

            self.audio.send(AudioCommand::Play(station.url));
            self.sync_volume();
        }
    }

    pub(super) fn toggle_pause(&mut self) {
        match self.playback.clone() {
            PlaybackState::Playing => {
                self.audio.send(AudioCommand::Pause);
            }
            PlaybackState::Paused => {
                self.audio.send(AudioCommand::Resume);
            }
            PlaybackState::Stopped | PlaybackState::Error(_) => {
                self.play_selected();
            }
            PlaybackState::Connecting => {
                self.stop_playback();
            }
        }
    }

    pub(super) fn stop_playback(&mut self) {
        self.audio.send(AudioCommand::Stop);
        self.playing_url = None;
        self.playback = PlaybackState::Stopped;
    }

    pub(super) fn stop_audio_before_quit(&mut self) {
        self.audio.send(AudioCommand::Stop);
    }

    pub(super) fn volume_up(&mut self) {
        self.volume = (self.volume + 5).min(100);
        self.muted = false;
        self.sync_volume();
    }

    pub(super) fn volume_down(&mut self) {
        self.volume = self.volume.saturating_sub(5);
        self.sync_volume();
    }

    pub(super) fn toggle_mute(&mut self) {
        self.muted = !self.muted;
        self.sync_volume();
    }

    /// Sync volume to audio engine, respecting mute state.
    pub(super) fn sync_volume(&self) {
        let vol = if self.muted {
            0.0
        } else {
            self.volume as f32 / 100.0
        };
        self.audio.send(AudioCommand::SetVolume(vol));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::favorites::Library;
    use crate::radio::Station;

    fn station(name: &str, url: &str) -> Station {
        Station {
            name: name.to_string(),
            url: url.to_string(),
            genre: "Synthwave".to_string(),
            country: "US".to_string(),
            bitrate: 128,
        }
    }

    fn test_app() -> App {
        App::new(Library::in_memory(vec![station("A", "http://a")]))
    }

    #[test]
    fn play_selected_sets_playing_url_last_played_url_and_connecting_state() {
        let mut app = test_app();

        app.play_selected();

        assert_eq!(app.playing_url.as_deref(), Some("http://a"));
        assert_eq!(
            app.library.settings.last_played_url.as_deref(),
            Some("http://a")
        );
        assert_eq!(app.playback, PlaybackState::Connecting);
    }

    #[test]
    fn stop_clears_playing_url_and_sets_stopped_state() {
        let mut app = test_app();
        app.playing_url = Some("http://a".to_string());
        app.playback = PlaybackState::Playing;

        app.stop_playback();

        assert_eq!(app.playing_url, None);
        assert_eq!(app.playback, PlaybackState::Stopped);
    }

    #[test]
    fn space_while_connecting_stops_instead_of_restarting_playback() {
        let mut app = test_app();
        app.play_selected();

        app.toggle_pause();

        assert_eq!(app.playing_url, None);
        assert_eq!(app.playback, PlaybackState::Stopped);
    }

    #[test]
    fn volume_up_unmutes() {
        let mut app = test_app();
        app.volume = 80;
        app.muted = true;

        app.volume_up();

        assert_eq!(app.volume, 85);
        assert!(!app.muted);
    }

    #[test]
    fn toggle_mute_preserves_volume_number() {
        let mut app = test_app();
        app.volume = 65;

        app.toggle_mute();

        assert!(app.muted);
        assert_eq!(app.volume, 65);
    }
}
