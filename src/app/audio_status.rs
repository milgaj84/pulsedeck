use super::*;
use crate::audio::AudioStatus;
use crate::radio::{find_station_index_by_url, station_url_matches};

const SONG_HISTORY_CAP: usize = 100;
const NOTIFY_IDLE_MS: u64 = 120_000;

pub(super) fn last_played_station_position(
    stations: &[Station],
    last_played_url: &str,
) -> Option<usize> {
    find_station_index_by_url(stations, last_played_url)
}

pub(super) fn unix_now_string() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string())
}

pub(super) fn current_unix_epoch() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

impl App {
    pub fn poll_audio_status(&mut self) {
        while let Some(status) = self.playback.audio.try_recv_status() {
            match status {
                AudioStatus::TrackChanged { url, title } => {
                    self.handle_track_changed(url, title);
                }
                AudioStatus::Playing => {
                    self.playback.view.state = PlaybackState::Playing;
                    self.playback.reconnect.disarm();
                    if let Some(url) = self.playback.view.playing_url.clone() {
                        if self.library.mark_station_success(&url, unix_now_string()) {
                            self.mark_library_dirty();
                        }
                    }
                    self.playback.diagnostics.decoder_state = DecoderState::Playing;
                    self.playback.diagnostics.last_event = Some("Playback started".to_string());
                    self.playback.diagnostics.last_error = None;
                }
                AudioStatus::Paused => {
                    self.playback.view.state = PlaybackState::Paused;
                    self.playback.diagnostics.last_event = Some("Playback paused".to_string());
                }
                AudioStatus::Stopped => self.handle_audio_stopped(),
                AudioStatus::Error(error) => {
                    self.playback.diagnostics.decoder_state = DecoderState::Failed;
                    self.playback.diagnostics.last_error = Some(error.clone());
                    self.handle_audio_error(error);
                }
                AudioStatus::FadingOut { current_volume } => {
                    self.playback.view.state = PlaybackState::FadingOut {
                        current_volume: current_volume.clamp(0.0, 1.0),
                    };
                    self.playback.diagnostics.last_event = Some("Fading out".to_string());
                }
                AudioStatus::Connecting => {
                    self.playback.view.current_track = None;
                    self.playback.view.state = PlaybackState::Connecting;
                    self.playback.diagnostics.decoder_state = DecoderState::Connecting;
                    self.playback.diagnostics.last_event = Some("Connecting to stream".to_string());
                }
                AudioStatus::Buffering { percent } => {
                    self.playback.diagnostics.decoder_state = DecoderState::Probing;
                    self.playback.diagnostics.last_event = Some(format!("Buffering ({percent}%)"));
                }
                AudioStatus::OutputDeviceChanged { active } => {
                    self.apply_confirmed_output_device(active);
                }
                AudioStatus::OutputDeviceChangeFailed {
                    requested,
                    active,
                    error,
                } => {
                    self.rollback_failed_output_device(requested, active, error);
                }
            }
        }
    }

    fn apply_confirmed_output_device(&mut self, active: Option<String>) {
        self.config.audio.output_device = active.clone();
        let display = crate::audio::output_device_display_name(active.as_deref());
        self.playback.diagnostics.output_device = display.clone();
        self.playback.diagnostics.last_event = Some(format!("Audio output changed to {display}"));
        self.playback.diagnostics.last_error = None;
        self.persist_config_change();
        self.set_info_notice(format!("Audio output: {display}"));
    }

    fn rollback_failed_output_device(
        &mut self,
        requested: Option<String>,
        active: Option<String>,
        error: String,
    ) {
        self.config.audio.output_device = active.clone();
        let active_display = crate::audio::output_device_display_name(active.as_deref());
        let requested_display = crate::audio::output_device_display_name(requested.as_deref());
        self.playback.diagnostics.output_device = active_display.clone();
        self.playback.diagnostics.last_event = Some("Audio output unchanged".to_string());
        self.playback.diagnostics.last_error = Some(error.clone());
        self.persist_config_change();
        self.set_info_notice(format!(
            "Could not use {requested_display}; still using {active_display}: {error}"
        ));
    }

    pub(super) fn handle_track_changed(&mut self, url: String, title: String) {
        if !self
            .playback
            .view
            .playing_url
            .as_deref()
            .is_some_and(|playing_url| station_url_matches(playing_url, &url))
        {
            return;
        }

        let is_new = !title.is_empty() && self.playback.view.current_track.as_ref() != Some(&title);
        self.playback.view.current_track = Some(title.clone());

        if !title.is_empty() && self.song_history.back() != Some(&title) {
            self.song_history.push_back(title.clone());
            while self.song_history.len() > SONG_HISTORY_CAP {
                self.song_history.pop_front();
            }
            if self.config.playback.save_history {
                let station_name = self
                    .now_playing()
                    .map(|station| station.name.clone())
                    .unwrap_or_else(|| "Radio Stream".to_string());
                self.history.record(title.clone(), station_name);
                self.mark_history_dirty();
            }
        }

        if is_new && self.config.ui.notifications_enabled {
            let user_is_active = super::idle::get_user_idle_ms()
                .map(|idle_ms| idle_ms <= NOTIFY_IDLE_MS)
                .unwrap_or(true);

            if user_is_active {
                let now = std::time::Instant::now();
                if self.notification_cooldown.may_notify(now) {
                    self.notification_cooldown.record_notification(now);
                    let station_name = self
                        .now_playing()
                        .map(|station| station.name.clone())
                        .unwrap_or_else(|| "Radio Stream".to_string());
                    self.notifier.notify_now_playing(&title, &station_name);
                }
            }
        }
    }

    fn handle_audio_stopped(&mut self) {
        let was_playing = self.playback.view.playing_url.is_some();
        if self.playback.view.intentional_stop || !was_playing {
            self.playback.view.intentional_stop = false;
            self.playback.view.playing_url = None;
            self.playback.view.reset_transient_status();
            self.playback.view.state = PlaybackState::Stopped;
            self.playback.diagnostics.decoder_state = DecoderState::Idle;
            self.playback.diagnostics.buffer_percent = 0;
            self.playback.diagnostics.buffer_seconds = 0;
            self.playback.diagnostics.last_event = Some("Playback stopped".to_string());
            self.playback.reconnect.disarm();
        } else if let Some(url) = self.playback.view.playing_url.clone() {
            self.playback.reconnect.arm(url, std::time::Instant::now());
            self.playback.view.state = PlaybackState::Connecting;
        }
    }

    fn handle_audio_error(&mut self, error: String) {
        if let Some(url) = self.playback.view.playing_url.clone() {
            self.playback
                .reconnect
                .arm(url.clone(), std::time::Instant::now());
            if self
                .library
                .mark_station_failure(&url, unix_now_string(), &error)
            {
                self.mark_library_dirty();
            }
        }
        self.playback.view.reset_transient_status();
        self.playback.diagnostics.buffer_percent = 0;
        self.playback.diagnostics.buffer_seconds = 0;
        self.playback.diagnostics.reconnect_attempts = 1;
        self.playback.diagnostics.last_recovery = Some("Queued automatic reconnect".to_string());
        self.playback.view.state = PlaybackState::Error(error);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::MockAudioSink;
    use crate::config_toml::AppConfig;
    use crate::favorites::Library;
    use std::time::{Duration, Instant};

    fn test_parts_with_audio(audio: MockAudioSink) -> super::super::startup::AppParts {
        super::super::startup::AppParts {
            library: Library::in_memory(vec![]),
            ui_state: super::super::ui_state::UiState::from_app_values(
                37,
                true,
                LayoutMode::RightOnly,
                VisualizerMode::SimOscilloscope,
                DisplayMode::Normal,
                None,
            ),
            ui_state_warning: None,
            history: crate::history::History::default(),
            history_warning: None,
            audio: Box::new(audio),
            sample_buffer: Arc::new(Mutex::new(VecDeque::with_capacity(4096))),
            config: AppConfig::default(),
            config_preserved: toml::Value::Table(toml::map::Map::new()),
            config_warnings: Vec::new(),
            config_loaded_from_file: false,
        }
    }

    fn test_parts() -> super::super::startup::AppParts {
        test_parts_with_audio(MockAudioSink::disconnected())
    }

    fn test_parts_with_library(library: Library) -> super::super::startup::AppParts {
        let mut parts = test_parts();
        parts.library = library;
        parts
    }

    #[test]
    fn failed_output_switch_rolls_back_setting_without_breaking_playback() {
        let audio = MockAudioSink::new();
        audio
            .statuses
            .borrow_mut()
            .push_back(AudioStatus::OutputDeviceChangeFailed {
                requested: Some("Missing DAC".to_string()),
                active: Some("Speakers".to_string()),
                error: "not found".to_string(),
            });
        let mut app = App::from_parts(test_parts_with_audio(audio));
        app.config.audio.output_device = Some("Missing DAC".to_string());
        app.playback.view.state = PlaybackState::Playing;

        app.poll_audio_status();

        assert!(matches!(app.playback.view.state, PlaybackState::Playing));
        assert_eq!(app.config.audio.output_device.as_deref(), Some("Speakers"));
        assert_eq!(app.playback.diagnostics.output_device, "Speakers");
        assert_eq!(
            app.playback.diagnostics.last_error.as_deref(),
            Some("not found")
        );
    }

    #[test]
    fn successful_output_switch_confirms_normalized_setting() {
        let audio = MockAudioSink::new();
        audio
            .statuses
            .borrow_mut()
            .push_back(AudioStatus::OutputDeviceChanged { active: None });
        let mut app = App::from_parts(test_parts_with_audio(audio));
        app.config.audio.output_device = Some("Default".to_string());

        app.poll_audio_status();

        assert!(app.config.audio.output_device.is_none());
        assert_eq!(app.playback.diagnostics.output_device, "Default");
    }

    #[test]
    fn last_played_station_position_matches_normalized_urls() {
        let stations = vec![Station::basic("A", " HTTP://STREAM/ ", "Radio", "US", 128)];
        assert_eq!(
            last_played_station_position(&stations, "http://stream"),
            Some(0)
        );
    }

    #[test]
    fn track_changed_matches_normalized_playing_url() {
        let mut app = App::new(Library::in_memory(vec![Station::basic(
            "A",
            "HTTP://STREAM/",
            "Radio",
            "US",
            128,
        )]));
        app.playback.view.playing_url = Some("http://stream".to_string());

        app.handle_track_changed(" HTTP://STREAM/ ".to_string(), "Artist - Title".to_string());

        assert_eq!(
            app.playback.view.current_track.as_deref(),
            Some("Artist - Title")
        );
    }

    #[test]
    fn new_title_updates_history_and_notifies_once() {
        let station_url = "http://stream";
        let mut app = App::from_parts(test_parts_with_library(Library::in_memory(vec![
            Station::basic("Test Station", station_url, "Radio", "US", 128),
        ])));
        app.config.ui.notifications_enabled = true;
        app.playback.view.playing_url = Some(station_url.to_string());

        app.handle_track_changed(station_url.to_string(), "Song Alpha".to_string());
        app.handle_track_changed(station_url.to_string(), "Song Beta".to_string());

        assert!(app.song_history.contains(&"Song Alpha".to_string()));
        assert!(app.song_history.contains(&"Song Beta".to_string()));
        assert_eq!(app.notifier.notification_count(), 1);
    }

    #[test]
    fn disabled_notifications_still_update_track_without_notifying() {
        let station_url = "http://stream";
        let mut app = App::from_parts(test_parts_with_library(Library::in_memory(vec![
            Station::basic("Test Station", station_url, "Radio", "US", 128),
        ])));
        app.config.ui.notifications_enabled = false;
        app.playback.view.playing_url = Some(station_url.to_string());

        app.handle_track_changed(station_url.to_string(), "Latest Title".to_string());

        assert_eq!(app.notifier.notification_count(), 0);
        assert_eq!(
            app.playback.view.current_track.as_deref(),
            Some("Latest Title")
        );
    }

    #[test]
    fn notification_cooldown_allows_fresh_title_after_elapsed_time() {
        let station_url = "http://stream";
        let mut app = App::from_parts(test_parts_with_library(Library::in_memory(vec![
            Station::basic("Test Station", station_url, "Radio", "US", 128),
        ])));
        app.config.ui.notifications_enabled = true;
        app.playback.view.playing_url = Some(station_url.to_string());
        app.notification_cooldown
            .record_notification(Instant::now() - Duration::from_secs(10));

        app.handle_track_changed(station_url.to_string(), "Fresh Song".to_string());

        assert_eq!(app.notifier.notification_count(), 1);
    }

    #[test]
    fn empty_title_does_not_notify() {
        let station_url = "http://stream";
        let mut app = App::from_parts(test_parts_with_library(Library::in_memory(vec![
            Station::basic("Test Station", station_url, "Radio", "US", 128),
        ])));
        app.config.ui.notifications_enabled = true;
        app.playback.view.playing_url = Some(station_url.to_string());

        app.handle_track_changed(station_url.to_string(), String::new());

        assert_eq!(app.notifier.notification_count(), 0);
    }
}
