use std::collections::VecDeque;
use std::time::Duration;

use crate::app::breadcrumb::compute_breadcrumb;
use crate::app::{
    ActiveOverlay, App, CommandPaletteState, DisplayMode, InputMode, LayoutMode, Navigation,
    NoticeState, Overlays, PaletteCommand, PlaybackDiagnostics, PlaybackState, PlaybackView,
    SearchState, SettingRow, SleepTimer, VisualizerMode,
};
use crate::elapsed_format::format_elapsed;
use crate::favorites::Library;
use crate::favorites_set::FavoritesSet;
use crate::history::History;
use crate::library_sort::SortMode;
use crate::radio::Station;
use crate::recommend::ScoredStation;

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
    pub display_mode: DisplayMode,
    pub elapsed_display: Option<String>,
    pub volume_flash_active: bool,
    pub discover_results: &'a [ScoredStation],
    pub discover_cursor: usize,
    pub discover_results_empty: bool,
    pub exclude_tags: Vec<String>,
    pub exclude_countries: Vec<String>,
    pub search_history_empty: bool,
    pub settings_undo_available: [bool; SettingRow::COUNT],
    pub sort_mode: SortMode,
    pub breadcrumb: String,
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

    pub fn has_settings_undo(&self, row: SettingRow) -> bool {
        self.settings_undo_available[row.index()]
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

        let elapsed_display = elapsed_display_for_state(
            &app.playback.view.state,
            app.playback.elapsed_timer.elapsed(),
        );

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
            display_mode: app.ui.display_mode,
            elapsed_display,
            volume_flash_active: app.ui.volume_flash_remaining > Duration::ZERO,
            discover_results: &app.discover_results,
            discover_cursor: app.discover_cursor,
            discover_results_empty: app.discover_results.is_empty(),
            exclude_tags: app.config.discover.exclude_tags.clone(),
            exclude_countries: app.config.discover.exclude_countries.clone(),
            search_history_empty: app.search_history.is_empty(),
            settings_undo_available: SettingRow::ALL
                .map(|row| app.settings_undo.has_entry(row.index())),
            sort_mode: app.sort_mode,
            breadcrumb: compute_breadcrumb(
                app.ui.overlays.active,
                &app.ui.input_mode,
                &app.search.query,
                app.library
                    .available_genres
                    .get(app.ui.nav.selected_genre_idx)
                    .map(|s| s.as_str()),
                app.search.results.len(),
            ),
            visible_stations: app.visible_stations(),
            now_playing: app.now_playing(),
        }
    }
}

/// Compute the formatted elapsed display string.
/// Returns Some when Playing or Paused; None when Stopped or other states.
fn elapsed_display_for_state(
    state: &PlaybackState,
    elapsed: std::time::Duration,
) -> Option<String> {
    match state {
        PlaybackState::Playing | PlaybackState::Paused => Some(format_elapsed(elapsed)),
        _ => None,
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

    #[test]
    fn ui_model_populates_display_mode_normal() {
        let mut app = App::new(Library::in_memory(vec![]));
        app.ui.display_mode = DisplayMode::Normal;

        let model = UiModel::from(&app);

        assert_eq!(model.display_mode, DisplayMode::Normal);
    }

    #[test]
    fn ui_model_populates_display_mode_mini() {
        let mut app = App::new(Library::in_memory(vec![]));
        app.ui.display_mode = DisplayMode::Mini;

        let model = UiModel::from(&app);

        assert_eq!(model.display_mode, DisplayMode::Mini);
    }

    #[test]
    fn ui_model_elapsed_display_some_when_playing() {
        let mut app = App::new(Library::in_memory(vec![]));
        app.playback.view.state = PlaybackState::Playing;
        app.playback.elapsed_timer.start();
        app.playback
            .elapsed_timer
            .tick(std::time::Duration::from_secs(67));

        let model = UiModel::from(&app);

        assert_eq!(model.elapsed_display, Some("01:07".to_string()));
    }

    #[test]
    fn ui_model_elapsed_display_some_when_paused() {
        let mut app = App::new(Library::in_memory(vec![]));
        app.playback.view.state = PlaybackState::Paused;
        app.playback.elapsed_timer.start();
        app.playback
            .elapsed_timer
            .tick(std::time::Duration::from_secs(3661));
        app.playback.elapsed_timer.pause();

        let model = UiModel::from(&app);

        assert_eq!(model.elapsed_display, Some("1:01:01".to_string()));
    }

    #[test]
    fn ui_model_elapsed_display_none_when_stopped() {
        let app = App::new(Library::in_memory(vec![]));

        let model = UiModel::from(&app);

        assert_eq!(model.elapsed_display, None);
    }

    #[test]
    fn ui_model_elapsed_display_none_when_connecting() {
        let mut app = App::new(Library::in_memory(vec![]));
        app.playback.view.state = PlaybackState::Connecting;

        let model = UiModel::from(&app);

        assert_eq!(model.elapsed_display, None);
    }

    #[test]
    fn ui_model_populates_discover_exclusion_diagnostics_empty_results() {
        let mut app = App::new(Library::in_memory(vec![]));
        app.config.discover.exclude_tags = vec!["politics".to_string(), "news".to_string()];
        app.config.discover.exclude_countries = vec!["US".to_string()];

        let model = UiModel::from(&app);

        assert!(model.discover_results_empty);
        assert_eq!(model.exclude_tags, vec!["politics", "news"]);
        assert_eq!(model.exclude_countries, vec!["US"]);
    }

    #[test]
    fn ui_model_populates_discover_exclusion_diagnostics_with_results() {
        use crate::recommend::ScoredStation;

        let mut app = App::new(Library::in_memory(vec![]));
        app.config.discover.exclude_tags = vec!["ads".to_string()];
        app.discover_results = vec![ScoredStation {
            station: Station::basic("Test", "http://test", "Jazz", "DE", 128),
            score: 5,
        }];

        let model = UiModel::from(&app);

        assert!(!model.discover_results_empty);
        assert_eq!(model.exclude_tags, vec!["ads"]);
    }

    #[test]
    fn settings_undo_marker_shown_after_change() {
        use crate::app::SettingSnapshot;

        let mut app = App::new(Library::in_memory(vec![]));
        app.settings_undo.capture(
            SettingRow::Notifications.index(),
            SettingSnapshot::Bool(true),
        );

        let model = UiModel::from(&app);

        assert!(model.has_settings_undo(SettingRow::Notifications));
    }

    #[test]
    fn settings_undo_marker_removed_after_undo() {
        use crate::app::SettingSnapshot;

        let mut app = App::new(Library::in_memory(vec![]));
        app.settings_undo.capture(
            SettingRow::Theme.index(),
            SettingSnapshot::String("dark".to_string()),
        );
        app.settings_undo.take(SettingRow::Theme.index());

        let model = UiModel::from(&app);

        assert!(!model.has_settings_undo(SettingRow::Theme));
    }

    #[test]
    fn settings_undo_marker_absent_with_no_change() {
        let app = App::new(Library::in_memory(vec![]));

        let model = UiModel::from(&app);

        for row in SettingRow::ALL {
            assert!(!model.has_settings_undo(row));
        }
    }
}
