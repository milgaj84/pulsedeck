// Config hot-reload: watches file modification time and re-parses on change.

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

use super::parse::parse_toml;
use super::AppConfig;

const DEBOUNCE_DURATION: Duration = Duration::from_millis(500);

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

/// Tracks file modification time for hot-reload detection with debounce.
pub struct ConfigWatcher {
    last_mtime: Option<SystemTime>,
    config_path: PathBuf,
    pending_since: Option<Instant>,
}

impl ConfigWatcher {
    pub fn new(config_path: PathBuf) -> Self {
        Self {
            last_mtime: None,
            config_path,
            pending_since: None,
        }
    }

    /// Check if config file changed since last load, with 500ms debounce.
    /// Returns Reloaded only after mtime stabilizes for the debounce duration.
    pub fn check_reload(&mut self, now: Instant) -> ReloadResult {
        let mtime = Self::get_mtime(&self.config_path);

        if self.mtime_changed(mtime) {
            self.last_mtime = mtime;
            self.pending_since = Some(now);
            return ReloadResult::Unchanged;
        }

        self.try_debounced_reload(now)
    }

    fn mtime_changed(&self, current: Option<SystemTime>) -> bool {
        match (current, self.last_mtime) {
            (Some(cur), Some(last)) => cur != last,
            (Some(_), None) => true,
            _ => false,
        }
    }

    fn try_debounced_reload(&mut self, now: Instant) -> ReloadResult {
        let pending = match self.pending_since {
            Some(t) => t,
            None => return ReloadResult::Unchanged,
        };

        if now.duration_since(pending) < DEBOUNCE_DURATION {
            return ReloadResult::Unchanged;
        }

        self.pending_since = None;
        self.reload_file()
    }

    fn get_mtime(path: &PathBuf) -> Option<SystemTime> {
        fs::metadata(path).ok()?.modified().ok()
    }

    fn reload_file(&mut self) -> ReloadResult {
        let content = match fs::read_to_string(&self.config_path) {
            Ok(c) => c,
            Err(e) => return ReloadResult::Error(e.to_string()),
        };

        match parse_toml(&content) {
            Ok(result) => ReloadResult::Reloaded(result.config, result.preserved),
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

        let result = watcher.check_reload(Instant::now());
        assert!(matches!(result, ReloadResult::Unchanged));
    }

    #[test]
    fn test_check_reload_first_call_detects_change_then_debounce_reloads() {
        let dir = temp_dir("first_call_debounce");
        let path = dir.join("pulsedeck.toml");
        fs::write(&path, VALID_TOML).unwrap();

        let mut watcher = ConfigWatcher::new(path);
        let t0 = Instant::now();

        // First call detects mtime change (None→Some), starts debounce
        let result = watcher.check_reload(t0);
        assert!(matches!(result, ReloadResult::Unchanged));

        // After 500ms with no further changes, reload fires
        let t1 = t0 + Duration::from_millis(500);
        let result = watcher.check_reload(t1);
        match result {
            ReloadResult::Reloaded(config, _) => {
                assert_eq!(config.audio.default_volume, 70);
            }
            _ => panic!("expected Reloaded, got {:?}", result),
        }
    }

    #[test]
    fn test_check_reload_unchanged_mtime_no_pending_returns_unchanged() {
        let dir = temp_dir("unchanged_mtime_debounce");
        let path = dir.join("pulsedeck.toml");
        fs::write(&path, VALID_TOML).unwrap();

        let mut watcher = ConfigWatcher::new(path);
        let t0 = Instant::now();

        // Detect + debounce + reload
        watcher.check_reload(t0);
        let t1 = t0 + Duration::from_millis(500);
        let _ = watcher.check_reload(t1);

        // Subsequent call without changes → Unchanged
        let t2 = t1 + Duration::from_millis(100);
        let result = watcher.check_reload(t2);
        assert!(matches!(result, ReloadResult::Unchanged));
    }

    #[test]
    fn test_check_reload_changed_mtime_triggers_new_debounce() {
        let dir = temp_dir("changed_mtime_debounce");
        let path = dir.join("pulsedeck.toml");
        fs::write(&path, VALID_TOML).unwrap();

        let mut watcher = ConfigWatcher::new(path.clone());
        let t0 = Instant::now();

        // Initial detect + debounce + reload
        watcher.check_reload(t0);
        let t1 = t0 + Duration::from_millis(500);
        let _ = watcher.check_reload(t1);

        // Modify file
        thread::sleep(Duration::from_millis(50));
        fs::write(&path, VALID_TOML_V2).unwrap();

        // Detect new change
        let t2 = t1 + Duration::from_millis(100);
        let result = watcher.check_reload(t2);
        assert!(matches!(result, ReloadResult::Unchanged));

        // After debounce, reload with new content
        let t3 = t2 + Duration::from_millis(500);
        let result = watcher.check_reload(t3);
        match result {
            ReloadResult::Reloaded(config, _) => {
                assert_eq!(config.audio.default_volume, 42);
            }
            _ => panic!("expected Reloaded, got {:?}", result),
        }
    }

    #[test]
    fn test_check_reload_rapid_changes_reset_debounce_timer() {
        let dir = temp_dir("rapid_changes");
        let path = dir.join("pulsedeck.toml");
        fs::write(&path, VALID_TOML).unwrap();

        let mut watcher = ConfigWatcher::new(path.clone());
        let t0 = Instant::now();

        // First detect
        watcher.check_reload(t0);

        // Simulate rapid write within debounce window
        thread::sleep(Duration::from_millis(50));
        fs::write(&path, VALID_TOML_V2).unwrap();

        let t1 = t0 + Duration::from_millis(200);
        let result = watcher.check_reload(t1);
        // Detects new mtime change, resets pending_since
        assert!(matches!(result, ReloadResult::Unchanged));

        // 500ms from first detect is NOT enough (timer was reset at t1)
        let t2 = t0 + Duration::from_millis(500);
        let result = watcher.check_reload(t2);
        assert!(matches!(result, ReloadResult::Unchanged));

        // 500ms from the RESET point (t1) triggers reload
        let t3 = t1 + Duration::from_millis(500);
        let result = watcher.check_reload(t3);
        match result {
            ReloadResult::Reloaded(config, _) => {
                assert_eq!(config.audio.default_volume, 42);
            }
            _ => panic!("expected Reloaded, got {:?}", result),
        }
    }

    #[test]
    fn test_check_reload_no_reload_during_debounce_window() {
        let dir = temp_dir("no_reload_during_debounce");
        let path = dir.join("pulsedeck.toml");
        fs::write(&path, VALID_TOML).unwrap();

        let mut watcher = ConfigWatcher::new(path);
        let t0 = Instant::now();

        // Detect change
        watcher.check_reload(t0);

        // Check at 100ms, 200ms, 300ms, 400ms — all should be Unchanged
        for offset_ms in [100, 200, 300, 400] {
            let t = t0 + Duration::from_millis(offset_ms);
            let result = watcher.check_reload(t);
            assert!(
                matches!(result, ReloadResult::Unchanged),
                "Expected Unchanged at {}ms",
                offset_ms
            );
        }
    }

    #[test]
    fn test_check_reload_deleted_file_returns_unchanged() {
        let dir = temp_dir("deleted_file_debounce");
        let path = dir.join("pulsedeck.toml");
        fs::write(&path, VALID_TOML).unwrap();

        let mut watcher = ConfigWatcher::new(path.clone());
        let t0 = Instant::now();

        // Detect + reload
        watcher.check_reload(t0);
        let t1 = t0 + Duration::from_millis(500);
        let _ = watcher.check_reload(t1);

        // Delete the file
        fs::remove_file(&path).unwrap();

        // mtime is now None — mtime_changed returns false (None vs Some), no trigger
        let t2 = t1 + Duration::from_millis(100);
        let result = watcher.check_reload(t2);
        assert!(matches!(result, ReloadResult::Unchanged));
    }

    #[test]
    fn test_check_reload_invalid_toml_returns_error_after_debounce() {
        let dir = temp_dir("invalid_toml_debounce");
        let path = dir.join("pulsedeck.toml");
        fs::write(&path, "this is [[[not valid").unwrap();

        let mut watcher = ConfigWatcher::new(path);
        let t0 = Instant::now();

        // Detect change
        let result = watcher.check_reload(t0);
        assert!(matches!(result, ReloadResult::Unchanged));

        // After debounce, attempt reload → error
        let t1 = t0 + Duration::from_millis(500);
        let result = watcher.check_reload(t1);
        match result {
            ReloadResult::Error(msg) => {
                assert!(!msg.is_empty());
            }
            _ => panic!("expected Error, got {:?}", result),
        }
    }

    #[test]
    fn test_check_reload_invalid_after_valid_returns_error() {
        let dir = temp_dir("invalid_after_valid_debounce");
        let path = dir.join("pulsedeck.toml");
        fs::write(&path, VALID_TOML).unwrap();

        let mut watcher = ConfigWatcher::new(path.clone());
        let t0 = Instant::now();

        // Initial detect + debounce + reload
        watcher.check_reload(t0);
        let t1 = t0 + Duration::from_millis(500);
        let _ = watcher.check_reload(t1);

        // Write invalid content
        thread::sleep(Duration::from_millis(50));
        fs::write(&path, "broken [[[toml").unwrap();

        // Detect change
        let t2 = t1 + Duration::from_millis(100);
        watcher.check_reload(t2);

        // After debounce → error
        let t3 = t2 + Duration::from_millis(500);
        let result = watcher.check_reload(t3);
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
        assert!(watcher.pending_since.is_none());
    }
}
