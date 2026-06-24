use super::*;
use crate::audio::AudioEngine;
use crate::elapsed_timer::ElapsedTimer;

pub struct PlaybackRuntime {
    pub view: PlaybackView,
    pub volume: u8,
    pub muted: bool,
    pub reconnect: Reconnect,
    pub diagnostics: PlaybackDiagnostics,
    pub sleep_timer: SleepTimer,
    pub audio: AudioEngine,
    pub sample_buffer: Arc<Mutex<VecDeque<f32>>>,
    pub elapsed_timer: ElapsedTimer,
}

impl PlaybackRuntime {
    pub(super) fn new(
        ui_state: &super::ui_state::UiState,
        output_device: String,
        metadata_enabled: bool,
        audio: AudioEngine,
        sample_buffer: Arc<Mutex<VecDeque<f32>>>,
    ) -> Self {
        Self {
            view: PlaybackView::default(),
            volume: ui_state.volume(),
            muted: ui_state.muted(),
            reconnect: Reconnect::default(),
            diagnostics: PlaybackDiagnostics::new(output_device, metadata_enabled, 3),
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
    use crate::favorites::Library;
    use crate::radio::Station;

    fn library_with_settings() -> Library {
        let mut library =
            Library::in_memory(vec![Station::basic("A", "http://a", "Radio", "US", 128)]);
        library.settings.output_device_name = Some("Headphones".to_string());
        library.settings.stream_metadata_enabled = false;
        library
    }

    #[test]
    fn playback_runtime_uses_loaded_volume_mute_and_diagnostics() {
        let ui_state = super::super::ui_state::UiState::from_app_values(
            37,
            true,
            LayoutMode::Split,
            VisualizerMode::RealOscilloscope,
            DisplayMode::Normal,
        );
        let library = library_with_settings();
        let sample_buffer = Arc::new(Mutex::new(VecDeque::new()));
        let audio = AudioEngine::disconnected_for_test();

        let runtime = PlaybackRuntime::new(
            &ui_state,
            crate::audio::output_device_display_name(
                library.settings.output_device_name.as_deref(),
            ),
            library.settings.stream_metadata_enabled,
            audio,
            sample_buffer,
        );

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
    use crate::audio::AudioEngine;
    use proptest::prelude::*;

    /// Helper to construct a PlaybackRuntime with specific volume and muted values.
    fn runtime_with(volume: u8, muted: bool) -> PlaybackRuntime {
        let ui_state = super::super::ui_state::UiState::from_app_values(
            volume,
            muted,
            LayoutMode::Split,
            VisualizerMode::RealOscilloscope,
            DisplayMode::Normal,
        );
        let sample_buffer = Arc::new(Mutex::new(VecDeque::new()));
        let audio = AudioEngine::disconnected_for_test();
        PlaybackRuntime::new(&ui_state, "Test".to_string(), false, audio, sample_buffer)
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
