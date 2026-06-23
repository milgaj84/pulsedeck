use crate::favorites::{ImportMode, ImportPreview, Library};
use crate::playlist::{self, PlaylistFormat};
use anyhow::{anyhow, Context};
use std::fs;
use std::path::Path;

#[derive(Debug)]
pub enum CliOutcome {
    RunTui,
    Handled,
}

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
    println!("  pulsedeck -h, --help       Show this help message");
    println!("  pulsedeck -V, --version    Show version information");
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
}
