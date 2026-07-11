/// Config file path: ~/.config/pulsedeck/library.json
///
/// If an existing DriftFM config exists and no PulseDeck config has been written yet,
/// copy the old file into the new config directory so users keep their library.
fn config_path() -> Option<PathBuf> {
    crate::config::config_path(LIBRARY_FILE)
}

fn read_library_file_or_fallback(
    path: &Path,
    policy: &MissingLibraryPolicy,
    load_warnings: &mut Vec<String>,
) -> (Vec<Station>, Settings, bool) {
    match fs::read_to_string(path) {
        Ok(contents) => match parse_library_file(&contents) {
            Ok((stations, settings, warning)) => {
                if let Some(warning) = warning {
                    load_warnings.push(warning);
                }
                (stations, settings, false)
            }
            Err(err) => recover_backup_or_use_safe_fallback(
                path,
                policy,
                load_warnings,
                format!("Could not parse library.json: {err}"),
            ),
        },
        Err(err) => recover_backup_or_use_safe_fallback(
            path,
            policy,
            load_warnings,
            format!("Could not read library.json: {err}"),
        ),
    }
}

fn recover_backup_or_use_safe_fallback(
    path: &Path,
    policy: &MissingLibraryPolicy,
    load_warnings: &mut Vec<String>,
    primary_error: String,
) -> (Vec<Station>, Settings, bool) {
    let backup = crate::persistence::backup_path(path);
    if let Ok(contents) = fs::read_to_string(&backup) {
        if let Ok((stations, settings, backup_warning)) = parse_library_file(&contents) {
            load_warnings.push(format!(
                "{primary_error}; recovered the last known good library from {}. The damaged primary file was not overwritten.",
                backup.display()
            ));
            if let Some(warning) = backup_warning {
                load_warnings.push(warning);
            }
            return (stations, settings, false);
        }
    }

    load_warnings.push(format!(
        "{primary_error}; using a safe in-memory fallback. PulseDeck will not overwrite the existing library automatically."
    ));
    fallback_for_unusable_library(policy)
}

fn fallback_for_missing_library(policy: &MissingLibraryPolicy) -> (Vec<Station>, Settings, bool) {
    match policy {
        MissingLibraryPolicy::SeedAndSave(stations) => {
            (stations.clone(), Settings::default(), true)
        }
        MissingLibraryPolicy::Empty => (Vec::new(), Settings::default(), false),
    }
}

fn fallback_for_unusable_library(
    policy: &MissingLibraryPolicy,
) -> (Vec<Station>, Settings, bool) {
    match policy {
        MissingLibraryPolicy::SeedAndSave(stations) => {
            (stations.clone(), Settings::default(), false)
        }
        MissingLibraryPolicy::Empty => (Vec::new(), Settings::default(), false),
    }
}

fn ensure_library_target_is_safe_to_replace(path: &Path) -> anyhow::Result<()> {
    if !path.exists() {
        return Ok(());
    }

    let contents = fs::read_to_string(path).map_err(|err| {
        anyhow::anyhow!(
            "refusing to replace unreadable library {}: {err}",
            path.display()
        )
    })?;
    let file = serde_json::from_str::<LibraryFile>(&contents).map_err(|err| {
        anyhow::anyhow!(
            "refusing to replace malformed library {}; preserve or repair it first: {err}",
            path.display()
        )
    })?;

    if file.version > default_library_version() {
        anyhow::bail!(
            "refusing to downgrade library {} from version {} to version {}",
            path.display(),
            file.version,
            default_library_version()
        );
    }

    Ok(())
}

fn compact_error_summary(error: &str) -> String {
    crate::text::truncate_with_ellipsis(error.trim(), 96)
}

fn import_skip_reason(station: &Station) -> Option<String> {
    if station.url.trim().is_empty() {
        Some("missing stream URL".to_string())
    } else if station.name.trim().is_empty() {
        Some("missing station name".to_string())
    } else {
        None
    }
}

fn import_skip_name(station: &Station) -> String {
    let name = station.name.trim();
    if name.is_empty() {
        "Unnamed station".to_string()
    } else {
        name.to_string()
    }
}

fn parse_library_file(
    contents: &str,
) -> serde_json::Result<(Vec<Station>, Settings, Option<String>)> {
    let file = serde_json::from_str::<LibraryFile>(contents)?;
    let warning = if file.version == default_library_version() {
        None
    } else if file.version > default_library_version() {
        Some(format!(
            "Library file version {} is newer than supported version {}; saving is disabled to prevent data loss",
            file.version,
            default_library_version()
        ))
    } else {
        Some(format!(
            "Library file version {} is older than supported version {}",
            file.version,
            default_library_version()
        ))
    };
    Ok((
        file.stations
            .into_iter()
            .map(normalize_loaded_station)
            .collect(),
        file.settings,
        warning,
    ))
}

#[cfg(test)]
mod persistence_safety_tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    struct TestDir(PathBuf);

    impl TestDir {
        fn new(name: &str) -> Self {
            let sequence = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "pulsedeck-library-safety-{name}-{}-{sequence}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn seed_station() -> Station {
        Station::basic("Seed", "https://seed.invalid", "Radio", "BA", 128)
    }

    fn valid_library(name: &str, version: u32) -> String {
        format!(
            r#"{{"version":{version},"stations":[{{"name":"{name}","url":"https://example.invalid","genre":"Radio","country":"BA","bitrate":128}}],"settings":{{}}}}"#
        )
    }

    #[test]
    fn corrupt_primary_uses_valid_backup_without_requesting_seed_save() {
        let dir = TestDir::new("backup-recovery");
        let target = dir.0.join(LIBRARY_FILE);
        fs::write(&target, "{broken").unwrap();
        fs::write(
            crate::persistence::backup_path(&target),
            valid_library("Recovered", 1),
        )
        .unwrap();
        let mut warnings = Vec::new();

        let (stations, _, should_save) = read_library_file_or_fallback(
            &target,
            &MissingLibraryPolicy::SeedAndSave(vec![seed_station()]),
            &mut warnings,
        );

        assert_eq!(stations.len(), 1);
        assert_eq!(stations[0].name, "Recovered");
        assert!(!should_save);
        assert!(warnings.iter().any(|warning| warning.contains("recovered")));
        assert_eq!(fs::read_to_string(&target).unwrap(), "{broken");
    }

    #[test]
    fn corrupt_primary_without_backup_never_requests_seed_save() {
        let dir = TestDir::new("no-backup");
        let target = dir.0.join(LIBRARY_FILE);
        fs::write(&target, "{broken").unwrap();
        let mut warnings = Vec::new();

        let (stations, _, should_save) = read_library_file_or_fallback(
            &target,
            &MissingLibraryPolicy::SeedAndSave(vec![seed_station()]),
            &mut warnings,
        );

        assert_eq!(stations.len(), 1);
        assert_eq!(stations[0].name, "Seed");
        assert!(!should_save);
        assert_eq!(fs::read_to_string(&target).unwrap(), "{broken");
    }

    #[test]
    fn malformed_library_is_never_safe_to_replace() {
        let dir = TestDir::new("malformed-preflight");
        let target = dir.0.join(LIBRARY_FILE);
        fs::write(&target, "{broken").unwrap();

        let error = ensure_library_target_is_safe_to_replace(&target).unwrap_err();

        assert!(error.to_string().contains("refusing to replace malformed"));
    }

    #[test]
    fn future_library_version_is_never_safe_to_downgrade() {
        let dir = TestDir::new("future-version");
        let target = dir.0.join(LIBRARY_FILE);
        fs::write(&target, valid_library("Future", 2)).unwrap();

        let error = ensure_library_target_is_safe_to_replace(&target).unwrap_err();

        assert!(error.to_string().contains("refusing to downgrade"));
    }
}