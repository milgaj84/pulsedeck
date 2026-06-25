// Config hot-reload: watches file modification time and re-parses on change.

use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

use super::parse::parse_toml;
use super::AppConfig;

/// Result of checking for config file changes.
#[derive(Debug)]
pub enum ReloadResult {
    /// File has not changed since last check.
    Unchanged,
    /// File changed and was re-parsed successfully.
    Reloaded(AppConfig, toml::Value),
    /// File changed but re-parse failed.
    Error(String),
}

/// Tracks file modification time for hot-reload detection.
pub struct ConfigWatcher {
    last_mtime: Option<SystemTime>,
    config_path: PathBuf,
}

impl ConfigWatcher {
    pub fn new(config_path: PathBuf) -> Self {
        Self {
            last_mtime: None,
            config_path,
        }
    }

    /// Check if config file changed since last load.
    /// Returns new config if changed, Unchanged if same, Error if parse fails.
    pub fn check_reload(&mut self) -> ReloadResult {
        let mtime = match Self::get_mtime(&self.config_path) {
            Some(t) => t,
            None => return ReloadResult::Unchanged,
        };

        if self.last_mtime == Some(mtime) {
            return ReloadResult::Unchanged;
        }

        self.reload_file(mtime)
    }

    fn get_mtime(path: &PathBuf) -> Option<SystemTime> {
        fs::metadata(path).ok()?.modified().ok()
    }

    fn reload_file(&mut self, mtime: SystemTime) -> ReloadResult {
        let content = match fs::read_to_string(&self.config_path) {
            Ok(c) => c,
            Err(e) => return ReloadResult::Error(e.to_string()),
        };

        match parse_toml(&content) {
            Ok(result) => {
                self.last_mtime = Some(mtime);
                ReloadResult::Reloaded(result.config, result.preserved)
            }
            Err(e) => ReloadResult::Error(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::thread;
    use std::time::Duration;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join("pulsedeck_hot_reload_tests")
            .join(name);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    const VALID_TOML: &str = "[audio]\ndefault_volume = 70\n";
    const VALID_TOML_V2: &str = "[audio]\ndefault_volume = 42\n";

    #[test]
    fn test_check_reload_nonexistent_file_returns_unchanged() {
        let dir = temp_dir("nonexistent");
        let path = dir.join("pulsedeck.toml");
        let mut watcher = ConfigWatcher::new(path);

        let result = watcher.check_reload();
        assert!(matches!(result, ReloadResult::Unchanged));
    }

    #[test]
    fn test_check_reload_first_call_returns_reloaded() {
        let dir = temp_dir("first_call");
        let path = dir.join("pulsedeck.toml");
        fs::write(&path, VALID_TOML).unwrap();

        let mut watcher = ConfigWatcher::new(path);
        let result = watcher.check_reload();

        match result {
            ReloadResult::Reloaded(config, _) => {
                assert_eq!(config.audio.default_volume, 70);
            }
            _ => panic!("expected Reloaded, got {:?}", result),
        }
    }

    #[test]
    fn test_check_reload_unchanged_mtime_returns_unchanged() {
        let dir = temp_dir("unchanged_mtime");
        let path = dir.join("pulsedeck.toml");
        fs::write(&path, VALID_TOML).unwrap();

        let mut watcher = ConfigWatcher::new(path);
        // First call loads the config
        let _ = watcher.check_reload();
        // Second call without changes → Unchanged
        let result = watcher.check_reload();
        assert!(matches!(result, ReloadResult::Unchanged));
    }

    #[test]
    fn test_check_reload_changed_mtime_returns_reloaded() {
        let dir = temp_dir("changed_mtime");
        let path = dir.join("pulsedeck.toml");
        fs::write(&path, VALID_TOML).unwrap();

        let mut watcher = ConfigWatcher::new(path.clone());
        let _ = watcher.check_reload();

        // Sleep to ensure mtime differs
        thread::sleep(Duration::from_millis(50));
        fs::write(&path, VALID_TOML_V2).unwrap();

        let result = watcher.check_reload();
        match result {
            ReloadResult::Reloaded(config, _) => {
                assert_eq!(config.audio.default_volume, 42);
            }
            _ => panic!("expected Reloaded, got {:?}", result),
        }
    }

    #[test]
    fn test_check_reload_deleted_file_returns_unchanged() {
        let dir = temp_dir("deleted_file");
        let path = dir.join("pulsedeck.toml");
        fs::write(&path, VALID_TOML).unwrap();

        let mut watcher = ConfigWatcher::new(path.clone());
        let _ = watcher.check_reload();

        // Delete the file
        fs::remove_file(&path).unwrap();

        let result = watcher.check_reload();
        assert!(matches!(result, ReloadResult::Unchanged));
    }

    #[test]
    fn test_check_reload_invalid_toml_returns_error() {
        let dir = temp_dir("invalid_toml");
        let path = dir.join("pulsedeck.toml");
        fs::write(&path, "this is [[[not valid").unwrap();

        let mut watcher = ConfigWatcher::new(path);
        let result = watcher.check_reload();

        match result {
            ReloadResult::Error(msg) => {
                assert!(!msg.is_empty());
            }
            _ => panic!("expected Error, got {:?}", result),
        }
    }

    #[test]
    fn test_check_reload_invalid_after_valid_returns_error() {
        let dir = temp_dir("invalid_after_valid");
        let path = dir.join("pulsedeck.toml");
        fs::write(&path, VALID_TOML).unwrap();

        let mut watcher = ConfigWatcher::new(path.clone());
        let _ = watcher.check_reload();

        // Sleep to ensure mtime differs, then write invalid content
        thread::sleep(Duration::from_millis(50));
        fs::write(&path, "broken [[[toml").unwrap();

        let result = watcher.check_reload();
        match result {
            ReloadResult::Error(msg) => {
                assert!(!msg.is_empty());
            }
            _ => panic!("expected Error, got {:?}", result),
        }
    }

    #[test]
    fn test_new_creates_watcher_with_no_mtime() {
        let path = PathBuf::from("/nonexistent/path/config.toml");
        let watcher = ConfigWatcher::new(path.clone());

        assert_eq!(watcher.config_path, path);
        assert!(watcher.last_mtime.is_none());
    }
}
