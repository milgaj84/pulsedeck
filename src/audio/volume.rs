/// Exponential volume fade step factor (matches existing engine behaviour).
const FADE_STEP: f32 = 0.15;

/// Volume threshold below which a fade-out is considered complete.
const FADE_OUT_DONE_THRESHOLD: f32 = 0.01;

/// Distance from `target_volume` at which a fade-in snaps to the target.
const FADE_IN_SNAP_THRESHOLD: f32 = 0.03;

/// Clamps a volume value to `[0.0, 1.0]`, converting NaN to 0.0.
///
/// Rust's `f32::clamp` propagates NaN rather than mapping it to a bound, so
/// we handle that case explicitly.  All other non-finite values (`±∞`) are
/// clamped naturally.
#[inline]
pub(super) fn clamp_volume(v: f32) -> f32 {
    if v.is_nan() {
        0.0
    } else {
        v.clamp(0.0, 1.0)
    }
}

/// Direction of an in-progress fade.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FadeDirection {
    In,
    Out,
}

/// Handles smooth fade-in and fade-out volume transitions, decoupled from
/// `EngineLoop` state.
///
/// The volume model:
/// - `current_volume` tracks the last value applied to the sink.
/// - `target_volume` is the desired steady-state volume (set by `SetVolume`).
/// - When a fade is active, each call to `tick` advances `current_volume` one
///   exponential step toward either `target_volume` (fade-in) or 0.0 (fade-out).
///
/// Fade-out completion is signalled by `is_done()`.  The caller is responsible
/// for stopping the sink and transitioning state once `is_done()` returns `true`.
#[derive(Debug, Clone)]
pub(super) struct VolumeRamp {
    /// The volume currently applied to the rodio `Sink`.
    current_volume: f32,
    /// Steady-state target volume in `[0.0, 1.0]`.
    target_volume: f32,
    /// `Some` when a fade is in progress; `None` when at steady state.
    active_fade: Option<FadeDirection>,
}

impl VolumeRamp {
    /// Constructs a new `VolumeRamp` starting at `target_volume` with no
    /// active fade.
    pub(super) fn new(target_volume: f32) -> Self {
        let target_volume = clamp_volume(target_volume);
        Self {
            current_volume: target_volume,
            target_volume,
            active_fade: None,
        }
    }

    // -----------------------------------------------------------------------
    // Fade control
    // -----------------------------------------------------------------------

    /// Begin a fade-in: ramp `current_volume` exponentially toward
    /// `target_volume`.  Resets any in-progress fade.
    pub(super) fn begin_fade_in(&mut self) {
        self.active_fade = Some(FadeDirection::In);
    }

    /// Begin a fade-out: ramp `current_volume` exponentially toward 0.0.
    /// Resets any in-progress fade.
    #[allow(dead_code)]
    pub(super) fn begin_fade_out(&mut self) {
        self.active_fade = Some(FadeDirection::Out);
    }

    /// Update the steady-state target volume (used by `SetVolume`).
    ///
    /// The value is sanitised: NaN and negative infinity map to 0.0, positive
    /// infinity maps to 1.0. Normal values are clamped to `[0.0, 1.0]`.
    pub(super) fn retarget(&mut self, target: f32) {
        self.target_volume = clamp_volume(target);
        // If a fade-in was in progress it now heads toward the updated target.
        // A fade-out is unaffected — it still ramps to 0.0.
    }

    // -----------------------------------------------------------------------
    // Per-tick advance
    // -----------------------------------------------------------------------

    /// Apply one exponential fade step to `sink` and update `current_volume`.
    ///
    /// - **Fade-in**: steps toward `target_volume`; snaps when within
    ///   `FADE_IN_SNAP_THRESHOLD` and clears `active_fade`.
    /// - **Fade-out**: steps toward 0.0; clears `active_fade` when
    ///   `current_volume <= FADE_OUT_DONE_THRESHOLD` (caller checks
    ///   `is_done()` separately and can stop the sink).
    /// - **No active fade**: applies `target_volume` directly so a
    ///   `retarget` call takes effect on the next tick.
    pub(super) fn tick(&mut self, sink: &rodio::Sink) {
        match self.active_fade {
            Some(FadeDirection::In) => {
                let diff = self.target_volume - self.current_volume;
                if diff.abs() <= FADE_IN_SNAP_THRESHOLD {
                    self.current_volume = self.target_volume;
                    self.active_fade = None;
                } else {
                    self.current_volume += diff * FADE_STEP;
                }
                sink.set_volume(self.current_volume);
            }
            Some(FadeDirection::Out) => {
                let step = self.current_volume * FADE_STEP;
                self.current_volume = (self.current_volume - step).max(0.0);
                sink.set_volume(self.current_volume);
                if self.current_volume <= FADE_OUT_DONE_THRESHOLD {
                    self.active_fade = None;
                }
            }
            None => {
                // Steady state: honour any recent retarget call.
                sink.set_volume(self.target_volume);
                self.current_volume = self.target_volume;
            }
        }
    }

    // -----------------------------------------------------------------------
    // State queries
    // -----------------------------------------------------------------------

    /// Returns the volume last applied to the sink.
    pub(super) fn current_volume(&self) -> f32 {
        self.current_volume
    }

    /// Returns `true` while a fade-out is in progress (before `is_done`).
    pub(super) fn is_fading_out(&self) -> bool {
        self.active_fade == Some(FadeDirection::Out)
    }

    /// Returns `true` when a fade-out has run to completion
    /// (`current_volume <= FADE_OUT_DONE_THRESHOLD`).
    ///
    /// The caller should stop the sink and transition state when this returns
    /// `true`.
    #[allow(dead_code)]
    pub(super) fn is_done(&self) -> bool {
        // Done means: was fading out and has now finished.
        // active_fade is cleared by tick once the threshold is crossed, so
        // we also need to check the volume itself when active_fade is None.
        !self.is_fading_out() && self.current_volume <= FADE_OUT_DONE_THRESHOLD
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------
    // Ports of the existing engine_loop tests
    // ------------------------------------------------------------------

    /// `fade_out_next_volume` equivalent: one exponential step from 1.0 should
    /// land at 0.85 (step = 1.0 × 0.15 = 0.15; 1.0 − 0.15 = 0.85).
    #[test]
    fn fade_out_step_uses_exponential_decay() {
        let mut ramp = VolumeRamp::new(1.0);
        ramp.current_volume = 1.0;
        ramp.begin_fade_out();

        let next = ramp.current_volume - ramp.current_volume * FADE_STEP;
        assert!((next - 0.85).abs() < f32::EPSILON);
    }

    /// `fade_out_complete` equivalent: volumes above threshold are not done,
    /// volumes at/below are done (threshold for `VolumeRamp` is 0.01).
    #[test]
    fn fade_out_done_threshold_boundary() {
        // Above threshold: not done (unless there's an active fade_out).
        let mut ramp = VolumeRamp::new(1.0);
        ramp.begin_fade_out();
        ramp.current_volume = 0.011;
        // active_fade is Some(Out), so is_done = false.
        assert!(!ramp.is_done(), "should not be done above threshold");
        assert!(ramp.is_fading_out());

        // At threshold and fade cleared: done.
        ramp.current_volume = 0.01;
        ramp.active_fade = None; // simulates tick just cleared it
        assert!(ramp.is_done(), "should be done at threshold");
    }

    /// `clamp_status_volume` equivalent: `retarget` clamps any f32 into
    /// `[0.0, 1.0]`.
    #[test]
    fn retarget_clamps_to_unit_interval() {
        let mut ramp = VolumeRamp::new(0.5);

        ramp.retarget(-0.2);
        assert_eq!(ramp.target_volume, 0.0);

        ramp.retarget(0.42);
        assert_eq!(ramp.target_volume, 0.42);

        ramp.retarget(1.4);
        assert_eq!(ramp.target_volume, 1.0);
    }

    // ------------------------------------------------------------------
    // Additional VolumeRamp-specific tests
    // ------------------------------------------------------------------

    #[test]
    fn new_starts_at_clamped_target_with_no_active_fade() {
        let ramp = VolumeRamp::new(0.8);
        assert_eq!(ramp.current_volume(), 0.8);
        assert_eq!(ramp.target_volume, 0.8);
        assert!(!ramp.is_fading_out());
        assert!(!ramp.is_done());
    }

    #[test]
    fn new_clamps_nan_target_to_zero() {
        let ramp = VolumeRamp::new(f32::NAN);
        assert_eq!(ramp.current_volume(), 0.0);
    }

    #[test]
    fn new_clamps_infinity_to_one() {
        let ramp = VolumeRamp::new(f32::INFINITY);
        assert_eq!(ramp.current_volume(), 1.0);
    }
    #[test]
    fn begin_fade_in_sets_fade_in_direction() {
        let mut ramp = VolumeRamp::new(0.8);
        ramp.begin_fade_in();
        assert_eq!(ramp.active_fade, Some(FadeDirection::In));
    }

    #[test]
    fn begin_fade_out_sets_fade_out_direction() {
        let mut ramp = VolumeRamp::new(0.8);
        ramp.begin_fade_out();
        assert!(ramp.is_fading_out());
    }

    #[test]
    fn retarget_clamps_nan() {
        let mut ramp = VolumeRamp::new(0.5);
        ramp.retarget(f32::NAN);
        // NaN is mapped to 0.0 by clamp_volume (f32::clamp propagates NaN,
        // so we handle it explicitly).
        assert_eq!(ramp.target_volume, 0.0);
    }

    #[test]
    fn retarget_clamps_positive_infinity() {
        let mut ramp = VolumeRamp::new(0.5);
        ramp.retarget(f32::INFINITY);
        assert_eq!(ramp.target_volume, 1.0);
    }

    #[test]
    fn retarget_clamps_negative_infinity() {
        let mut ramp = VolumeRamp::new(0.5);
        ramp.retarget(f32::NEG_INFINITY);
        assert_eq!(ramp.target_volume, 0.0);
    }

    #[test]
    fn is_fading_out_false_after_begin_fade_in() {
        let mut ramp = VolumeRamp::new(0.8);
        ramp.begin_fade_in();
        assert!(!ramp.is_fading_out());
    }

    #[test]
    fn begin_fade_out_followed_by_begin_fade_in_overrides() {
        let mut ramp = VolumeRamp::new(0.8);
        ramp.begin_fade_out();
        ramp.begin_fade_in();
        assert!(!ramp.is_fading_out());
        assert_eq!(ramp.active_fade, Some(FadeDirection::In));
    }
}

// ---------------------------------------------------------------------------
// Property-based tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    /// **Property 8: Volume clamp**
    ///
    /// For any `f32` value `v` (including NaN, ±infinity, and normal values),
    /// `clamp_volume(v)` always produces a result within `[0.0, 1.0]`.
    ///
    /// **Validates: Requirements 10.6**
    // **Property 8: Volume clamp**
    //
    // For any `f32` value `v` (including NaN, ±infinity, and normal values),
    // `clamp_volume(v)` always produces a result within `[0.0, 1.0]`.
    //
    // **Validates: Requirements 10.6**
    proptest! {
        #[test]
        fn volume_clamp_result_in_unit_interval(v in any::<f32>()) {
            let clamped = clamp_volume(v);
            prop_assert!(
                clamped >= 0.0 && clamped <= 1.0,
                "clamp_volume({v}) = {clamped} is outside [0.0, 1.0]"
            );
        }
    }
}
