use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const AUDIO_EXTENSIONS: &[&str] = &["mp3", "flac", "wav", "ogg", "opus", "m4a", "aac"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TapeArchive {
    pub root: PathBuf,
    pub folders: Vec<TapeFolder>,
    pub flattened: Vec<TapeArchiveRow>,
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
            } => self
                .folders
                .get(*folder_index)
                .and_then(|folder| folder.tracks.get(*track_index)),
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

    pub fn toggle_selected_folder(&mut self) {
        match self.selected_row().cloned() {
            Some(TapeArchiveRow::AllRecordings) => self.toggle_all_folders(),
            Some(TapeArchiveRow::Folder { folder_index }) => {
                if let Some(folder) = self.folders.get_mut(folder_index) {
                    folder.expanded = !folder.expanded;
                }
                self.rebuild_flattened();
            }
            _ => {}
        }
    }

    pub fn rebuild_flattened(&mut self) {
        let selected_before = self.selected;
        self.flattened.clear();
        self.flattened.push(TapeArchiveRow::AllRecordings);

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

        self.selected = clamp_index(selected_before, self.flattened.len());
    }

    fn toggle_all_folders(&mut self) {
        let should_expand = self.folders.iter().any(|folder| !folder.expanded);
        for folder in &mut self.folders {
            folder.expanded = should_expand;
        }
        self.rebuild_flattened();
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

    Some(TapeTrack {
        title,
        artist,
        filename,
        path,
        extension,
        size_bytes: metadata.len(),
        modified: metadata.modified().ok(),
        duration_hint: None,
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

fn modified_sort_key(track: &TapeTrack) -> u64 {
    track
        .modified
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
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
}
