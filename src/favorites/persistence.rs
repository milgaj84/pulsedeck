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
            Err(err) => {
                load_warnings.push(format!(
                    "Could not parse library.json; using {}: {err}",
                    fallback_label(policy)
                ));
                fallback_for_missing_library(policy)
            }
        },
        Err(err) => {
            load_warnings.push(format!(
                "Could not read library.json; using {}: {err}",
                fallback_label(policy)
            ));
            fallback_for_missing_library(policy)
        }
    }
}

fn fallback_for_missing_library(policy: &MissingLibraryPolicy) -> (Vec<Station>, Settings, bool) {
    match policy {
        MissingLibraryPolicy::SeedAndSave(stations) => {
            (stations.clone(), Settings::default(), true)
        }
        MissingLibraryPolicy::Empty => (Vec::new(), Settings::default(), false),
    }
}

fn fallback_label(policy: &MissingLibraryPolicy) -> &'static str {
    match policy {
        MissingLibraryPolicy::SeedAndSave(_) => "starter stations",
        MissingLibraryPolicy::Empty => "empty library",
    }
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
    let warning = if file.version == 1 {
        None
    } else {
        Some(format!(
            "Library file version {} is newer than supported version 1",
            file.version
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
