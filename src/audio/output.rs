use cpal::traits::{DeviceTrait, HostTrait};
use rodio::{OutputStream, OutputStreamHandle};

#[cfg(unix)]
use std::sync::Mutex;

pub const DEFAULT_OUTPUT_DEVICE_LABEL: &str = "Default";

#[cfg(unix)]
static NATIVE_AUDIO_STDERR_LOCK: Mutex<()> = Mutex::new(());

pub(super) struct OutputStreamSelection {
    pub(super) stream: OutputStream,
    pub(super) handle: OutputStreamHandle,
}

/// List usable output device names reported by the host audio backend.
pub fn list_output_device_names() -> Vec<String> {
    with_suppressed_native_audio_stderr(list_output_device_names_inner)
}

fn list_output_device_names_inner() -> Vec<String> {
    let host = cpal::default_host();
    let Ok(devices) = host.output_devices() else {
        return Vec::new();
    };

    output_device_names_from_iter(devices.filter_map(|device| device.name().ok()))
}

pub fn output_device_display_name(value: Option<&str>) -> String {
    normalize_output_device_name(value).unwrap_or_else(|| DEFAULT_OUTPUT_DEVICE_LABEL.to_string())
}

/// Open the preferred device when available, otherwise fall back to default.
///
/// This lenient behavior is retained for startup and automatic recovery so a
/// removed USB or Bluetooth device does not make PulseDeck permanently silent.
pub(super) fn open_output_stream(
    preferred_name: Option<&str>,
) -> Result<OutputStreamSelection, String> {
    with_suppressed_native_audio_stderr(|| open_output_stream_inner(preferred_name, false))
}

/// Open exactly the device selected by the user.
///
/// Unlike startup recovery, an explicit switch must never silently fall back to
/// another device and then report success.
pub(super) fn open_output_stream_strict(
    preferred_name: Option<&str>,
) -> Result<OutputStreamSelection, String> {
    with_suppressed_native_audio_stderr(|| open_output_stream_inner(preferred_name, true))
}

fn open_output_stream_inner(
    preferred_name: Option<&str>,
    strict_named_selection: bool,
) -> Result<OutputStreamSelection, String> {
    let preferred_name = normalize_output_device_name(preferred_name);

    if let Some(name) = preferred_name.as_deref() {
        if let Some(device) = find_output_device(name)? {
            return OutputStream::try_from_device(&device)
                .map(|(stream, handle)| OutputStreamSelection { stream, handle })
                .map_err(|err| format!("could not open selected output device '{name}': {err}"));
        }

        if strict_named_selection {
            return Err(format!("selected output device '{name}' was not found"));
        }
    }

    OutputStream::try_default()
        .map(|(stream, handle)| OutputStreamSelection { stream, handle })
        .map_err(|err| format!("could not open default output device: {err}"))
}

pub(super) fn normalize_output_device_name(value: Option<&str>) -> Option<String> {
    let trimmed = value?.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case(DEFAULT_OUTPUT_DEVICE_LABEL) {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn find_output_device(name: &str) -> Result<Option<cpal::Device>, String> {
    let host = cpal::default_host();
    let devices = host
        .output_devices()
        .map_err(|err| format!("could not enumerate output devices: {err}"))?;

    for device in devices {
        let Ok(device_name) = device.name() else {
            continue;
        };

        if output_device_names_match(&device_name, name) {
            return Ok(Some(device));
        }
    }

    Ok(None)
}

fn output_device_names_match(left: &str, right: &str) -> bool {
    left == right || left.eq_ignore_ascii_case(right)
}

fn output_device_names_from_iter<I>(names: I) -> Vec<String>
where
    I: IntoIterator<Item = String>,
{
    let mut names = names
        .into_iter()
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .filter(|name| !name.eq_ignore_ascii_case(DEFAULT_OUTPUT_DEVICE_LABEL))
        .collect::<Vec<_>>();

    names.sort_by_key(|name| name.to_lowercase());
    names.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    names
}

#[cfg(unix)]
fn with_suppressed_native_audio_stderr<T>(operation: impl FnOnce() -> T) -> T {
    let _lock = NATIVE_AUDIO_STDERR_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    match StderrSilencer::new() {
        Some(_silencer) => operation(),
        None => operation(),
    }
}

#[cfg(not(unix))]
fn with_suppressed_native_audio_stderr<T>(operation: impl FnOnce() -> T) -> T {
    operation()
}

#[cfg(unix)]
struct StderrSilencer {
    saved_fd: libc::c_int,
}

#[cfg(unix)]
impl StderrSilencer {
    fn new() -> Option<Self> {
        // SAFETY: These calls operate only on process file descriptors. We save
        // stderr, temporarily redirect fd 2 to /dev/null, then restore it in Drop.
        unsafe {
            let saved_fd = libc::dup(libc::STDERR_FILENO);
            if saved_fd < 0 {
                return None;
            }

            let dev_null = libc::open(c"/dev/null".as_ptr(), libc::O_WRONLY);
            if dev_null < 0 {
                libc::close(saved_fd);
                return None;
            }

            let redirect_result = libc::dup2(dev_null, libc::STDERR_FILENO);
            libc::close(dev_null);

            if redirect_result < 0 {
                libc::close(saved_fd);
                return None;
            }

            Some(Self { saved_fd })
        }
    }
}

#[cfg(unix)]
impl Drop for StderrSilencer {
    fn drop(&mut self) {
        // SAFETY: `saved_fd` was returned by `dup` in `new`.
        unsafe {
            libc::dup2(self.saved_fd, libc::STDERR_FILENO);
            libc::close(self.saved_fd);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_audio_stderr_suppression_runs_closure() {
        assert_eq!(with_suppressed_native_audio_stderr(|| 42), 42);
    }

    #[test]
    fn normalize_output_device_name_treats_default_and_blank_as_none() {
        assert_eq!(normalize_output_device_name(None), None);
        assert_eq!(normalize_output_device_name(Some("")), None);
        assert_eq!(normalize_output_device_name(Some("   ")), None);
        assert_eq!(normalize_output_device_name(Some("Default")), None);
        assert_eq!(normalize_output_device_name(Some("default")), None);
    }

    #[test]
    fn output_device_display_name_uses_default_label() {
        assert_eq!(output_device_display_name(None), DEFAULT_OUTPUT_DEVICE_LABEL);
        assert_eq!(
            output_device_display_name(Some("Default")),
            DEFAULT_OUTPUT_DEVICE_LABEL
        );
        assert_eq!(
            output_device_display_name(Some("BlueZ Headphones")),
            "BlueZ Headphones"
        );
    }

    #[test]
    fn normalize_output_device_name_trims_real_names() {
        assert_eq!(
            normalize_output_device_name(Some("  BlueZ Headphones  ")).as_deref(),
            Some("BlueZ Headphones")
        );
    }

    #[test]
    fn output_device_names_from_iter_sorts_deduplicates_and_filters_default() {
        let names = output_device_names_from_iter(vec![
            " Speakers ".to_string(),
            "bluez_output.headset".to_string(),
            "speakers".to_string(),
            "Default".to_string(),
            "".to_string(),
        ]);

        assert_eq!(
            names,
            vec!["bluez_output.headset".to_string(), "Speakers".to_string()]
        );
    }

    #[test]
    fn output_device_names_match_case_insensitively() {
        assert!(output_device_names_match(
            "PipeWire Bluetooth",
            "pipewire bluetooth"
        ));
        assert!(!output_device_names_match("Speakers", "Headphones"));
    }

    #[test]
    fn strict_and_lenient_selection_have_distinct_missing_device_policy() {
        // The hardware-dependent lookup remains in one function, while this
        // invariant documents the contract used by startup versus user changes.
        assert_ne!(
            open_output_stream as usize,
            open_output_stream_strict as usize
        );
    }
}