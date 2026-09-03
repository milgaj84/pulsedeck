use super::*;
use crate::audio::AudioSink;
use crate::elapsed_timer::ElapsedTimer;

/// Groups playback configuration parameters for PlaybackRuntime construction.
#[derive(Debug, Clone)]
pub struct PlaybackOptions {
    pub output_device: String,
    pub metadata_enabled: bool,
    pub reconnect_max_attempts: u8,
    pub reconnect_backoff_seconds: Vec<u64>,
}

pub struct PlaybackRuntime {
    pub view: PlaybackView,
    pub volume: u8,
    pub muted: bool,
    pub reconnect: Reconnect,
    pub diagnostics: PlaybackDiagnostics,
    pub sleep_timer: SleepTimer,
    pub audio: Box<dyn AudioSink>,
    pub sample_buffer: Arc<Mutex<VecDeque<f32>>>,
    pub elapsed_timer: ElapsedTimer,
}

impl PlaybackRuntime {
    pub(super) fn new(
        ui_state: &super::ui_state::UiState,
        options: PlaybackOptions,
        audio: Box<dyn AudioSink>,
        sample_buffer: Arc<Mutex<VecDeque<f32>>>,
    ) -> Self {
        Self {
            view: PlaybackView::default(),
            volume: ui_state.volume(),
            muted: ui_state.muted(),
            reconnect: Reconnect::new(
                options.reconnect_max_attempts,
                options.reconnect_backoff_seconds,
            ),
            diagnostics: PlaybackDiagnostics::new(
                options.output_device,
                options.metadata_enabled,
                options.reconnect_max_attempts,
            ),
            sleep_timer: SleepTimer::default(),
            audio,
            sample_buffer,
            elapsed_timer: ElapsedTimer::new(),
        }
    }

    pub fn output_volume_fraction(&self) -> f32 {
        if self.muted {
            0.0
        } else {
            self.volume as f32 / 100.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::MockAudioSink;

    #[test]
    fn playback_runtime_uses_loaded_volume_mute_and_diagnostics() {
        let ui_state = super::super::ui_state::UiState::from_app_values(
            37,
            true,
            LayoutMode::Split,
            VisualizerMode::RealOscilloscope,
            DisplayMode::Normal,
            None,
        );
        let sample_buffer = Arc::new(Mutex::new(VecDeque::new()));
        let audio = MockAudioSink::disconnected();

        let options = PlaybackOptions {
            output_device: "Headphones".to_string(),
            metadata_enabled: false,
            reconnect_max_attempts: 3,
            reconnect_backoff_seconds: vec![3, 6, 12],
        };

        let runtime = PlaybackRuntime::new(&ui_state, options, Box::new(audio), sample_buffer);

        assert_eq!(runtime.volume, 37);
        assert!(runtime.muted);
        assert_eq!(runtime.output_volume_fraction(), 0.0);
        assert_eq!(runtime.diagnostics.output_device, "Headphones");
        assert!(!runtime.diagnostics.metadata_enabled);
        assert_eq!(runtime.diagnostics.reconnect_limit, 3);
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use crate::audio::MockAudioSink;
    use proptest::prelude::*;

    /// Helper to construct a PlaybackRuntime with specific volume and muted values.
    fn runtime_with(volume: u8, muted: bool) -> PlaybackRuntime {
        let ui_state = super::super::ui_state::UiState::from_app_values(
            volume,
            muted,
            LayoutMode::Split,
            VisualizerMode::RealOscilloscope,
            DisplayMode::Normal,
            None,
        );
        let sample_buffer = Arc::new(Mutex::new(VecDeque::new()));
        let audio = MockAudioSink::disconnected();
        let options = PlaybackOptions {
            output_device: "Test".to_string(),
            metadata_enabled: false,
            reconnect_max_attempts: 3,
            reconnect_backoff_seconds: vec![3, 6, 12],
        };
        PlaybackRuntime::new(&ui_state, options, Box::new(audio), sample_buffer)
    }

    proptest! {
        /// **Feature: test-coverage-improvement, Property 10: Volume fraction bounds and mute invariant**
        ///
        /// For any volume in 0..=100 and muted state in {true, false},
        /// `output_volume_fraction` returns a value in [0.0, 1.0],
        /// returns 0.0 when muted, returns 1.0 when unmuted at max volume,
        /// and returns 0.0 when unmuted at zero volume.
        ///
        /// **Validates: Requirements 14.1, 14.2, 14.3, 14.4**
        #[test]
        fn volume_fraction_bounds_and_mute_invariant(volume in 0u8..=100u8, muted in proptest::bool::ANY) {
            let runtime = runtime_with(volume, muted);
            let fraction = runtime.output_volume_fraction();

            // Requirement 14.1: result is always in [0.0, 1.0]
            prop_assert!((0.0..=1.0).contains(&fraction),
                "fraction {} out of bounds for volume={}, muted={}", fraction, volume, muted);

            // Requirement 14.2: muted always yields 0.0
            if muted {
                prop_assert_eq!(fraction, 0.0,
                    "expected 0.0 when muted, got {} for volume={}", fraction, volume);
            }

            // Requirement 14.3: unmuted at volume 100 yields exactly 1.0
            if !muted && volume == 100 {
                prop_assert_eq!(fraction, 1.0,
                    "expected 1.0 for unmuted volume=100, got {}", fraction);
            }

            // Requirement 14.4: unmuted at volume 0 yields exactly 0.0
            if !muted && volume == 0 {
                prop_assert_eq!(fraction, 0.0,
                    "expected 0.0 for unmuted volume=0, got {}", fraction);
            }
        }
    }
}
