use super::*;

impl App {
    pub(super) fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
        if self.show_help {
            self.show_settings = false;
        }
    }

    pub(super) fn toggle_settings(&mut self) {
        self.show_settings = !self.show_settings;
        if self.show_settings {
            self.show_help = false;
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

    pub(super) fn next_deck_page(&mut self) {
        self.active_deck_page = (self.active_deck_page + 1) % 2;
        self.pending_tape_delete = None;
        if self.active_deck_page == 1 {
            self.request_tape_archive_scan_if_needed();
        }
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
    fn toggle_help_closes_settings() {
        let mut app = test_app();
        app.show_settings = true;

        app.toggle_help();

        assert!(app.show_help);
        assert!(!app.show_settings);
    }

    #[test]
    fn toggle_settings_closes_help() {
        let mut app = test_app();
        app.show_help = true;

        app.toggle_settings();

        assert!(app.show_settings);
        assert!(!app.show_help);
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
    fn next_deck_page_wraps() {
        let mut app = test_app();

        app.next_deck_page();
        assert_eq!(app.active_deck_page, 1);
        assert!(app.tape_archive_scan_requested);
        app.next_deck_page();
        assert_eq!(app.active_deck_page, 0);
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
