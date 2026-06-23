use std::collections::VecDeque;

use crate::app::{
    ActiveOverlay, App, CommandPaletteState, InputMode, LayoutMode, Navigation, NoticeState,
    Overlays, PaletteCommand, PlaybackDiagnostics, PlaybackView, SearchState, SleepTimer,
    VisualizerMode,
};
use crate::favorites::Library;
use crate::favorites_set::FavoritesSet;
use crate::history::History;
use crate::radio::Station;

pub struct UiModel<'a> {
    pub library: &'a Library,
    pub nav: &'a Navigation,
    pub search: &'a SearchState,
    pub command_palette: &'a CommandPaletteState,
    pub command_palette_commands: Vec<PaletteCommand>,
    pub player: &'a PlaybackView,
    pub volume: u8,
    pub muted: bool,
    pub notice: &'a NoticeState,
    pub input_mode: InputMode,
    pub tick_count: u64,
    pub layout_mode: LayoutMode,
    pub overlays: &'a Overlays,
    pub song_history: &'a VecDeque<String>,
    pub diagnostics: &'a PlaybackDiagnostics,
    pub sleep_timer: &'a SleepTimer,
    pub history: &'a History,
    pub samples: Vec<f32>,
    pub visualizer_mode: VisualizerMode,
    pub visualizer_peaks: &'a [f32],
    pub library_filter_query: &'a str,
    pub library_filter_active: bool,
    pub number_jump_display: &'a str,
    pub number_jump_active: bool,
    pub favorites: &'a FavoritesSet,
    visible_stations: Vec<&'a Station>,
    now_playing: Option<&'a Station>,
}

impl<'a> UiModel<'a> {
    pub fn visible_stations(&self) -> &[&'a Station] {
        &self.visible_stations
    }

    pub fn selected_station(&self) -> Option<&'a Station> {
        self.visible_stations.get(self.nav.selected).copied()
    }

    pub fn now_playing(&self) -> Option<&'a Station> {
        self.now_playing
    }

    pub fn visible_count(&self) -> usize {
        self.visible_stations.len()
    }

    pub fn command_palette_commands(&self) -> &[PaletteCommand] {
        &self.command_palette_commands
    }

    pub fn show_help(&self) -> bool {
        self.overlays.active == ActiveOverlay::Help
    }

    pub fn show_station_details(&self) -> bool {
        self.overlays.active == ActiveOverlay::StationDetails
    }

    pub fn show_recent_tracks(&self) -> bool {
        self.overlays.active == ActiveOverlay::RecentTracks
    }

    pub fn show_sleep_timer(&self) -> bool {
        self.overlays.active == ActiveOverlay::SleepTimer
    }
}

impl<'a> From<&'a App> for UiModel<'a> {
    fn from(app: &'a App) -> Self {
        let samples = app
            .playback
            .sample_buffer
            .lock()
            .map(|buf| buf.iter().copied().collect())
            .unwrap_or_default();

        Self {
            library: &app.library,
            nav: &app.ui.nav,
            search: &app.search,
            command_palette: &app.ui.command_palette,
            command_palette_commands: app.command_palette_commands(),
            player: &app.playback.view,
            volume: app.playback.volume,
            muted: app.playback.muted,
            notice: &app.ui.notice,
            input_mode: app.ui.input_mode.clone(),
            tick_count: app.ui.tick_count,
            layout_mode: app.ui.layout_mode,
            overlays: &app.ui.overlays,
            song_history: &app.song_history,
            diagnostics: &app.playback.diagnostics,
            sleep_timer: &app.playback.sleep_timer,
            history: &app.history,
            samples,
            visualizer_mode: app.ui.visualizer_mode,
            visualizer_peaks: &app.ui.visualizer_peaks,
            library_filter_query: &app.library_filter_query,
            library_filter_active: app.ui.input_mode == InputMode::LibraryFilter,
            number_jump_display: app.number_jump.display(),
            number_jump_active: app.number_jump.is_active(),
            favorites: &app.library.settings.favorites,
            visible_stations: app.visible_stations(),
            now_playing: app.now_playing(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::favorites::Library;
    use crate::radio::Station;

    #[test]
    fn ui_model_captures_layout_overlay_and_input_mode() {
        let mut app = App::new(Library::in_memory(vec![]));
        app.ui.input_mode = InputMode::Search;
        app.ui.layout_mode = LayoutMode::RightOnly;
        app.ui.overlays.active = ActiveOverlay::Help;

        let model = UiModel::from(&app);

        assert_eq!(model.input_mode, InputMode::Search);
        assert_eq!(model.layout_mode, LayoutMode::RightOnly);
        assert_eq!(model.overlays.active, ActiveOverlay::Help);
    }

    #[test]
    fn ui_model_uses_app_selectors_for_visible_selected_and_now_playing() {
        let mut app = App::new(Library::in_memory(vec![
            Station::basic("A", "http://a", "Synthwave", "US", 128),
            Station::basic("B", "http://b", "Synthwave", "US", 128),
        ]));
        app.ui.nav.selected = 1;
        app.playback.view.playing_url = Some("http://a".to_string());

        let model = UiModel::from(&app);

        assert_eq!(model.visible_stations().len(), 2);
        assert_eq!(
            model
                .selected_station()
                .map(|station| station.name.as_str()),
            Some("B")
        );
        assert_eq!(
            model.now_playing().map(|station| station.name.as_str()),
            Some("A")
        );
    }

    #[test]
    fn ui_model_borrows_visible_station_data() {
        let app = App::new(Library::in_memory(vec![Station::basic(
            "A",
            "http://a",
            "Synthwave",
            "US",
            128,
        )]));

        let model = UiModel::from(&app);

        assert!(std::ptr::eq(
            model.visible_stations()[0],
            &app.library.stations[0],
        ));
    }

    #[test]
    fn ui_model_reports_command_palette_visibility() {
        let mut app = App::new(Library::in_memory(vec![]));
        app.ui.input_mode = InputMode::CommandPalette;

        let model = UiModel::from(&app);

        assert_eq!(model.input_mode, InputMode::CommandPalette);
    }

    #[test]
    fn ui_model_overlay_helpers_match_active_overlay() {
        let mut app = App::new(Library::in_memory(vec![]));
        app.ui.overlays.active = ActiveOverlay::RecentTracks;

        let model = UiModel::from(&app);

        assert!(model.show_recent_tracks());
        assert!(!model.show_help());
        assert!(!model.show_station_details());
        assert!(!model.show_sleep_timer());
    }
}
