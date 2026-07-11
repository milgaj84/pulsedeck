use anyhow::Result;
use serde::{de::DeserializeOwned, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const NEW_CONFIG_DIR: &str = "pulsedeck";
const OLD_CONFIG_DIR: &str = "driftfm";

pub fn config_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|base| base.join(NEW_CONFIG_DIR))
}

pub fn config_path(file: &str) -> Option<PathBuf> {
    migrate_legacy(file);
    config_dir().map(|dir| dir.join(file))
}

pub fn migrate_legacy(file: &str) {
    let Some(base) = dirs::config_dir() else {
        return;
    };

    let new_path = path_for(&base, NEW_CONFIG_DIR, file);
    let old_path = path_for(&base, OLD_CONFIG_DIR, file);
    if new_path.exists() || !old_path.exists() {
        return;
    }

    if let Some(parent) = new_path.parent() {
        if fs::create_dir_all(parent).is_err() {
            return;
        }
    }

    let _ = fs::copy(old_path, new_path);
}

#[cfg_attr(test, allow(dead_code))]
pub fn load_json_with_warning<T: DeserializeOwned + Default>(file: &str) -> (T, Option<String>) {
    let Some(path) = config_path(file) else {
        return (T::default(), None);
    };

    load_json_from_path_with_warning(&path, file)
}

pub fn load_json_from_path_with_warning<T: DeserializeOwned + Default>(
    path: &Path,
    display_name: &str,
) -> (T, Option<String>) {
    if !path.exists() {
        return (T::default(), None);
    }

    match fs::read_to_string(path) {
        Ok(contents) => parse_json_with_warning(display_name, &contents),
        Err(err) => (
            T::default(),
            Some(format!(
                "Could not read {display_name}; using defaults: {err}"
            )),
        ),
    }
}

#[cfg_attr(test, allow(dead_code))]
pub fn save_json<T: Serialize>(file: &str, value: &T) -> Result<()> {
    let Some(dir) = config_dir() else {
        return Ok(());
    };

    save_json_to_path(&dir.join(file), value)
}

pub fn save_json_to_path<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value)?;
    crate::persistence::atomic_write(path, &bytes)?;
    Ok(())
}

pub fn path_for(base: &Path, config_dir: &str, file: &str) -> PathBuf {
    base.join(config_dir).join(file)
}

fn parse_json_with_warning<T: DeserializeOwned + Default>(
    file: &str,
    contents: &str,
) -> (T, Option<String>) {
    match serde_json::from_str::<T>(contents) {
        Ok(value) => (value, None),
        Err(err) => (
            T::default(),
            Some(format!("Could not parse {file}; using defaults: {err}")),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
    struct TestConfig {
        #[serde(default)]
        value: u8,
    }

    fn unique_temp_path(name: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!(
                "pulsedeck-config-test-{}-{name}",
                std::process::id()
            ))
            .join("state.json")
    }

    #[test]
    fn path_for_uses_requested_config_dir_and_file() {
        let base = PathBuf::from("/tmp/config");

        assert_eq!(
            path_for(&base, NEW_CONFIG_DIR, "library.json"),
            PathBuf::from("/tmp/config/pulsedeck/library.json")
        );
        assert_eq!(
            path_for(&base, OLD_CONFIG_DIR, "history.json"),
            PathBuf::from("/tmp/config/driftfm/history.json")
        );
    }

    #[test]
    fn parse_json_with_warning_accepts_valid_json() {
        let (value, warning) =
            parse_json_with_warning::<TestConfig>("ui-state.json", "{\"value\":7}");

        assert_eq!(value, TestConfig { value: 7 });
        assert!(warning.is_none());
    }

    #[test]
    fn parse_json_with_warning_returns_default_and_warning_for_malformed_json() {
        let (value, warning) = parse_json_with_warning::<TestConfig>("history.json", "{not json");

        assert_eq!(value, TestConfig::default());
        assert!(warning
            .unwrap()
            .contains("Could not parse history.json; using defaults"));
    }

    #[test]
    fn path_based_save_and_load_use_real_atomic_persistence() {
        let path = unique_temp_path("round-trip");
        let _ = fs::remove_dir_all(path.parent().unwrap());

        save_json_to_path(&path, &TestConfig { value: 41 }).unwrap();
        save_json_to_path(&path, &TestConfig { value: 42 }).unwrap();
        let (loaded, warning) = load_json_from_path_with_warning::<TestConfig>(&path, "state.json");

        assert_eq!(loaded, TestConfig { value: 42 });
        assert!(warning.is_none());
        assert!(crate::persistence::backup_path(&path).exists());
    }

    #[test]
    fn path_based_load_reports_malformed_json() {
        let path = unique_temp_path("malformed");
        let _ = fs::remove_dir_all(path.parent().unwrap());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "{broken").unwrap();

        let (loaded, warning) = load_json_from_path_with_warning::<TestConfig>(&path, "state.json");

        assert_eq!(loaded, TestConfig::default());
        assert!(warning.unwrap().contains("Could not parse state.json"));
    }
}
