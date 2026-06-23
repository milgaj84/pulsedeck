use std::process::{Command, Stdio};

const APP_NOTIFICATION_TITLE: &str = "PulseDeck - Now Playing";

pub(super) fn notify_now_playing(title: &str, station_name: &str) {
    if is_wsl() {
        // On WSL, D-Bus often accepts notifications into a black hole (no visible
        // daemon), causing them to appear minutes late or never. Skip native and
        // go straight to Windows toast notifications.
        let _ = spawn_windows_toast(APP_NOTIFICATION_TITLE, title, station_name);
        return;
    }

    let body = format!("♫ {title}\nStation: {station_name}");
    let _ = notify_rust::Notification::new()
        .summary(APP_NOTIFICATION_TITLE)
        .body(&body)
        .icon("audio-card")
        .timeout(4000)
        .hint(notify_rust::Hint::SuppressSound(true))
        .show();
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

fn spawn_windows_toast(summary: &str, title: &str, station: &str) -> std::io::Result<()> {
    let script = windows_toast_script(summary, title, station);
    Command::new("powershell.exe")
        .arg("-NoProfile")
        .arg("-NonInteractive")
        .arg("-Command")
        .arg(script)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
}

fn windows_toast_script(summary: &str, title: &str, station: &str) -> String {
    let summary = xml_escape(summary);
    let title = xml_escape(title);
    let station = xml_escape(station);
    // Use PowerShell's registered AppUserModelID so Windows shows the toast
    // without requiring PulseDeck to have its own Start Menu shortcut/AUMID.
    format!(
        "[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] | Out-Null; \
         [Windows.Data.Xml.Dom.XmlDocument, Windows.Data.Xml.Dom, ContentType = WindowsRuntime] | Out-Null; \
         $xml = New-Object Windows.Data.Xml.Dom.XmlDocument; \
         $xml.LoadXml('<toast><visual><binding template=\"ToastGeneric\">\
         <text>{summary}</text>\
         <text>♫ {title}</text>\
         <text>Station: {station}</text>\
         </binding></visual><audio silent=\"true\"/></toast>'); \
         $toast = [Windows.UI.Notifications.ToastNotification]::new($xml); \
         [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('{{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}}\\WindowsPowerShell\\v1.0\\powershell.exe').Show($toast)"
    )
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
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
    fn xml_escape_handles_special_chars() {
        assert_eq!(xml_escape("Bob's <Station> & \"More\""), "Bob&apos;s &lt;Station&gt; &amp; &quot;More&quot;");
    }

    #[test]
    fn windows_toast_script_contains_escaped_content() {
        let script = windows_toast_script("PulseDeck", "Bob's Track", "Radio <FM>");

        assert!(script.contains("ToastNotificationManager"));
        assert!(script.contains("PulseDeck"));
        assert!(script.contains("Bob&apos;s Track"));
        assert!(script.contains("Radio &lt;FM&gt;"));
    }
}
