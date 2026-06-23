use super::*;
use crate::audio::AudioEngine;

pub struct PlaybackRuntime {
    pub view: PlaybackView,
    pub volume: u8,
    pub muted: bool,
    pub reconnect: Reconnect,
    pub diagnostics: PlaybackDiagnostics,
    pub sleep_timer: SleepTimer,
    pub audio: AudioEngine,
    pub sample_buffer: Arc<Mutex<VecDeque<f32>>>,
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
        let ui_state =
            super::super::ui_state::UiState::from_app_values(37, true, LayoutMode::Split, VisualizerMode::RealOscilloscope);
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
