use super::*;

/// Exactly one overlay can be active at a time, or none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ActiveOverlay {
    #[default]
    None,
    Help,
    StationDetails,
    RecentTracks,
    Settings,
    PlaybackDoctor,
    SleepTimer,
}

pub struct Overlays {
    pub active: ActiveOverlay,
    pub selected_setting_idx: usize,
}

impl Default for Overlays {
    fn default() -> Self {
        Self {
            active: ActiveOverlay::None,
            selected_setting_idx: 0,
        }
    }
}

impl App {
    pub(super) fn set_overlay(&mut self, which: ActiveOverlay) {
        let next = if self.overlays.active == which {
            ActiveOverlay::None
        } else {
            which
        };

        self.overlays.active = next;
        if next == ActiveOverlay::SleepTimer {
            self.input_mode = InputMode::SleepTimer;
        } else if self.input_mode == InputMode::SleepTimer {
            self.input_mode = InputMode::Normal;
        }
    }

    pub(super) fn toggle_help(&mut self) {
        self.set_overlay(ActiveOverlay::Help);
    }

    pub(super) fn toggle_station_details(&mut self) {
        self.set_overlay(ActiveOverlay::StationDetails);
    }

    pub(super) fn toggle_recent_tracks(&mut self) {
        self.set_overlay(ActiveOverlay::RecentTracks);
    }

    pub(super) fn toggle_playback_doctor(&mut self) {
        self.set_overlay(ActiveOverlay::PlaybackDoctor);
    }

    pub(super) fn close_any_overlay(&mut self) -> bool {
        if self.overlays.active == ActiveOverlay::None {
            false
        } else {
            self.overlays.active = ActiveOverlay::None;
            if self.input_mode == InputMode::SleepTimer {
                self.input_mode = InputMode::Normal;
            }
            true
        }
    }

    pub(super) fn toggle_settings(&mut self) {
        self.set_overlay(ActiveOverlay::Settings);
    }

    pub(super) fn toggle_sleep_timer_overlay(&mut self) {
        self.set_overlay(ActiveOverlay::SleepTimer);
    }

    #[cfg(test)]
    pub fn show_help(&self) -> bool {
        self.overlays.active == ActiveOverlay::Help
    }

    #[cfg(test)]
    pub fn show_station_details(&self) -> bool {
        self.overlays.active == ActiveOverlay::StationDetails
    }

    #[cfg(test)]
    pub fn show_recent_tracks(&self) -> bool {
        self.overlays.active == ActiveOverlay::RecentTracks
    }

    pub fn show_settings(&self) -> bool {
        self.overlays.active == ActiveOverlay::Settings
    }

    pub(super) fn cycle_layout(&mut self) {
        self.layout_mode = match self.layout_mode {
            LayoutMode::Split => LayoutMode::LeftOnly,
            LayoutMode::LeftOnly => LayoutMode::RightOnly,
            LayoutMode::RightOnly => LayoutMode::Split,
        };
        self.mark_ui_state_dirty();
    }

    pub(super) fn toggle_visualizer_mode(&mut self) {
        self.visualizer_mode = (self.visualizer_mode + 1) % 3;
        self.mark_ui_state_dirty();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::favorites::Library;

    fn test_app() -> App {
        App::new(Library::in_memory(vec![]))
    }

    #[test]
    fn toggle_help_closes_settings_and_context_overlays() {
        let mut app = test_app();
        app.overlays.active = ActiveOverlay::Settings;

        app.toggle_help();

        assert!(app.show_help());
        assert!(!app.show_settings());
        assert!(!app.show_station_details());
    }

    #[test]
    fn toggle_settings_closes_help_and_context_overlays() {
        let mut app = test_app();
        app.overlays.active = ActiveOverlay::Help;

        app.toggle_settings();

        assert!(app.show_settings());
        assert!(!app.show_help());
        assert!(!app.show_recent_tracks());
    }

    #[test]
    fn station_details_and_recent_tracks_are_mutually_exclusive() {
        let mut app = test_app();

        app.toggle_station_details();
        assert!(app.show_station_details());

        app.toggle_recent_tracks();
        assert!(!app.show_station_details());
        assert!(app.show_recent_tracks());
    }

    #[test]
    fn playback_doctor_is_mutually_exclusive() {
        let mut app = test_app();

        app.toggle_station_details();
        assert!(app.show_station_details());

        app.toggle_playback_doctor();
        assert!(!app.show_station_details());
        assert_eq!(app.overlays.active, ActiveOverlay::PlaybackDoctor);

        app.toggle_playback_doctor();
        assert_eq!(app.overlays.active, ActiveOverlay::None);
    }

    #[test]
    fn close_any_overlay_closes_help_and_context_overlays() {
        let mut app = test_app();
        app.overlays.active = ActiveOverlay::Help;

        assert!(app.close_any_overlay());
        assert!(!app.show_help());
        assert!(!app.show_station_details());
        assert!(!app.close_any_overlay());
    }

    #[test]
    fn only_one_overlay_active_at_a_time() {
        let mut app = test_app();

        app.toggle_help();
        assert_eq!(app.overlays.active, ActiveOverlay::Help);

        app.toggle_settings();
        assert_eq!(app.overlays.active, ActiveOverlay::Settings);

        assert!(app.close_any_overlay());
        assert_eq!(app.overlays.active, ActiveOverlay::None);
    }

    #[test]
    fn cycle_layout_wraps() {
        let mut app = test_app();

        app.cycle_layout();
        assert_eq!(app.layout_mode, LayoutMode::LeftOnly);
        app.cycle_layout();
        assert_eq!(app.layout_mode, LayoutMode::RightOnly);
        app.cycle_layout();
        assert_eq!(app.layout_mode, LayoutMode::Split);
    }

    #[test]
    fn toggle_visualizer_mode_wraps() {
        let mut app = test_app();

        app.toggle_visualizer_mode();
        assert_eq!(app.visualizer_mode, 1);
        app.toggle_visualizer_mode();
        assert_eq!(app.visualizer_mode, 2);
        app.toggle_visualizer_mode();
        assert_eq!(app.visualizer_mode, 0);
    }
}
