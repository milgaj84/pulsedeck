use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const JOURNAL_FILENAME: &str = ".pulsedeck-recording-session.json";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordingRecovery {
    pub journal_path: PathBuf,
    pub station_name: Option<String>,
    pub active_file: Option<String>,
    pub state: Option<String>,
}

impl RecordingRecovery {
    pub fn active_file_path(&self) -> Option<PathBuf> {
        self.active_file
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .map(PathBuf::from)
    }

    pub fn summary(&self) -> String {
        let station = self
            .station_name
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("unknown station");

        let file = self
            .active_file
            .as_deref()
            .and_then(|path| Path::new(path).file_name().and_then(|name| name.to_str()))
            .unwrap_or("unfinished capture");

        format!("Recovery journal found for {station}: {file}")
    }
}

#[allow(clippy::too_many_arguments)]
pub fn write_session_journal(
    recording_dir: &str,
    station_name: Option<&str>,
    station_url: Option<&str>,
    category: Option<&str>,
    state: &str,
    started_at: SystemTime,
    active_file: Option<&str>,
    track_title: Option<&str>,
) -> Result<(), String> {
    let dir = PathBuf::from(recording_dir);
    fs::create_dir_all(&dir).map_err(|err| {
        format!(
            "Could not create recording directory '{}': {err}",
            dir.display()
        )
    })?;

    let path = journal_path(recording_dir);
    let started_unix_secs = started_at
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let json = format!(
        "{{\n  \"station_name\": {},\n  \"station_url\": {},\n  \"category\": {},\n  \"state\": {},\n  \"started_unix_secs\": {},\n  \"active_file\": {},\n  \"track_title\": {}\n}}\n",
        json_string_opt(station_name),
        json_string_opt(station_url),
        json_string_opt(category),
        json_string(state),
        started_unix_secs,
        json_string_opt(active_file),
        json_string_opt(track_title),
    );

    fs::write(&path, json).map_err(|err| {
        format!(
            "Could not write recording journal '{}': {err}",
            path.display()
        )
    })
}

pub fn remove_session_journal(recording_dir: &str) -> Result<(), String> {
    remove_journal_file(&journal_path(recording_dir))
}

pub fn remove_journal_file(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(format!(
            "Could not remove recording journal '{}': {err}",
            path.display()
        )),
    }
}

pub fn detect_recovery_journal(recording_dir: &str) -> Result<Option<RecordingRecovery>, String> {
    let path = journal_path(recording_dir);

    if !path.exists() {
        return Ok(None);
    }

    let text = fs::read_to_string(&path).map_err(|err| {
        format!(
            "Could not read recording journal '{}': {err}",
            path.display()
        )
    })?;

    Ok(Some(RecordingRecovery {
        journal_path: path,
        station_name: extract_json_string(&text, "station_name"),
        active_file: extract_json_string(&text, "active_file"),
        state: extract_json_string(&text, "state"),
    }))
}

pub fn journal_path(recording_dir: &str) -> PathBuf {
    PathBuf::from(recording_dir).join(JOURNAL_FILENAME)
}

pub fn format_elapsed(started_at: Option<SystemTime>) -> String {
    let Some(started_at) = started_at else {
        return "--:--".to_string();
    };

    let elapsed = started_at.elapsed().unwrap_or_default();
    format_elapsed_duration(elapsed)
}

pub fn format_elapsed_duration(duration: Duration) -> String {
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

fn json_string(value: &str) -> String {
    format!("\"{}\"", escape_json(value))
}

fn json_string_opt(value: Option<&str>) -> String {
    value.map(json_string).unwrap_or_else(|| "null".to_string())
}

fn escape_json(value: &str) -> String {
    value
        .chars()
        .flat_map(|ch| match ch {
            '"' => "\\\"".chars().collect::<Vec<_>>(),
            '\\' => "\\\\".chars().collect::<Vec<_>>(),
            '\n' => "\\n".chars().collect::<Vec<_>>(),
            '\r' => "\\r".chars().collect::<Vec<_>>(),
            '\t' => "\\t".chars().collect::<Vec<_>>(),
            other => vec![other],
        })
        .collect()
}

fn extract_json_string(text: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\":");
    let start = text.find(&needle)? + needle.len();
    let rest = text[start..].trim_start();

    if rest.starts_with("null") {
        return None;
    }

    let rest = rest.strip_prefix('"')?;
    let mut value = String::new();
    let mut chars = rest.chars();

    while let Some(ch) = chars.next() {
        match ch {
            '"' => return Some(value),
            '\\' => {
                if let Some(escaped) = chars.next() {
                    value.push(match escaped {
                        'n' => '\n',
                        'r' => '\r',
                        't' => '\t',
                        '"' => '"',
                        '\\' => '\\',
                        other => other,
                    });
                }
            }
            other => value.push(other),
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn journal_path_uses_hidden_session_file() {
        assert_eq!(
            journal_path("recordings"),
            PathBuf::from("recordings").join(JOURNAL_FILENAME)
        );
    }

    #[test]
    fn elapsed_duration_formats_minutes_and_seconds() {
        assert_eq!(format_elapsed_duration(Duration::from_secs(0)), "00:00");
        assert_eq!(format_elapsed_duration(Duration::from_secs(125)), "02:05");
        assert_eq!(
            format_elapsed_duration(Duration::from_secs(3725)),
            "1:02:05"
        );
    }

    #[test]
    fn recovery_summary_uses_station_and_filename() {
        let recovery = RecordingRecovery {
            journal_path: PathBuf::from("recordings/.pulsedeck-recording-session.json"),
            station_name: Some("Night Drive".to_string()),
            active_file: Some("recordings/Synthwave/Artist - Track.mp3".to_string()),
            state: Some("active".to_string()),
        };

        assert_eq!(
            recovery.summary(),
            "Recovery journal found for Night Drive: Artist - Track.mp3"
        );
    }

    #[test]
    fn json_string_extractor_handles_escaped_values() {
        let text = r#"{"station_name": "Night \"Drive\"", "active_file": null}"#;

        assert_eq!(
            extract_json_string(text, "station_name"),
            Some("Night \"Drive\"".to_string())
        );
        assert_eq!(extract_json_string(text, "active_file"), None);
    }

    #[test]
    fn active_file_path_returns_pathbuf_when_present() {
        let recovery = RecordingRecovery {
            journal_path: PathBuf::from("recordings/.pulsedeck-recording-session.json"),
            station_name: None,
            active_file: Some("recordings/Synthwave/capture.mp3".to_string()),
            state: Some("active".to_string()),
        };

        assert_eq!(
            recovery.active_file_path(),
            Some(PathBuf::from("recordings/Synthwave/capture.mp3"))
        );
    }
}
