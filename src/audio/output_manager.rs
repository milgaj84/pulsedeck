use rodio::{OutputStream, OutputStreamHandle, Sink};

use super::output::{normalize_output_device_name, open_output_stream, open_output_stream_strict};
use super::types::{DecodedSource, EngineError};

/// Encapsulates cpal/rodio device selection, sink lifecycle, and recovery.
pub(super) struct OutputManager {
    stream: Option<OutputStream>,
    handle: Option<OutputStreamHandle>,
    sink: Option<Sink>,
    preferred_device: Option<String>,
    recovery_retries: u8,
}

impl OutputManager {
    pub(super) fn new() -> Self {
        Self {
            stream: None,
            handle: None,
            sink: None,
            preferred_device: None,
            recovery_retries: 0,
        }
    }

    pub(super) fn ensure_open(&mut self) -> Result<&OutputStreamHandle, EngineError> {
        if self.handle.is_none() {
            let selection = open_output_stream(self.preferred_device.as_deref())
                .map_err(EngineError::Output)?;
            self.stream = Some(selection.stream);
            self.handle = Some(selection.handle);
        }

        self.handle
            .as_ref()
            .ok_or_else(|| EngineError::Output("output device handle unavailable".to_string()))
    }

    /// Switch to a user-selected output device transactionally.
    ///
    /// The candidate stream is opened before any current sink, handle, stream,
    /// or preferred-device value is changed. A failed switch therefore leaves
    /// healthy playback and the active preference untouched.
    pub(super) fn switch_device(
        &mut self,
        requested: Option<String>,
    ) -> Result<Option<String>, EngineError> {
        let normalized = normalize_output_device_name(requested.as_deref());

        if normalized == self.preferred_device && self.handle.is_some() {
            return Ok(normalized);
        }

        let selection =
            open_output_stream_strict(normalized.as_deref()).map_err(EngineError::Output)?;

        self.sink = None;
        self.handle = Some(selection.handle);
        self.stream = Some(selection.stream);
        self.preferred_device = normalized.clone();
        self.recovery_retries = 0;

        Ok(normalized)
    }

    /// Reopen the current preference for automatic recovery.
    ///
    /// Startup/recovery remains lenient and may fall back to the default device
    /// if a previously saved named device disappeared.
    pub(super) fn reopen(&mut self) -> Result<(), EngineError> {
        self.recovery_retries = self.recovery_retries.saturating_add(1);

        let selection =
            open_output_stream(self.preferred_device.as_deref()).map_err(EngineError::Output)?;

        self.sink = None;
        self.handle = Some(selection.handle);
        self.stream = Some(selection.stream);
        self.recovery_retries = 0;
        Ok(())
    }

    pub(super) fn attach(&mut self, source: DecodedSource) -> Result<(), EngineError> {
        self.ensure_open()?;
        let handle = self.handle.as_ref().ok_or_else(|| {
            EngineError::Output("output handle lost after ensure_open".to_string())
        })?;

        let sink = Sink::try_new(handle)
            .map_err(|error| EngineError::Output(format!("failed to create sink: {error}")))?;
        sink.append(source);
        self.sink = Some(sink);
        Ok(())
    }

    pub(super) fn pause(&mut self) {
        if let Some(sink) = &self.sink {
            sink.pause();
        }
    }

    pub(super) fn resume(&mut self) {
        if let Some(sink) = &self.sink {
            sink.play();
        }
    }

    pub(super) fn stop(&mut self) {
        self.sink = None;
    }

    pub(super) fn is_sink_drained(&self) -> bool {
        self.sink.as_ref().is_some_and(Sink::empty)
    }

    pub(super) fn recovery_retries(&self) -> u8 {
        self.recovery_retries
    }

    pub(super) fn preferred_device(&self) -> Option<&str> {
        self.preferred_device.as_deref()
    }

    pub(super) fn apply_volume_ramp(&mut self, ramp: &mut super::volume::VolumeRamp) {
        if let Some(sink) = &self.sink {
            ramp.tick(sink);
        }
    }

    #[cfg(test)]
    pub(super) fn set_recovery_retries(&mut self, retries: u8) {
        self.recovery_retries = retries;
    }

    #[cfg(test)]
    pub(super) fn set_preferred_device_for_test(&mut self, value: Option<String>) {
        self.preferred_device = normalize_output_device_name(value.as_deref());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_starts_without_resources_or_preference() {
        let manager = OutputManager::new();
        assert!(manager.stream.is_none());
        assert!(manager.handle.is_none());
        assert!(manager.sink.is_none());
        assert!(manager.preferred_device().is_none());
        assert_eq!(manager.recovery_retries(), 0);
    }

    #[test]
    fn test_preference_helper_normalizes_default_and_blank() {
        let mut manager = OutputManager::new();
        manager.set_preferred_device_for_test(Some(" Default ".to_string()));
        assert!(manager.preferred_device().is_none());

        manager.set_preferred_device_for_test(Some("   ".to_string()));
        assert!(manager.preferred_device().is_none());

        manager.set_preferred_device_for_test(Some(" Headphones ".to_string()));
        assert_eq!(manager.preferred_device(), Some("Headphones"));
    }

    #[test]
    #[ignore] // Requires audio hardware; crashes on headless CI (macOS/Windows)
    fn failed_strict_switch_preserves_previous_preference() {
        let mut manager = OutputManager::new();
        manager.set_preferred_device_for_test(Some("Existing Device".to_string()));

        let result = manager.switch_device(Some(
            "__pulsedeck_device_that_should_not_exist__".to_string(),
        ));

        assert!(matches!(result, Err(EngineError::Output(_))));
        assert_eq!(manager.preferred_device(), Some("Existing Device"));
    }

    #[test]
    fn no_sink_operations_are_safe() {
        let mut manager = OutputManager::new();
        manager.pause();
        manager.resume();
        manager.stop();
        assert!(!manager.is_sink_drained());
    }

    #[test]
    fn recovery_retry_test_hook_is_exact() {
        let mut manager = OutputManager::new();
        manager.set_recovery_retries(4);
        assert_eq!(manager.recovery_retries(), 4);
    }

    #[test]
    #[ignore]
    fn default_device_switch_opens_output_when_hardware_is_available() {
        let mut manager = OutputManager::new();
        let result = manager.switch_device(None);
        assert!(result.is_ok());
        assert!(manager.handle.is_some());
    }

    #[test]
    #[ignore]
    fn attach_creates_sink_when_hardware_is_available() {
        let mut manager = OutputManager::new();
        let source: DecodedSource = Box::new(rodio::source::SineWave::new(440.0));
        manager.attach(source).unwrap();
        assert!(manager.sink.is_some());
    }
}
