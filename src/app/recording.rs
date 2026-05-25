use super::*;
use crate::audio::AudioCommand;

impl App {
    pub(super) fn toggle_recording(&mut self) {
        if self.playing_url.is_some() {
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
                }
                RecordingState::Pending | RecordingState::Active => {
                    self.audio.send(AudioCommand::StopRecording);
                    self.recording_state = RecordingState::Off;
                    self.active_record_filepath = None;
                }
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

    #[test]
    fn toggle_recording_without_playing_does_nothing() {
        let mut app = test_app();

        app.toggle_recording();

        assert_eq!(app.recording_state, RecordingState::Off);
    }

    #[test]
    fn toggle_recording_when_playing_sets_pending() {
        let mut app = test_app();
        app.playing_url = Some("http://a".to_string());

        app.toggle_recording();

        assert_eq!(app.recording_state, RecordingState::Pending);
    }

    #[test]
    fn toggle_recording_when_pending_turns_off() {
        let mut app = test_app();
        app.playing_url = Some("http://a".to_string());
        app.recording_state = RecordingState::Pending;
        app.active_record_filepath = Some("capture.mp3".to_string());

        app.toggle_recording();

        assert_eq!(app.recording_state, RecordingState::Off);
        assert_eq!(app.active_record_filepath, None);
    }
}
