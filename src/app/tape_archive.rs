use super::*;
use crate::action::Action;
use crate::audio::AudioCommand;
use crate::system_open;
use crate::system_trash;
use crate::tape_archive::{TapeArchive, TapeArchiveRow, TapeArchiveStatus, TapeTrack};
use std::path::{Path, PathBuf};

const TAPE_ARCHIVE_PAGE: usize = 1;

impl App {
    pub fn is_tape_archive_page(&self) -> bool {
        !self.show_help && !self.show_settings && self.active_deck_page == TAPE_ARCHIVE_PAGE
    }

    pub fn is_tape_archive_focused(&self) -> bool {
        self.input_mode == InputMode::Normal && self.is_tape_archive_page()
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

    pub(super) fn cycle_tape_playback_mode(&mut self) {
        self.tape_playback_mode = self.tape_playback_mode.next();
        self.set_info_notice(format!(
            "Tape playback mode: {} — {}",
            self.tape_playback_mode.label(),
            self.tape_playback_mode.description()
        ));
    }

    pub(super) fn handle_local_tape_finished(&mut self, path: PathBuf) {
        let next = match self.tape_playback_mode {
            TapePlaybackMode::StopAtEnd => None,
            TapePlaybackMode::Folder => self.tape_archive.next_track_after(&path),
            TapePlaybackMode::AllRecordings => self.tape_archive.next_track_after_any_folder(&path),
            TapePlaybackMode::RepeatOne => self.tape_archive.track_by_path(&path),
            TapePlaybackMode::ShuffleAll => self
                .tape_archive
                .deterministic_shuffle_track_after(&path, self.tick_count),
        };

        if let Some(next) = next {
            let title = next.title.clone();
            let mode = self.tape_playback_mode.label();
            self.play_tape_track(next);
            self.set_info_notice(format!("Tape {mode}: {title}"));
        } else {
            self.local_playback_path = None;
            self.current_track = None;
            self.playback = PlaybackState::Stopped;
            self.set_info_notice(match self.tape_playback_mode {
                TapePlaybackMode::StopAtEnd => "Tape playback stopped at end",
                TapePlaybackMode::Folder => "End of tape folder",
                TapePlaybackMode::AllRecordings => "End of all recordings",
                TapePlaybackMode::RepeatOne => "Tape repeat ended",
                TapePlaybackMode::ShuffleAll => "No other tapes to shuffle",
            });
        }
    }

    pub(super) fn toggle_tape_details(&mut self) {
        if !self.is_tape_archive_page() {
            return;
        }

        if self.tape_archive.selected_track().is_none() {
            self.set_info_notice("Select a tape file to inspect details");
            return;
        }

        self.tape_details_visible = !self.tape_details_visible;
        self.set_info_notice(if self.tape_details_visible {
            "Tape details visible"
        } else {
            "Tape details hidden"
        });
    }

    pub(super) fn enter_tape_rename(&mut self) {
        if !self.is_tape_archive_focused() {
            return;
        }

        let Some(track) = self.tape_archive.selected_track() else {
            self.set_info_notice("Select a tape file to rename");
            return;
        };

        self.pending_tape_delete = None;
        self.tape_edit_buffer = track
            .path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or(track.title.as_str())
            .to_string();
        self.input_mode = InputMode::TapeRename;
        self.set_info_notice("Rename tape: edit name and press Enter");
    }

    pub(super) fn enter_tape_move(&mut self) {
        if !self.is_tape_archive_focused() {
            return;
        }

        if self.tape_archive.selected_track().is_none() {
            self.set_info_notice("Select a tape file to move");
            return;
        }

        self.pending_tape_delete = None;
        self.tape_edit_buffer = self
            .tape_archive
            .selected_track_folder_name()
            .unwrap_or("")
            .to_string();
        self.input_mode = InputMode::TapeMove;
        self.set_info_notice("Move tape: type target folder and press Enter");
    }

    pub(super) fn handle_tape_manager_action(&mut self, action: Action) {
        match action {
            Action::TapeManagerInput(ch) => self.tape_manager_input(ch),
            Action::TapeManagerBackspace => self.tape_manager_backspace(),
            Action::ConfirmTapeRename => self.confirm_tape_rename(),
            Action::ConfirmTapeMove => self.confirm_tape_move(),
            Action::CancelTapeManager => self.cancel_tape_manager(),
            Action::Tick => self.tick(),
            Action::Quit => self.quit(),
            _ => {}
        }
    }

    pub(super) fn tape_manager_input(&mut self, ch: char) {
        if matches!(self.input_mode, InputMode::TapeRename | InputMode::TapeMove)
            && !ch.is_control()
        {
            self.tape_edit_buffer.push(ch);
        }
    }

    pub(super) fn tape_manager_backspace(&mut self) {
        if matches!(self.input_mode, InputMode::TapeRename | InputMode::TapeMove) {
            self.tape_edit_buffer.pop();
        }
    }

    pub(super) fn cancel_tape_manager(&mut self) {
        if matches!(self.input_mode, InputMode::TapeRename | InputMode::TapeMove) {
            self.input_mode = InputMode::Normal;
            self.tape_edit_buffer.clear();
            self.set_info_notice("Tape file edit cancelled");
        }
    }

    pub(super) fn confirm_tape_rename(&mut self) {
        if self.input_mode != InputMode::TapeRename {
            return;
        }

        let Some(track) = self.tape_archive.selected_track().cloned() else {
            self.cancel_tape_manager();
            return;
        };

        let Ok(new_stem) = sanitized_tape_component(self.tape_edit_buffer.as_str()) else {
            self.set_error_notice("Tape name cannot be empty");
            return;
        };

        let extension = track
            .path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or(track.extension.as_str());

        let filename = if extension.trim().is_empty() {
            new_stem
        } else {
            format!("{new_stem}.{extension}")
        };

        let Some(parent) = track.path.parent() else {
            self.set_error_notice("Selected tape has no containing folder");
            return;
        };

        let target = parent.join(filename);
        self.apply_tape_file_move(track.path.as_path(), target.as_path(), "Tape renamed");
    }

    pub(super) fn confirm_tape_move(&mut self) {
        if self.input_mode != InputMode::TapeMove {
            return;
        }

        let Some(track) = self.tape_archive.selected_track().cloned() else {
            self.cancel_tape_manager();
            return;
        };

        let Ok(folder_name) = sanitized_tape_component(self.tape_edit_buffer.as_str()) else {
            self.set_error_notice("Target folder cannot be empty");
            return;
        };

        let target_dir = self.current_tape_archive_root().join(folder_name);
        let target = target_dir.join(track.filename.as_str());
        self.apply_tape_file_move(track.path.as_path(), target.as_path(), "Tape moved");
    }

    fn apply_tape_file_move(&mut self, source: &Path, target: &Path, success_notice: &'static str) {
        if source == target {
            self.input_mode = InputMode::Normal;
            self.tape_edit_buffer.clear();
            self.set_info_notice("No tape file change needed");
            return;
        }

        if target.exists() {
            self.set_error_notice(format!("Target already exists: {}", target.display()));
            return;
        }

        if let Some(parent) = target.parent() {
            if let Err(err) = std::fs::create_dir_all(parent) {
                self.set_error_notice(format!("Could not create target folder: {err}"));
                return;
            }
        }

        if self.local_playback_path.as_deref() == Some(source) {
            self.audio.send(AudioCommand::Stop);
            self.local_playback_path = None;
            self.current_track = None;
            self.playback = PlaybackState::Stopped;
        }

        match std::fs::rename(source, target) {
            Ok(()) => {
                self.input_mode = InputMode::Normal;
                self.tape_edit_buffer.clear();
                self.pending_tape_delete = None;
                self.tape_archive_scan_requested = true;
                self.set_info_notice(success_notice);
            }
            Err(err) => {
                self.set_error_notice(format!("Could not update tape file: {err}"));
            }
        }
    }

    pub(super) fn refresh_tape_archive(&mut self) {
        if self.is_tape_archive_focused() {
            self.pending_tape_delete = None;
            self.tape_archive_scan_requested = true;
            self.set_info_notice("Refreshing Local Tape Library");
        }
    }

    pub(super) fn enter_tape_filter(&mut self) {
        if self.is_tape_archive_page() {
            self.pending_tape_delete = None;
            self.input_mode = InputMode::TapeFilter;
            self.set_info_notice("Filtering Local Tape Library");
        }
    }

    pub(super) fn exit_tape_filter(&mut self) {
        if self.input_mode == InputMode::TapeFilter {
            self.input_mode = InputMode::Normal;
            self.tape_archive.clear_filter_query();
            self.set_info_notice("Tape filter cleared");
        }
    }

    pub(super) fn tape_filter_input(&mut self, ch: char) {
        if self.input_mode == InputMode::TapeFilter && !ch.is_control() {
            self.tape_archive.push_filter_char(ch);
        }
    }

    pub(super) fn tape_filter_backspace(&mut self) {
        if self.input_mode == InputMode::TapeFilter {
            self.tape_archive.pop_filter_char();
        }
    }

    pub(super) fn handle_tape_filter_action(&mut self, action: Action) {
        match action {
            Action::TapeFilterInput(ch) => self.tape_filter_input(ch),
            Action::TapeFilterBackspace => self.tape_filter_backspace(),
            Action::ExitTapeFilter => self.exit_tape_filter(),
            Action::NextStation => self.next_tape_archive_row(),
            Action::PrevStation => self.prev_tape_archive_row(),
            Action::PlaySelected => self.play_selected_tape_or_toggle(),
            Action::RefreshTapeArchive => {
                self.pending_tape_delete = None;
                self.tape_archive_scan_requested = true;
                self.set_info_notice("Refreshing Local Tape Library");
            }
            Action::Tick => self.tick(),
            Action::Quit => self.quit(),
            _ => {}
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
            Some(TapeArchiveRow::Track { .. }) | Some(TapeArchiveRow::AllRecordingTrack { .. }) => {
                self.play_selected_tape()
            }
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
            Some(TapeArchiveRow::Track { .. }) | Some(TapeArchiveRow::AllRecordingTrack { .. }) => {
                match self.playback {
                    PlaybackState::Playing
                    | PlaybackState::Paused
                    | PlaybackState::FadingOut { .. } => {
                        self.toggle_pause();
                    }
                    PlaybackState::Stopped
                    | PlaybackState::Error(_)
                    | PlaybackState::Connecting => {
                        self.play_selected_tape();
                    }
                }
            }
            None => {}
        }
    }

    pub(super) fn open_selected_tape_folder(&mut self) {
        if !self.is_tape_archive_page() {
            return;
        }

        let Some(track) = self.tape_archive.selected_track() else {
            self.set_info_notice("Select a tape file to open its folder");
            return;
        };

        match system_open::open_containing_folder(&track.path) {
            Ok(()) => self.set_info_notice("Opened tape folder"),
            Err(err) => self.set_error_notice(format!("Could not open tape folder: {err}")),
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

        match system_trash::move_to_trash(&path) {
            Ok(()) => {
                self.set_info_notice("Tape moved to trash");
                self.tape_archive_scan_requested = true;
            }
            Err(err) => {
                self.set_error_notice(format!("Could not move tape to trash: {err}"));
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

        self.play_tape_track(track);
    }

    pub(super) fn play_tape_track(&mut self, track: TapeTrack) {
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

fn sanitized_tape_component(value: &str) -> Result<String, ()> {
    let cleaned = value
        .chars()
        .map(|ch| match ch {
            '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            other => other,
        })
        .collect::<String>()
        .trim()
        .trim_matches('.')
        .to_string();

    if cleaned.is_empty() {
        Err(())
    } else {
        Ok(cleaned)
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
