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

impl PlaybackView {
    /// Reset transient buffer/track status without changing playback state or URL.
    pub fn reset_transient_status(&mut self) {
        self.current_track = None;
        self.buffer_percent = 0;
        self.buffer_seconds = 0;
    }
}

impl App {
    pub(super) fn send_audio_command(&mut self, command: AudioCommand) -> bool {
        if self.playback.audio.send(command) {
            true
        } else {
            self.playback.view.reset_transient_status();
            self.playback.view.state = PlaybackState::Error("Audio engine stopped".to_string());
            self.playback.diagnostics.decoder_state = DecoderState::Failed;
            self.playback.diagnostics.last_error =
                Some("Audio engine command channel closed".to_string());
            self.set_error_notice("Audio engine is not available");
            false
        }
    }

    pub(super) fn play_selected(&mut self) {
        let station = self
            .visible_stations()
            .get(self.ui.nav.selected)
            .copied()
            .cloned();
        if let Some(station) = station {
            if !self.validate_station_playback_capability(&station) {
                return;
            }

            self.playback.reconnect.disarm();
            let next_playback = self.playback_after_play_command();
            self.playback.view.playing_url = Some(station.url.clone());
            self.playback.view.state = next_playback;

            // Persist last played station URL only after capability is confirmed.
            self.library.settings.last_played_url = Some(station.url.clone());
            self.mark_library_dirty();

            if self.send_audio_command(AudioCommand::Play(station.url)) {
                self.sync_volume();
            }
        }
    }

    /// Returns `true` when the station codec is safe to attempt playback.
    /// For `Unsupported` codecs, sets an error state and notice before returning `false`.
    pub(super) fn validate_station_playback_capability(
        &mut self,
        station: &crate::radio::Station,
    ) -> bool {
        use crate::audio::PlaybackCapability;

        let capability = crate::audio::codec_capability(&station.codec);

        match capability.capability {
            PlaybackCapability::Supported | PlaybackCapability::Unknown => true,
            PlaybackCapability::Unsupported => {
                self.playback.view.reset_transient_status();
                self.playback.view.state = PlaybackState::Error(format!(
                    "Unsupported codec: {}",
                    capability.normalized_codec
                ));
                self.playback.view.playing_url = None;
                self.playback.reconnect.disarm();
                self.playback.diagnostics.decoder_state = DecoderState::Failed;
                self.playback.diagnostics.last_error = Some(format!(
                    "{}: {}",
                    capability.normalized_codec, capability.reason
                ));
                self.set_error_notice(format!(
                    "{} is not playable yet ({})",
                    display_station_codec(&station.codec),
                    capability.reason
                ));
                false
            }
        }
    }

    /// Thin wrapper used by lifecycle autoplay so the codec gate is in one place.
    pub(super) fn can_attempt_station_playback(
        &mut self,
        station: &crate::radio::Station,
    ) -> bool {
        self.validate_station_playback_capability(station)
    }

    pub(super) fn retry_stream(&mut self) {
        let Some(url) = self.playback.view.playing_url.clone() else {
            self.set_error_notice("No stream to retry");
            return;
        };

        self.playback.reconnect.disarm();
        self.playback.view.reset_transient_status();
        self.playback.view.state = PlaybackState::Connecting;
        if self.send_audio_command(AudioCommand::Play(url)) {
            self.sync_volume();
            self.set_info_notice("Retrying stream");
        }
    }

    pub(super) fn toggle_pause(&mut self) {
        match self.playback.view.state.clone() {
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
        self.playback.view.intentional_stop = true;
        if !self.send_audio_command(AudioCommand::Stop) {
            self.playback.view.playing_url = None;
            return;
        }

        if matches!(
            &self.playback.view.state,
            PlaybackState::Playing | PlaybackState::Paused | PlaybackState::FadingOut { .. }
        ) {
            self.playback.view.state = PlaybackState::FadingOut {
                current_volume: self.current_output_volume_fraction(),
            };
        } else {
            self.playback.view.playing_url = None;
            self.playback.view.state = PlaybackState::Stopped;
        }
    }

    pub(super) fn stop_audio_before_quit(&mut self) {
        self.playback.view.intentional_stop = true;
        self.force_flush_persistence();
        self.playback.audio.send(AudioCommand::Stop);
    }

    pub(super) fn volume_up(&mut self) {
        let step = progressive_volume_step(self.playback.volume);
        self.playback.volume = self.playback.volume.saturating_add(step).min(100);
        self.playback.muted = false;
        self.sync_volume();
        self.mark_ui_state_dirty();
    }

    pub(super) fn volume_down(&mut self) {
        let step = progressive_volume_step(self.playback.volume);
        self.playback.volume = self.playback.volume.saturating_sub(step);
        self.sync_volume();
        self.mark_ui_state_dirty();
    }

    pub(super) fn toggle_mute(&mut self) {
        self.playback.muted = !self.playback.muted;
        self.sync_volume();
        self.mark_ui_state_dirty();
    }

    fn playback_after_play_command(&self) -> PlaybackState {
        if matches!(
            &self.playback.view.state,
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
        self.playback.output_volume_fraction()
    }

    /// Sync volume to audio engine, respecting mute state.
    pub(super) fn sync_volume(&self) -> bool {
        self.playback.audio.send(AudioCommand::SetVolume(
            self.current_output_volume_fraction(),
        ))
    }

    pub(super) fn export_library(&mut self) {
        let Some(dir) = self.export_directory() else {
            self.set_error_notice("Could not resolve config directory for export");
            return;
        };

        match crate::playlist_export::export_library_m3u(
            &self.library.stations,
            &dir,
            current_unix_time(),
        ) {
            Ok(filepath) => {
                self.set_info_notice(format!("Library exported to {}", filepath.display()))
            }
            Err(err) => self.set_error_notice(format!("Export failed: {err}")),
        }
    }

    fn export_directory(&self) -> Option<std::path::PathBuf> {
        self.library
            .path
            .as_ref()
            .and_then(|path| path.parent().map(|dir| dir.to_path_buf()))
            .or_else(|| dirs::config_dir().map(|base| base.join("pulsedeck")))
    }
}

fn display_station_codec(codec: &str) -> String {
    let codec = codec.trim();
    if codec.is_empty() {
        "Unknown codec".to_string()
    } else {
        codec.to_ascii_uppercase()
    }
}

fn current_unix_time() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
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

        assert_eq!(app.playback.view.playing_url.as_deref(), Some("http://a"));
        assert_eq!(
            app.library.settings.last_played_url.as_deref(),
            Some("http://a")
        );
        assert_eq!(app.playback.view.state, PlaybackState::Connecting);
    }

    #[test]
    fn retry_stream_reuses_current_url_and_resets_transient_status() {
        let mut app = test_app();
        app.playback.view.playing_url = Some("http://a".to_string());
        app.playback.view.state = PlaybackState::Error("device vanished".to_string());
        app.playback.view.current_track = Some("Old Track".to_string());
        app.playback.view.buffer_percent = 80;
        app.playback.view.buffer_seconds = 12;

        app.retry_stream();

        assert_eq!(app.playback.view.playing_url.as_deref(), Some("http://a"));
        assert_eq!(app.playback.view.state, PlaybackState::Connecting);
        assert_eq!(app.playback.view.current_track, None);
        assert_eq!(app.playback.view.buffer_percent, 0);
        assert_eq!(app.playback.view.buffer_seconds, 0);
    }

    #[test]
    fn retry_stream_without_url_sets_error_notice() {
        let mut app = test_app();

        app.retry_stream();

        assert_eq!(app.playback.view.playing_url, None);
        assert_eq!(app.playback.view.state, PlaybackState::Stopped);
        assert!(matches!(
            app.ui.notice.current.as_ref(),
            Some(AppNotice::Error(_))
        ));
    }

    #[test]
    fn play_selected_while_playing_enters_fading_out_state() {
        let mut app = test_app();
        app.playback.view.state = PlaybackState::Playing;
        app.playback.volume = 80;

        app.play_selected();

        assert_eq!(app.playback.view.playing_url.as_deref(), Some("http://a"));
        match app.playback.view.state {
            PlaybackState::FadingOut { current_volume } => {
                assert!((current_volume - 0.8).abs() < 0.001);
            }
            other => panic!("expected fading out state, got {other:?}"),
        }
    }

    #[test]
    fn stop_while_playing_enters_fading_out_and_keeps_station_context() {
        let mut app = test_app();
        app.playback.view.playing_url = Some("http://a".to_string());
        app.playback.view.state = PlaybackState::Playing;
        app.playback.volume = 80;

        app.stop_playback();

        assert_eq!(app.playback.view.playing_url.as_deref(), Some("http://a"));
        match app.playback.view.state {
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

        assert_eq!(app.playback.view.playing_url, None);
        assert_eq!(app.playback.view.state, PlaybackState::Stopped);
    }

    #[test]
    fn play_selected_surfaces_dead_audio_engine() {
        let mut app = test_app();
        app.playback.audio = crate::audio::AudioEngine::disconnected_for_test();

        app.play_selected();

        assert!(matches!(app.playback.view.state, PlaybackState::Error(_)));
        assert!(matches!(app.ui.notice.current, Some(AppNotice::Error(_))));
        assert_eq!(
            app.playback.diagnostics.last_error.as_deref(),
            Some("Audio engine command channel closed")
        );
    }

    #[test]
    fn sync_volume_reports_dead_audio_engine_without_changing_playback_state() {
        let mut app = test_app();
        app.playback.audio = crate::audio::AudioEngine::disconnected_for_test();
        app.playback.view.state = PlaybackState::Playing;

        assert!(!app.sync_volume());
        assert_eq!(app.playback.view.state, PlaybackState::Playing);
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

        app.playback.volume = 12;
        app.volume_up();
        assert_eq!(app.playback.volume, 14);

        app.playback.volume = 45;
        app.volume_up();
        assert_eq!(app.playback.volume, 50);

        app.playback.volume = 95;
        app.volume_up();
        assert_eq!(app.playback.volume, 100);
    }

    #[test]
    fn volume_down_uses_progressive_steps_and_saturates() {
        let mut app = test_app();

        app.playback.volume = 12;
        app.volume_down();
        assert_eq!(app.playback.volume, 10);

        app.playback.volume = 45;
        app.volume_down();
        assert_eq!(app.playback.volume, 40);

        app.playback.volume = 80;
        app.volume_down();
        assert_eq!(app.playback.volume, 70);

        app.playback.volume = 1;
        app.volume_down();
        assert_eq!(app.playback.volume, 0);
    }

    #[test]
    fn volume_up_unmutes() {
        let mut app = test_app();
        app.playback.volume = 80;
        app.playback.muted = true;

        app.volume_up();

        assert_eq!(app.playback.volume, 90);
        assert!(!app.playback.muted);
    }

    #[test]
    fn toggle_mute_preserves_volume_number() {
        let mut app = test_app();
        app.playback.volume = 65;

        app.toggle_mute();

        assert!(app.playback.muted);
        assert_eq!(app.playback.volume, 65);
    }

    #[test]
    fn mp3_codec_starts_playback() {
        let mut st = station("MP3 Radio", "http://mp3");
        st.codec = "MP3".to_string();
        let mut app = App::new(Library::in_memory(vec![st]));

        app.play_selected();

        assert_eq!(app.playback.view.playing_url.as_deref(), Some("http://mp3"));
        assert_eq!(app.playback.view.state, PlaybackState::Connecting);
    }

    #[test]
    fn empty_codec_is_allowed_to_try_playback() {
        let mut st = station("Mystery Radio", "http://mystery");
        st.codec = String::new();
        let mut app = App::new(Library::in_memory(vec![st]));

        app.play_selected();

        assert_eq!(
            app.playback.view.playing_url.as_deref(),
            Some("http://mystery")
        );
        assert_eq!(app.playback.view.state, PlaybackState::Connecting);
    }

    #[test]
    fn aac_codec_now_starts_playback() {
        // AAC is now supported via Symphonia; the codec gate should allow it through.
        let mut st = station("AAC Radio", "http://aac");
        st.codec = "AAC".to_string();
        let mut app = App::new(Library::in_memory(vec![st]));

        app.play_selected();

        // playing_url and last_played_url should be set (gate was not tripped).
        assert_eq!(
            app.playback.view.playing_url.as_deref(),
            Some("http://aac")
        );
        assert_eq!(
            app.library.settings.last_played_url.as_deref(),
            Some("http://aac")
        );
        assert_eq!(app.playback.view.state, PlaybackState::Connecting);
    }

    #[test]
    fn hls_codec_is_blocked_before_audio_command() {
        // HLS remains unsupported; it should be blocked.
        let mut st = station("HLS Radio", "http://hls");
        st.codec = "HLS".to_string();
        let mut app = App::new(Library::in_memory(vec![st]));

        app.play_selected();

        assert_eq!(app.playback.view.playing_url, None);
        assert_eq!(app.library.settings.last_played_url, None);
        assert!(matches!(app.playback.view.state, PlaybackState::Error(_)));
        assert!(app
            .playback
            .diagnostics
            .last_error
            .as_deref()
            .is_some_and(|msg| msg.contains("HLS")));
    }

    #[test]
    fn hls_codec_does_not_set_playing_url_or_last_played_url() {
        // HLS remains unsupported; playing_url should never be set.
        let mut st = station("HLS Radio", "http://hls");
        st.codec = "HLS".to_string();
        let mut app = App::new(Library::in_memory(vec![st]));

        app.play_selected();

        assert_eq!(app.playback.view.playing_url, None);
        assert_eq!(app.library.settings.last_played_url, None);
        assert!(matches!(app.playback.view.state, PlaybackState::Error(_)));
    }

    #[test]
    fn ogg_codec_now_starts_playback() {
        // OGG is now supported via Symphonia; it should not be blocked.
        let mut st = station("OGG Radio", "http://ogg");
        st.codec = "OGG".to_string();
        let mut app = App::new(Library::in_memory(vec![st]));

        app.play_selected();

        // playing_url is set since the codec gate does not block OGG.
        assert_eq!(
            app.playback.view.playing_url.as_deref(),
            Some("http://ogg")
        );
        assert_eq!(app.playback.view.state, PlaybackState::Connecting);
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
            app.ui.notice.current.as_ref(),
            Some(crate::app::types::AppNotice::Info(_))
        ));

        // clean up
        let _ = std::fs::remove_dir_all(temp_dir);
    }
}
