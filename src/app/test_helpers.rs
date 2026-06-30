//! Shared integration test utilities for multi-step scenario tests.
//! Provides reusable helpers for constructing apps with real config directories,
//! creating temp directories, and reloading config from disk.

#[cfg(test)]
pub(crate) mod helpers {
    use crate::app::App;
    use crate::config_toml::AppConfig;
    use crate::favorites::Library;
    use crate::radio::Station;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Create a unique temporary directory for a test.
    /// Each call produces a unique path based on test name, PID, and timestamp.
    pub fn unique_temp_dir(test_name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "pulsedeck-integ-{test_name}-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    /// Create a basic test station.
    pub fn station(name: &str, url: &str) -> Station {
        Station::basic(name, url, "Synthwave", "US", 128)
    }

    /// Create an App with a real config directory and 3 test stations.
    pub fn app_with_config_dir(dir: &Path) -> App {
        let mut app = App::new(Library::in_memory(vec![
            station("Station A", "http://a"),
            station("Station B", "http://b"),
            station("Station C", "http://c"),
        ]));
        app.config_dir = Some(dir.to_path_buf());
        app.library.path = Some(dir.join("library.json"));
        app
    }

    /// Reload AppConfig from a config directory (simulates restart TOML load).
    pub fn reload_config_from_dir(dir: &Path) -> AppConfig {
        let result = crate::config_toml::io::load_config(dir);
        result.config
    }
}
