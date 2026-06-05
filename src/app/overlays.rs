use super::*;

impl App {
    pub(super) fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
        if self.show_help {
            self.close_context_overlays();
            self.show_settings = false;
        }
    }

    pub(super) fn toggle_station_details(&mut self) {
        self.show_station_details = !self.show_station_details;
        if self.show_station_details {
            self.show_help = false;
            self.show_recent_tracks = false;
            self.show_settings = false;
        }
    }

    pub(super) fn toggle_recent_tracks(&mut self) {
        self.show_recent_tracks = !self.show_recent_tracks;
        if self.show_recent_tracks {
            self.show_help = false;
            self.show_station_details = false;
            self.show_settings = false;
        }
    }

    pub(super) fn close_context_overlays(&mut self) {
        self.show_station_details = false;
        self.show_recent_tracks = false;
    }

    pub(super) fn close_any_overlay(&mut self) -> bool {
        if self.show_help || self.show_station_details || self.show_recent_tracks {
            self.show_help = false;
            self.close_context_overlays();
            true
        } else {
            false
        }
    }

    pub(super) fn toggle_settings(&mut self) {
        self.show_settings = !self.show_settings;
        if self.show_settings {
            self.show_help = false;
            self.close_context_overlays();
        }
    }

    pub(super) fn cycle_layout(&mut self) {
        self.layout_mode = match self.layout_mode {
            LayoutMode::Split => LayoutMode::LeftOnly,
            LayoutMode::LeftOnly => LayoutMode::RightOnly,
            LayoutMode::RightOnly => LayoutMode::Split,
        };
        super::ui_state::save_ui_state_or_notice(self);
    }

    pub(super) fn toggle_visualizer_mode(&mut self) {
        self.visualizer_mode = (self.visualizer_mode + 1) % 3;
        super::ui_state::save_ui_state_or_notice(self);
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
        app.show_settings = true;
        app.show_station_details = true;

        app.toggle_help();

        assert!(app.show_help);
        assert!(!app.show_settings);
        assert!(!app.show_station_details);
    }

    #[test]
    fn toggle_settings_closes_help_and_context_overlays() {
        let mut app = test_app();
        app.show_help = true;
        app.show_recent_tracks = true;

        app.toggle_settings();

        assert!(app.show_settings);
        assert!(!app.show_help);
        assert!(!app.show_recent_tracks);
    }

    #[test]
    fn station_details_and_recent_tracks_are_mutually_exclusive() {
        let mut app = test_app();

        app.toggle_station_details();
        assert!(app.show_station_details);

        app.toggle_recent_tracks();
        assert!(!app.show_station_details);
        assert!(app.show_recent_tracks);
    }

    #[test]
    fn close_any_overlay_closes_help_and_context_overlays() {
        let mut app = test_app();
        app.show_help = true;
        app.show_station_details = true;

        assert!(app.close_any_overlay());
        assert!(!app.show_help);
        assert!(!app.show_station_details);
        assert!(!app.close_any_overlay());
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
