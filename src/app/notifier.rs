use std::process::{Command, Stdio};

const APP_NOTIFICATION_TITLE: &str = "PulseDeck - Now Playing";

pub(super) fn notify_now_playing(title: &str, station_name: &str) {
    let body = format!("♫ {title}\nStation: {station_name}");
    let native_result = notify_rust::Notification::new()
        .summary(APP_NOTIFICATION_TITLE)
        .body(&body)
        .icon("audio-card")
        .timeout(4000)
        .show();

    if native_result.is_err() && is_wsl() {
        let _ = spawn_windows_balloon(APP_NOTIFICATION_TITLE, &body);
    }
}

fn is_wsl() -> bool {
    std::fs::read_to_string("/proc/sys/kernel/osrelease")
        .map(|release| is_wsl_osrelease(&release))
        .unwrap_or(false)
}

fn is_wsl_osrelease(release: &str) -> bool {
    let release = release.to_ascii_lowercase();
    release.contains("microsoft") || release.contains("wsl")
}

fn spawn_windows_balloon(summary: &str, body: &str) -> std::io::Result<()> {
    let script = windows_balloon_script(summary, body);
    Command::new("powershell.exe")
        .arg("-NoProfile")
        .arg("-NonInteractive")
        .arg("-STA")
        .arg("-WindowStyle")
        .arg("Hidden")
        .arg("-Command")
        .arg(script)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
}

fn windows_balloon_script(summary: &str, body: &str) -> String {
    let summary = powershell_single_quote(summary);
    let body = powershell_single_quote(body);
    format!(
        "Add-Type -AssemblyName System.Windows.Forms; \
         Add-Type -AssemblyName System.Drawing; \
         $n = New-Object System.Windows.Forms.NotifyIcon; \
         $n.Icon = [System.Drawing.SystemIcons]::Information; \
         $n.Visible = $true; \
         $n.ShowBalloonTip(4000, {summary}, {body}, [System.Windows.Forms.ToolTipIcon]::Info); \
         Start-Sleep -Milliseconds 4500; \
         $n.Dispose();"
    )
}

fn powershell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_wsl_osrelease_variants() {
        assert!(is_wsl_osrelease("5.15.153.1-microsoft-standard-WSL2"));
        assert!(is_wsl_osrelease("6.6.87.2-microsoft-standard"));
        assert!(!is_wsl_osrelease("6.8.0-31-generic"));
    }

    #[test]
    fn powershell_single_quote_escapes_embedded_quotes() {
        assert_eq!(powershell_single_quote("PulseDeck"), "'PulseDeck'");
        assert_eq!(powershell_single_quote("Bob's Station"), "'Bob''s Station'");
    }

    #[test]
    fn windows_balloon_script_contains_escaped_content() {
        let script = windows_balloon_script("PulseDeck", "Bob's Track");

        assert!(script.contains("System.Windows.Forms.NotifyIcon"));
        assert!(script.contains("'PulseDeck'"));
        assert!(script.contains("'Bob''s Track'"));
    }
}
