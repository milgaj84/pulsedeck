use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Linux,
    Macos,
    Windows,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
}

pub fn move_to_trash(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Err(format!("{} does not exist", path.display()));
    }

    let specs = trash_command_specs(path, current_platform());
    let mut errors = Vec::new();

    for spec in specs {
        match Command::new(&spec.program).args(&spec.args).status() {
            Ok(status) if status.success() => return Ok(()),
            Ok(status) => errors.push(format!("{} exited with {}", spec.program, status)),
            Err(err) => errors.push(format!("{} failed: {err}", spec.program)),
        }
    }

    Err(format!(
        "No trash command succeeded for {} ({})",
        path.display(),
        errors.join("; ")
    ))
}

pub fn trash_command_specs(path: &Path, platform: Platform) -> Vec<CommandSpec> {
    let path_text = path.to_string_lossy().to_string();

    match platform {
        Platform::Linux => vec![
            CommandSpec {
                program: "gio".to_string(),
                args: vec!["trash".to_string(), path_text.clone()],
            },
            CommandSpec {
                program: "trash-put".to_string(),
                args: vec![path_text],
            },
        ],
        Platform::Macos => vec![CommandSpec {
            program: "osascript".to_string(),
            args: vec![
                "-e".to_string(),
                format!(
                    "tell application \"Finder\" to delete POSIX file \"{}\"",
                    escape_applescript_string(&path_text)
                ),
            ],
        }],
        Platform::Windows => vec![CommandSpec {
            program: "powershell.exe".to_string(),
            args: vec![
                "-NoProfile".to_string(),
                "-Command".to_string(),
                format!(
                    "Add-Type -AssemblyName Microsoft.VisualBasic; [Microsoft.VisualBasic.FileIO.FileSystem]::DeleteFile('{}', 'OnlyErrorDialogs', 'SendToRecycleBin')",
                    escape_powershell_single_quoted(&path_text)
                ),
            ],
        }],
    }
}

fn current_platform() -> Platform {
    if cfg!(target_os = "macos") {
        Platform::Macos
    } else if cfg!(target_os = "windows") {
        Platform::Windows
    } else {
        Platform::Linux
    }
}

fn escape_applescript_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn escape_powershell_single_quoted(value: &str) -> String {
    value.replace('\'', "''")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn linux_trash_prefers_gio_then_trash_put() {
        let specs = trash_command_specs(Path::new("/tmp/tape.mp3"), Platform::Linux);

        assert_eq!(specs[0].program, "gio");
        assert_eq!(specs[0].args, vec!["trash", "/tmp/tape.mp3"]);
        assert_eq!(specs[1].program, "trash-put");
        assert_eq!(specs[1].args, vec!["/tmp/tape.mp3"]);
    }

    #[test]
    fn macos_trash_uses_finder_delete() {
        let specs = trash_command_specs(Path::new("/tmp/tape.mp3"), Platform::Macos);

        assert_eq!(specs[0].program, "osascript");
        assert!(specs[0].args[1].contains("Finder"));
        assert!(specs[0].args[1].contains("delete POSIX file"));
    }

    #[test]
    fn windows_trash_uses_recycle_bin_delete_file() {
        let specs =
            trash_command_specs(&PathBuf::from(r"C:\recordings\tape.mp3"), Platform::Windows);

        assert_eq!(specs[0].program, "powershell.exe");
        assert!(specs[0].args[2].contains("SendToRecycleBin"));
    }
}
