use super::*;
use crate::audio::AudioCommand;

pub struct PlaybackView {
    pub state: PlaybackState,
    pub playing_url: Option<String>,
    pub current_track: Option<String>,
    pub buffer_percent: u8,
    pub buffer_seconds: u32,
    pub intentional_stop: bool,
}

impl Default for PlaybackView {
    fn default() -> Self {
        Self {
            state: PlaybackState::Stopped,
            playing_url: None,
            current_track: None,
            buffer_percent: 0,
            buffer_seconds: 0,
            intentional_stop: false,
        }
    }
}

impl App {
    pub(super) fn send_audio_command(&mut self, command: AudioCommand) -> bool {
        if self.audio.send(command) {
            true
        } else {
            self.player.current_track = None;
            self.player.buffer_percent = 0;
            self.player.buffer_seconds = 0;
            self.player.state = PlaybackState::Error("Audio engine stopped".to_string());
            self.diagnostics.decoder_state = DecoderState::Failed;
            self.diagnostics.last_error = Some("Audio engine command channel closed".to_string());
            self.set_error_notice("Audio engine is not available");
            false
        }
    }

    pub(super) fn play_selected(&mut self) {
        let station = self
            .visible_stations()
            .get(self.nav.selected)
            .copied()
            .cloned();
        if let Some(station) = station {
            self.reconnect.disarm();
            let next_playback = self.playback_after_play_command();
            self.player.playing_url = Some(station.url.clone());
            self.player.state = next_playback;

            // Persist last played station URL.
            self.library.settings.last_played_url = Some(station.url.clone());
            self.mark_library_dirty();

            if self.send_audio_command(AudioCommand::Play(station.url)) {
                self.sync_volume();
            }
        }
    }

    pub(super) fn retry_stream(&mut self) {
        let Some(url) = self.player.playing_url.clone() else {
            self.set_error_notice("No stream to retry");
            return;
        };

        self.reconnect.disarm();
        self.player.current_track = None;
        self.player.buffer_percent = 0;
        self.player.buffer_seconds = 0;
        self.player.state = PlaybackState::Connecting;
        if self.send_audio_command(AudioCommand::Play(url)) {
            self.sync_volume();
            self.set_info_notice("Retrying stream");
        }
    }

    pub(super) fn toggle_pause(&mut self) {
        match self.player.state.clone() {
            PlaybackState::Playing => {
                self.send_audio_command(AudioCommand::Pause);
            }
            PlaybackState::Paused => {
                self.send_audio_command(AudioCommand::Resume);
            }
            PlaybackState::Stopped | PlaybackState::Error(_) => {
                self.play_selected();
            }
            PlaybackState::Connecting | PlaybackState::FadingOut { .. } => {
                self.stop_playback();
            }
        }
    }

    pub(super) fn stop_playback(&mut self) {
        self.player.intentional_stop = true;
        if !self.send_audio_command(AudioCommand::Stop) {
            self.player.playing_url = None;
            return;
        }

        if matches!(
            &self.player.state,
            PlaybackState::Playing | PlaybackState::Paused | PlaybackState::FadingOut { .. }
        ) {
            self.player.state = PlaybackState::FadingOut {
                current_volume: self.current_output_volume_fraction(),
            };
        } else {
            self.player.playing_url = None;
            self.player.state = PlaybackState::Stopped;
        }
    }

    pub(super) fn stop_audio_before_quit(&mut self) {
        self.player.intentional_stop = true;
        self.flush_persistence();
        self.audio.send(AudioCommand::Stop);
    }

    pub(super) fn volume_up(&mut self) {
        let step = progressive_volume_step(self.volume);
        self.volume = self.volume.saturating_add(step).min(100);
        self.muted = false;
        self.sync_volume();
        self.mark_ui_state_dirty();
    }

    pub(super) fn volume_down(&mut self) {
        let step = progressive_volume_step(self.volume);
        self.volume = self.volume.saturating_sub(step);
        self.sync_volume();
        self.mark_ui_state_dirty();
    }

    pub(super) fn toggle_mute(&mut self) {
        self.muted = !self.muted;
        self.sync_volume();
        self.mark_ui_state_dirty();
    }

    fn playback_after_play_command(&self) -> PlaybackState {
        if matches!(
            &self.player.state,
            PlaybackState::Playing | PlaybackState::Paused | PlaybackState::FadingOut { .. }
        ) {
            PlaybackState::FadingOut {
                current_volume: self.current_output_volume_fraction(),
            }
        } else {
            PlaybackState::Connecting
        }
    }

    fn current_output_volume_fraction(&self) -> f32 {
        if self.muted {
            0.0
        } else {
            self.volume as f32 / 100.0
        }
    }

    /// Sync volume to audio engine, respecting mute state.
    pub(super) fn sync_volume(&self) -> bool {
        self.audio.send(AudioCommand::SetVolume(
            self.current_output_volume_fraction(),
        ))
    }

    pub(super) fn export_library(&mut self) {
        let unixtime = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let export_dir = self
            .library
            .path
            .as_ref()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
            .or_else(|| dirs::config_dir().map(|base| base.join("pulsedeck")));

        if let Some(dir) = export_dir {
            if let Err(e) = std::fs::create_dir_all(&dir) {
                self.set_error_notice(format!("Failed to create export directory: {e}"));
                return;
            }
            let filepath = dir.join(format!("pulsedeck-export-{}.m3u", unixtime));
            let m3u_content = crate::playlist::to_m3u(&self.library.stations);
            match std::fs::write(&filepath, m3u_content) {
                Ok(_) => {
                    self.set_info_notice(format!("Library exported to {}", filepath.display()))
                }
                Err(e) => self.set_error_notice(format!("Export failed: {e}")),
            }
        } else {
            self.set_error_notice("Could not resolve config directory for export");
        }
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
        Station::basic(name, url, "Synthwave", "US", 128)
    }

    fn test_app() -> App {
        App::new(Library::in_memory(vec![station("A", "http://a")]))
    }

    #[test]
    fn play_selected_sets_playing_url_last_played_url_and_connecting_state() {
        let mut app = test_app();

        app.play_selected();

        assert_eq!(app.player.playing_url.as_deref(), Some("http://a"));
        assert_eq!(
            app.library.settings.last_played_url.as_deref(),
            Some("http://a")
        );
        assert_eq!(app.player.state, PlaybackState::Connecting);
    }

    #[test]
    fn retry_stream_reuses_current_url_and_resets_transient_status() {
        let mut app = test_app();
        app.player.playing_url = Some("http://a".to_string());
        app.player.state = PlaybackState::Error("device vanished".to_string());
        app.player.current_track = Some("Old Track".to_string());
        app.player.buffer_percent = 80;
        app.player.buffer_seconds = 12;

        app.retry_stream();

        assert_eq!(app.player.playing_url.as_deref(), Some("http://a"));
        assert_eq!(app.player.state, PlaybackState::Connecting);
        assert_eq!(app.player.current_track, None);
        assert_eq!(app.player.buffer_percent, 0);
        assert_eq!(app.player.buffer_seconds, 0);
    }

    #[test]
    fn retry_stream_without_url_sets_error_notice() {
        let mut app = test_app();

        app.retry_stream();

        assert_eq!(app.player.playing_url, None);
        assert_eq!(app.player.state, PlaybackState::Stopped);
        assert!(matches!(
            app.notice.current.as_ref(),
            Some(AppNotice::Error(_))
        ));
    }

    #[test]
    fn play_selected_while_playing_enters_fading_out_state() {
        let mut app = test_app();
        app.player.state = PlaybackState::Playing;
        app.volume = 80;

        app.play_selected();

        assert_eq!(app.player.playing_url.as_deref(), Some("http://a"));
        match app.player.state {
            PlaybackState::FadingOut { current_volume } => {
                assert!((current_volume - 0.8).abs() < 0.001);
            }
            other => panic!("expected fading out state, got {other:?}"),
        }
    }

    #[test]
    fn stop_while_playing_enters_fading_out_and_keeps_station_context() {
        let mut app = test_app();
        app.player.playing_url = Some("http://a".to_string());
        app.player.state = PlaybackState::Playing;
        app.volume = 80;

        app.stop_playback();

        assert_eq!(app.player.playing_url.as_deref(), Some("http://a"));
        match app.player.state {
            PlaybackState::FadingOut { current_volume } => {
                assert!((current_volume - 0.8).abs() < 0.001);
            }
            other => panic!("expected fading out state, got {other:?}"),
        }
    }

    #[test]
    fn space_while_connecting_stops_instead_of_restarting_playback() {
        let mut app = test_app();
        app.play_selected();

        app.toggle_pause();

        assert_eq!(app.player.playing_url, None);
        assert_eq!(app.player.state, PlaybackState::Stopped);
    }

    #[test]
    fn play_selected_surfaces_dead_audio_engine() {
        let mut app = test_app();
        app.audio = crate::audio::AudioEngine::disconnected_for_test();

        app.play_selected();

        assert!(matches!(app.player.state, PlaybackState::Error(_)));
        assert!(matches!(app.notice.current, Some(AppNotice::Error(_))));
        assert_eq!(
            app.diagnostics.last_error.as_deref(),
            Some("Audio engine command channel closed")
        );
    }

    #[test]
    fn sync_volume_reports_dead_audio_engine_without_changing_playback_state() {
        let mut app = test_app();
        app.audio = crate::audio::AudioEngine::disconnected_for_test();
        app.player.state = PlaybackState::Playing;

        assert!(!app.sync_volume());
        assert_eq!(app.player.state, PlaybackState::Playing);
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

    #[test]
    fn test_export_library_success() {
        let mut app = test_app();
        let temp_dir = std::env::temp_dir().join(format!(
            "pulsedeck_test_export_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
        ));
        let _ = std::fs::create_dir_all(&temp_dir);
        let lib_path = temp_dir.join("library.json");
        app.library.path = Some(lib_path);

        app.export_library();

        let mut found_m3u = false;
        for entry in std::fs::read_dir(&temp_dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "m3u") {
                let name = path.file_name().unwrap().to_str().unwrap();
                assert!(name.starts_with("pulsedeck-export-"));
                let content = std::fs::read_to_string(&path).unwrap();
                assert!(content.contains("#EXTM3U"));
                assert!(content.contains(",A"));
                found_m3u = true;
            }
        }
        assert!(found_m3u, "Expected .m3u export file to be created");
        assert!(matches!(
            app.notice.current.as_ref(),
            Some(crate::app::types::AppNotice::Info(_))
        ));

        // clean up
        let _ = std::fs::remove_dir_all(temp_dir);
    }
}
