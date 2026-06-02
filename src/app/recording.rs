use super::*;
use crate::audio::AudioCommand;
use std::time::SystemTime;

impl App {
    pub(super) fn toggle_recording(&mut self) {
        if self.local_playback_path.is_some() && self.playing_url.is_none() {
            self.set_info_notice("Recording is only available for live streams");
            return;
        }

        if self.playing_url.is_none() {
            self.set_info_notice("Start playback before recording");
            return;
        }

        match self.recording_state {
            RecordingState::Off => {
                let station = self.now_playing().cloned();
                let category = station
                    .as_ref()
                    .map(|s| s.genre.clone())
                    .unwrap_or_else(|| "Unknown".to_string());
                let rec_dir = self.library.settings.recording_dir.clone();
                let keep_snippets = self.library.settings.keep_snippets;
                let min_secs = self.library.settings.min_song_duration_secs;

                self.recording_started_at = Some(SystemTime::now());
                self.recording_station_name = station.as_ref().map(|s| s.name.clone());
                self.recording_station_url = station.as_ref().map(|s| s.url.clone());
                self.recording_category = Some(category.clone());
                self.recording_recovery_notice = None;
                self.write_recording_journal("pending");

                self.audio.send(AudioCommand::StartRecording {
                    recording_dir: rec_dir,
                    category,
                    keep_snippets,
                    min_song_duration_secs: min_secs,
                });
                self.recording_state = RecordingState::Pending;
                self.set_info_notice("Recording will start at next track boundary");
            }
            RecordingState::Pending | RecordingState::Active => {
                self.audio.send(AudioCommand::StopRecording);
                self.clear_recording_session();
                self.set_info_notice("Recording stopped");
            }
        }
    }
    pub(super) fn sync_recording_status(&mut self, state: u8, filepath: Option<String>) {
        self.recording_state = match state {
            1 => RecordingState::Pending,
            2 => RecordingState::Active,
            _ => RecordingState::Off,
        };
        self.active_record_filepath = filepath;

        match self.recording_state {
            RecordingState::Off => self.clear_recording_session(),
            RecordingState::Pending => {
                if self.recording_started_at.is_none() {
                    self.recording_started_at = Some(SystemTime::now());
                }
                self.write_recording_journal("pending");
            }
            RecordingState::Active => {
                if self.recording_started_at.is_none() {
                    self.recording_started_at = Some(SystemTime::now());
                }
                self.write_recording_journal("active");
            }
        }
    }

    pub(super) fn clear_recording_session(&mut self) {
        let recording_dir = self.library.settings.recording_dir.clone();
        let _ = crate::recording_journal::remove_session_journal(&recording_dir);

        self.recording_state = RecordingState::Off;
        self.active_record_filepath = None;
        self.recording_started_at = None;
        self.recording_station_name = None;
        self.recording_station_url = None;
        self.recording_category = None;
    }

    fn write_recording_journal(&mut self, state: &str) {
        let Some(started_at) = self.recording_started_at else {
            return;
        };

        let recording_dir = self.library.settings.recording_dir.clone();
        let result = crate::recording_journal::write_session_journal(
            &recording_dir,
            self.recording_station_name.as_deref(),
            self.recording_station_url.as_deref(),
            self.recording_category.as_deref(),
            state,
            started_at,
            self.active_record_filepath.as_deref(),
            self.current_track.as_deref(),
        );

        if let Err(err) = result {
            self.set_error_notice(err);
        }
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
        let mut library = Library::in_memory(vec![station("A", "http://a")]);
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        library.settings.recording_dir = std::env::temp_dir()
            .join(format!("pulsedeck-recording-test-{unique}"))
            .display()
            .to_string();
        App::new(library)
    }

    fn notice_text(app: &App) -> Option<&str> {
        match app.notice.as_ref() {
            Some(AppNotice::Info(message)) | Some(AppNotice::Error(message)) => Some(message),
            None => None,
        }
    }

    #[test]
    fn toggle_recording_without_playing_shows_notice() {
        let mut app = test_app();

        app.toggle_recording();

        assert_eq!(app.recording_state, RecordingState::Off);
        assert_eq!(notice_text(&app), Some("Start playback before recording"));
    }

    #[test]
    fn toggle_recording_while_local_tape_is_playing_shows_notice() {
        let mut app = test_app();
        app.local_playback_path = Some(std::path::PathBuf::from("recordings/tape.mp3"));

        app.toggle_recording();

        assert_eq!(app.recording_state, RecordingState::Off);
        assert_eq!(
            notice_text(&app),
            Some("Recording is only available for live streams")
        );
    }

    #[test]
    fn toggle_recording_when_playing_sets_pending_and_shows_notice() {
        let mut app = test_app();
        app.playing_url = Some("http://a".to_string());

        app.toggle_recording();

        assert_eq!(app.recording_state, RecordingState::Pending);
        assert_eq!(
            notice_text(&app),
            Some("Recording will start at next track boundary")
        );
    }

    #[test]
    fn toggle_recording_when_pending_turns_off_and_shows_notice() {
        let mut app = test_app();
        app.playing_url = Some("http://a".to_string());
        app.recording_state = RecordingState::Pending;
        app.active_record_filepath = Some("capture.mp3".to_string());

        app.toggle_recording();

        assert_eq!(app.recording_state, RecordingState::Off);
        assert_eq!(app.active_record_filepath, None);
        assert_eq!(notice_text(&app), Some("Recording stopped"));
    }

    #[test]
    fn recording_status_changed_active_updates_dashboard_fields() {
        let mut app = test_app();

        app.sync_recording_status(2, Some("recordings/Synthwave/capture.mp3".to_string()));

        assert_eq!(app.recording_state, RecordingState::Active);
        assert_eq!(
            app.active_record_filepath.as_deref(),
            Some("recordings/Synthwave/capture.mp3")
        );
        assert!(app.recording_started_at.is_some());
    }

    #[test]
    fn clearing_recording_session_resets_dashboard_fields() {
        let mut app = test_app();
        app.recording_state = RecordingState::Active;
        app.active_record_filepath = Some("capture.mp3".to_string());
        app.recording_started_at = Some(std::time::SystemTime::now());
        app.recording_station_name = Some("A".to_string());

        app.clear_recording_session();

        assert_eq!(app.recording_state, RecordingState::Off);
        assert!(app.active_record_filepath.is_none());
        assert!(app.recording_started_at.is_none());
        assert!(app.recording_station_name.is_none());
    }
}
