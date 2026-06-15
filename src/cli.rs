use crate::favorites::Library;
use crate::playlist::{self, PlaylistFormat};
use anyhow::{anyhow, Context};
use std::fs;

pub enum CliOutcome {
    RunTui,
    Handled,
}

fn print_help() {
    println!("PulseDeck CLI - Move your station library between machines");
    println!();
    println!("Usage:");
    println!("  pulsedeck export <path>    Export station library to M3U or JSON");
    println!("  pulsedeck import <path>    Import and merge stations from M3U or JSON");
    println!("  pulsedeck -h, --help       Show this help message");
    println!("  pulsedeck -V, --version    Show version information");
}

pub fn run<I: Iterator<Item = String>>(mut args: I) -> anyhow::Result<CliOutcome> {
    let _bin = args.next();
    match args.next().as_deref() {
        Some("export") => {
            let Some(path) = args.next() else {
                return Err(anyhow!(
                    "Missing output path for export. Usage: pulsedeck export <path>"
                ));
            };
            let library = Library::load_existing();
            let format = playlist::format_for_path(&path);
            let content = match format {
                PlaylistFormat::M3u => playlist::to_m3u(&library.stations),
                PlaylistFormat::Json => playlist::to_json(&library.stations)
                    .context("Failed to serialize library to JSON")?,
            };
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
            let summary = library
                .import_stations(stations)
                .context("Failed to import stations into library")?;
            println!(
                "Import completed: added {} new stations, skipped {} duplicates.",
                summary.added, summary.skipped
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
        _ => Ok(CliOutcome::RunTui),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_cli_unknown_arg() {
        let args = vec!["pulsedeck".to_string(), "invalid_flag".to_string()];
        let res = run(args.into_iter()).unwrap();
        assert!(matches!(res, CliOutcome::RunTui));
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
        let temp_dir = std::env::temp_dir().join(format!(
            "pulsedeck_cli_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
        ));
        let _ = std::fs::create_dir_all(&temp_dir);
        let export_path = temp_dir.join("export.m3u").to_str().unwrap().to_string();

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
}
