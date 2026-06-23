use super::LayoutMode;
use super::VisualizerMode;
use serde::{Deserialize, Serialize};

const DEFAULT_VOLUME: u8 = 80;
const MAX_VOLUME: u8 = 100;
const VISUALIZER_MODE_COUNT: usize = 3;
#[cfg(not(test))]
const UI_STATE_FILE: &str = "ui-state.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct UiState {
    #[serde(default = "default_volume")]
    volume: u8,
    #[serde(default)]
    muted: bool,
    #[serde(default = "default_layout_mode_key")]
    layout_mode: String,
    #[serde(default)]
    visualizer_mode: usize,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            volume: DEFAULT_VOLUME,
            muted: false,
            layout_mode: default_layout_mode_key(),
            visualizer_mode: 0,
        }
    }
}

impl UiState {
    #[cfg(not(test))]
    #[allow(dead_code)]
    pub(super) fn load() -> Self {
        Self::load_with_warning().0
    }

    #[cfg(test)]
    pub(super) fn load() -> Self {
        Self::default()
    }

    #[cfg(not(test))]
    pub(super) fn load_with_warning() -> (Self, Option<String>) {
        let (state, warning) = crate::config::load_json_with_warning::<Self>(UI_STATE_FILE);
        (state.sanitized(), warning)
    }

    #[cfg(test)]
    pub(super) fn load_with_warning() -> (Self, Option<String>) {
        (Self::default(), None)
    }

    pub(super) fn from_app_values(
        volume: u8,
        muted: bool,
        layout_mode: LayoutMode,
        visualizer_mode: VisualizerMode,
    ) -> Self {
        Self {
            volume,
            muted,
            layout_mode: layout_mode_key(layout_mode).to_string(),
            visualizer_mode: visualizer_mode.to_index(),
        }
        .sanitized()
    }

    pub(super) fn volume(&self) -> u8 {
        self.volume
    }

    pub(super) fn muted(&self) -> bool {
        self.muted
    }

    pub(super) fn layout_mode(&self) -> LayoutMode {
        parse_layout_mode_key(&self.layout_mode).unwrap_or(LayoutMode::Split)
    }

    pub(super) fn visualizer_mode(&self) -> usize {
        self.visualizer_mode.min(VISUALIZER_MODE_COUNT - 1)
    }

    #[cfg(not(test))]
    pub(super) fn save(&self) -> anyhow::Result<()> {
        crate::config::save_json(UI_STATE_FILE, &self.clone().sanitized())
    }

    #[cfg(test)]
    pub(super) fn save(&self) -> anyhow::Result<()> {
        Ok(())
    }

    fn sanitized(mut self) -> Self {
        self.volume = self.volume.min(MAX_VOLUME);
        if parse_layout_mode_key(&self.layout_mode).is_none() {
            self.layout_mode = default_layout_mode_key();
        }
        self.visualizer_mode = self.visualizer_mode.min(VISUALIZER_MODE_COUNT - 1);
        self
    }
}

fn default_volume() -> u8 {
    DEFAULT_VOLUME
}

fn default_layout_mode_key() -> String {
    layout_mode_key(LayoutMode::Split).to_string()
}

fn layout_mode_key(layout_mode: LayoutMode) -> &'static str {
    match layout_mode {
        LayoutMode::Split => "split",
        LayoutMode::LeftOnly => "left-only",
        LayoutMode::RightOnly => "right-only",
    }
}

fn parse_layout_mode_key(key: &str) -> Option<LayoutMode> {
    match key {
        "split" => Some(LayoutMode::Split),
        "left-only" => Some(LayoutMode::LeftOnly),
        "right-only" => Some(LayoutMode::RightOnly),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_uses_defaults() {
        let state = UiState::load();

        assert_eq!(state.volume(), 80);
        assert!(!state.muted());
        assert_eq!(state.layout_mode(), LayoutMode::Split);
        assert_eq!(state.visualizer_mode(), 0);
    }

    #[test]
    fn test_load_with_warning_uses_defaults_in_tests() {
        let (state, warning) = UiState::load_with_warning();

        assert_eq!(state.volume(), 80);
        assert!(warning.is_none());
    }

    #[test]
    fn sanitizes_loaded_values() {
        let state = UiState {
            volume: 255,
            muted: true,
            layout_mode: "garbage".to_string(),
            visualizer_mode: 99,
        };

        let state = state.sanitized();

        assert_eq!(state.volume(), 100);
        assert!(state.muted());
        assert_eq!(state.layout_mode(), LayoutMode::Split);
        assert_eq!(state.visualizer_mode(), 2);
    }

    #[test]
    fn layout_mode_keys_roundtrip() {
        for mode in [
            LayoutMode::Split,
            LayoutMode::LeftOnly,
            LayoutMode::RightOnly,
        ] {
            assert_eq!(parse_layout_mode_key(layout_mode_key(mode)), Some(mode));
        }
    }

    #[test]
    fn from_app_values_clamps_visualizer_mode() {
        let state = UiState::from_app_values(
            65,
            true,
            LayoutMode::RightOnly,
            VisualizerMode::SimOscilloscope,
        );

        assert_eq!(state.volume(), 65);
        assert!(state.muted());
        assert_eq!(state.layout_mode(), LayoutMode::RightOnly);
        assert_eq!(state.visualizer_mode(), 2);
    }
}
