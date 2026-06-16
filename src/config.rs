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
pub fn load_json<T: DeserializeOwned + Default>(file: &str) -> T {
    let Some(path) = config_path(file) else {
        return T::default();
    };

    fs::read_to_string(path)
        .ok()
        .and_then(|contents| serde_json::from_str::<T>(&contents).ok())
        .unwrap_or_default()
}

#[cfg_attr(test, allow(dead_code))]
pub fn save_json<T: Serialize>(file: &str, value: &T) -> Result<()> {
    let Some(dir) = config_dir() else {
        return Ok(());
    };

    fs::create_dir_all(&dir)?;
    fs::write(dir.join(file), serde_json::to_string_pretty(value)?)?;
    Ok(())
}

pub fn path_for(base: &Path, config_dir: &str, file: &str) -> PathBuf {
    base.join(config_dir).join(file)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
