use super::*;
use crate::audio::AudioCommand;
use crate::tape_archive::{TapeArchive, TapeArchiveRow, TapeArchiveStatus};
use std::path::PathBuf;

const TAPE_ARCHIVE_PAGE: usize = 1;

impl App {
    pub fn is_tape_archive_focused(&self) -> bool {
        self.input_mode == InputMode::Normal
            && !self.show_help
            && !self.show_settings
            && self.active_deck_page == TAPE_ARCHIVE_PAGE
    }

    pub fn take_tape_archive_scan_request(&mut self) -> Option<PathBuf> {
        if !self.tape_archive_scan_requested || self.tape_archive_scan_inflight {
            return None;
        }

        let root = self.current_tape_archive_root();
        self.tape_archive_scan_requested = false;
        self.tape_archive_scan_inflight = true;
        self.tape_archive.root = root.clone();
        self.tape_archive.status = TapeArchiveStatus::Scanning;
        Some(root)
    }

    pub fn apply_tape_archive_scan(&mut self, root: PathBuf, result: Result<TapeArchive, String>) {
        if root != self.current_tape_archive_root() {
            self.tape_archive_scan_inflight = false;
            self.tape_archive_scan_requested = true;
            return;
        }

        self.tape_archive_scan_inflight = false;
        let previous_selected = self.tape_archive.selected;

        match result {
            Ok(mut archive) => {
                archive.selected = previous_selected.min(archive.row_count().saturating_sub(1));
                self.tape_archive = archive;
            }
            Err(err) => {
                self.tape_archive.status = TapeArchiveStatus::Error(err);
                self.tape_archive.rebuild_flattened();
            }
        }
    }

    pub(super) fn request_tape_archive_scan_if_needed(&mut self) {
        let root = self.current_tape_archive_root();
        if self.tape_archive.root != root
            || matches!(
                self.tape_archive.status,
                TapeArchiveStatus::NotLoaded | TapeArchiveStatus::Error(_)
            )
        {
            self.tape_archive_scan_requested = true;
        }
    }

    pub(super) fn refresh_tape_archive(&mut self) {
        if self.is_tape_archive_focused() {
            self.pending_tape_delete = None;
            self.tape_archive_scan_requested = true;
            self.set_info_notice("Refreshing Local Tape Library");
        }
    }

    pub(super) fn next_tape_archive_row(&mut self) {
        self.pending_tape_delete = None;
        self.tape_archive.next_row();
    }

    pub(super) fn prev_tape_archive_row(&mut self) {
        self.pending_tape_delete = None;
        self.tape_archive.prev_row();
    }

    pub(super) fn play_selected_tape_or_toggle(&mut self) {
        if self.pending_tape_delete.is_some() {
            self.confirm_tape_delete();
            return;
        }

        match self.tape_archive.selected_row().cloned() {
            Some(TapeArchiveRow::Track { .. }) => self.play_selected_tape(),
            Some(TapeArchiveRow::Folder { .. }) | Some(TapeArchiveRow::AllRecordings) => {
                self.tape_archive.toggle_selected_folder();
            }
            None => {}
        }
    }

    pub(super) fn toggle_tape_archive_folder_or_pause(&mut self) {
        match self.tape_archive.selected_row().cloned() {
            Some(TapeArchiveRow::Folder { .. }) | Some(TapeArchiveRow::AllRecordings) => {
                self.pending_tape_delete = None;
                self.tape_archive.toggle_selected_folder();
            }
            Some(TapeArchiveRow::Track { .. }) => match self.playback {
                PlaybackState::Playing
                | PlaybackState::Paused
                | PlaybackState::FadingOut { .. } => {
                    self.toggle_pause();
                }
                PlaybackState::Stopped | PlaybackState::Error(_) | PlaybackState::Connecting => {
                    self.play_selected_tape();
                }
            },
            None => {}
        }
    }

    pub(super) fn request_delete_selected_tape(&mut self) {
        if !self.is_tape_archive_focused() {
            return;
        }

        let Some(track) = self.tape_archive.selected_track() else {
            self.set_info_notice("Select a tape file to delete");
            return;
        };

        self.pending_tape_delete = Some(track.path.clone());
        self.set_info_notice(format!(
            "Delete {}? Press y to confirm, n to cancel",
            track.filename
        ));
    }

    pub(super) fn confirm_tape_delete(&mut self) {
        let Some(path) = self.pending_tape_delete.take() else {
            return;
        };

        if self.local_playback_path.as_ref() == Some(&path) {
            self.audio.send(AudioCommand::Stop);
            self.local_playback_path = None;
            self.current_track = None;
            self.playback = PlaybackState::Stopped;
        }

        match std::fs::remove_file(&path) {
            Ok(()) => {
                self.set_info_notice("Tape deleted");
                self.tape_archive_scan_requested = true;
            }
            Err(err) => {
                self.set_error_notice(format!("Could not delete tape: {err}"));
            }
        }
    }

    pub(super) fn cancel_tape_delete(&mut self) {
        if self.pending_tape_delete.take().is_some() {
            self.set_info_notice("Tape delete cancelled");
        }
    }

    #[cfg(test)]
    pub(super) fn tape_archive_selected_track_path(&self) -> Option<PathBuf> {
        self.tape_archive
            .selected_track()
            .map(|track| track.path.clone())
    }

    fn play_selected_tape(&mut self) {
        let Some(track) = self.tape_archive.selected_track().cloned() else {
            return;
        };

        self.playing_url = None;
        self.local_playback_path = Some(track.path.clone());
        self.current_track = Some(track.title.clone());
        self.playback = PlaybackState::Connecting;
        self.buffer_percent = 0;
        self.buffer_seconds = 0;
        self.pending_tape_delete = None;

        self.audio.send(AudioCommand::PlayLocalFile(track.path));
        self.sync_volume();
    }

    fn current_tape_archive_root(&self) -> PathBuf {
        PathBuf::from(self.library.settings.recording_dir.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::favorites::Library;
    use crate::tape_archive::{TapeFolder, TapeTrack};
    use std::time::SystemTime;

    fn test_track(name: &str, path: &str) -> TapeTrack {
        TapeTrack {
            title: name.to_string(),
            artist: None,
            filename: format!("{name}.mp3"),
            path: PathBuf::from(path),
            extension: "mp3".to_string(),
            size_bytes: 42,
            modified: Some(SystemTime::UNIX_EPOCH),
            duration_hint: None,
        }
    }

    fn archive_with_track() -> TapeArchive {
        let mut archive = TapeArchive::new("recordings");
        archive.folders = vec![TapeFolder {
            name: "Synthwave".to_string(),
            path: PathBuf::from("recordings/Synthwave"),
            tracks: vec![test_track("Track", "recordings/Synthwave/Track.mp3")],
            expanded: true,
        }];
        archive.status = TapeArchiveStatus::Ready;
        archive.rebuild_flattened();
        archive
    }

    fn test_app() -> App {
        App::new(Library::in_memory(vec![]))
    }

    #[test]
    fn entering_tape_archive_requests_scan_when_not_loaded() {
        let mut app = test_app();
        app.active_deck_page = 1;

        app.request_tape_archive_scan_if_needed();

        assert!(app.tape_archive_scan_requested);
    }

    #[test]
    fn next_and_prev_tape_archive_rows_wrap() {
        let mut app = test_app();
        app.tape_archive = archive_with_track();
        app.active_deck_page = 1;

        app.prev_tape_archive_row();
        assert_eq!(app.tape_archive.selected, 2);

        app.next_tape_archive_row();
        assert_eq!(app.tape_archive.selected, 0);
    }

    #[test]
    fn toggle_folder_rebuilds_flattened_rows() {
        let mut app = test_app();
        app.tape_archive = archive_with_track();
        app.tape_archive.selected = 1;

        app.play_selected_tape_or_toggle();

        assert_eq!(app.tape_archive.flattened.len(), 2);
    }

    #[test]
    fn selected_track_path_returns_track_only() {
        let mut app = test_app();
        app.tape_archive = archive_with_track();

        app.tape_archive.selected = 1;
        assert!(app.tape_archive_selected_track_path().is_none());

        app.tape_archive.selected = 2;
        assert_eq!(
            app.tape_archive_selected_track_path(),
            Some(PathBuf::from("recordings/Synthwave/Track.mp3"))
        );
    }
}
