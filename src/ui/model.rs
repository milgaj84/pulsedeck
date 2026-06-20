use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use crate::app::{
    ActiveOverlay, App, CommandPaletteState, InputMode, LayoutMode, Navigation,
    NoticeState, Overlays, PaletteCommand, PlaybackDiagnostics, PlaybackView,
SearchState, SleepTimer,
};
use crate::favorites::{resolve_parent_genre, Library};
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
    pub sample_buffer: &'a Arc<Mutex<VecDeque<f32>>>,
    pub visualizer_mode: usize,
    pub visualizer_peaks: &'a [f32],
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
        Self {
            library: &app.library,
            nav: &app.nav,
            search: &app.search,
            command_palette: &app.command_palette,
            command_palette_commands: app.command_palette_commands(),
            player: &app.player,
            volume: app.volume,
            muted: app.muted,
            notice: &app.notice,
            input_mode: app.input_mode.clone(),
            tick_count: app.tick_count,
            layout_mode: app.layout_mode,
            overlays: &app.overlays,
            song_history: &app.song_history,
            diagnostics: &app.diagnostics,
            sleep_timer: &app.sleep_timer,
            history: &app.history,
            sample_buffer: &app.sample_buffer,
            visualizer_mode: app.visualizer_mode,
            visualizer_peaks: &app.visualizer_peaks,
            visible_stations: visible_stations_for(app),
            now_playing: app.now_playing(),
        }
    }
}

fn visible_stations_for(app: &App) -> Vec<&Station> {
    if app.input_mode == InputMode::Search {
        return app.search.results.iter().collect();
    }

    if let Some(genre) = app.library.available_genres.get(app.nav.selected_genre_idx) {
        if genre == "All" {
            app.library.stations.iter().collect()
        } else {
            app.library
                .stations
                .iter()
                .filter(|station| resolve_parent_genre(&station.genre).eq_ignore_ascii_case(genre))
                .collect()
        }
    } else {
        app.library.stations.iter().collect()
    }
}
