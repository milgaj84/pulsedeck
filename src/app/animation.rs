/// Step size per frame: completes in 2 frames at 10 FPS (200ms total).
pub const ANIMATION_STEP: f32 = 0.5;

/// Tracks interpolation progress for overlay fade-in and scroll transitions.
/// Progress is a normalized f32 clamped to [0.0, 1.0].
#[derive(Debug, Clone)]
pub struct AnimationState {
    progress: f32,
    from: f32,
    to: f32,
    active: bool,
}

impl AnimationState {
    /// Create an inactive (idle) animation state.
    pub fn idle() -> Self {
        Self {
            progress: 0.0,
            from: 0.0,
            to: 0.0,
            active: false,
        }
    }

    /// Start a new animation interpolating from `from` to `to`.
    pub fn start(from: f32, to: f32) -> Self {
        Self {
            progress: 0.0,
            from,
            to,
            active: true,
        }
    }

    /// Advance progress by `step`, clamping to [0.0, 1.0].
    pub fn advance(&mut self, step: f32) {
        if !self.active {
            return;
        }
        self.progress = (self.progress + step).clamp(0.0, 1.0);
    }

    /// Restart animation from the current interpolated value toward a new target.
    pub fn restart(&mut self, new_to: f32) {
        self.from = self.current_value();
        self.to = new_to;
        self.progress = 0.0;
        self.active = true;
    }

    /// Current interpolation progress in [0.0, 1.0].
    pub fn progress(&self) -> f32 {
        self.progress
    }

    /// Current interpolated value: lerp(from, to, progress).
    pub fn current_value(&self) -> f32 {
        self.from + (self.to - self.from) * self.progress
    }

    /// Whether the animation has reached completion (progress >= 1.0).
    pub fn is_complete(&self) -> bool {
        self.progress >= 1.0
    }

    /// Whether the animation is currently active.
    pub fn is_active(&self) -> bool {
        self.active
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_idle_state() {
        let state = AnimationState::idle();
        assert_eq!(state.progress(), 0.0);
        assert_eq!(state.current_value(), 0.0);
        assert!(!state.is_active());
        assert!(!state.is_complete());
    }

    #[test]
    fn test_start_sets_initial_values() {
        let state = AnimationState::start(10.0, 20.0);
        assert_eq!(state.progress(), 0.0);
        assert_eq!(state.current_value(), 10.0);
        assert!(state.is_active());
        assert!(!state.is_complete());
    }

    #[test]
    fn test_advance_increments_progress() {
        let mut state = AnimationState::start(0.0, 100.0);
        state.advance(0.5);
        assert_eq!(state.progress(), 0.5);
        assert_eq!(state.current_value(), 50.0);
    }

    #[test]
    fn test_advance_clamps_at_one() {
        let mut state = AnimationState::start(0.0, 100.0);
        state.advance(0.7);
        state.advance(0.7);
        assert_eq!(state.progress(), 1.0);
        assert_eq!(state.current_value(), 100.0);
    }

    #[test]
    fn test_advance_noop_when_idle() {
        let mut state = AnimationState::idle();
        state.advance(0.5);
        assert_eq!(state.progress(), 0.0);
    }

    #[test]
    fn test_two_frame_completion() {
        let mut state = AnimationState::start(0.0, 1.0);
        state.advance(ANIMATION_STEP);
        assert_eq!(state.progress(), 0.5);
        assert!(!state.is_complete());
        state.advance(ANIMATION_STEP);
        assert_eq!(state.progress(), 1.0);
        assert!(state.is_complete());
    }

    #[test]
    fn test_current_value_interpolates_correctly() {
        let mut state = AnimationState::start(20.0, 80.0);
        state.advance(0.25);
        // 20 + (80-20) * 0.25 = 20 + 15 = 35
        assert!((state.current_value() - 35.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_restart_preserves_current_position() {
        let mut state = AnimationState::start(0.0, 100.0);
        state.advance(0.5); // current_value = 50
        state.restart(200.0);
        assert_eq!(state.progress(), 0.0);
        assert_eq!(state.current_value(), 50.0); // from is now 50
        assert!(state.is_active());

        state.advance(1.0);
        assert_eq!(state.current_value(), 200.0);
    }

    #[test]
    fn test_restart_from_complete() {
        let mut state = AnimationState::start(0.0, 100.0);
        state.advance(1.0); // complete, current_value = 100
        state.restart(50.0);
        assert_eq!(state.progress(), 0.0);
        assert_eq!(state.current_value(), 100.0); // from is now 100
        state.advance(1.0);
        assert_eq!(state.current_value(), 50.0);
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// Property 3: For any sequence of advance(step) calls, progress stays in [0.0, 1.0].
        #[test]
        fn prop_animation_progress_invariant(steps in proptest::collection::vec(0.0f32..5.0, 1..20)) {
            let mut state = AnimationState::start(0.0, 100.0);
            for step in steps {
                state.advance(step);
                prop_assert!(state.progress() >= 0.0);
                prop_assert!(state.progress() <= 1.0);
            }
        }

        /// Property 4: current_value equals from + (to - from) * min(progress, 1.0).
        #[test]
        fn prop_animation_interpolation_correctness(
            from in -100.0f32..100.0,
            to in -100.0f32..100.0,
            frames in 0u32..=10
        ) {
            let mut state = AnimationState::start(from, to);
            for _ in 0..frames {
                state.advance(ANIMATION_STEP);
            }
            let expected_progress = (frames as f32 * ANIMATION_STEP).clamp(0.0, 1.0);
            let expected_value = from + (to - from) * expected_progress;
            let actual = state.current_value();
            prop_assert!((actual - expected_value).abs() < 1e-4,
                "expected {}, got {}", expected_value, actual);
        }

        /// Property 5: After restart, new from == old current_value, progress == 0.0.
        #[test]
        fn prop_animation_restart_preserves_current(
            from in -100.0f32..100.0,
            to in -100.0f32..100.0,
            progress in 0.0f32..=1.0,
            new_to in -100.0f32..100.0
        ) {
            let mut state = AnimationState::start(from, to);
            // Manually set progress to arbitrary value via advance
            state.advance(progress);
            let old_value = state.current_value();
            state.restart(new_to);
            prop_assert_eq!(state.progress(), 0.0);
            prop_assert!((state.current_value() - old_value).abs() < 1e-4,
                "from should be old current_value");
        }
    }
}
