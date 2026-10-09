//! `pulsedeck doctor`: headless diagnostics for bug reports.
//!
//! The report is built from plain data (`DoctorInputs`) by pure functions, so
//! it is testable without a terminal, real config directory or audio hardware.

use super::CliOutcome;
use crate::app::audio_check::check_audio_devices;
use crate::favorites::{Library, LibrarySummary};
use anyhow::anyhow;
use std::path::{Path, PathBuf};

const FILES: [&str; 4] = [
    "pulsedeck.toml",
    "library.json",
    "ui-state.json",
    "keybindings.json",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Status {
    Ok,
    Warn,
    Fail,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Check {
    pub status: Status,
    pub name: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Section {
    pub title: &'static str,
    pub checks: Vec<Check>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DoctorReport {
    pub sections: Vec<Section>,
}

impl DoctorReport {
    fn count(&self, status: Status) -> usize {
        self.sections
            .iter()
            .flat_map(|section| &section.checks)
            .filter(|check| check.status == status)
            .count()
    }

    pub(super) fn has_failures(&self) -> bool {
        self.count(Status::Fail) > 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum KeybindingsState {
    /// No custom keybindings file; defaults are used.
    NotFound,
    Valid,
    Warnings(Vec<String>),
    Unreadable(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DoctorInputs {
    pub version: &'static str,
    /// Candidate config base directories, in lookup order.
    pub bases: Vec<PathBuf>,
    /// Config directory PulseDeck would use (existing or preferred).
    pub config_dir: Option<PathBuf>,
    /// For each known file, the path that holds it (if any).
    pub files: Vec<(&'static str, Option<PathBuf>)>,
    pub legacy_dirs: Vec<PathBuf>,
    /// Warnings from loading `pulsedeck.toml`; empty when absent or valid.
    pub config_warnings: Vec<String>,
    pub keybindings: KeybindingsState,
    pub audio_devices: Vec<String>,
    pub configured_device: Option<String>,
    pub library: Option<LibrarySummary>,
    /// `None` when the network check was not requested.
    pub network: Option<Vec<(String, Result<(), String>)>>,
}

fn check(status: Status, name: impl Into<String>, detail: impl Into<String>) -> Check {
    Check {
        status,
        name: name.into(),
        detail: detail.into(),
    }
}

pub(super) fn build_report(inputs: &DoctorInputs) -> DoctorReport {
    DoctorReport {
        sections: vec![
            paths_section(inputs),
            config_section(inputs),
            audio_section(inputs),
            library_section(inputs),
            network_section(inputs),
        ]
        .into_iter()
        .flatten()
        .collect(),
    }
}

fn paths_section(inputs: &DoctorInputs) -> Option<Section> {
    let mut checks = vec![check(Status::Ok, "version", inputs.version)];

    checks.push(match &inputs.config_dir {
        Some(dir) => check(Status::Ok, "config directory", dir.display().to_string()),
        None => check(
            Status::Fail,
            "config directory",
            "could not be determined for this platform",
        ),
    });

    for base in &inputs.bases {
        checks.push(check(Status::Ok, "searched", base.display().to_string()));
    }

    for dir in &inputs.legacy_dirs {
        checks.push(check(
            Status::Warn,
            "legacy data",
            format!(
                "{} (old driftfm config; copied to pulsedeck on next launch)",
                dir.display()
            ),
        ));
    }

    Some(Section {
        title: "Paths",
        checks,
    })
}

fn config_section(inputs: &DoctorInputs) -> Option<Section> {
    let mut checks = Vec::new();

    for (name, found) in &inputs.files {
        checks.push(match found {
            Some(path) => check(Status::Ok, *name, path.display().to_string()),
            None if *name == "pulsedeck.toml" => check(
                Status::Warn,
                *name,
                "not found; defaults in use (run `pulsedeck config init`)",
            ),
            None => check(Status::Ok, *name, "not present; defaults in use"),
        });
    }

    for warning in &inputs.config_warnings {
        let status = if warning.contains("Could not parse") {
            Status::Fail
        } else {
            Status::Warn
        };
        checks.push(check(status, "pulsedeck.toml", warning.clone()));
    }

    match &inputs.keybindings {
        KeybindingsState::NotFound => {
            checks.push(check(Status::Ok, "keybindings", "default bindings"))
        }
        KeybindingsState::Valid => {
            checks.push(check(Status::Ok, "keybindings", "custom file is valid"))
        }
        KeybindingsState::Warnings(warnings) => {
            for warning in warnings {
                checks.push(check(Status::Fail, "keybindings", warning.clone()));
            }
        }
        KeybindingsState::Unreadable(err) => {
            checks.push(check(Status::Fail, "keybindings", err.clone()))
        }
    }

    Some(Section {
        title: "Configuration",
        checks,
    })
}

fn audio_section(inputs: &DoctorInputs) -> Option<Section> {
    let result = check_audio_devices(&inputs.audio_devices);
    let status = if inputs.audio_devices.is_empty() {
        Status::Fail
    } else {
        Status::Ok
    };
    let mut checks = vec![check(
        status,
        "output devices",
        format!("{} ({} found)", result.label(), inputs.audio_devices.len()),
    )];

    for device in &inputs.audio_devices {
        checks.push(check(Status::Ok, "device", device.clone()));
    }

    let configured = crate::audio::output_device_display_name(inputs.configured_device.as_deref());
    let configured_known = inputs.configured_device.is_none()
        || inputs
            .audio_devices
            .iter()
            .any(|device| device.eq_ignore_ascii_case(&configured));
    checks.push(if configured_known {
        check(Status::Ok, "configured output", configured)
    } else {
        check(
            Status::Warn,
            "configured output",
            format!("{configured} is not connected; playback falls back to the default device"),
        )
    });

    Some(Section {
        title: "Audio",
        checks,
    })
}

fn library_section(inputs: &DoctorInputs) -> Option<Section> {
    let summary = inputs.library.as_ref()?;
    let mut checks = vec![check(
        Status::Ok,
        "library",
        format!(
            "{} stations, {} favorites",
            summary.stations, summary.favorites
        ),
    )];
    for warning in &summary.warnings {
        checks.push(check(Status::Warn, "library", warning.clone()));
    }
    Some(Section {
        title: "Library",
        checks,
    })
}

fn network_section(inputs: &DoctorInputs) -> Option<Section> {
    let results = inputs.network.as_ref()?;
    let checks = results
        .iter()
        .map(|(server, result)| match result {
            Ok(()) => check(Status::Ok, "radio-browser", format!("{server} reachable")),
            Err(err) => check(
                Status::Fail,
                "radio-browser",
                format!("{server} unreachable: {err}"),
            ),
        })
        .collect();
    Some(Section {
        title: "Network",
        checks,
    })
}

pub(super) fn format_report(report: &DoctorReport) -> String {
    let mut out = String::new();
    for section in &report.sections {
        out.push_str(&format!("{}\n", section.title));
        for check in &section.checks {
            let tag = match check.status {
                Status::Ok => "ok",
                Status::Warn => "warn",
                Status::Fail => "FAIL",
            };
            out.push_str(&format!("  [{tag:>4}] {}: {}\n", check.name, check.detail));
        }
        out.push('\n');
    }
    out.push_str(&format!(
        "Summary: {} ok, {} warnings, {} failures\n",
        report.count(Status::Ok),
        report.count(Status::Warn),
        report.count(Status::Fail)
    ));
    out
}

/// Parse `doctor` options. Returns whether the network check was requested.
pub(super) fn parse_doctor_args(args: impl Iterator<Item = String>) -> anyhow::Result<bool> {
    let mut network = false;
    for arg in args {
        match arg.as_str() {
            "--network" => network = true,
            other => {
                return Err(anyhow!(
                    "Unknown doctor option: {other}. Usage: pulsedeck doctor [--network]"
                ));
            }
        }
    }
    Ok(network)
}

/// Gather everything the report needs. Read-only: never migrates, seeds or
/// writes files. Audio devices and the network result are passed in so tests
/// do not touch hardware or the network.
pub(super) fn collect_inputs(
    bases: Vec<PathBuf>,
    audio_devices: Vec<String>,
    network: Option<Vec<(String, Result<(), String>)>>,
) -> DoctorInputs {
    let config_dir = crate::config::resolve_config_dir(&bases);
    let files: Vec<(&'static str, Option<PathBuf>)> = FILES
        .iter()
        .map(|name| (*name, crate::config::existing_config_file(&bases, name)))
        .collect();
    let found = |name: &str| {
        files
            .iter()
            .find(|(file, _)| *file == name)
            .and_then(|(_, path)| path.clone())
    };

    let loaded = found("pulsedeck.toml")
        .as_deref()
        .and_then(Path::parent)
        .map(crate::config_toml::io::load_config);
    let (config_warnings, configured_device) = match loaded {
        Some(result) => (result.warnings, result.config.audio.output_device),
        None => (Vec::new(), None),
    };

    let keybindings = match found("keybindings.json") {
        None => KeybindingsState::NotFound,
        Some(path) => match super::validate_keybindings_file(&path) {
            Ok(warnings) if warnings.is_empty() => KeybindingsState::Valid,
            Ok(warnings) => KeybindingsState::Warnings(warnings),
            Err(err) => KeybindingsState::Unreadable(err.to_string()),
        },
    };

    let library = found("library.json").map(|path| Library::summarize_file(&path));

    DoctorInputs {
        version: env!("CARGO_PKG_VERSION"),
        legacy_dirs: crate::config::legacy_config_dirs(&bases),
        bases,
        config_dir,
        files,
        config_warnings,
        keybindings,
        audio_devices,
        configured_device,
        library,
        network,
    }
}

/// Probe each Radio Browser server. Runs on its own thread because the
/// blocking HTTP client must not be created inside the tokio runtime.
fn check_radio_browser() -> Vec<(String, Result<(), String>)> {
    let handle = std::thread::spawn(|| {
        let client = reqwest::blocking::Client::builder()
            .user_agent(format!("PulseDeck/{}", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(5))
            .build();
        crate::radio::RADIO_BROWSER_HTTPS_SERVERS
            .iter()
            .map(|server| {
                let result = match &client {
                    Ok(client) => client
                        .get(format!("{server}/json/stats"))
                        .send()
                        .and_then(|response| response.error_for_status())
                        .map(|_| ())
                        .map_err(|err| err.to_string()),
                    Err(err) => Err(err.to_string()),
                };
                (server.to_string(), result)
            })
            .collect()
    });
    handle
        .join()
        .unwrap_or_else(|_| vec![("radio-browser".to_string(), Err("check panicked".into()))])
}

pub(super) fn handle_doctor(args: impl Iterator<Item = String>) -> anyhow::Result<CliOutcome> {
    let network = parse_doctor_args(args)?;
    let inputs = collect_inputs(
        crate::config::candidate_base_dirs(),
        crate::audio::list_output_device_names(),
        network.then(check_radio_browser),
    );
    let report = build_report(&inputs);
    print!("{}", format_report(&report));
    if report.has_failures() {
        std::process::exit(1);
    }
    Ok(CliOutcome::Handled)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn base_inputs() -> DoctorInputs {
        DoctorInputs {
            version: "9.9.9",
            bases: vec![PathBuf::from("/cfg")],
            config_dir: Some(PathBuf::from("/cfg/pulsedeck")),
            files: vec![
                (
                    "pulsedeck.toml",
                    Some(PathBuf::from("/cfg/pulsedeck/pulsedeck.toml")),
                ),
                (
                    "library.json",
                    Some(PathBuf::from("/cfg/pulsedeck/library.json")),
                ),
                ("ui-state.json", None),
                ("keybindings.json", None),
            ],
            legacy_dirs: vec![],
            config_warnings: vec![],
            keybindings: KeybindingsState::NotFound,
            audio_devices: vec!["Speakers".to_string()],
            configured_device: None,
            library: Some(LibrarySummary {
                stations: 12,
                favorites: 3,
                warnings: vec![],
            }),
            network: None,
        }
    }

    fn find<'a>(report: &'a DoctorReport, name: &str) -> Vec<&'a Check> {
        report
            .sections
            .iter()
            .flat_map(|section| &section.checks)
            .filter(|check| check.name == name)
            .collect()
    }

    fn unique_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "pulsedeck-doctor-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    /// Sorted (name, length) pairs: detects created, removed or rewritten files.
    fn dir_listing(dir: &Path) -> Vec<(String, u64)> {
        let mut entries: Vec<_> = fs::read_dir(dir)
            .unwrap()
            .map(|entry| {
                let entry = entry.unwrap();
                (
                    entry.file_name().to_string_lossy().into_owned(),
                    entry.metadata().unwrap().len(),
                )
            })
            .collect();
        entries.sort();
        entries
    }

    #[test]
    fn healthy_inputs_produce_no_failures_or_warnings() {
        let report = build_report(&base_inputs());

        assert!(!report.has_failures());
        assert_eq!(report.count(Status::Warn), 0);
        let text = format_report(&report);
        assert!(text.contains("version: 9.9.9"));
        assert!(text.contains("12 stations, 3 favorites"));
        assert!(text.contains("Summary:"));
    }

    #[test]
    fn missing_toml_is_a_warning_with_a_hint() {
        let mut inputs = base_inputs();
        inputs.files[0].1 = None;

        let report = build_report(&inputs);
        let toml = find(&report, "pulsedeck.toml");

        assert_eq!(toml[0].status, Status::Warn);
        assert!(toml[0].detail.contains("pulsedeck config init"));
        assert!(!report.has_failures());
    }

    #[test]
    fn unparseable_toml_fails_but_other_warnings_do_not() {
        let mut inputs = base_inputs();
        inputs.config_warnings = vec![
            "Could not parse pulsedeck.toml: bad".to_string(),
            "Unknown key foo".to_string(),
        ];

        let report = build_report(&inputs);
        let toml = find(&report, "pulsedeck.toml");

        assert_eq!(toml.iter().filter(|c| c.status == Status::Fail).count(), 1);
        assert_eq!(toml.iter().filter(|c| c.status == Status::Warn).count(), 1);
        assert!(report.has_failures());
    }

    #[test]
    fn no_audio_device_fails() {
        let mut inputs = base_inputs();
        inputs.audio_devices.clear();

        let report = build_report(&inputs);

        assert_eq!(find(&report, "output devices")[0].status, Status::Fail);
        assert!(format_report(&report).contains("No output device found"));
        assert!(report.has_failures());
    }

    #[test]
    fn disconnected_configured_device_warns() {
        let mut inputs = base_inputs();
        inputs.configured_device = Some("Gone DAC".to_string());

        let report = build_report(&inputs);
        let configured = find(&report, "configured output");

        assert_eq!(configured[0].status, Status::Warn);
        assert!(configured[0].detail.contains("Gone DAC"));
        assert!(!report.has_failures());
    }

    #[test]
    fn connected_configured_device_is_ok() {
        let mut inputs = base_inputs();
        inputs.configured_device = Some("speakers".to_string());

        let report = build_report(&inputs);

        assert_eq!(find(&report, "configured output")[0].status, Status::Ok);
    }

    #[test]
    fn invalid_keybindings_fail_and_defaults_do_not() {
        let mut inputs = base_inputs();
        inputs.keybindings = KeybindingsState::Warnings(vec!["bad key".to_string()]);
        assert!(build_report(&inputs).has_failures());

        inputs.keybindings = KeybindingsState::Unreadable("denied".to_string());
        assert!(build_report(&inputs).has_failures());

        inputs.keybindings = KeybindingsState::Valid;
        assert!(!build_report(&inputs).has_failures());
    }

    #[test]
    fn legacy_dirs_are_listed_as_warnings() {
        let mut inputs = base_inputs();
        inputs.legacy_dirs = vec![PathBuf::from("/cfg/driftfm")];

        let report = build_report(&inputs);

        assert_eq!(find(&report, "legacy data")[0].status, Status::Warn);
    }

    #[test]
    fn library_warnings_are_reported_without_failing() {
        let mut inputs = base_inputs();
        inputs.library = Some(LibrarySummary {
            stations: 0,
            favorites: 0,
            warnings: vec!["Could not parse library.json: x; recovered".to_string()],
        });

        let report = build_report(&inputs);

        assert_eq!(find(&report, "library")[1].status, Status::Warn);
        assert!(!report.has_failures());
    }

    #[test]
    fn network_section_only_appears_when_requested() {
        let mut inputs = base_inputs();
        assert!(!format_report(&build_report(&inputs)).contains("Network"));

        inputs.network = Some(vec![
            ("https://a".to_string(), Ok(())),
            ("https://b".to_string(), Err("timeout".to_string())),
        ]);
        let report = build_report(&inputs);

        let checks = find(&report, "radio-browser");
        assert_eq!(checks[0].status, Status::Ok);
        assert_eq!(checks[1].status, Status::Fail);
        assert!(checks[1].detail.contains("timeout"));
        assert!(report.has_failures());
    }

    #[test]
    fn parse_doctor_args_accepts_network_and_rejects_unknown() {
        assert!(!parse_doctor_args(std::iter::empty()).unwrap());
        assert!(parse_doctor_args(["--network".to_string()].into_iter()).unwrap());
        let err = parse_doctor_args(["--bogus".to_string()].into_iter()).unwrap_err();
        assert!(err.to_string().contains("Unknown doctor option: --bogus"));
    }

    #[test]
    fn collect_inputs_reads_files_and_never_writes() {
        let root = unique_dir("collect");
        let cfg = root.join("pulsedeck");
        fs::create_dir_all(&cfg).unwrap();
        fs::write(
            cfg.join("pulsedeck.toml"),
            "[audio]\noutput_device = \"USB DAC\"\n",
        )
        .unwrap();
        fs::write(
            cfg.join("library.json"),
            r#"{"version":1,"stations":[{"name":"A","url":"https://a.invalid","genre":"Radio","country":"BA","bitrate":128}],"settings":{}}"#,
        )
        .unwrap();
        fs::write(cfg.join("keybindings.json"), "{not json").unwrap();
        let before = dir_listing(&cfg);

        let inputs = collect_inputs(vec![root.clone()], vec!["USB DAC".to_string()], None);

        assert_eq!(inputs.config_dir, Some(cfg.clone()));
        assert_eq!(inputs.configured_device.as_deref(), Some("USB DAC"));
        assert_eq!(inputs.library.as_ref().map(|l| l.stations), Some(1));
        assert!(inputs
            .files
            .iter()
            .all(|(name, path)| { (*name == "ui-state.json") == path.is_none() }));
        assert!(!matches!(inputs.keybindings, KeybindingsState::Valid));
        assert_eq!(dir_listing(&cfg), before);
    }

    #[test]
    fn collect_inputs_on_an_empty_config_reports_defaults_and_creates_nothing() {
        let root = unique_dir("empty");
        fs::create_dir_all(&root).unwrap();

        let inputs = collect_inputs(vec![root.clone()], vec![], None);

        assert!(inputs.files.iter().all(|(_, path)| path.is_none()));
        assert!(inputs.library.is_none());
        assert_eq!(inputs.keybindings, KeybindingsState::NotFound);
        assert_eq!(fs::read_dir(&root).unwrap().count(), 0);
    }

    #[test]
    fn collect_inputs_lists_legacy_dirs_without_migrating() {
        let root = unique_dir("legacy");
        fs::create_dir_all(root.join("driftfm")).unwrap();
        fs::write(root.join("driftfm").join("library.json"), "{}").unwrap();

        let inputs = collect_inputs(vec![root.clone()], vec![], None);

        assert_eq!(inputs.legacy_dirs, vec![root.join("driftfm")]);
        assert!(!root.join("pulsedeck").exists());
    }
}
