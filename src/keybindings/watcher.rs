// Keybinding hot-reload: watches keybindings JSON file mtime and re-parses on change.

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

use super::registry::KeybindingRegistry;
use super::KeyBinding;
use crate::mtime_debounce::MtimeDebounce;

const DEBOUNCE_DURATION: Duration = Duration::from_millis(500);
const STARTUP_COOLDOWN: Duration = Duration::from_millis(500);

/// Result of checking for keybinding file changes.
#[derive(Debug)]
pub enum KeybindingReloadResult {
    /// File has not changed since last check.
    Unchanged,
    /// File changed and was re-parsed successfully.
    Reloaded(Vec<KeyBinding>),
    /// File changed but re-parse failed.
    Error(String),
}

/// Tracks keybinding file modification time for hot-reload with debounce.
/// Delegates mtime/debounce logic to `MtimeDebounce` with a 500ms startup cooldown.
pub struct KeybindingWatcher {
    path: Option<PathBuf>,
    debounce: MtimeDebounce,
}

impl KeybindingWatcher {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self {
            path,
            debounce: MtimeDebounce::new(DEBOUNCE_DURATION, Some(STARTUP_COOLDOWN), Instant::now()),
        }
    }

    /// Check if keybinding file changed since last load, with 500ms debounce.
    /// Returns Reloaded only after mtime stabilizes for the debounce duration.
    /// If path is None, always returns Unchanged.
    pub fn check_reload(&mut self, now: Instant) -> KeybindingReloadResult {
        let path = match &self.path {
            Some(p) => p.clone(),
            None => return KeybindingReloadResult::Unchanged,
        };

        let mtime = get_mtime(&path);

        if !self.debounce.check(mtime, now) {
            return KeybindingReloadResult::Unchanged;
        }

        reload_keybindings(&path)
    }
}

fn get_mtime(path: &PathBuf) -> Option<SystemTime> {
    fs::metadata(path).ok()?.modified().ok()
}

fn reload_keybindings(path: &PathBuf) -> KeybindingReloadResult {
    let content = match fs::read(path) {
        Ok(c) => c,
        Err(e) => return KeybindingReloadResult::Error(e.to_string()),
    };

    let mut warnings = Vec::new();
    let registry = KeybindingRegistry::from_json(&content, &mut warnings);

    if !warnings.is_empty() {
        return KeybindingReloadResult::Error(warnings.join("; "));
    }

    KeybindingReloadResult::Reloaded(registry.customs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::thread;
    use std::time::Duration;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join("pulsedeck_keybinding_watcher_tests")
            .join(name);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    const VALID_JSON: &str = r#"[
        {"key": "char(k)", "modifiers": ["ctrl"], "action": "prev_station", "mode": "Normal"}
    ]"#;

    #[test]
    fn test_none_path_always_unchanged() {
        let mut watcher = KeybindingWatcher::new(None);

        let result = watcher.check_reload(Instant::now());
        assert!(matches!(result, KeybindingReloadResult::Unchanged));

        let later = Instant::now() + Duration::from_secs(10);
        let result = watcher.check_reload(later);
        assert!(matches!(result, KeybindingReloadResult::Unchanged));
    }

    #[test]
    fn test_file_change_triggers_debounce_then_reload() {
        let dir = temp_dir("debounce_reload");
        let path = dir.join("keybindings.json");
        fs::write(&path, VALID_JSON).unwrap();

        let mut watcher = KeybindingWatcher::new(Some(path));
        let t0 = Instant::now();

        // During cooldown (first 500ms), all signals suppressed
        let result = watcher.check_reload(t0);
        assert!(matches!(result, KeybindingReloadResult::Unchanged));

        // After cooldown, first check detects mtime change (None→Some), starts debounce
        let t1 = t0 + Duration::from_millis(501);
        let result = watcher.check_reload(t1);
        assert!(matches!(result, KeybindingReloadResult::Unchanged));

        // After debounce elapses, reload fires
        let t2 = t1 + Duration::from_millis(500);
        let result = watcher.check_reload(t2);
        match result {
            KeybindingReloadResult::Reloaded(bindings) => {
                assert_eq!(bindings.len(), 1);
                assert_eq!(bindings[0].action, crate::action::Action::PrevStation);
            }
            other => panic!("expected Reloaded, got {:?}", other),
        }
    }

    #[test]
    fn test_invalid_json_returns_error() {
        let dir = temp_dir("invalid_json");
        let path = dir.join("keybindings.json");
        fs::write(&path, "not valid json{{{").unwrap();

        let mut watcher = KeybindingWatcher::new(Some(path));
        let t0 = Instant::now();

        // After cooldown, detect mtime change
        let t1 = t0 + Duration::from_millis(501);
        let result = watcher.check_reload(t1);
        assert!(matches!(result, KeybindingReloadResult::Unchanged));

        // After debounce, attempt reload → error
        let t2 = t1 + Duration::from_millis(500);
        let result = watcher.check_reload(t2);
        match result {
            KeybindingReloadResult::Error(msg) => {
                assert!(!msg.is_empty());
            }
            other => panic!("expected Error, got {:?}", other),
        }
    }

    #[test]
    fn test_no_change_returns_unchanged() {
        let dir = temp_dir("no_change");
        let path = dir.join("keybindings.json");
        fs::write(&path, VALID_JSON).unwrap();

        let mut watcher = KeybindingWatcher::new(Some(path));
        let t0 = Instant::now();

        // After cooldown, detect + debounce + reload
        let t1 = t0 + Duration::from_millis(501);
        watcher.check_reload(t1);
        let t2 = t1 + Duration::from_millis(500);
        let _ = watcher.check_reload(t2);

        // Subsequent call without file change → Unchanged
        let t3 = t2 + Duration::from_millis(100);
        let result = watcher.check_reload(t3);
        assert!(matches!(result, KeybindingReloadResult::Unchanged));
    }

    #[test]
    fn test_startup_cooldown_suppresses_changes() {
        let dir = temp_dir("cooldown_suppresses");
        let path = dir.join("keybindings.json");
        fs::write(&path, VALID_JSON).unwrap();

        let mut watcher = KeybindingWatcher::new(Some(path));
        let t0 = Instant::now();

        // During the 500ms startup cooldown, all calls return Unchanged
        for offset_ms in [0, 100, 200, 300, 400, 499] {
            let t = t0 + Duration::from_millis(offset_ms);
            let result = watcher.check_reload(t);
            assert!(
                matches!(result, KeybindingReloadResult::Unchanged),
                "Expected Unchanged at {}ms during cooldown",
                offset_ms
            );
        }
    }

    #[test]
    #[ignore] // Filesystem-dependent: uses thread::sleep and real mtime changes
    fn test_file_modification_after_initial_reload() {
        let dir = temp_dir("modification_after_reload");
        let path = dir.join("keybindings.json");
        fs::write(&path, VALID_JSON).unwrap();

        let mut watcher = KeybindingWatcher::new(Some(path.clone()));
        let t0 = Instant::now();

        // After cooldown, detect + debounce + reload
        let t1 = t0 + Duration::from_millis(501);
        watcher.check_reload(t1);
        let t2 = t1 + Duration::from_millis(500);
        let _ = watcher.check_reload(t2);

        // Modify file
        thread::sleep(Duration::from_millis(50));
        let new_json = r#"[
            {"key": "char(j)", "modifiers": [], "action": "next_station"},
            {"key": "char(k)", "modifiers": [], "action": "prev_station"}
        ]"#;
        fs::write(&path, new_json).unwrap();

        // Detect new change
        let t3 = t2 + Duration::from_millis(100);
        let result = watcher.check_reload(t3);
        assert!(matches!(result, KeybindingReloadResult::Unchanged));

        // After debounce, reload with new content
        let t4 = t3 + Duration::from_millis(500);
        let result = watcher.check_reload(t4);
        match result {
            KeybindingReloadResult::Reloaded(bindings) => {
                assert_eq!(bindings.len(), 2);
            }
            other => panic!("expected Reloaded, got {:?}", other),
        }
    }
}
