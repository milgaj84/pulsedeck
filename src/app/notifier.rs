#[cfg(not(test))]
use std::process::{Command, Stdio};

/// Abstraction for desktop notification dispatch.
/// Enables test isolation without `#[cfg(not(test))]` guards.
pub trait Notifier: Send {
    fn notify_now_playing(&self, title: &str, station_name: &str);

    /// Returns the number of notifications dispatched. Used for test assertions.
    #[allow(dead_code)]
    fn notification_count(&self) -> u32 {
        0
    }
}

/// Production notifier using notify-rust on Linux and PowerShell toast on WSL.
#[cfg(not(test))]
pub struct DesktopNotifier;

#[cfg(not(test))]
const APP_NOTIFICATION_TITLE: &str = "PulseDeck - Now Playing";

#[cfg(not(test))]
impl Notifier for DesktopNotifier {
    fn notify_now_playing(&self, title: &str, station_name: &str) {
        if is_wsl() {
            let _ = spawn_windows_toast(APP_NOTIFICATION_TITLE, title, station_name);
            return;
        }

        let body = format!("♫ {title}\nStation: {station_name}");
        let mut notification = notify_rust::Notification::new();
        notification
            .summary(APP_NOTIFICATION_TITLE)
            .body(&body)
            .icon("audio-card")
            .timeout(4000);

        #[cfg(target_os = "linux")]
        {
            notification.hint(notify_rust::Hint::SuppressSound(true));
        }

        let _ = notification.show();
    }
}

/// Test-only notifier that counts dispatch calls.
/// Enables assertions on notification count without OS side effects.
#[cfg(test)]
pub(crate) struct CountingNotifier {
    pub count: std::cell::Cell<u32>,
}

#[cfg(test)]
impl CountingNotifier {
    pub fn new() -> Self {
        Self {
            count: std::cell::Cell::new(0),
        }
    }
}

#[cfg(test)]
impl Notifier for CountingNotifier {
    fn notify_now_playing(&self, _title: &str, _station_name: &str) {
        self.count.set(self.count.get() + 1);
    }

    fn notification_count(&self) -> u32 {
        self.count.get()
    }
}

#[cfg(not(test))]
fn is_wsl() -> bool {
    std::fs::read_to_string("/proc/sys/kernel/osrelease")
        .map(|release| is_wsl_osrelease(&release))
        .unwrap_or(false)
}

fn is_wsl_osrelease(release: &str) -> bool {
    let release = release.to_ascii_lowercase();
    release.contains("microsoft") || release.contains("wsl")
}

#[cfg(not(test))]
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
    let app_id =
        "{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\\\\WindowsPowerShell\\\\v1.0\\\\powershell.exe";
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
         $toast.Tag = 'PulseDeckNowPlaying'; \
         $toast.Group = 'PulseDeck'; \
         $notifier = [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('{app_id}'); \
         try {{ $notifier.Hide($toast) }} catch {{}}; \
         $notifier.Show($toast)"
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
        assert_eq!(
            xml_escape("Bob's <Station> & \"More\""),
            "Bob&apos;s &lt;Station&gt; &amp; &quot;More&quot;"
        );
    }

    #[test]
    fn windows_toast_script_contains_escaped_content() {
        let script = windows_toast_script("PulseDeck", "Bob's Track", "Radio <FM>");

        assert!(script.contains("ToastNotificationManager"));
        assert!(script.contains("PulseDeck"));
        assert!(script.contains("Bob&apos;s Track"));
        assert!(script.contains("Radio &lt;FM&gt;"));
        assert!(script.contains("$toast.Tag = 'PulseDeckNowPlaying'"));
        assert!(script.contains("$toast.Group = 'PulseDeck'"));
    }
}
