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
        let step = progressive_volume_step(self.volume);
        self.volume = self.volume.saturating_add(step).min(100);
        self.muted = false;
        self.sync_volume();
        super::ui_state::save_ui_state_or_notice(self);
    }

    pub(super) fn volume_down(&mut self) {
        let step = progressive_volume_step(self.volume);
        self.volume = self.volume.saturating_sub(step);
        self.sync_volume();
        super::ui_state::save_ui_state_or_notice(self);
    }

    pub(super) fn toggle_mute(&mut self) {
        self.muted = !self.muted;
        self.sync_volume();
        super::ui_state::save_ui_state_or_notice(self);
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

fn progressive_volume_step(volume: u8) -> u8 {
    match volume {
        0..=15 => 2,
        16..=70 => 5,
        _ => 10,
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
    fn progressive_volume_step_uses_range_based_steps() {
        assert_eq!(progressive_volume_step(0), 2);
        assert_eq!(progressive_volume_step(15), 2);
        assert_eq!(progressive_volume_step(16), 5);
        assert_eq!(progressive_volume_step(70), 5);
        assert_eq!(progressive_volume_step(71), 10);
        assert_eq!(progressive_volume_step(100), 10);
    }

    #[test]
    fn volume_up_uses_progressive_steps_and_clamps() {
        let mut app = test_app();

        app.volume = 12;
        app.volume_up();
        assert_eq!(app.volume, 14);

        app.volume = 45;
        app.volume_up();
        assert_eq!(app.volume, 50);

        app.volume = 95;
        app.volume_up();
        assert_eq!(app.volume, 100);
    }

    #[test]
    fn volume_down_uses_progressive_steps_and_saturates() {
        let mut app = test_app();

        app.volume = 12;
        app.volume_down();
        assert_eq!(app.volume, 10);

        app.volume = 45;
        app.volume_down();
        assert_eq!(app.volume, 40);

        app.volume = 80;
        app.volume_down();
        assert_eq!(app.volume, 70);

        app.volume = 1;
        app.volume_down();
        assert_eq!(app.volume, 0);
    }

    #[test]
    fn volume_up_unmutes() {
        let mut app = test_app();
        app.volume = 80;
        app.muted = true;

        app.volume_up();

        assert_eq!(app.volume, 90);
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
