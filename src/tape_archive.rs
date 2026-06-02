use rodio::Source;
use std::fs;
use std::fs::File;
use std::io;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const AUDIO_EXTENSIONS: &[&str] = &["mp3", "flac", "wav", "ogg", "opus", "m4a", "aac"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TapeArchive {
    pub root: PathBuf,
    pub folders: Vec<TapeFolder>,
    pub flattened: Vec<TapeArchiveRow>,
    pub all_recordings_flattened: bool,
    pub filter_query: String,
    pub selected: usize,
    pub status: TapeArchiveStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TapeFolder {
    pub name: String,
    pub path: PathBuf,
    pub tracks: Vec<TapeTrack>,
    pub expanded: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TapeTrack {
    pub title: String,
    pub artist: Option<String>,
    pub filename: String,
    pub path: PathBuf,
    pub extension: String,
    pub size_bytes: u64,
    pub modified: Option<SystemTime>,
    pub duration_hint: Option<Duration>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TapeArchiveRow {
    AllRecordings,
    Folder {
        folder_index: usize,
    },
    Track {
        folder_index: usize,
        track_index: usize,
    },
    AllRecordingTrack {
        folder_index: usize,
        track_index: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TapeArchiveStatus {
    NotLoaded,
    Scanning,
    Ready,
    Empty,
    Error(String),
}

impl TapeArchive {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        let mut archive = Self {
            root,
            folders: Vec::new(),
            flattened: Vec::new(),
            all_recordings_flattened: false,
            filter_query: String::new(),
            selected: 0,
            status: TapeArchiveStatus::NotLoaded,
        };
        archive.rebuild_flattened();
        archive
    }

    pub fn total_tracks(&self) -> usize {
        self.folders.iter().map(|folder| folder.tracks.len()).sum()
    }

    pub fn row_count(&self) -> usize {
        self.flattened.len()
    }

    pub fn selected_row(&self) -> Option<&TapeArchiveRow> {
        self.flattened.get(self.selected)
    }

    pub fn selected_track(&self) -> Option<&TapeTrack> {
        match self.selected_row()? {
            TapeArchiveRow::Track {
                folder_index,
                track_index,
            }
            | TapeArchiveRow::AllRecordingTrack {
                folder_index,
                track_index,
            } => self
                .folders
                .get(*folder_index)
                .and_then(|folder| folder.tracks.get(*track_index)),
            _ => None,
        }
    }

    #[allow(dead_code)]
    pub fn selected_track_folder_name(&self) -> Option<&str> {
        match self.selected_row()? {
            TapeArchiveRow::Track { folder_index, .. }
            | TapeArchiveRow::AllRecordingTrack { folder_index, .. } => self
                .folders
                .get(*folder_index)
                .map(|folder| folder.name.as_str()),
            _ => None,
        }
    }

    pub fn next_row(&mut self) {
        let count = self.row_count();
        if count > 0 {
            self.selected = (self.selected + 1) % count;
        }
    }

    pub fn prev_row(&mut self) {
        let count = self.row_count();
        if count > 0 {
            self.selected = if self.selected == 0 {
                count - 1
            } else {
                self.selected - 1
            };
        }
    }

    #[allow(dead_code)]
    pub fn set_filter_query(&mut self, query: impl Into<String>) {
        self.filter_query = query.into();
        self.selected = 0;
        self.rebuild_flattened();
    }

    #[allow(dead_code)]
    pub fn push_filter_char(&mut self, ch: char) {
        self.filter_query.push(ch);
        self.selected = 0;
        self.rebuild_flattened();
    }

    #[allow(dead_code)]
    pub fn pop_filter_char(&mut self) {
        self.filter_query.pop();
        self.selected = 0;
        self.rebuild_flattened();
    }

    #[allow(dead_code)]
    pub fn clear_filter_query(&mut self) {
        self.filter_query.clear();
        self.selected = 0;
        self.rebuild_flattened();
    }

    pub fn is_filtering(&self) -> bool {
        !self.filter_query.trim().is_empty()
    }

    pub fn toggle_selected_folder(&mut self) {
        match self.selected_row().cloned() {
            Some(TapeArchiveRow::AllRecordings) => self.toggle_all_recordings_mode(),
            Some(TapeArchiveRow::Folder { folder_index }) => {
                if let Some(folder) = self.folders.get_mut(folder_index) {
                    folder.expanded = !folder.expanded;
                }
                self.rebuild_flattened();
            }
            _ => {}
        }
    }

    pub fn toggle_all_recordings_mode(&mut self) {
        self.all_recordings_flattened = !self.all_recordings_flattened;
        self.selected = 0;
        self.rebuild_flattened();
    }

    #[allow(dead_code)]
    pub fn exit_all_recordings_mode(&mut self) {
        if self.all_recordings_flattened {
            self.all_recordings_flattened = false;
            self.selected = 0;
            self.rebuild_flattened();
        }
    }

    #[allow(dead_code)]
    pub fn next_track_after(&self, path: &Path) -> Option<TapeTrack> {
        for folder in &self.folders {
            let Some(position) = folder.tracks.iter().position(|track| track.path == path) else {
                continue;
            };

            return folder.tracks.get(position + 1).cloned();
        }

        None
    }

    pub fn track_by_path(&self, path: &Path) -> Option<TapeTrack> {
        self.folders
            .iter()
            .flat_map(|folder| folder.tracks.iter())
            .find(|track| track.path == path)
            .cloned()
    }

    pub fn next_track_after_any_folder(&self, path: &Path) -> Option<TapeTrack> {
        let refs = self.all_track_refs_newest_first();
        let position = refs.iter().position(|(folder_index, track_index)| {
            self.folders[*folder_index].tracks[*track_index].path == path
        })?;

        let (next_folder, next_track) = refs.get(position + 1).copied()?;
        self.folders
            .get(next_folder)
            .and_then(|folder| folder.tracks.get(next_track))
            .cloned()
    }

    pub fn deterministic_shuffle_track_after(&self, path: &Path, salt: u64) -> Option<TapeTrack> {
        let mut tracks: Vec<TapeTrack> = self
            .folders
            .iter()
            .flat_map(|folder| folder.tracks.iter().cloned())
            .collect();

        if tracks.len() <= 1 {
            return None;
        }

        tracks.sort_by_key(|track| shuffle_key(track, path, salt));

        tracks.into_iter().find(|track| track.path != path)
    }

    pub fn rebuild_flattened(&mut self) {
        let selected_before = self.selected;
        let filter = normalized_query(self.filter_query.as_str());

        self.flattened.clear();
        self.flattened.push(TapeArchiveRow::AllRecordings);

        if self.all_recordings_flattened || filter.is_some() {
            for (folder_index, track_index) in self.all_track_refs_newest_first() {
                if self.track_matches_filter(folder_index, track_index, filter.as_deref()) {
                    self.flattened.push(TapeArchiveRow::AllRecordingTrack {
                        folder_index,
                        track_index,
                    });
                }
            }
        } else {
            for (folder_index, folder) in self.folders.iter().enumerate() {
                self.flattened.push(TapeArchiveRow::Folder { folder_index });

                if folder.expanded {
                    for (track_index, _track) in folder.tracks.iter().enumerate() {
                        self.flattened.push(TapeArchiveRow::Track {
                            folder_index,
                            track_index,
                        });
                    }
                }
            }
        }

        self.selected = clamp_index(selected_before, self.flattened.len());
    }

    pub fn all_track_refs_newest_first(&self) -> Vec<(usize, usize)> {
        let mut refs = Vec::new();

        for (folder_index, folder) in self.folders.iter().enumerate() {
            for (track_index, _track) in folder.tracks.iter().enumerate() {
                refs.push((folder_index, track_index));
            }
        }

        refs.sort_by(|left, right| {
            let left_track = &self.folders[left.0].tracks[left.1];
            let right_track = &self.folders[right.0].tracks[right.1];
            compare_tracks_newest_first(left_track, right_track)
        });
        refs
    }

    fn track_matches_filter(
        &self,
        folder_index: usize,
        track_index: usize,
        filter: Option<&str>,
    ) -> bool {
        let Some(filter) = filter else {
            return true;
        };

        let Some(folder) = self.folders.get(folder_index) else {
            return false;
        };

        let Some(track) = folder.tracks.get(track_index) else {
            return false;
        };

        let haystack = format!(
            "{} {} {} {} {}",
            folder.name,
            track.title,
            track.artist.as_deref().unwrap_or(""),
            track.filename,
            track.extension
        )
        .to_lowercase();

        haystack.contains(filter)
    }
}

pub fn scan_tape_archive(root: PathBuf) -> Result<TapeArchive, io::Error> {
    let mut archive = TapeArchive::new(root.clone());

    if !root.exists() {
        archive.status = TapeArchiveStatus::Empty;
        archive.rebuild_flattened();
        return Ok(archive);
    }

    if !root.is_dir() {
        archive.status =
            TapeArchiveStatus::Error(format!("{} is not a recording directory", root.display()));
        return Ok(archive);
    }

    let mut root_tracks = Vec::new();

    for entry in fs::read_dir(&root)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = match entry.metadata() {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };

        if metadata.is_dir() {
            let mut folder = scan_folder(path)?;
            if !folder.tracks.is_empty() {
                folder.tracks.sort_by(compare_tracks_newest_first);
                archive.folders.push(folder);
            }
        } else if metadata.is_file() && is_audio_file(&path) {
            if let Some(track) = track_from_path(path, metadata) {
                root_tracks.push(track);
            }
        }
    }

    if !root_tracks.is_empty() {
        root_tracks.sort_by(compare_tracks_newest_first);
        archive.folders.push(TapeFolder {
            name: "Unsorted".to_string(),
            path: root.clone(),
            tracks: root_tracks,
            expanded: true,
        });
    }

    archive
        .folders
        .sort_by_key(|folder| folder.name.to_lowercase());

    archive.status = if archive.total_tracks() == 0 {
        TapeArchiveStatus::Empty
    } else {
        TapeArchiveStatus::Ready
    };
    archive.rebuild_flattened();

    Ok(archive)
}

pub fn is_audio_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| {
            let ext = ext.trim_start_matches('.').to_lowercase();
            AUDIO_EXTENSIONS.contains(&ext.as_str())
        })
        .unwrap_or(false)
}

pub fn format_file_size(size_bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;

    let size = size_bytes as f64;
    if size >= GB {
        format!("{:.1} GB", size / GB)
    } else if size >= MB {
        format!("{:.1} MB", size / MB)
    } else if size >= KB {
        format!("{:.1} KB", size / KB)
    } else {
        format!("{size_bytes} B")
    }
}

pub fn format_duration(duration: Duration) -> String {
    let total_seconds = duration.as_secs();
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;

    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
}

pub fn track_duration_label(track: &TapeTrack) -> Option<String> {
    track.duration_hint.map(format_duration)
}

pub fn track_metadata_label(track: &TapeTrack) -> String {
    let mut parts = vec![track.extension.to_uppercase()];

    if let Some(duration) = track_duration_label(track) {
        parts.push(duration);
    }

    parts.push(format_file_size(track.size_bytes));
    parts.join(" · ")
}

pub fn display_track_title(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .map(|stem| stem.trim().to_string())
        .filter(|stem| !stem.is_empty())
        .or_else(|| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.to_string())
        })
        .unwrap_or_else(|| "Local tape".to_string())
}

pub fn probe_audio_duration(path: &Path) -> Option<Duration> {
    let file = File::open(path).ok()?;
    let decoder = rodio::Decoder::new(BufReader::new(file)).ok()?;
    decoder.total_duration()
}

fn scan_folder(path: PathBuf) -> Result<TapeFolder, io::Error> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.to_string())
        .unwrap_or_else(|| path.display().to_string());

    let mut tracks = Vec::new();

    for entry in fs::read_dir(&path)? {
        let entry = entry?;
        let track_path = entry.path();
        let metadata = match entry.metadata() {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };

        if metadata.is_file() && is_audio_file(&track_path) {
            if let Some(track) = track_from_path(track_path, metadata) {
                tracks.push(track);
            }
        }
    }

    Ok(TapeFolder {
        name,
        path,
        tracks,
        expanded: true,
    })
}

fn track_from_path(path: PathBuf, metadata: fs::Metadata) -> Option<TapeTrack> {
    let filename = path.file_name()?.to_string_lossy().to_string();
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_lowercase();
    let title = display_track_title(&path);
    let artist = title
        .split_once(" - ")
        .map(|(artist, _title)| artist.trim().to_string())
        .filter(|artist| !artist.is_empty());
    let duration_hint = probe_audio_duration(&path);

    Some(TapeTrack {
        title,
        artist,
        filename,
        path,
        extension,
        size_bytes: metadata.len(),
        modified: metadata.modified().ok(),
        duration_hint,
    })
}

fn compare_tracks_newest_first(left: &TapeTrack, right: &TapeTrack) -> std::cmp::Ordering {
    modified_sort_key(right)
        .cmp(&modified_sort_key(left))
        .then_with(|| {
            left.filename
                .to_lowercase()
                .cmp(&right.filename.to_lowercase())
        })
}

fn shuffle_key(track: &TapeTrack, current_path: &Path, salt: u64) -> u64 {
    let mut hash = 0xcbf29ce484222325u64 ^ salt;

    for byte in track.path.to_string_lossy().bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }

    for byte in current_path.to_string_lossy().bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }

    hash
}

fn modified_sort_key(track: &TapeTrack) -> u64 {
    track
        .modified
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn normalized_query(query: &str) -> Option<String> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        None
    } else {
        Some(query)
    }
}

fn clamp_index(index: usize, len: usize) -> usize {
    if len == 0 {
        0
    } else {
        index.min(len - 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::thread;
    use std::time::Duration as StdDuration;

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(name: &str) -> Self {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!("pulsedeck-{name}-{nanos}"));
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn touch(path: &Path) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        File::create(path).unwrap();
    }

    #[test]
    fn scan_missing_recording_dir_returns_empty_status() {
        let root = std::env::temp_dir().join("pulsedeck-definitely-missing-recordings");
        let archive = scan_tape_archive(root).unwrap();

        assert_eq!(archive.status, TapeArchiveStatus::Empty);
        assert_eq!(archive.total_tracks(), 0);
    }

    #[test]
    fn scan_groups_tracks_by_folder() {
        let temp = TempDir::new("groups");
        touch(&temp.path.join("Synthwave").join("Lazerhawk - King.mp3"));
        touch(&temp.path.join("Ambient").join("Stars.flac"));

        let archive = scan_tape_archive(temp.path.clone()).unwrap();

        assert_eq!(archive.status, TapeArchiveStatus::Ready);
        assert_eq!(archive.total_tracks(), 2);
        assert_eq!(archive.folders.len(), 2);
        assert!(archive
            .folders
            .iter()
            .any(|folder| folder.name == "Synthwave"));
        assert!(archive
            .folders
            .iter()
            .any(|folder| folder.name == "Ambient"));
    }

    #[test]
    fn scan_ignores_non_audio_files() {
        let temp = TempDir::new("ignores");
        touch(&temp.path.join("Synthwave").join("notes.txt"));
        touch(&temp.path.join("Synthwave").join("track.mp3"));

        let archive = scan_tape_archive(temp.path.clone()).unwrap();

        assert_eq!(archive.total_tracks(), 1);
        assert_eq!(archive.folders[0].tracks[0].filename, "track.mp3");
    }

    #[test]
    fn scan_sorts_recent_tracks_first() {
        let temp = TempDir::new("sorts");
        let old = temp.path.join("Synthwave").join("old.mp3");
        let new = temp.path.join("Synthwave").join("new.mp3");

        touch(&old);
        thread::sleep(StdDuration::from_millis(5));
        touch(&new);

        let archive = scan_tape_archive(temp.path.clone()).unwrap();

        assert_eq!(archive.folders[0].tracks[0].filename, "new.mp3");
    }

    #[test]
    fn flatten_archive_respects_expanded_folders() {
        let temp = TempDir::new("flatten");
        touch(&temp.path.join("Synthwave").join("track.mp3"));

        let mut archive = scan_tape_archive(temp.path.clone()).unwrap();
        assert_eq!(archive.flattened.len(), 3);

        archive.selected = 1;
        archive.toggle_selected_folder();

        assert_eq!(archive.flattened.len(), 2);
    }

    #[test]
    fn selected_row_clamps_after_refresh() {
        let mut archive = TapeArchive::new("recordings");
        archive.selected = 99;
        archive.rebuild_flattened();

        assert_eq!(archive.selected, 0);
    }

    #[test]
    fn file_size_formatting_uses_readable_units() {
        assert_eq!(format_file_size(42), "42 B");
        assert_eq!(format_file_size(2048), "2.0 KB");
        assert_eq!(format_file_size(2 * 1024 * 1024), "2.0 MB");
    }

    #[test]
    fn display_track_title_uses_stem() {
        assert_eq!(
            display_track_title(Path::new("/tmp/Lazerhawk - King.mp3")),
            "Lazerhawk - King"
        );
    }

    #[test]
    fn format_duration_under_one_hour() {
        assert_eq!(format_duration(Duration::from_secs(228)), "03:48");
    }

    #[test]
    fn format_duration_over_one_hour() {
        assert_eq!(format_duration(Duration::from_secs(3862)), "1:04:22");
    }

    #[test]
    fn track_metadata_includes_duration_when_available() {
        let track = TapeTrack {
            title: "Track".to_string(),
            artist: None,
            filename: "Track.mp3".to_string(),
            path: PathBuf::from("Track.mp3"),
            extension: "mp3".to_string(),
            size_bytes: 2048,
            modified: None,
            duration_hint: Some(Duration::from_secs(228)),
        };

        assert_eq!(track_metadata_label(&track), "MP3 · 03:48 · 2.0 KB");
    }

    #[test]
    fn all_recordings_mode_flattens_tracks_newest_first() {
        let temp = TempDir::new("all-recordings");
        let old = temp.path.join("Ambient").join("old.mp3");
        let new = temp.path.join("Synthwave").join("new.mp3");

        touch(&old);
        thread::sleep(StdDuration::from_millis(5));
        touch(&new);

        let mut archive = scan_tape_archive(temp.path.clone()).unwrap();
        archive.toggle_all_recordings_mode();

        assert!(archive.all_recordings_flattened);
        assert!(matches!(
            archive.flattened.get(1),
            Some(TapeArchiveRow::AllRecordingTrack { .. })
        ));
        assert_eq!(
            archive
                .selected_track()
                .map(|track| track.filename.as_str()),
            None
        );

        archive.selected = 1;
        assert_eq!(
            archive
                .selected_track()
                .map(|track| track.filename.as_str()),
            Some("new.mp3")
        );
    }

    #[test]
    fn filter_matches_track_title() {
        let temp = TempDir::new("filter-title");
        touch(&temp.path.join("Synthwave").join("Lazerhawk - King.mp3"));
        touch(&temp.path.join("Ambient").join("Stars.mp3"));

        let mut archive = scan_tape_archive(temp.path.clone()).unwrap();
        archive.set_filter_query("king");

        assert_eq!(archive.flattened.len(), 2);
        archive.selected = 1;
        assert_eq!(
            archive
                .selected_track()
                .map(|track| track.filename.as_str()),
            Some("Lazerhawk - King.mp3")
        );
    }

    #[test]
    fn filter_matches_folder_name() {
        let temp = TempDir::new("filter-folder");
        touch(&temp.path.join("Synthwave").join("A.mp3"));
        touch(&temp.path.join("Ambient").join("B.mp3"));

        let mut archive = scan_tape_archive(temp.path.clone()).unwrap();
        archive.set_filter_query("ambient");

        assert_eq!(archive.flattened.len(), 2);
        archive.selected = 1;
        assert_eq!(
            archive
                .selected_track()
                .map(|track| track.filename.as_str()),
            Some("B.mp3")
        );
    }

    #[test]
    fn clearing_filter_restores_tree_rows() {
        let temp = TempDir::new("filter-clear");
        touch(&temp.path.join("Synthwave").join("A.mp3"));

        let mut archive = scan_tape_archive(temp.path.clone()).unwrap();
        archive.set_filter_query("nothing");
        assert_eq!(archive.flattened.len(), 1);

        archive.clear_filter_query();
        assert_eq!(archive.flattened.len(), 3);
    }

    #[test]
    fn next_track_after_returns_next_in_same_folder() {
        let mut archive = TapeArchive::new("recordings");
        archive.folders = vec![TapeFolder {
            name: "Synthwave".to_string(),
            path: PathBuf::from("recordings/Synthwave"),
            expanded: true,
            tracks: vec![
                TapeTrack {
                    title: "First".to_string(),
                    artist: None,
                    filename: "First.mp3".to_string(),
                    path: PathBuf::from("recordings/Synthwave/First.mp3"),
                    extension: "mp3".to_string(),
                    size_bytes: 42,
                    modified: Some(UNIX_EPOCH + Duration::from_secs(2)),
                    duration_hint: None,
                },
                TapeTrack {
                    title: "Second".to_string(),
                    artist: None,
                    filename: "Second.mp3".to_string(),
                    path: PathBuf::from("recordings/Synthwave/Second.mp3"),
                    extension: "mp3".to_string(),
                    size_bytes: 42,
                    modified: Some(UNIX_EPOCH + Duration::from_secs(1)),
                    duration_hint: None,
                },
            ],
        }];
        archive.status = TapeArchiveStatus::Ready;
        archive.rebuild_flattened();

        assert_eq!(
            archive
                .next_track_after(Path::new("recordings/Synthwave/First.mp3"))
                .map(|track| track.filename),
            Some("Second.mp3".to_string())
        );
    }
}
