use super::*;
use std::time::Duration;

pub struct UiRuntimeState {
    pub nav: Navigation,
    pub command_palette: CommandPaletteState,
    pub should_quit: bool,
    pub notice: NoticeState,
    pub input_mode: InputMode,
    pub tick_count: u64,
    pub layout_mode: LayoutMode,
    pub overlays: Overlays,
    pub visualizer_mode: VisualizerMode,
    pub visualizer_peaks: Vec<f32>,
    pub display_mode: DisplayMode,
    pub last_tick_instant: std::time::Instant,
    pub volume_flash_remaining: Duration,
}

impl UiRuntimeState {
    pub(super) fn from_ui_state(ui_state: &super::ui_state::UiState) -> Self {
        Self {
            nav: Navigation::default(),
            command_palette: CommandPaletteState::default(),
            should_quit: false,
            notice: NoticeState::default(),
            input_mode: InputMode::Normal,
            tick_count: 0,
            layout_mode: ui_state.layout_mode(),
            overlays: Overlays::default(),
            visualizer_mode: VisualizerMode::from_index(ui_state.visualizer_mode()),
            visualizer_peaks: Vec::new(),
            display_mode: ui_state.display_mode(),
            last_tick_instant: std::time::Instant::now(),
            volume_flash_remaining: Duration::ZERO,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_runtime_uses_loaded_layout_and_visualizer_mode() {
        let ui_state = super::super::ui_state::UiState::from_app_values(
            42,
            true,
            LayoutMode::RightOnly,
            VisualizerMode::SimOscilloscope,
            DisplayMode::Mini,
            None,
        );

        let runtime = UiRuntimeState::from_ui_state(&ui_state);

        assert_eq!(runtime.layout_mode, LayoutMode::RightOnly);
        assert_eq!(runtime.visualizer_mode, VisualizerMode::SimOscilloscope);
        assert_eq!(runtime.input_mode, InputMode::Normal);
        assert_eq!(runtime.display_mode, DisplayMode::Mini);
        assert!(!runtime.should_quit);
    }
}
