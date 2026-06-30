use super::DisplayMode;
use super::LayoutMode;
use super::VisualizerMode;
use serde::{Deserialize, Serialize};

const DEFAULT_VOLUME: u8 = 80;
const MAX_VOLUME: u8 = 100;
const VISUALIZER_MODE_COUNT: usize = 3;
/// Suppression window: 7 days in seconds.
const STALE_SUPPRESSION_SECONDS: u64 = 604_800;
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
    #[serde(default = "default_display_mode_key")]
    display_mode: String,
    #[serde(default)]
    pub stale_dismissed_at: Option<u64>,
}

/// Returns `true` when the stale notice should be suppressed (dismissed < 7 days ago).
pub(super) fn should_suppress_stale_notice(dismissed_at: Option<u64>, now_epoch: u64) -> bool {
    match dismissed_at {
        None => false,
        Some(at) => now_epoch.saturating_sub(at) < STALE_SUPPRESSION_SECONDS,
    }
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            volume: DEFAULT_VOLUME,
            muted: false,
            layout_mode: default_layout_mode_key(),
            visualizer_mode: 0,
            display_mode: default_display_mode_key(),
            stale_dismissed_at: None,
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
        display_mode: DisplayMode,
        stale_dismissed_at: Option<u64>,
    ) -> Self {
        Self {
            volume,
            muted,
            layout_mode: layout_mode_key(layout_mode).to_string(),
            visualizer_mode: visualizer_mode.to_index(),
            display_mode: display_mode_key(display_mode).to_string(),
            stale_dismissed_at,
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

    pub(super) fn display_mode(&self) -> DisplayMode {
        parse_display_mode_key(&self.display_mode).unwrap_or(DisplayMode::Normal)
    }

    pub(super) fn visualizer_mode(&self) -> usize {
        self.visualizer_mode.min(VISUALIZER_MODE_COUNT - 1)
    }

    pub(super) fn stale_dismissed_at(&self) -> Option<u64> {
        self.stale_dismissed_at
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
        if parse_display_mode_key(&self.display_mode).is_none() {
            self.display_mode = default_display_mode_key();
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

fn default_display_mode_key() -> String {
    display_mode_key(DisplayMode::Normal).to_string()
}

fn display_mode_key(display_mode: DisplayMode) -> &'static str {
    match display_mode {
        DisplayMode::Normal => "normal",
        DisplayMode::Mini => "mini",
    }
}

fn parse_display_mode_key(key: &str) -> Option<DisplayMode> {
    match key {
        "normal" => Some(DisplayMode::Normal),
        "mini" => Some(DisplayMode::Mini),
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
        assert_eq!(state.display_mode(), DisplayMode::Normal);
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
            display_mode: "garbage".to_string(),
            stale_dismissed_at: None,
        };

        let state = state.sanitized();

        assert_eq!(state.volume(), 100);
        assert!(state.muted());
        assert_eq!(state.layout_mode(), LayoutMode::Split);
        assert_eq!(state.display_mode(), DisplayMode::Normal);
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
    fn display_mode_keys_roundtrip() {
        for mode in [DisplayMode::Normal, DisplayMode::Mini] {
            assert_eq!(parse_display_mode_key(display_mode_key(mode)), Some(mode));
        }
    }

    #[test]
    fn display_mode_invalid_key_defaults_to_normal() {
        let state = UiState {
            display_mode: "invalid".to_string(),
            ..UiState::default()
        };

        assert_eq!(state.display_mode(), DisplayMode::Normal);
    }

    #[test]
    fn display_mode_sanitized_clamps_invalid() {
        let state = UiState {
            display_mode: "unknown".to_string(),
            ..UiState::default()
        };

        let state = state.sanitized();

        assert_eq!(state.display_mode, "normal");
    }

    #[test]
    fn from_app_values_clamps_visualizer_mode() {
        let state = UiState::from_app_values(
            65,
            true,
            LayoutMode::RightOnly,
            VisualizerMode::SimOscilloscope,
            DisplayMode::Mini,
            None,
        );

        assert_eq!(state.volume(), 65);
        assert!(state.muted());
        assert_eq!(state.layout_mode(), LayoutMode::RightOnly);
        assert_eq!(state.display_mode(), DisplayMode::Mini);
        assert_eq!(state.visualizer_mode(), 2);
    }

    #[test]
    fn should_suppress_stale_notice_none_not_suppressed() {
        assert!(!should_suppress_stale_notice(None, 1_700_000_000));
    }

    #[test]
    fn should_suppress_stale_notice_recent_timestamp_suppressed() {
        let now = 1_700_000_000;
        let dismissed = now - 3600; // 1 hour ago
        assert!(should_suppress_stale_notice(Some(dismissed), now));
    }

    #[test]
    fn should_suppress_stale_notice_seven_plus_days_not_suppressed() {
        let now = 1_700_000_000;
        let dismissed = now - 604_800; // exactly 7 days ago
        assert!(!should_suppress_stale_notice(Some(dismissed), now));

        let dismissed_older = now - 700_000; // more than 7 days
        assert!(!should_suppress_stale_notice(Some(dismissed_older), now));
    }

    #[test]
    fn should_suppress_stale_notice_future_timestamp_suppressed() {
        let now = 1_700_000_000;
        let dismissed = now + 3600; // future timestamp
        assert!(should_suppress_stale_notice(Some(dismissed), now));
    }

    #[test]
    fn from_app_values_preserves_stale_dismissed_at() {
        let state = UiState::from_app_values(
            80,
            false,
            LayoutMode::Split,
            VisualizerMode::RealOscilloscope,
            DisplayMode::Normal,
            Some(1_700_000_000),
        );

        assert_eq!(state.stale_dismissed_at(), Some(1_700_000_000));
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    // Feature: v0113-code-quality, Property 3: UiState JSON Round-Trip
    // For all valid field combinations, serialize → deserialize produces identical values.
    proptest! {
        #[test]
        fn prop_ui_state_json_round_trip(
            volume in 0u8..=100,
            muted in proptest::bool::ANY,
            layout_idx in 0usize..3,
            vis_idx in 0usize..3,
            display_idx in 0usize..2,
            stale_at in proptest::option::of(0u64..2_000_000_000)
        ) {
            let layouts = [LayoutMode::Split, LayoutMode::LeftOnly, LayoutMode::RightOnly];
            let vis_modes = [
                VisualizerMode::Spectrum,
                VisualizerMode::RealOscilloscope,
                VisualizerMode::SimOscilloscope,
            ];
            let displays = [DisplayMode::Normal, DisplayMode::Mini];

            let state = UiState::from_app_values(
                volume,
                muted,
                layouts[layout_idx],
                vis_modes[vis_idx],
                displays[display_idx],
                stale_at,
            );

            // Serialize to JSON
            let json = serde_json::to_string(&state).expect("serialize");

            // Deserialize back
            let reloaded: UiState = serde_json::from_str(&json).expect("deserialize");

            // Verify all fields survived
            prop_assert_eq!(reloaded.volume(), state.volume());
            prop_assert_eq!(reloaded.muted(), state.muted());
            prop_assert_eq!(reloaded.layout_mode(), state.layout_mode());
            prop_assert_eq!(reloaded.visualizer_mode(), state.visualizer_mode());
            prop_assert_eq!(reloaded.display_mode(), state.display_mode());
            prop_assert_eq!(reloaded.stale_dismissed_at(), state.stale_dismissed_at());
        }
    }
}
