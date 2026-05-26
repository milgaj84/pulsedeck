use super::*;
use crate::audio::AudioCommand;

impl App {
    pub(super) fn toggle_recording(&mut self) {
        if self.playing_url.is_none() {
            self.set_info_notice("Start playback before recording");
            return;
        }

        match self.recording_state {
            RecordingState::Off => {
                let category = self
                    .now_playing()
                    .map(|s| s.genre.clone())
                    .unwrap_or_else(|| "Unknown".to_string());
                let rec_dir = self.library.settings.recording_dir.clone();
                let keep_snippets = self.library.settings.keep_snippets;
                let min_secs = self.library.settings.min_song_duration_secs;

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
                self.recording_state = RecordingState::Off;
                self.active_record_filepath = None;
                self.set_info_notice("Recording stopped");
            }
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
        App::new(Library::in_memory(vec![station("A", "http://a")]))
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
}
