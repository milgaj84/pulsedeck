use crate::favorites::{ImportMode, ImportPreview, Library};
use crate::keybindings::KeybindingRegistry;
use crate::playlist::{self, PlaylistFormat};
use anyhow::{anyhow, Context};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum CliOutcome {
    RunTui,
    Handled,
}

pub const CONFIG_TEMPLATE: &str = r#"# PulseDeck Configuration File
# Edit this file to customize your settings.
# Changes to theme, volume, and playback settings are hot-reloaded.
# Keybinding changes require a restart.

[audio]
# output_device = "Built-in Speakers"  # Uncomment to set a specific device
default_volume = 80  # 0–100

[ui]
theme = "Retrowave"  # Retrowave | Catppuccin Mocha | Catppuccin Macchiato | Catppuccin Frappé | Catppuccin Latte | Terminal
notifications_enabled = true
stream_metadata_enabled = true

[playback]
autoplay_last = false
save_history = false

[keybindings]
# path = "keybindings.json"  # Uncomment to load custom key mappings
"#;

fn print_help() {
    println!("PulseDeck CLI - Move your station library between machines");
    println!();
    println!("Usage:");
    println!("  pulsedeck export <path>                  Export station library to M3U or JSON");
    println!(
        "  pulsedeck import <path>                  Import and merge stations from M3U or JSON"
    );
    println!("  pulsedeck import <path> --preview        Preview import changes without saving");
    println!("  pulsedeck import <path> --enrich-only    Refresh matching stations without adding new ones");
    println!("  pulsedeck config init                    Generate default configuration file");
    println!("  pulsedeck keybindings validate [path]    Validate keybindings file for errors");
    println!("  pulsedeck -h, --help       Show this help message");
    println!("  pulsedeck -V, --version    Show version information");
}

fn resolve_config_dir() -> anyhow::Result<PathBuf> {
    crate::config::config_dir()
        .ok_or_else(|| anyhow!("Could not determine config directory for this platform"))
}

fn write_config_init(config_dir: &Path) -> anyhow::Result<CliOutcome> {
    let config_path = config_dir.join("pulsedeck.toml");

    if config_path.exists() {
        println!("Config file already exists: {}", config_path.display());
        return Ok(CliOutcome::Handled);
    }

    fs::create_dir_all(config_dir)
        .with_context(|| format!("Failed to create config directory {}", config_dir.display()))?;

    fs::write(&config_path, CONFIG_TEMPLATE)
        .with_context(|| format!("Failed to write config file to {}", config_path.display()))?;

    println!("Created config file: {}", config_path.display());
    Ok(CliOutcome::Handled)
}

fn handle_config_init() -> anyhow::Result<CliOutcome> {
    let config_dir = resolve_config_dir()?;
    write_config_init(&config_dir)
}

fn resolve_keybindings_path(path: Option<&str>) -> PathBuf {
    match path {
        Some(p) => PathBuf::from(p),
        None => crate::config::config_path("keybindings.json")
            .unwrap_or_else(|| PathBuf::from("keybindings.json")),
    }
}

fn validate_keybindings_file(file_path: &Path) -> anyhow::Result<Vec<String>> {
    if !file_path.exists() {
        return Err(anyhow!(
            "Keybindings file not found: {}",
            file_path.display()
        ));
    }

    let content = fs::read(file_path)
        .with_context(|| format!("Failed to read keybindings file: {}", file_path.display()))?;

    let mut warnings = Vec::new();
    let _ = KeybindingRegistry::from_json(&content, &mut warnings);
    Ok(warnings)
}

fn handle_keybindings_validate(path: Option<&str>) -> anyhow::Result<CliOutcome> {
    let file_path = resolve_keybindings_path(path);
    let warnings = validate_keybindings_file(&file_path)?;

    for warning in &warnings {
        eprintln!("{warning}");
    }

    if warnings.is_empty() {
        println!("✓ keybindings.json is valid");
        Ok(CliOutcome::Handled)
    } else {
        std::process::exit(1);
    }
}

fn ensure_export_parent(path: &str) -> anyhow::Result<()> {
    let path = Path::new(path);
    let Some(parent) = path.parent() else {
        return Ok(());
    };

    if parent.as_os_str().is_empty() {
        return Ok(());
    }

    fs::create_dir_all(parent)
        .with_context(|| format!("Failed to create export directory {}", parent.display()))
}

fn print_load_warnings(library: &Library) {
    for warning in &library.load_warnings {
        eprintln!("Warning: {warning}");
    }
}

fn print_import_preview(preview: &ImportPreview) {
    if preview.is_empty() {
        println!("Import preview: no valid station changes found.");
        return;
    }

    println!(
        "Import preview: {} new, {} enrichments, {} duplicates, {} skipped.",
        preview.new_stations.len(),
        preview.enrichments.len(),
        preview.duplicates.len(),
        preview.skipped.len()
    );
    for skip in &preview.skipped {
        println!("Skipped: {} ({})", skip.name, skip.reason);
    }
}

fn parse_import_options(args: impl Iterator<Item = String>) -> anyhow::Result<(bool, ImportMode)> {
    let mut preview_only = false;
    let mut mode = ImportMode::All;

    for arg in args {
        match arg.as_str() {
            "--preview" => preview_only = true,
            "--enrich-only" => mode = ImportMode::EnrichExistingOnly,
            other => {
                return Err(anyhow!(
                    "Unknown import option: {other}. Usage: pulsedeck import <path> [--preview|--enrich-only]"
                ));
            }
        }
    }

    Ok((preview_only, mode))
}

pub fn run<I: Iterator<Item = String>>(mut args: I) -> anyhow::Result<CliOutcome> {
    let _bin = args.next();
    match args.next().as_deref() {
        None => Ok(CliOutcome::RunTui),
        Some("export") => {
            let Some(path) = args.next() else {
                return Err(anyhow!(
                    "Missing output path for export. Usage: pulsedeck export <path>"
                ));
            };
            let library = Library::load_existing();
            print_load_warnings(&library);
            let format = playlist::format_for_path(&path);
            let content = match format {
                PlaylistFormat::M3u => playlist::to_m3u(&library.stations),
                PlaylistFormat::Json => playlist::to_json(&library.stations)
                    .context("Failed to serialize library to JSON")?,
            };
            ensure_export_parent(&path)?;
            fs::write(&path, content)
                .with_context(|| format!("Failed to write export file to {}", path))?;
            println!("Exported {} stations to {}", library.stations.len(), path);
            Ok(CliOutcome::Handled)
        }
        Some("import") => {
            let Some(path) = args.next() else {
                return Err(anyhow!(
                    "Missing input path for import. Usage: pulsedeck import <path>"
                ));
            };
            let (preview_only, mode) = parse_import_options(args)?;
            let content = fs::read_to_string(&path)
                .with_context(|| format!("Failed to read import file from {}", path))?;
            let format = playlist::format_for_path(&path);
            let stations = match format {
                PlaylistFormat::M3u => playlist::from_m3u(&content),
                PlaylistFormat::Json => {
                    playlist::from_json(&content).context("Failed to parse JSON playlist")?
                }
            };
            let mut library = Library::load_existing();
            print_load_warnings(&library);
            let summary = if preview_only || mode == ImportMode::EnrichExistingOnly {
                let preview = library.preview_import(stations);
                if preview_only {
                    print_import_preview(&preview);
                    return Ok(CliOutcome::Handled);
                }
                library
                    .apply_import_preview(preview, mode)
                    .context("Failed to import stations into library")?
            } else {
                library
                    .import_stations(stations)
                    .context("Failed to import stations into library")?
            };
            library
                .save()
                .context("Failed to save imported stations into library")?;
            println!(
                "Import completed: added {} new stations, enriched {}, skipped {}.",
                summary.added, summary.enriched, summary.skipped
            );
            Ok(CliOutcome::Handled)
        }
        Some("config") => match args.next().as_deref() {
            Some("init") => handle_config_init(),
            Some(sub) => Err(anyhow!(
                "Unknown config subcommand: {sub}. Usage: pulsedeck config init"
            )),
            None => Err(anyhow!(
                "Missing config subcommand. Usage: pulsedeck config init"
            )),
        },
        Some("keybindings") => match args.next().as_deref() {
            Some("validate") => handle_keybindings_validate(args.next().as_deref()),
            Some(sub) => Err(anyhow!(
                "Unknown keybindings subcommand: {sub}. Usage: pulsedeck keybindings validate [path]"
            )),
            None => Err(anyhow!(
                "Missing keybindings subcommand. Usage: pulsedeck keybindings validate [path]"
            )),
        },
        Some("--version" | "-V") => {
            println!("pulsedeck {}", env!("CARGO_PKG_VERSION"));
            Ok(CliOutcome::Handled)
        }
        Some("--help" | "-h") => {
            print_help();
            Ok(CliOutcome::Handled)
        }
        Some(command) => Err(anyhow!(
            "Unknown command: {command}. Run `pulsedeck --help` for usage."
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_temp_dir(prefix: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}_{nanos}"))
    }

    #[test]
    fn test_cli_help_and_version() {
        let args = vec!["pulsedeck".to_string(), "--help".to_string()];
        let res = run(args.into_iter()).unwrap();
        assert!(matches!(res, CliOutcome::Handled));

        let args = vec!["pulsedeck".to_string(), "-V".to_string()];
        let res = run(args.into_iter()).unwrap();
        assert!(matches!(res, CliOutcome::Handled));
    }

    #[test]
    fn test_cli_run_tui_on_empty() {
        let args = vec!["pulsedeck".to_string()];
        let res = run(args.into_iter()).unwrap();
        assert!(matches!(res, CliOutcome::RunTui));
    }

    #[test]
    fn test_cli_unknown_arg_errors() {
        let args = vec!["pulsedeck".to_string(), "invalid_flag".to_string()];
        let err = run(args.into_iter()).unwrap_err();
        assert!(err.to_string().contains("Unknown command: invalid_flag"));
    }

    #[test]
    fn test_cli_missing_paths() {
        let args = vec!["pulsedeck".to_string(), "export".to_string()];
        assert!(run(args.into_iter()).is_err());

        let args = vec!["pulsedeck".to_string(), "import".to_string()];
        assert!(run(args.into_iter()).is_err());
    }

    #[test]
    fn test_cli_export_import_roundtrip() {
        let temp_dir = unique_temp_dir("pulsedeck_cli_test");
        let _ = std::fs::create_dir_all(&temp_dir);
        let export_path = temp_dir.join("export.m3u").to_string_lossy().to_string();

        // Let's run export
        let args = vec![
            "pulsedeck".to_string(),
            "export".to_string(),
            export_path.clone(),
        ];
        let res = run(args.into_iter()).unwrap();
        assert!(matches!(res, CliOutcome::Handled));
        assert!(std::path::Path::new(&export_path).exists());

        // Let's run import
        let args = vec!["pulsedeck".to_string(), "import".to_string(), export_path];
        let res = run(args.into_iter()).unwrap();
        assert!(matches!(res, CliOutcome::Handled));

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_cli_export_creates_parent_directories() {
        let temp_dir = unique_temp_dir("pulsedeck_cli_nested_export");
        let export_path = temp_dir
            .join("nested")
            .join("backup")
            .join("library.m3u")
            .to_string_lossy()
            .to_string();

        let args = vec![
            "pulsedeck".to_string(),
            "export".to_string(),
            export_path.clone(),
        ];

        let res = run(args.into_iter()).unwrap();
        assert!(matches!(res, CliOutcome::Handled));
        assert!(std::path::Path::new(&export_path).exists());

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_config_init_writes_default_file() {
        let temp_dir = unique_temp_dir("pulsedeck_config_init");
        let res = write_config_init(&temp_dir).unwrap();
        assert!(matches!(res, CliOutcome::Handled));

        let config_path = temp_dir.join("pulsedeck.toml");
        assert!(config_path.exists());

        let content = std::fs::read_to_string(&config_path).unwrap();
        assert_eq!(content, CONFIG_TEMPLATE);

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_config_init_skips_existing_file() {
        let temp_dir = unique_temp_dir("pulsedeck_config_init_exists");
        std::fs::create_dir_all(&temp_dir).unwrap();
        let config_path = temp_dir.join("pulsedeck.toml");
        std::fs::write(&config_path, "existing content").unwrap();

        let res = write_config_init(&temp_dir).unwrap();
        assert!(matches!(res, CliOutcome::Handled));

        let content = std::fs::read_to_string(&config_path).unwrap();
        assert_eq!(content, "existing content");

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_config_template_has_no_scrobble_section() {
        assert!(!CONFIG_TEMPLATE.contains("[scrobble]"));
    }

    #[test]
    fn test_config_template_contains_all_sections() {
        assert!(CONFIG_TEMPLATE.contains("[audio]"));
        assert!(CONFIG_TEMPLATE.contains("[ui]"));
        assert!(CONFIG_TEMPLATE.contains("[playback]"));
        assert!(CONFIG_TEMPLATE.contains("[keybindings]"));
    }

    #[test]
    fn test_config_template_contains_comments() {
        assert!(CONFIG_TEMPLATE.contains("# PulseDeck Configuration File"));
        assert!(CONFIG_TEMPLATE.contains("# Changes to theme"));
    }

    #[test]
    fn test_config_init_creates_directories() {
        let temp_dir = unique_temp_dir("pulsedeck_config_init_nested");
        let nested_dir = temp_dir.join("deep").join("nested");

        let res = write_config_init(&nested_dir).unwrap();
        assert!(matches!(res, CliOutcome::Handled));
        assert!(nested_dir.join("pulsedeck.toml").exists());

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_cli_config_init_missing_subcommand() {
        let args = vec!["pulsedeck".to_string(), "config".to_string()];
        let err = run(args.into_iter()).unwrap_err();
        assert!(err.to_string().contains("Missing config subcommand"));
    }

    #[test]
    fn test_cli_config_unknown_subcommand() {
        let args = vec![
            "pulsedeck".to_string(),
            "config".to_string(),
            "unknown".to_string(),
        ];
        let err = run(args.into_iter()).unwrap_err();
        assert!(err
            .to_string()
            .contains("Unknown config subcommand: unknown"));
    }

    #[test]
    fn test_keybindings_validate_valid_file_returns_no_warnings() {
        let temp_dir = unique_temp_dir("pulsedeck_kb_valid");
        std::fs::create_dir_all(&temp_dir).unwrap();
        let kb_path = temp_dir.join("keybindings.json");

        let valid_json = serde_json::to_vec(&serde_json::json!([
            {"key": "char(q)", "modifiers": [], "action": "quit", "mode": "Normal"},
            {"key": "enter", "modifiers": [], "action": "play_selected"}
        ]))
        .unwrap();
        std::fs::write(&kb_path, &valid_json).unwrap();

        let warnings = validate_keybindings_file(&kb_path).unwrap();
        assert!(warnings.is_empty());

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_keybindings_validate_invalid_entries_returns_warnings() {
        let temp_dir = unique_temp_dir("pulsedeck_kb_invalid");
        std::fs::create_dir_all(&temp_dir).unwrap();
        let kb_path = temp_dir.join("keybindings.json");

        let invalid_json = serde_json::to_vec(&serde_json::json!([
            {"key": "badkey", "modifiers": [], "action": "quit"},
            {"key": "enter", "modifiers": [], "action": "nonexistent_action"}
        ]))
        .unwrap();
        std::fs::write(&kb_path, &invalid_json).unwrap();

        let warnings = validate_keybindings_file(&kb_path).unwrap();
        assert_eq!(warnings.len(), 2);
        assert!(warnings[0].contains("invalid key"));
        assert!(warnings[1].contains("invalid action"));

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_keybindings_validate_missing_file_returns_error() {
        let path = PathBuf::from("/nonexistent/keybindings.json");
        let err = validate_keybindings_file(&path).unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn test_keybindings_validate_malformed_json_returns_warning() {
        let temp_dir = unique_temp_dir("pulsedeck_kb_malformed");
        std::fs::create_dir_all(&temp_dir).unwrap();
        let kb_path = temp_dir.join("keybindings.json");

        std::fs::write(&kb_path, b"not valid json{{{").unwrap();

        let warnings = validate_keybindings_file(&kb_path).unwrap();
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("Malformed keybindings JSON"));

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_keybindings_validate_via_cli_valid_file() {
        let temp_dir = unique_temp_dir("pulsedeck_kb_cli_valid");
        std::fs::create_dir_all(&temp_dir).unwrap();
        let kb_path = temp_dir.join("keybindings.json");

        let valid_json = serde_json::to_vec(&serde_json::json!([
            {"key": "char(q)", "modifiers": [], "action": "quit"}
        ]))
        .unwrap();
        std::fs::write(&kb_path, &valid_json).unwrap();

        let args = vec![
            "pulsedeck".to_string(),
            "keybindings".to_string(),
            "validate".to_string(),
            kb_path.to_string_lossy().to_string(),
        ];
        let res = run(args.into_iter()).unwrap();
        assert!(matches!(res, CliOutcome::Handled));

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_keybindings_validate_via_cli_missing_file() {
        let args = vec![
            "pulsedeck".to_string(),
            "keybindings".to_string(),
            "validate".to_string(),
            "/nonexistent/keybindings.json".to_string(),
        ];
        let err = run(args.into_iter()).unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn test_cli_keybindings_missing_subcommand() {
        let args = vec!["pulsedeck".to_string(), "keybindings".to_string()];
        let err = run(args.into_iter()).unwrap_err();
        assert!(err.to_string().contains("Missing keybindings subcommand"));
    }

    #[test]
    fn test_cli_keybindings_unknown_subcommand() {
        let args = vec![
            "pulsedeck".to_string(),
            "keybindings".to_string(),
            "unknown".to_string(),
        ];
        let err = run(args.into_iter()).unwrap_err();
        assert!(err
            .to_string()
            .contains("Unknown keybindings subcommand: unknown"));
    }
}
