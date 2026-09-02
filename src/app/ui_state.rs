use super::DisplayMode;
use super::LayoutMode;
use super::VisualizerMode;
use serde::{Deserialize, Serialize};
use std::path::Path;

const DEFAULT_VOLUME: u8 = 80;
const MAX_VOLUME: u8 = 100;
const VISUALIZER_MODE_COUNT: usize = 3;
const STALE_SUPPRESSION_SECONDS: u64 = 604_800;
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
    #[allow(dead_code)]
    pub(super) fn load() -> Self {
        Self::load_with_warning().0
    }

    pub(super) fn load_with_warning() -> (Self, Option<String>) {
        let Some(path) = crate::config::config_path(UI_STATE_FILE) else {
            return (Self::default(), None);
        };
        Self::load_from_path(&path)
    }

    pub(super) fn load_from_path(path: &Path) -> (Self, Option<String>) {
        let (state, warning) =
            crate::config::load_json_from_path_with_warning::<Self>(path, UI_STATE_FILE);
        (state.sanitized(), warning)
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

    pub(super) fn save(&self) -> anyhow::Result<()> {
        let Some(path) = crate::config::config_path(UI_STATE_FILE) else {
            return Ok(());
        };
        self.save_to_path(&path)
    }

    fn save_to_path(&self, path: &Path) -> anyhow::Result<()> {
        crate::config::save_json_to_path(path, &self.clone().sanitized())
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
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path(name: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir()
            .join(format!("pulsedeck-ui-state-{}-{nanos}", std::process::id()))
            .join(name)
    }

    #[test]
    fn defaults_are_stable() {
        let state = UiState::default();
        assert_eq!(state.volume(), 80);
        assert!(!state.muted());
        assert_eq!(state.layout_mode(), LayoutMode::Split);
        assert_eq!(state.display_mode(), DisplayMode::Normal);
        assert_eq!(state.visualizer_mode(), 0);
    }

    #[test]
    fn real_persistence_round_trips_sanitized_state() {
        let path = temp_path(UI_STATE_FILE);
        let state = UiState {
            volume: 255,
            muted: true,
            layout_mode: "garbage".to_string(),
            visualizer_mode: 99,
            display_mode: "garbage".to_string(),
            stale_dismissed_at: Some(42),
        };

        state.save_to_path(&path).unwrap();
        let (loaded, warning) = UiState::load_from_path(&path);

        assert!(warning.is_none());
        assert_eq!(loaded.volume(), 100);
        assert!(loaded.muted());
        assert_eq!(loaded.layout_mode(), LayoutMode::Split);
        assert_eq!(loaded.display_mode(), DisplayMode::Normal);
        assert_eq!(loaded.visualizer_mode(), 2);
        assert_eq!(loaded.stale_dismissed_at(), Some(42));
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn replacement_creates_backup_with_previous_state() {
        let path = temp_path(UI_STATE_FILE);
        let first = UiState::from_app_values(
            25,
            false,
            LayoutMode::Split,
            VisualizerMode::RealOscilloscope,
            DisplayMode::Normal,
            None,
        );
        let second = UiState::from_app_values(
            75,
            true,
            LayoutMode::RightOnly,
            VisualizerMode::SimOscilloscope,
            DisplayMode::Mini,
            Some(99),
        );

        first.save_to_path(&path).unwrap();
        second.save_to_path(&path).unwrap();

        let backup = crate::persistence::backup_path(&path);
        let (previous, warning) = UiState::load_from_path(&backup);
        assert!(warning.is_none());
        assert_eq!(previous.volume(), 25);
        assert!(!previous.muted());
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn malformed_state_returns_defaults_and_warning() {
        let path = temp_path(UI_STATE_FILE);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "{broken").unwrap();

        let (state, warning) = UiState::load_from_path(&path);

        assert_eq!(state.volume(), DEFAULT_VOLUME);
        assert!(warning.unwrap().contains("Could not parse ui-state.json"));
        let _ = fs::remove_dir_all(path.parent().unwrap());
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
    fn stale_notice_suppression_boundaries_are_stable() {
        let now = 1_700_000_000;
        assert!(!should_suppress_stale_notice(None, now));
        assert!(should_suppress_stale_notice(Some(now - 3600), now));
        assert!(!should_suppress_stale_notice(Some(now - 604_800), now));
        assert!(should_suppress_stale_notice(Some(now + 3600), now));
    }
}
