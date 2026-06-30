/// Result of the startup audio device check.
#[derive(Debug, Clone, PartialEq)]
pub enum AudioCheckResult {
    /// At least one output device is available.
    DeviceAvailable,
    /// No audio output device was found.
    NoDeviceFound,
    /// A device was found but initialization failed.
    InitFailed(String),
}

/// Check whether audio output devices are available.
/// Accepts a list of device names (from device enumeration).
/// Returns the appropriate check result.
pub fn check_audio_devices(device_names: &[String]) -> AudioCheckResult {
    if device_names.is_empty() {
        AudioCheckResult::NoDeviceFound
    } else {
        AudioCheckResult::DeviceAvailable
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_device_list_returns_no_device() {
        assert_eq!(check_audio_devices(&[]), AudioCheckResult::NoDeviceFound);
    }

    #[test]
    fn test_one_device_returns_available() {
        let devices = vec!["Built-in Speakers".to_string()];
        assert_eq!(
            check_audio_devices(&devices),
            AudioCheckResult::DeviceAvailable
        );
    }

    #[test]
    fn test_multiple_devices_returns_available() {
        let devices = vec![
            "Built-in Speakers".to_string(),
            "HDMI Output".to_string(),
            "Bluetooth Headphones".to_string(),
        ];
        assert_eq!(
            check_audio_devices(&devices),
            AudioCheckResult::DeviceAvailable
        );
    }
}
