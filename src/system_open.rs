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

pub fn open_containing_folder(path: &Path) -> Result<(), String> {
    let folder = path
        .parent()
        .ok_or_else(|| format!("{} has no containing folder", path.display()))?;
    let spec = open_folder_command_spec(folder, current_platform());

    let status = Command::new(&spec.program)
        .args(&spec.args)
        .status()
        .map_err(|err| format!("Could not launch {}: {err}", spec.program))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("{} exited with status {}", spec.program, status))
    }
}

pub fn open_folder_command_spec(folder: &Path, platform: Platform) -> CommandSpec {
    let folder = folder.to_string_lossy().to_string();

    match platform {
        Platform::Linux => CommandSpec {
            program: "xdg-open".to_string(),
            args: vec![folder],
        },
        Platform::Macos => CommandSpec {
            program: "open".to_string(),
            args: vec![folder],
        },
        Platform::Windows => CommandSpec {
            program: "explorer.exe".to_string(),
            args: vec![folder],
        },
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn linux_opens_parent_with_xdg_open() {
        let spec =
            open_folder_command_spec(Path::new("/tmp/recordings/Synthwave"), Platform::Linux);
        assert_eq!(spec.program, "xdg-open");
        assert_eq!(spec.args, vec!["/tmp/recordings/Synthwave"]);
    }

    #[test]
    fn macos_opens_parent_with_open() {
        let spec =
            open_folder_command_spec(Path::new("/tmp/recordings/Synthwave"), Platform::Macos);
        assert_eq!(spec.program, "open");
        assert_eq!(spec.args, vec!["/tmp/recordings/Synthwave"]);
    }

    #[test]
    fn windows_opens_parent_with_explorer() {
        let spec = open_folder_command_spec(
            &PathBuf::from(r"C:\recordings\Synthwave"),
            Platform::Windows,
        );
        assert_eq!(spec.program, "explorer.exe");
        assert_eq!(spec.args, vec![r"C:\recordings\Synthwave"]);
    }
}
