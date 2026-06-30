use super::*;
use crate::action::Action;

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
    Keybindings,
}

pub struct Overlays {
    pub active: ActiveOverlay,
    pub selected_setting_idx: usize,
    pub keybindings_scroll: usize,
}

impl Default for Overlays {
    fn default() -> Self {
        Self {
            active: ActiveOverlay::None,
            selected_setting_idx: 0,
            keybindings_scroll: 0,
        }
    }
}

impl App {
    pub(super) fn set_overlay(&mut self, which: ActiveOverlay) {
        let next = if self.ui.overlays.active == which {
            ActiveOverlay::None
        } else {
            which
        };

        // Start fade-in animation when opening an overlay.
        if next != ActiveOverlay::None && self.ui.overlays.active == ActiveOverlay::None {
            self.ui.overlay_animation = crate::app::animation::AnimationState::start(0.0, 1.0);
        } else if next == ActiveOverlay::None {
            self.ui.overlay_animation = crate::app::animation::AnimationState::idle();
        }

        self.ui.overlays.active = next;
        if next == ActiveOverlay::SleepTimer {
            self.ui.input_mode = InputMode::SleepTimer;
        } else if self.ui.input_mode == InputMode::SleepTimer {
            self.ui.input_mode = InputMode::Normal;
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
        if self.ui.overlays.active == ActiveOverlay::None {
            false
        } else {
            if self.ui.overlays.active == ActiveOverlay::Settings {
                self.settings_undo.clear();
            }
            self.ui.overlays.active = ActiveOverlay::None;
            if self.ui.input_mode == InputMode::SleepTimer {
                self.ui.input_mode = InputMode::Normal;
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

    pub(super) fn show_keybindings(&mut self) {
        self.ui.overlays.keybindings_scroll = 0;
        self.set_overlay(ActiveOverlay::Keybindings);
    }

    pub(super) fn handle_keybindings_overlay_action(&mut self, action: Action) {
        match action {
            Action::Quit => {
                self.close_any_overlay();
            }
            Action::Tick => self.tick(),
            _ => {}
        }
    }

    pub fn show_help(&self) -> bool {
        self.ui.overlays.active == ActiveOverlay::Help
    }

    #[cfg(test)]
    pub fn show_station_details(&self) -> bool {
        self.ui.overlays.active == ActiveOverlay::StationDetails
    }

    #[cfg(test)]
    pub fn show_recent_tracks(&self) -> bool {
        self.ui.overlays.active == ActiveOverlay::RecentTracks
    }

    pub fn show_settings(&self) -> bool {
        self.ui.overlays.active == ActiveOverlay::Settings
    }

    pub(super) fn cycle_layout(&mut self) {
        self.ui.layout_mode = match self.ui.layout_mode {
            LayoutMode::Split => LayoutMode::LeftOnly,
            LayoutMode::LeftOnly => LayoutMode::RightOnly,
            LayoutMode::RightOnly => LayoutMode::Split,
        };
        self.mark_ui_state_dirty();
    }

    pub(super) fn toggle_visualizer_mode(&mut self) {
        self.ui.visualizer_mode = self.ui.visualizer_mode.next();
        self.mark_ui_state_dirty();
    }

    pub(super) fn toggle_mini_mode(&mut self) {
        if self.ui.input_mode == InputMode::LibraryFilter {
            self.exit_library_filter();
        } else if self.ui.input_mode != InputMode::Normal {
            return;
        }
        self.ui.display_mode = match self.ui.display_mode {
            DisplayMode::Normal => DisplayMode::Mini,
            DisplayMode::Mini => DisplayMode::Normal,
        };
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
        app.ui.overlays.active = ActiveOverlay::Settings;

        app.toggle_help();

        assert!(app.show_help());
        assert!(!app.show_settings());
        assert!(!app.show_station_details());
    }

    #[test]
    fn toggle_settings_closes_help_and_context_overlays() {
        let mut app = test_app();
        app.ui.overlays.active = ActiveOverlay::Help;

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
        assert_eq!(app.ui.overlays.active, ActiveOverlay::PlaybackDoctor);

        app.toggle_playback_doctor();
        assert_eq!(app.ui.overlays.active, ActiveOverlay::None);
    }

    #[test]
    fn close_any_overlay_closes_help_and_context_overlays() {
        let mut app = test_app();
        app.ui.overlays.active = ActiveOverlay::Help;

        assert!(app.close_any_overlay());
        assert!(!app.show_help());
        assert!(!app.show_station_details());
        assert!(!app.close_any_overlay());
    }

    #[test]
    fn only_one_overlay_active_at_a_time() {
        let mut app = test_app();

        app.toggle_help();
        assert_eq!(app.ui.overlays.active, ActiveOverlay::Help);

        app.toggle_settings();
        assert_eq!(app.ui.overlays.active, ActiveOverlay::Settings);

        assert!(app.close_any_overlay());
        assert_eq!(app.ui.overlays.active, ActiveOverlay::None);
    }

    #[test]
    fn cycle_layout_wraps() {
        let mut app = test_app();

        app.cycle_layout();
        assert_eq!(app.ui.layout_mode, LayoutMode::LeftOnly);
        app.cycle_layout();
        assert_eq!(app.ui.layout_mode, LayoutMode::RightOnly);
        app.cycle_layout();
        assert_eq!(app.ui.layout_mode, LayoutMode::Split);
    }

    #[test]
    fn toggle_visualizer_mode_wraps() {
        let mut app = test_app();

        app.toggle_visualizer_mode();
        assert_eq!(app.ui.visualizer_mode, VisualizerMode::RealOscilloscope);
        app.toggle_visualizer_mode();
        assert_eq!(app.ui.visualizer_mode, VisualizerMode::SimOscilloscope);
        app.toggle_visualizer_mode();
        assert_eq!(app.ui.visualizer_mode, VisualizerMode::Spectrum);
    }

    #[test]
    fn show_keybindings_sets_overlay_and_resets_scroll() {
        let mut app = test_app();
        app.ui.overlays.keybindings_scroll = 5;

        app.show_keybindings();

        assert_eq!(app.ui.overlays.active, ActiveOverlay::Keybindings);
        assert_eq!(app.ui.overlays.keybindings_scroll, 0);
    }

    #[test]
    fn show_keybindings_action_opens_overlay() {
        let mut app = test_app();

        app.update(Action::ShowKeybindings);

        assert_eq!(app.ui.overlays.active, ActiveOverlay::Keybindings);
    }

    #[test]
    fn keybindings_overlay_dismiss_on_quit_action() {
        let mut app = test_app();
        app.ui.overlays.active = ActiveOverlay::Keybindings;

        app.update(Action::Quit);

        assert_eq!(app.ui.overlays.active, ActiveOverlay::None);
        assert!(!app.ui.should_quit);
    }

    #[test]
    fn keybindings_overlay_swallows_non_quit_actions() {
        let mut app = test_app();
        app.ui.overlays.active = ActiveOverlay::Keybindings;

        app.update(Action::NextStation);

        assert_eq!(app.ui.overlays.active, ActiveOverlay::Keybindings);
        assert_eq!(app.ui.nav.selected, 0);
    }

    #[test]
    fn show_keybindings_replaces_other_overlay() {
        let mut app = test_app();
        app.ui.overlays.active = ActiveOverlay::Help;

        app.show_keybindings();

        assert_eq!(app.ui.overlays.active, ActiveOverlay::Keybindings);
    }
}
