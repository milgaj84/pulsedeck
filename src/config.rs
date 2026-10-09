use anyhow::Result;
use serde::{de::DeserializeOwned, Serialize};
use std::path::{Path, PathBuf};
use std::{env, fs};

const NEW_CONFIG_DIR: &str = "pulsedeck";
const OLD_CONFIG_DIR: &str = "driftfm";

/// An XDG base directory must be absolute; the spec says to ignore empty or
/// relative values.
fn absolute_dir(value: Option<std::ffi::OsString>) -> Option<PathBuf> {
    value.map(PathBuf::from).filter(|path| path.is_absolute())
}

/// Config base directories in lookup order. On macOS the XDG style locations
/// (`$XDG_CONFIG_HOME`, `~/.config`) are tried before the native one.
fn base_dirs(
    xdg: Option<PathBuf>,
    home: Option<PathBuf>,
    native: Option<PathBuf>,
    xdg_style: bool,
) -> Vec<PathBuf> {
    let xdg_dirs = if xdg_style {
        [xdg, home.map(|h| h.join(".config"))]
    } else {
        [None, None]
    };

    let mut dirs = Vec::new();
    for path in xdg_dirs.into_iter().chain([native]).flatten() {
        if !dirs.contains(&path) {
            dirs.push(path);
        }
    }
    dirs
}

#[cfg(not(test))]
pub(crate) fn candidate_base_dirs() -> Vec<PathBuf> {
    base_dirs(
        absolute_dir(env::var_os("XDG_CONFIG_HOME")),
        absolute_dir(env::var_os("HOME")),
        native_config_dir(),
        cfg!(target_os = "macos"),
    )
}

/// Test builds never see the developer's real configuration: every lookup and
/// save lands in a scratch directory that belongs to this test process.
#[cfg(test)]
pub(crate) fn candidate_base_dirs() -> Vec<PathBuf> {
    vec![test_config_root()]
}

#[cfg(not(test))]
fn native_config_dir() -> Option<PathBuf> {
    dirs::config_dir()
}

#[cfg(test)]
fn native_config_dir() -> Option<PathBuf> {
    Some(test_config_root())
}

/// Scratch config root for test builds, unique per process and created lazily.
#[cfg(test)]
fn test_config_root() -> PathBuf {
    static ROOT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    ROOT.get_or_init(|| {
        prune_stale_test_roots(&env::temp_dir(), std::time::Duration::from_secs(3600));
        let root = env::temp_dir().join(format!("pulsedeck-test-config-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        root
    })
    .clone()
}

/// Remove scratch roots left behind by earlier test runs (best effort). Only
/// directories named exactly `pulsedeck-test-config-<digits>` that have not
/// been modified for `max_age` are removed.
#[cfg(test)]
fn prune_stale_test_roots(dir: &Path, max_age: std::time::Duration) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(suffix) = name
            .to_str()
            .and_then(|n| n.strip_prefix("pulsedeck-test-config-"))
        else {
            continue;
        };
        let old_enough = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .is_some_and(|age| age >= max_age);
        if !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit()) && old_enough {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

pub(crate) fn resolve_config_dir(bases: &[PathBuf]) -> Option<PathBuf> {
    bases
        .iter()
        .map(|base| base.join(NEW_CONFIG_DIR))
        .find(|dir| dir.exists())
        .or_else(|| native_config_dir().map(|dir| dir.join(NEW_CONFIG_DIR)))
}

pub fn config_dir() -> Option<PathBuf> {
    resolve_config_dir(&candidate_base_dirs())
}

/// Path of `file`: the first base that already holds it, else the preferred
/// config directory (so a stray empty `~/.config/pulsedeck` cannot shadow a
/// real config elsewhere).
fn resolve_config_path(bases: &[PathBuf], file: &str) -> Option<PathBuf> {
    existing_config_file(bases, file)
        .or_else(|| resolve_config_dir(bases).map(|dir| dir.join(file)))
}

/// First base that already holds `file` under the PulseDeck config directory.
/// Read-only: unlike `config_path` it never migrates legacy files.
pub(crate) fn existing_config_file(bases: &[PathBuf], file: &str) -> Option<PathBuf> {
    bases
        .iter()
        .map(|base| path_for(base, NEW_CONFIG_DIR, file))
        .find(|path| path.exists())
}

/// Bases that still hold a legacy `driftfm` config directory.
pub(crate) fn legacy_config_dirs(bases: &[PathBuf]) -> Vec<PathBuf> {
    bases
        .iter()
        .map(|base| base.join(OLD_CONFIG_DIR))
        .filter(|dir| dir.exists())
        .collect()
}

pub fn config_path(file: &str) -> Option<PathBuf> {
    migrate_legacy(file);
    resolve_config_path(&candidate_base_dirs(), file)
}

pub fn migrate_legacy(file: &str) {
    migrate_legacy_in(&candidate_base_dirs(), file);
}

fn migrate_legacy_in(bases: &[PathBuf], file: &str) {
    if bases
        .iter()
        .any(|base| path_for(base, NEW_CONFIG_DIR, file).exists())
    {
        return;
    }

    for base in bases {
        let old_path = path_for(base, OLD_CONFIG_DIR, file);

        if !old_path.exists() {
            continue;
        }

        let new_path = path_for(base, NEW_CONFIG_DIR, file);

        if let Some(parent) = new_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let _ = fs::copy(old_path, new_path);
        return;
    }
}

pub fn load_json_from_path_with_warning<T: DeserializeOwned + Default>(
    path: &Path,
    display_name: &str,
) -> (T, Option<String>) {
    if !path.exists() {
        return (T::default(), None);
    }

    match fs::read_to_string(path) {
        Ok(contents) => {
            let (value, warning) = parse_json_with_warning(display_name, &contents);
            if warning.is_some() {
                let _ = crate::persistence::preserve_corrupt_copy(path);
            }
            (value, warning)
        }
        Err(err) => (
            T::default(),
            Some(format!(
                "Could not read {display_name}; using defaults: {err}"
            )),
        ),
    }
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

    #[test]
    fn absolute_dir_ignores_empty_and_relative_values() {
        use std::ffi::OsString;

        assert_eq!(absolute_dir(None), None);
        assert_eq!(absolute_dir(Some(OsString::new())), None);
        assert_eq!(absolute_dir(Some("relative/dir".into())), None);
        let abs = std::env::temp_dir();
        assert_eq!(absolute_dir(Some(abs.clone().into())), Some(abs));
    }

    #[test]
    fn base_dirs_prefers_xdg_style_dirs_when_enabled_and_dedupes() {
        let xdg = Some(PathBuf::from("/x"));
        let home = Some(PathBuf::from("/h"));
        let native = Some(PathBuf::from("/h/Library/Application Support"));

        assert_eq!(
            base_dirs(xdg.clone(), home.clone(), native.clone(), true),
            vec![
                PathBuf::from("/x"),
                PathBuf::from("/h/.config"),
                PathBuf::from("/h/Library/Application Support"),
            ]
        );
        assert_eq!(
            base_dirs(
                Some(PathBuf::from("/h/.config")),
                home,
                Some(PathBuf::from("/h/.config")),
                true
            ),
            vec![PathBuf::from("/h/.config")]
        );
        assert_eq!(base_dirs(xdg, None, native.clone(), true).len(), 2);
    }

    #[test]
    fn base_dirs_uses_only_native_dir_when_xdg_style_disabled() {
        let native = PathBuf::from("/home/u/.config");

        assert_eq!(
            base_dirs(
                Some(PathBuf::from("/x")),
                Some(PathBuf::from("/h")),
                Some(native.clone()),
                false
            ),
            vec![native]
        );
    }

    #[test]
    fn resolve_config_dir_picks_first_existing_candidate() {
        let root = unique_temp_path("resolve").parent().unwrap().to_path_buf();
        let _ = fs::remove_dir_all(&root);
        let (first, second) = (root.join("a"), root.join("b"));
        fs::create_dir_all(second.join(NEW_CONFIG_DIR)).unwrap();

        assert_eq!(
            resolve_config_dir(&[first.clone(), second.clone()]),
            Some(second.join(NEW_CONFIG_DIR))
        );

        fs::create_dir_all(first.join(NEW_CONFIG_DIR)).unwrap();
        assert_eq!(
            resolve_config_dir(&[first.clone(), second]),
            Some(first.join(NEW_CONFIG_DIR))
        );
    }

    #[test]
    fn migrate_legacy_copies_within_the_base_that_holds_the_old_file() {
        let root = unique_temp_path("migrate").parent().unwrap().to_path_buf();
        let _ = fs::remove_dir_all(&root);
        let (first, second) = (root.join("a"), root.join("b"));
        let old = path_for(&second, OLD_CONFIG_DIR, "state.json");
        fs::create_dir_all(old.parent().unwrap()).unwrap();
        fs::write(&old, "{\"value\":3}").unwrap();

        migrate_legacy_in(&[first.clone(), second.clone()], "state.json");

        assert!(!path_for(&first, NEW_CONFIG_DIR, "state.json").exists());
        assert_eq!(
            fs::read_to_string(path_for(&second, NEW_CONFIG_DIR, "state.json")).unwrap(),
            "{\"value\":3}"
        );
    }

    #[test]
    fn migrate_legacy_skips_when_new_file_exists_in_any_base() {
        let root = unique_temp_path("migrate-skip")
            .parent()
            .unwrap()
            .to_path_buf();
        let _ = fs::remove_dir_all(&root);
        let (first, second) = (root.join("a"), root.join("b"));
        for (base, dir) in [(&first, NEW_CONFIG_DIR), (&second, OLD_CONFIG_DIR)] {
            let path = path_for(base, dir, "state.json");
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, dir).unwrap();
        }

        migrate_legacy_in(&[first, second.clone()], "state.json");

        assert!(!path_for(&second, NEW_CONFIG_DIR, "state.json").exists());
    }

    #[test]
    fn resolve_config_path_prefers_the_base_that_holds_the_file() {
        let root = unique_temp_path("path-file")
            .parent()
            .unwrap()
            .to_path_buf();
        let _ = fs::remove_dir_all(&root);
        let (first, second) = (root.join("a"), root.join("b"));
        fs::create_dir_all(first.join(NEW_CONFIG_DIR)).unwrap();
        let real = path_for(&second, NEW_CONFIG_DIR, "state.json");
        fs::create_dir_all(real.parent().unwrap()).unwrap();
        fs::write(&real, "{}").unwrap();

        // `first` has an empty pulsedeck dir but `second` holds the file.
        assert_eq!(
            resolve_config_path(&[first.clone(), second], "state.json"),
            Some(real)
        );
        // Nobody holds the file: fall back to the first existing config dir.
        assert_eq!(
            resolve_config_path(std::slice::from_ref(&first), "other.json"),
            Some(first.join(NEW_CONFIG_DIR).join("other.json"))
        );
    }

    #[test]
    fn test_builds_resolve_only_to_a_scratch_directory() {
        let scratch = env::temp_dir();
        let bases = candidate_base_dirs();

        assert_eq!(bases.len(), 1);
        assert!(bases[0].starts_with(&scratch), "{bases:?}");

        // Whatever the environment says, nothing resolves to the real config.
        let real = dirs::config_dir();
        for path in [
            config_dir(),
            config_path("ui-state.json"),
            config_path("library.json"),
        ]
        .into_iter()
        .flatten()
        {
            assert!(path.starts_with(&scratch), "{path:?}");
            if let Some(real) = &real {
                assert!(!path.starts_with(real), "{path:?} is inside {real:?}");
            }
        }
    }

    #[test]
    fn test_scratch_root_is_stable_within_a_process() {
        assert_eq!(test_config_root(), test_config_root());
        assert_eq!(candidate_base_dirs(), vec![test_config_root()]);
    }

    #[test]
    fn prune_removes_only_old_scratch_roots() {
        let parent = unique_temp_path("prune").parent().unwrap().to_path_buf();
        let _ = fs::remove_dir_all(&parent);
        let make = |name: &str| {
            let dir = parent.join(name);
            fs::create_dir_all(&dir).unwrap();
            dir
        };
        let scratch = make("pulsedeck-test-config-12345");
        let other_prefix = make("pulsedeck-test-config-abc");
        let unrelated = make("something-else");
        let bare = make("pulsedeck-test-config-");

        // Nothing is old enough yet.
        prune_stale_test_roots(&parent, std::time::Duration::from_secs(3600));
        assert!(scratch.exists());

        // With a zero age limit only the exact scratch naming is removed.
        prune_stale_test_roots(&parent, std::time::Duration::ZERO);
        assert!(!scratch.exists());
        assert!(other_prefix.exists());
        assert!(unrelated.exists());
        assert!(bare.exists());
        let _ = fs::remove_dir_all(&parent);
    }
}
