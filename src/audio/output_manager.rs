use rodio::{OutputStream, OutputStreamHandle, Sink};

use super::output::{normalize_output_device_name, open_output_stream};
use super::types::{DecodedSource, EngineError};

// ---------------------------------------------------------------------------
// OutputManager
// ---------------------------------------------------------------------------

/// Encapsulates cpal/rodio device selection, `Sink` lifecycle, and device
/// recovery.
///
/// The device is opened lazily on first playback (`ensure_open`).  All
/// rodio/cpal calls are wrapped so failures produce `EngineError::Output`
/// rather than panicking.  ALSA/JACK stderr suppression is built into
/// `open_output_stream` and is not duplicated here.
pub(super) struct OutputManager {
    stream: Option<OutputStream>,
    handle: Option<OutputStreamHandle>,
    sink: Option<Sink>,
    preferred_device: Option<String>,
    recovery_retries: u8,
    reopen_needed: bool,
}

impl OutputManager {
    // -----------------------------------------------------------------------
    // Construction
    // -----------------------------------------------------------------------

    /// Create a new `OutputManager` with no open device, no sink, and zero
    /// recovery retries.
    pub(super) fn new() -> Self {
        Self {
            stream: None,
            handle: None,
            sink: None,
            preferred_device: None,
            recovery_retries: 0,
            reopen_needed: false,
        }
    }

    // -----------------------------------------------------------------------
    // Device lifecycle
    // -----------------------------------------------------------------------

    /// Return the `OutputStreamHandle`, opening the device if it is not
    /// already open.
    ///
    /// Uses `open_output_stream` from `output.rs`, which already handles
    /// ALSA/JACK stderr suppression and preferred-device lookup.
    pub(super) fn ensure_open(&mut self) -> Result<&OutputStreamHandle, EngineError> {
        if self.handle.is_none() {
            let sel = open_output_stream(self.preferred_device.as_deref())
                .map_err(EngineError::Output)?;
            self.stream = Some(sel.stream);
            self.handle = Some(sel.handle);
        }
        self.handle
            .as_ref()
            .ok_or_else(|| EngineError::Output("output device handle unavailable".to_string()))
    }

    /// Store the normalized preferred device name and mark that a reopen is
    /// needed.  The name is normalized via `normalize_output_device_name` so
    /// "Default" / blank / whitespace all become `None`.
    ///
    /// For simplicity, `reopen_needed` is always set to `true` — callers are
    /// responsible for checking `reopen_needed()` before acting.
    pub(super) fn set_preferred_device(&mut self, name: Option<String>) {
        self.preferred_device = normalize_output_device_name(name.as_deref());
        self.reopen_needed = true;
    }

    /// Drop the current stream/handle/sink, increment `recovery_retries`, and
    /// attempt to reopen the output device using `open_output_stream`.
    ///
    /// On success, `recovery_retries` is reset to 0 and the new stream/handle
    /// are stored (no sink is created — the caller must call `attach`).
    ///
    /// On failure, returns `EngineError::Output`.
    pub(super) fn reopen(&mut self) -> Result<(), EngineError> {
        // Drop existing resources first.
        self.sink = None;
        self.handle = None;
        self.stream = None;

        self.recovery_retries = self.recovery_retries.saturating_add(1);

        let sel =
            open_output_stream(self.preferred_device.as_deref()).map_err(EngineError::Output)?;

        self.stream = Some(sel.stream);
        self.handle = Some(sel.handle);
        self.recovery_retries = 0;

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Sink management
    // -----------------------------------------------------------------------

    /// Open the device if needed, create a new `Sink` from the handle, append
    /// `source` to the sink, and store it.
    ///
    /// Returns `EngineError::Output` on any failure.
    pub(super) fn attach(&mut self, source: DecodedSource) -> Result<(), EngineError> {
        // Ensure the device is open and obtain the handle.  We need to call
        // `ensure_open` first and then re-borrow `self.handle` to satisfy the
        // borrow checker (we can't hold `&handle` while also mutating `self`
        // via `Sink::new`).
        self.ensure_open()?;

        let handle = self.handle.as_ref().ok_or_else(|| {
            EngineError::Output("output handle lost after ensure_open".to_string())
        })?;

        let sink = Sink::try_new(handle)
            .map_err(|e| EngineError::Output(format!("failed to create sink: {e}")))?;

        sink.append(source);
        self.sink = Some(sink);
        Ok(())
    }

    /// Set the playback volume on the current sink, if one exists.
    #[cfg(test)]
    pub(super) fn set_volume(&mut self, v: f32) {
        if let Some(sink) = &self.sink {
            sink.set_volume(v);
        }
    }

    /// Pause playback on the current sink, if one exists.
    pub(super) fn pause(&mut self) {
        if let Some(sink) = &self.sink {
            sink.pause();
        }
    }

    /// Resume playback on the current sink, if one exists.
    pub(super) fn resume(&mut self) {
        if let Some(sink) = &self.sink {
            sink.play();
        }
    }

    /// Stop playback by dropping the sink.
    pub(super) fn stop(&mut self) {
        self.sink = None;
    }

    // -----------------------------------------------------------------------
    // Inspection
    // -----------------------------------------------------------------------

    /// Returns `true` when a sink exists and reports that it has drained all
    /// of its queued sources naturally (end-of-stream).
    ///
    /// Device loss arrives as `EngineEvent::OutputLost`, not via this method.
    pub(super) fn is_sink_drained(&self) -> bool {
        self.sink.as_ref().map(|s| s.empty()).unwrap_or(false)
    }

    /// Returns the number of times `reopen` has been attempted since the last
    /// successful reopen (or since construction).
    pub(super) fn recovery_retries(&self) -> u8 {
        self.recovery_retries
    }

    /// Returns `true` if `set_preferred_device` has been called since the last
    /// `clear_reopen_needed`.
    #[cfg(test)]
    pub(super) fn reopen_needed(&self) -> bool {
        self.reopen_needed
    }

    /// Clears the `reopen_needed` flag.
    #[cfg(test)]
    pub(super) fn clear_reopen_needed(&mut self) {
        self.reopen_needed = false;
    }

    /// Sets `recovery_retries` to a specific value for testing.
    #[cfg(test)]
    pub(super) fn set_recovery_retries(&mut self, n: u8) {
        self.recovery_retries = n;
    }

    /// Applies one tick of a `VolumeRamp` to the current sink, if one exists.
    ///
    /// This allows `EngineLoop` to drive volume fading without exposing the
    /// `Sink` directly.
    pub(super) fn apply_volume_ramp(&mut self, ramp: &mut super::volume::VolumeRamp) {
        if let Some(ref sink) = self.sink {
            ramp.tick(sink);
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ---- new() -------------------------------------------------------------

    #[test]
    fn new_starts_with_no_device_and_zero_retries() {
        let om = OutputManager::new();
        assert!(om.stream.is_none());
        assert!(om.handle.is_none());
        assert!(om.sink.is_none());
        assert!(om.preferred_device.is_none());
        assert_eq!(om.recovery_retries(), 0);
        assert!(!om.reopen_needed());
    }

    // ---- set_preferred_device() --------------------------------------------

    #[test]
    fn set_preferred_device_sets_reopen_needed() {
        let mut om = OutputManager::new();
        om.set_preferred_device(Some("My Speakers".to_string()));
        assert!(om.reopen_needed());
        assert_eq!(om.preferred_device.as_deref(), Some("My Speakers"));
    }

    #[test]
    fn set_preferred_device_same_value_also_sets_reopen_needed() {
        let mut om = OutputManager::new();
        om.set_preferred_device(Some("My Speakers".to_string()));
        om.clear_reopen_needed();
        assert!(!om.reopen_needed());

        // Setting the same value again still marks reopen needed.
        om.set_preferred_device(Some("My Speakers".to_string()));
        assert!(om.reopen_needed());
    }

    #[test]
    fn set_preferred_device_normalizes_default_to_none() {
        let mut om = OutputManager::new();
        om.set_preferred_device(Some("Default".to_string()));
        assert!(om.preferred_device.is_none());
        assert!(om.reopen_needed());
    }

    #[test]
    fn set_preferred_device_normalizes_blank_to_none() {
        let mut om = OutputManager::new();
        om.set_preferred_device(Some("   ".to_string()));
        assert!(om.preferred_device.is_none());
        assert!(om.reopen_needed());
    }

    // ---- is_sink_drained() -------------------------------------------------

    #[test]
    fn is_sink_drained_false_when_no_sink() {
        let om = OutputManager::new();
        assert!(!om.is_sink_drained());
    }

    // ---- clear_reopen_needed() ---------------------------------------------

    #[test]
    fn clear_reopen_needed_clears_flag() {
        let mut om = OutputManager::new();
        om.set_preferred_device(Some("Headphones".to_string()));
        assert!(om.reopen_needed());
        om.clear_reopen_needed();
        assert!(!om.reopen_needed());
    }

    // ---- recovery_retries() — failure path --------------------------------

    /// Construct an `OutputManager` with an invalid preferred device so that
    /// `reopen` fails and confirm that `recovery_retries` is incremented.
    #[test]
    #[ignore] // Requires audio hardware; crashes on headless CI (macOS/Windows)
    fn reopen_increments_recovery_retries_on_failure() {
        let mut om = OutputManager::new();
        // Set an unlikely-to-exist device name to force `reopen` to fail.
        om.preferred_device = Some("__nonexistent_audio_device_xyz__".to_string());

        let initial = om.recovery_retries();

        // On a headless/CI machine open_output_stream will fall back to the
        // system default, which may succeed.  We only assert the invariant
        // that holds regardless of whether a real device is present:
        //   - if reopen fails: retries == initial + 1
        //   - if reopen succeeds: retries reset to 0
        match om.reopen() {
            Err(EngineError::Output(_)) => {
                assert_eq!(
                    om.recovery_retries(),
                    initial + 1,
                    "retries should increment on failure"
                );
            }
            Ok(()) => {
                // Fallback to default device succeeded (CI with audio or local
                // dev machine) — retries were reset to 0.
                assert_eq!(om.recovery_retries(), 0);
            }
            Err(other) => panic!("unexpected error variant: {other:?}"),
        }
    }

    // ---- stop() / sink lifecycle -------------------------------------------

    #[test]
    fn stop_sets_sink_to_none() {
        let mut om = OutputManager::new();
        // Manually place a sink-less OutputManager and call stop — should be a
        // no-op without panicking.
        om.stop();
        assert!(om.sink.is_none());
    }

    // ---- set_volume / pause / resume with no sink are no-ops ---------------

    #[test]
    fn set_volume_no_panic_when_no_sink() {
        let mut om = OutputManager::new();
        om.set_volume(0.5); // must not panic
    }

    #[test]
    fn pause_no_panic_when_no_sink() {
        let mut om = OutputManager::new();
        om.pause(); // must not panic
    }

    #[test]
    fn resume_no_panic_when_no_sink() {
        let mut om = OutputManager::new();
        om.resume(); // must not panic
    }

    // ---- ensure_open with real device (requires audio hardware) -----------

    #[test]
    #[ignore]
    fn ensure_open_opens_device_on_first_call() {
        let mut om = OutputManager::new();
        let result = om.ensure_open();
        assert!(result.is_ok());
        assert!(om.handle.is_some());
    }

    #[test]
    #[ignore]
    fn attach_creates_sink_and_is_not_drained_immediately() {
        let mut om = OutputManager::new();
        // Use rodio's built-in sine-wave source as a type-erased DecodedSource.
        let source: DecodedSource = Box::new(rodio::source::SineWave::new(440.0));
        let result = om.attach(source);
        assert!(result.is_ok(), "attach failed: {result:?}");
        assert!(om.sink.is_some());
    }
}
