use cpal::traits::{DeviceTrait, HostTrait};
use rodio::{OutputStream, OutputStreamHandle};

pub const DEFAULT_OUTPUT_DEVICE_LABEL: &str = "Default";

pub(super) struct OutputStreamSelection {
    pub(super) stream: OutputStream,
    pub(super) handle: OutputStreamHandle,
}

/// List usable output device names reported by the host audio backend.
///
/// This is best-effort because CI and headless machines often expose no output
/// devices. The settings UI still offers `Default` even when this list is empty.
pub fn list_output_device_names() -> Vec<String> {
    let host = cpal::default_host();
    let Ok(devices) = host.output_devices() else {
        return Vec::new();
    };

    output_device_names_from_iter(devices.filter_map(|device| device.name().ok()))
}

pub fn output_device_display_name(value: Option<&str>) -> String {
    normalize_output_device_name(value).unwrap_or_else(|| DEFAULT_OUTPUT_DEVICE_LABEL.to_string())
}

pub(super) fn open_output_stream(
    preferred_name: Option<&str>,
) -> Result<OutputStreamSelection, String> {
    let preferred_name = normalize_output_device_name(preferred_name);

    if let Some(name) = preferred_name.as_deref() {
        if let Some(device) = find_output_device(name)? {
            return OutputStream::try_from_device(&device)
                .map(|(stream, handle)| OutputStreamSelection { stream, handle })
                .map_err(|err| format!("could not open selected output device '{name}': {err}"));
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

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(
            output_device_display_name(None),
            DEFAULT_OUTPUT_DEVICE_LABEL
        );
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
}
