# PulseDeck 0.4.4 UI Read-Model Plan

Release theme: **Render From a Window, Not From the Whole House**.

0.4.1 stabilized the active audio seam. 0.4.2 removed persistence and search-prefix duplication. 0.4.3 split startup/runtime orchestration away from the terminal loop. 0.4.4 should continue the cleanup by introducing a UI read model so render code no longer needs direct access to the full `App` object.

This is a structural UI release. It must not change visible layout, keybindings, search semantics, playback behavior, station identity, persistence, or audio decoding. The intended user-visible result is boring: PulseDeck should look and behave the same. The intended developer-visible result is important: render modules read a curated `UiModel` instead of rummaging through all of `App`.

---

## Current baseline

Current UI entrypoint:

```rust
// src/ui/mod.rs
pub fn draw(frame: &mut Frame, app: &App) {
    let size = frame.area();
    // background
    // compact terminal guard
    // header
    // station list / search / deck layout
    // controls
    // overlays
    // command palette
}
```

Current top-level dependencies:

```text
src/ui/mod.rs imports:
- crate::app::App
- crate::app::ActiveOverlay
- crate::app::InputMode
- crate::app::LayoutMode
```

Current render modules still accept `&App` directly:

```text
src/ui/header.rs::render(frame, area, app: &App)
src/ui/controls.rs::render(frame, area, app: &App)
src/ui/stations.rs::render(frame, area, app: &App)
src/ui/search.rs::render(frame, area, app: &App)
src/ui/deck/mod.rs::render(frame, area, app: &App)
src/ui/station_details.rs::render(frame, area, app: &App)
src/ui/recent_tracks.rs::render(frame, area, app: &App)
src/ui/help.rs::render(frame, area, app: &App)
src/ui/settings.rs::render(frame, area, app: &App)
src/ui/playback_doctor.rs::render(frame, area, app: &App)
src/ui/sleep_timer.rs::render(frame, area, app: &App)
src/ui/command_palette.rs::render(frame, area, app: &App)
```

Important `App` selector methods already exist:

```rust
// src/app/selectors.rs
pub fn visible_stations(&self) -> Vec<&Station>
pub fn selected_station(&self) -> Option<&Station>
pub fn now_playing(&self) -> Option<&Station>
pub fn visible_count(&self) -> usize
```

Those selectors are the bridge. 0.4.4 should expose their output through `UiModel`, not reimplement selection logic in UI code.

---

## Non-negotiable rules for 0.4.4

### Rule 1: No behavior changes

The following must stay byte-for-byte equivalent in behavior unless tests prove otherwise:

```text
- layout modes: Split, Library Focus, Signal Focus
- overlay priority
- command palette display condition
- compact terminal warning threshold
- visible station ordering
- selected station logic
- now-playing station lookup
- saved/search result markers
- footer hints
- Playback Doctor content
- sleep timer display
- station details grouping
```

### Rule 2: Do not touch active audio code

No edits to these unless strictly required by compilation, which should not happen:

```text
src/audio.rs
src/audio/engine_loop.rs
src/audio/session.rs
src/audio/stream_reader.rs
src/app/playback.rs audio command behavior
```

### Rule 3: Migrate in layers, not with a wrecking ball

Do not convert every UI module in one massive pass. The safe order is:

```text
1. Add src/ui/model.rs.
2. Make ui::draw build UiModel from &App.
3. Convert top-level layout decisions in ui::draw to UiModel.
4. Convert low-risk modules first: header, controls, search.
5. Convert list/detail overlays next.
6. Convert deck/visualizer last because it touches sample buffers and timing.
```

### Rule 4: `UiModel` is read-only

`UiModel` must not own mutation methods. It should expose immutable data and computed view helpers only.

Good:

```rust
pub struct UiModel<'a> {
    pub input_mode: InputMode,
    pub layout_mode: LayoutMode,
    pub active_overlay: ActiveOverlay,
    pub playback: &'a PlaybackView,
    pub selected_station: Option<&'a Station>,
    pub now_playing: Option<&'a Station>,
}
```

Bad:

```rust
impl UiModel<'_> {
    pub fn update(&mut self, action: Action) { ... }
    pub fn remove_station(&mut self) { ... }
}
```

### Rule 5: Avoid cloning large state

`UiModel` should borrow from `App`. It should not clone the station library, search results, history, or sample buffer.

Good:

```rust
pub visible_stations: Vec<&'a Station>
```

Acceptable for 0.4.4 because `App::visible_stations()` already allocates a `Vec<&Station>` today.

Bad:

```rust
pub visible_stations: Vec<Station>
```

---

# Fix A: Add `src/ui/model.rs` with a top-level `UiModel`

## Goal

Create a read-only snapshot of exactly what the UI needs. This is the foundation for all later renderer migration.

## Files and symbols

Add:

```text
src/ui/model.rs
```

Update:

```text
src/ui/mod.rs
```

New symbols:

```text
src/ui/model.rs::UiModel
src/ui/model.rs::UiPlaybackModel, optional later
src/ui/model.rs::UiLibraryModel, optional later
src/ui/model.rs::UiSearchModel, optional later
src/ui/model.rs::UiSettingsModel, optional later
```

Initial `UiModel` should be broad but shallow. Do not over-nest prematurely.

## Proposed initial model

```rust
// src/ui/model.rs
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use crate::app::{
    ActiveOverlay, App, AppNotice, CommandPaletteState, DecoderState, InputMode, LayoutMode,
    Overlays, PlaybackDiagnostics, PlaybackState, PlaybackView, SearchState, SettingRow, SleepTimer,
};
use crate::favorites::Library;
use crate::history::History;
use crate::radio::Station;

pub struct UiModel<'a> {
    pub input_mode: InputMode,
    pub layout_mode: LayoutMode,
    pub active_overlay: ActiveOverlay,
    pub selected_setting_idx: usize,
    pub tick_count: u64,

    pub library: &'a Library,
    pub visible_stations: Vec<&'a Station>,
    pub selected_station: Option<&'a Station>,
    pub now_playing: Option<&'a Station>,
    pub visible_count: usize,

    pub search: &'a SearchState,
    pub command_palette: &'a CommandPaletteState,
    pub player: &'a PlaybackView,
    pub diagnostics: &'a PlaybackDiagnostics,
    pub sleep_timer: &'a SleepTimer,
    pub history: &'a History,
    pub song_history: &'a VecDeque<String>,
    pub notice: Option<&'a AppNotice>,

    pub nav_selected: usize,
    pub nav_selected_genre_idx: usize,
    pub volume: u8,
    pub muted: bool,
    pub visualizer_mode: usize,
    pub visualizer_peaks: &'a [f32],
    pub sample_buffer: &'a Arc<Mutex<VecDeque<f32>>>,
}

impl<'a> UiModel<'a> {
    pub fn from_app(app: &'a App) -> Self {
        Self {
            input_mode: app.input_mode,
            layout_mode: app.layout_mode,
            active_overlay: app.overlays.active,
            selected_setting_idx: app.overlays.selected_setting_idx,
            tick_count: app.tick_count,

            library: &app.library,
            visible_stations: app.visible_stations(),
            selected_station: app.selected_station(),
            now_playing: app.now_playing(),
            visible_count: app.visible_count(),

            search: &app.search,
            command_palette: &app.command_palette,
            player: &app.player,
            diagnostics: &app.diagnostics,
            sleep_timer: &app.sleep_timer,
            history: &app.history,
            song_history: &app.song_history,
            notice: app.notice.current.as_ref(),

            nav_selected: app.nav.selected,
            nav_selected_genre_idx: app.nav.selected_genre_idx,
            volume: app.volume,
            muted: app.muted,
            visualizer_mode: app.visualizer_mode,
            visualizer_peaks: &app.visualizer_peaks,
            sample_buffer: &app.sample_buffer,
        }
    }

    pub fn is_searching(&self) -> bool {
        self.input_mode == InputMode::Search
    }

    pub fn show_command_palette(&self) -> bool {
        self.input_mode == InputMode::CommandPalette
    }
}

impl<'a> From<&'a App> for UiModel<'a> {
    fn from(app: &'a App) -> Self {
        Self::from_app(app)
    }
}
```

## Why broad and shallow first?

A fully beautiful model would have submodels:

```rust
UiModel {
    layout: UiLayoutModel,
    playback: UiPlaybackModel,
    library: UiLibraryModel,
    search: UiSearchModel,
    overlays: UiOverlayModel,
}
```

But doing that immediately risks mixing two hard tasks:

1. introducing a read model
2. redesigning UI data boundaries

For 0.4.4, prefer the bridge model. Once render modules no longer require `&App`, 0.4.5 or later can split `UiModel` into submodels.

## Required tests

Add tests in `src/ui/model.rs`.

### Model captures core layout flags

```rust
#[test]
fn ui_model_captures_layout_overlay_and_input_mode() {
    let mut app = App::new(Library::in_memory(vec![]));
    app.input_mode = InputMode::Search;
    app.layout_mode = LayoutMode::RightOnly;
    app.overlays.active = ActiveOverlay::Help;

    let model = UiModel::from(&app);

    assert!(model.is_searching());
    assert_eq!(model.layout_mode, LayoutMode::RightOnly);
    assert_eq!(model.active_overlay, ActiveOverlay::Help);
}
```

### Model uses existing selectors

```rust
#[test]
fn ui_model_uses_app_selectors_for_visible_selected_and_now_playing() {
    let mut app = App::new(Library::in_memory(vec![
        Station::basic("A", "http://a", "Synthwave", "US", 128),
        Station::basic("B", "http://b", "Synthwave", "US", 128),
    ]));
    app.nav.selected = 1;
    app.player.playing_url = Some("http://a".to_string());

    let model = UiModel::from(&app);

    assert_eq!(model.visible_stations.len(), 2);
    assert_eq!(model.selected_station.map(|station| station.name.as_str()), Some("B"));
    assert_eq!(model.now_playing.map(|station| station.name.as_str()), Some("A"));
}
```

### Model does not clone station data

This is mostly a code-review invariant. If you want a lightweight test, compare pointer identity:

```rust
#[test]
fn ui_model_borrows_visible_station_data() {
    let app = App::new(Library::in_memory(vec![Station::basic(
        "A", "http://a", "Synthwave", "US", 128,
    )]));

    let model = UiModel::from(&app);

    assert!(std::ptr::eq(model.visible_stations[0], &app.library.stations[0]));
}
```

## Pitfalls

### Pitfall: exposing `Overlays` directly

Avoid this if possible:

```rust
pub overlays: &'a Overlays
```

That keeps UI coupled to the whole overlay state bag. Prefer fields that the UI currently needs:

```rust
pub active_overlay: ActiveOverlay
pub selected_setting_idx: usize
```

### Pitfall: cloning command lists every frame too early

`command_palette_commands()` currently lives on `App`. If `command_palette.rs` still needs it during the first migration, either:

1. leave command palette on `&App` for the first pass, or
2. add a model helper that calls the same logic without cloning more than today.

For 0.4.4, it is acceptable to leave command palette conversion for a later step inside this same release, but do not block the top-level `UiModel` on it.

### Pitfall: computing visible stations twice

`selected_station()` currently calls `visible_stations()` internally. If `UiModel::from_app` calls both `visible_stations()` and `selected_station()`, it may allocate twice. That is acceptable as a first pass because UI code already does repeated selector calls. A follow-up optimization can compute selected from `visible_stations`:

```rust
let visible_stations = app.visible_stations();
let selected_station = visible_stations.get(app.nav.selected).copied();
```

Recommended implementation:

```rust
let visible_stations = app.visible_stations();
let selected_station = visible_stations.get(app.nav.selected).copied();
let visible_count = visible_stations.len();
```

Do not use this if it changes behavior in search mode or genre filtering. It should not, because it reuses the selector output.

---

# Fix B: Convert `src/ui/mod.rs::draw` to use `UiModel`

## Goal

Keep the public draw entrypoint unchanged for `main.rs`, but convert the actual render tree root to use `UiModel`.

`main.rs` should still call:

```rust
terminal.draw(|frame| ui::draw(frame, &app))?;
```

`ui::draw` should become a thin adapter:

```rust
pub fn draw(frame: &mut Frame, app: &App) {
    let model = UiModel::from(app);
    draw_model(frame, &model);
}
```

Then the real root render function uses the model:

```rust
fn draw_model(frame: &mut Frame, model: &UiModel<'_>) {
    let size = frame.area();
    // same body as before, but top-level decisions read model
}
```

## Files and symbols

Update:

```text
src/ui/mod.rs
src/ui/model.rs
```

Add module declaration:

```rust
// src/ui/mod.rs
pub mod model;
```

Update imports:

```rust
use crate::app::App;
use model::UiModel;
```

Top-level decisions should change from:

```rust
let is_searching = app.input_mode == InputMode::Search;
match app.layout_mode { ... }
match app.overlays.active { ... }
if app.input_mode == InputMode::CommandPalette { ... }
```

to:

```rust
let is_searching = model.is_searching();
match model.layout_mode { ... }
match model.active_overlay { ... }
if model.show_command_palette() { ... }
```

## Transitional renderer calls

At first, `draw_model` may still call module renderers with `&App` if the module has not been migrated yet. But once `draw_model` only has `&UiModel`, that is not possible. Pick one of these strategies:

### Strategy 1: top-level adapter keeps both `app` and `model`

```rust
pub fn draw(frame: &mut Frame, app: &App) {
    let model = UiModel::from(app);
    draw_model(frame, app, &model);
}

fn draw_model(frame: &mut Frame, app: &App, model: &UiModel<'_>) {
    // top-level decisions use model
    header::render(frame, chunks[0], app); // not migrated yet
}
```

This is the safest first commit. It proves the model and root decisions without forcing every module to migrate immediately.

### Strategy 2: convert all direct children at once

```rust
header::render(frame, chunks[0], model);
stations::render(frame, left_area, model);
```

This is cleaner but much larger. For 0.4.4, use Strategy 1 first, then migrate direct children one by one.

## Required tests

Existing compact terminal tests should still pass:

```text
ui::tests::compact_terminal_rejects_width_below_minimum
ui::tests::compact_terminal_rejects_height_below_minimum
ui::tests::compact_terminal_accepts_exact_minimum
ui::tests::compact_terminal_accepts_larger_terminal
```

Add one small test for the model-powered predicates instead of screenshot testing:

```rust
#[test]
fn ui_model_reports_command_palette_visibility() {
    let mut app = App::new(Library::in_memory(vec![]));
    app.input_mode = InputMode::CommandPalette;

    let model = UiModel::from(&app);

    assert!(model.show_command_palette());
}
```

## Pitfalls

### Pitfall: changing overlay order

Preserve this exact priority:

```rust
match app.overlays.active {
    ActiveOverlay::StationDetails => station_details::render(...),
    ActiveOverlay::RecentTracks => recent_tracks::render(...),
    ActiveOverlay::Help => help::render(...),
    ActiveOverlay::Settings => settings::render(...),
    ActiveOverlay::PlaybackDoctor => playback_doctor::render(...),
    ActiveOverlay::SleepTimer => sleep_timer::render(...),
    ActiveOverlay::None => {}
}

if app.input_mode == InputMode::CommandPalette {
    command_palette::render(...)
}
```

Command palette currently renders after normal overlays. Keep it that way.

### Pitfall: compact terminal guard must run before other model-heavy UI work

Currently `draw` builds no station rows if terminal is too compact. If `UiModel::from(app)` computes visible stations before the compact guard, it may do more work than before on tiny terminals.

Preferred compromise for 0.4.4:

```rust
pub fn draw(frame: &mut Frame, app: &App) {
    let size = frame.area();
    render_background(frame, size);

    if is_compact_terminal(size) {
        render_compact_terminal_warning(frame, size);
        return;
    }

    let model = UiModel::from(app);
    draw_model(frame, size, app, &model);
}
```

This preserves the compact-terminal fast path.

---

# Fix C: Convert low-risk modules from `&App` to `&UiModel`

## Goal

Move the least risky modules away from `&App` first. These modules mostly read scalar fields or existing selector outputs.

Recommended first cluster:

```text
src/ui/header.rs
src/ui/search.rs
src/ui/controls.rs
```

Do **not** start with `stations.rs` or visualizer modules. Those have more list/selection and sample-buffer details.

---

## C1: Convert `src/ui/header.rs`

### Current dependencies

```rust
use crate::app::{App, PlaybackState};

pub fn render(frame: &mut Frame, area: Rect, app: &App) { ... }
fn render_now_playing(frame: &mut Frame, area: Rect, app: &App) { ... }
```

It uses:

```text
app.player.state
app.now_playing()
app.player.current_track
```

### Desired signature

```rust
use crate::app::PlaybackState;
use crate::ui::model::UiModel;

pub fn render(frame: &mut Frame, area: Rect, model: &UiModel<'_>) { ... }
fn render_now_playing(frame: &mut Frame, area: Rect, model: &UiModel<'_>) { ... }
```

### Replacement logic

Change:

```rust
match (&app.player.state, app.now_playing())
```

to:

```rust
match (&model.player.state, model.now_playing)
```

Change:

```rust
if let Some(ref track) = app.player.current_track
```

to:

```rust
if let Some(ref track) = model.player.current_track
```

### Tests

Existing header tests, if any, should pass. If there are no header tests, rely on compile plus full UI tests.

### Pitfalls

Do not clone station name or current track just to satisfy lifetimes. Borrow string slices where possible.

---

## C2: Convert `src/ui/search.rs`

### Current dependencies

```rust
use crate::app::{App, SearchStatus};
```

It uses:

```text
app.search.results
app.nav.selected
app.library.contains_station(station)
app.search.status
app.tick_count
app.search.query
```

### Desired signature

```rust
use crate::app::SearchStatus;
use crate::ui::model::UiModel;

pub fn render(frame: &mut Frame, area: Rect, model: &UiModel<'_>) { ... }
fn highlighted_result_explanation(model: &UiModel<'_>) -> Option<String> { ... }
```

### Replacement logic

Change:

```rust
let result_count = app.search.results.len();
let selected_result_saved = app.search.results
    .get(app.nav.selected)
    .map(|station| app.library.contains_station(station))
    .unwrap_or(false);
```

to:

```rust
let result_count = model.search.results.len();
let selected_result_saved = model.search.results
    .get(model.nav_selected)
    .map(|station| model.library.contains_station(station))
    .unwrap_or(false);
```

Change:

```rust
Span::styled(debounce_indicator_text(query, app.tick_count), theme::dim())
```

to:

```rust
Span::styled(debounce_indicator_text(query, model.tick_count), theme::dim())
```

Change:

```rust
let station = app.search.results.get(app.nav.selected)?;
let query = StationSearchQuery::parse(&app.search.query);
let is_saved = app.library.contains_station(station);
```

to:

```rust
let station = model.search.results.get(model.nav_selected)?;
let query = StationSearchQuery::parse(&model.search.query);
let is_saved = model.library.contains_station(station);
```

### Tests

Existing tests should continue passing:

```text
ui::search::tests::compact_search_label_truncates_long_queries
ui::search::tests::compact_explanation_label_truncates_safely
ui::search::tests::debounce_indicator_text_feels_active_without_saying_soon
ui::search::tests::search_debounce_frame_wraps_through_spinner_frames
ui::search::tests::stale_response_text_reports_discarded_query
```

### Pitfalls

`model.visible_stations` should not be used here. Search bar specifically cares about `model.search.results`, not the visible list title logic.

---

## C3: Convert `src/ui/controls.rs`

### Current dependencies

```rust
use crate::app::{App, AppNotice, InputMode, LayoutMode, PlaybackState};
```

It uses many scalar fields:

```text
app.player.state
app.now_playing()
app.player.current_track
app.layout_mode
app.visualizer_mode
app.sleep_timer.remaining(now)
app.notice.current
app.muted
app.volume
app.show_help()
app.show_station_details()
app.show_recent_tracks()
app.show_sleep_timer()
app.library.settings.save_history
app.input_mode
app.visible_count()
```

### Desired signature

```rust
use crate::app::{AppNotice, InputMode, LayoutMode, PlaybackState};
use crate::ui::model::UiModel;

pub fn render(frame: &mut Frame, area: Rect, model: &UiModel<'_>) { ... }
```

### Required `UiModel` helper methods

Add these helpers to avoid leaking overlay logic back into controls:

```rust
impl UiModel<'_> {
    pub fn show_help(&self) -> bool {
        self.active_overlay == ActiveOverlay::Help
    }

    pub fn show_station_details(&self) -> bool {
        self.active_overlay == ActiveOverlay::StationDetails
    }

    pub fn show_recent_tracks(&self) -> bool {
        self.active_overlay == ActiveOverlay::RecentTracks
    }

    pub fn show_sleep_timer(&self) -> bool {
        self.active_overlay == ActiveOverlay::SleepTimer
    }
}
```

### Replacement logic

Change:

```rust
match (&app.player.state, app.now_playing())
```

to:

```rust
match (&model.player.state, model.now_playing)
```

Change:

```rust
layout_label(app.layout_mode)
visualizer_label(app.visualizer_mode)
```

to:

```rust
layout_label(model.layout_mode)
visualizer_label(model.visualizer_mode)
```

Change:

```rust
if let Some(remaining) = app.sleep_timer.remaining(std::time::Instant::now())
```

to:

```rust
if let Some(remaining) = model.sleep_timer.remaining(std::time::Instant::now())
```

Change:

```rust
if let Some(ref notice) = app.notice.current
```

to:

```rust
if let Some(notice) = model.notice
```

Change:

```rust
if app.visible_count() == 0
```

to:

```rust
if model.visible_count == 0
```

### Tests

Existing controls tests should continue passing:

```text
ui::controls::tests::layout_labels_use_user_facing_focus_terms
ui::controls::tests::visualizer_labels_drop_scope_jargon
```

Add a model helper test:

```rust
#[test]
fn ui_model_overlay_helpers_match_active_overlay() {
    let mut app = App::new(Library::in_memory(vec![]));
    app.overlays.active = ActiveOverlay::RecentTracks;

    let model = UiModel::from(&app);

    assert!(model.show_recent_tracks());
    assert!(!model.show_help());
}
```

### Pitfalls

`notice` is an `Option<&AppNotice>`. Pattern matching changes slightly:

```rust
match notice {
    AppNotice::Info(message) => ...
    AppNotice::Error(message) => ...
}
```

not:

```rust
match &notice { ... }
```

---

# Fix D: Convert station list and detail overlays after the first cluster

## Goal

Once top-level, header, search, and controls compile through `UiModel`, migrate the modules that read station collections and selected stations.

Recommended second cluster:

```text
src/ui/stations.rs
src/ui/station_details.rs
src/ui/recent_tracks.rs
```

---

## D1: Convert `src/ui/stations.rs`

### Current dependencies

```rust
use crate::app::{App, InputMode};
```

It uses:

```text
app.visible_stations()
app.input_mode
app.library.available_genres
app.nav.selected_genre_idx
app.nav.selected
app.player.playing_url
app.library.contains_station(station)
app.search.query
app.search.searching_api
```

### Desired signature

```rust
use crate::app::InputMode;
use crate::ui::model::UiModel;

pub fn render(frame: &mut Frame, area: Rect, model: &UiModel<'_>) { ... }
```

### Replacement logic

Change:

```rust
let visible = app.visible_stations();
```

to:

```rust
let visible = &model.visible_stations;
```

Be careful: existing code may expect `Vec<&Station>` and iterate by value. With `&Vec<&Station>`, iteration yields `&&Station`. Prefer:

```rust
for (idx, station) in model.visible_stations.iter().copied().enumerate() {
    // station: &Station
}
```

Change:

```rust
let is_playing = app.player.playing_url.as_ref() == Some(&station.url);
```

to:

```rust
let is_playing = model.player.playing_url.as_ref() == Some(&station.url);
```

Change:

```rust
let is_selected = app.nav.selected == idx;
```

to:

```rust
let is_selected = model.nav_selected == idx;
```

Change:

```rust
app.library.contains_station(station)
```

to:

```rust
model.library.contains_station(station)
```

Change title helper:

```rust
fn station_list_title(app: &App, visible_count: usize) -> String
```

to:

```rust
fn station_list_title(model: &UiModel<'_>, visible_count: usize) -> String
```

### Required tests

Existing station tests are critical. Keep them green:

```text
ui::stations::tests::empty_library_onboarding_only_renders_for_empty_normal_mode
ui::stations::tests::search_title_explains_preview_and_save_actions
ui::stations::tests::search_truncation_*
ui::stations::tests::station_meta_*
ui::stations::tests::station_health_badge_compares_numeric_timestamps
ui::stations::tests::truncation_*
```

### Pitfalls

#### `visible` type mismatch

A common bug after this conversion:

```rust
let visible = &model.visible_stations;
let station = visible[idx]; // station: &Station, fine
for station in visible { ... } // station: &&Station, maybe not fine
```

Use `.iter().copied()` in loops.

#### Search-mode saved marker

Do not change this condition:

```rust
model.input_mode == InputMode::Search && model.library.contains_station(station)
```

Saved markers in normal library mode are different from saved markers in search mode.

#### Empty onboarding

Do not change:

```rust
model.input_mode == InputMode::Normal && visible_count == 0
```

Search mode with no results should not show first-run library onboarding.

---

## D2: Convert `src/ui/station_details.rs`

### Current dependencies

```rust
use crate::app::App;
```

It uses:

```text
app.selected_station()
app.library.contains_station(station)
app.player.playing_url
app.player.current_track
app.player.state
```

### Desired signature

```rust
use crate::ui::model::UiModel;

pub fn render(frame: &mut Frame, area: Rect, model: &UiModel<'_>) { ... }
fn station_detail_lines(model: &UiModel<'_>) -> Vec<Line<'static>> { ... }
fn station_detail_sections(model: &UiModel<'_>) -> Vec<DetailSection> { ... }
```

### Replacement logic

Change:

```rust
let Some(station) = app.selected_station() else { ... };
```

to:

```rust
let Some(station) = model.selected_station else { ... };
```

Change:

```rust
let saved = if app.library.contains_station(station) { ... }
```

to:

```rust
let saved = if model.library.contains_station(station) { ... }
```

Change:

```rust
.filter(|_| app.player.playing_url.as_ref() == Some(&station.url))
```

to:

```rust
.filter(|_| model.player.playing_url.as_ref() == Some(&station.url))
```

### Required tests

Existing details tests should stay green:

```text
ui::station_details::tests::detail_sections_group_expected_fields
ui::station_details::tests::detail_sections_use_missing_metadata_fallbacks
ui::station_details::tests::local_health_prefers_newer_numeric_failure_over_older_success
ui::station_details::tests::metadata_list_uses_fallback_or_joined_values
ui::station_details::tests::compact_detail_value_truncates_long_metadata
```

### Pitfalls

Some tests inside `station_details.rs` may construct an `App` directly and call private helper functions. After migration, tests should create a `UiModel` from the app before calling helper functions:

```rust
let model = UiModel::from(&app);
let sections = station_detail_sections(&model);
```

Keep the `app` alive for at least as long as `model`.

---

## D3: Convert `src/ui/recent_tracks.rs`

### Current dependencies

```rust
use crate::app::App;
```

It uses:

```text
app.library.settings.save_history
app.history
app.song_history
app.player.state
```

### Desired signature

```rust
use crate::ui::model::UiModel;

pub fn render(frame: &mut Frame, area: Rect, model: &UiModel<'_>) { ... }
fn recent_track_lines(model: &UiModel<'_>) -> Vec<Line<'static>> { ... }
```

### Replacement logic

Change:

```rust
if app.library.settings.save_history { ... }
```

to:

```rust
if model.library.settings.save_history { ... }
```

Change:

```rust
for (idx, entry) in app.history.recent(MAX_VISIBLE_TRACKS).enumerate()
```

to:

```rust
for (idx, entry) in model.history.recent(MAX_VISIBLE_TRACKS).enumerate()
```

Change:

```rust
if app.song_history.is_empty()
```

to:

```rust
if model.song_history.is_empty()
```

### Required tests

Existing tests should pass:

```text
ui::recent_tracks::tests::recent_title_reflects_history_persistence
ui::recent_tracks::tests::recent_overlay_accepts_minimum_area
ui::recent_tracks::tests::recent_overlay_rejects_tiny_area
```

---

# Fix E: Convert settings, help, sleep timer, Playback Doctor, command palette

## Goal

Migrate overlays and command palette once the lower-risk modules are stable.

Recommended third cluster:

```text
src/ui/help.rs
src/ui/settings.rs
src/ui/sleep_timer.rs
src/ui/playback_doctor.rs
src/ui/command_palette.rs
```

---

## E1: Convert `src/ui/help.rs`

### Current dependencies

```text
app.player.state
```

Only needed for critical engine fault banner.

### Desired change

```rust
critical::split_overlay_alert_area(inner_area, &model.player.state);
critical::render_engine_fault_banner(frame, alert_area, &model.player.state);
```

### Pitfall

No behavior change. This is mostly mechanical.

---

## E2: Convert `src/ui/settings.rs`

### Current dependencies

```text
app.overlays.selected_setting_idx
app.library.settings.notifications_enabled
app.library.settings.autoplay_last
app.library.settings.output_device_name
app.library.settings.theme
app.library.settings.stream_metadata_enabled
app.library.settings.save_history
app.player.state
```

### Desired changes

```rust
let is_selected = model.selected_setting_idx == row.index();
let row = SettingRow::from_index(model.selected_setting_idx)
```

Settings rows:

```rust
model.library.settings.notifications_enabled
model.library.settings.autoplay_last
model.library.settings.output_device_name.as_deref()
model.library.settings.theme
model.library.settings.stream_metadata_enabled
model.library.settings.save_history
```

### Pitfall

Do not move settings mutation into `UiModel`. This is render-only.

---

## E3: Convert `src/ui/sleep_timer.rs`

### Current dependencies

```text
app.sleep_timer.remaining(now)
app.sleep_timer.is_waiting_for_playback()
app.sleep_timer.minutes()
app.player.state
```

### Desired changes

```rust
model.sleep_timer.remaining(now)
model.sleep_timer.is_waiting_for_playback()
model.sleep_timer.minutes()
model.player.state
```

### Pitfall

Calls to `Instant::now()` remain inside UI for now. Do not introduce a clock abstraction in this release.

---

## E4: Convert `src/ui/playback_doctor.rs`

### Current dependencies

```text
app.now_playing()
app.player.playing_url
app.player.current_track
app.player.state
app.diagnostics
```

### Desired changes

```rust
let station = model.now_playing.map(|s| s.name.as_str()).unwrap_or("N/A");
let url = model.player.playing_url.as_deref().unwrap_or("N/A");
let track = model.player.current_track.as_deref().unwrap_or("N/A");
let last_error = model.diagnostics.last_error.as_deref().unwrap_or("N/A");
```

### Pitfall

Playback Doctor is a troubleshooting surface. Copy and ordering should not change.

---

## E5: Convert `src/ui/command_palette.rs`

### Current dependencies

```text
app.command_palette.query
app.command_palette_commands()
app.command_palette.selected
command_label(command)
```

This is the trickiest non-visualizer overlay because `command_palette_commands()` is currently an `App` method.

### Recommended 0.4.4 option

Add commands to `UiModel` as an owned vector computed once per frame:

```rust
use crate::app::CommandPaletteCommand;

pub struct UiModel<'a> {
    // ...
    pub command_palette_commands: Vec<CommandPaletteCommand>,
}

impl<'a> UiModel<'a> {
    pub fn from_app(app: &'a App) -> Self {
        Self {
            // ...
            command_palette_commands: app.command_palette_commands(),
        }
    }
}
```

Then command palette rendering becomes:

```rust
let commands = &model.command_palette_commands;
let selected = model
    .command_palette
    .selected
    .min(commands.len().saturating_sub(1));
```

### Pitfall

If `CommandPaletteCommand` is not exported, either export it from `app` or leave `command_palette.rs` as the last module still taking `&App`. Prefer exporting only if the enum is already UI-facing through labels.

Avoid recomputing `command_palette_commands()` inside multiple render helpers.

---

# Fix F: Convert deck and visualizer last

## Goal

Migrate signal-deck rendering to `UiModel` after every other module is stable.

Recommended final cluster:

```text
src/ui/deck/mod.rs
src/ui/deck/meta.rs
src/ui/deck/cassette.rs
src/ui/deck/visualizer/mod.rs
src/ui/deck/visualizer/spectrum.rs
src/ui/deck/visualizer/oscilloscope.rs
```

---

## F1: Convert `src/ui/deck/mod.rs`

### Current dependencies

```text
app.layout_mode
```

### Desired changes

```rust
let full_deck = model.layout_mode == LayoutMode::RightOnly;
```

---

## F2: Convert `src/ui/deck/meta.rs`

### Current dependencies

```text
app.player.state
app.now_playing()
app.player.buffer_percent
app.player.buffer_seconds
```

### Desired changes

```rust
match model.player.state { ... }
let station = model.now_playing;
let filled = (model.player.buffer_percent / 10) as usize;
```

---

## F3: Convert `src/ui/deck/cassette.rs`

### Current dependencies

```text
app.tick_count
app.player.state
```

### Desired changes

```rust
let lines = build_deck_lines(DECK_INNER_WIDTH, model.tick_count, &model.player.state);
```

---

## F4: Convert visualizer modules

### Current dependencies

```text
app.visualizer_mode
app.player.state
app.volume
app.sample_buffer
app.tick_count
app.visualizer_peaks
```

### Desired changes

```rust
model.visualizer_mode
model.player.state
model.volume
model.sample_buffer
model.tick_count
model.visualizer_peaks
```

### Pitfalls

#### Sample buffer lock behavior must not change

Current visualizer behavior:

```rust
if let Ok(buf) = app.sample_buffer.lock() {
    // read samples
}
```

Keep the same non-blocking failure behavior. Do not unwrap the lock.

#### Do not clone sample buffers

`UiModel` should borrow the `Arc<Mutex<VecDeque<f32>>>`. Do not clone the `VecDeque` for rendering.

#### Connecting/fading visualizer behavior must stay

Tests already cover:

```text
ui::deck::visualizer::tests::spectrum_renderer_stays_active_while_connecting
ui::deck::visualizer::tests::spectrum_renderer_stays_active_while_fading_out
ui::deck::visualizer::tests::fading_out_visualizer_gain_uses_audio_ramp_volume
```

Keep those green.

---

# Fix G: Remove `crate::ui::text` compatibility facade if safe

## Goal

Finish the text helper migration started earlier. This is optional for 0.4.4 but pairs well with UI boundary cleanup.

Current situation:

```text
src/text.rs contains the real helpers.
src/ui/text.rs re-exports them.
UI modules still call crate::ui::text::*.
```

Current callers include:

```text
src/ui/stations.rs
src/ui/deck/cassette.rs
```

## Desired change

Replace:

```rust
crate::ui::text::visible_len(value)
crate::ui::text::truncate_to_chars(value, max)
crate::ui::text::truncate_with_ellipsis(value, max)
```

with:

```rust
crate::text::visible_len(value)
crate::text::truncate_to_chars(value, max)
crate::text::truncate_with_ellipsis(value, max)
```

Then delete module declaration:

```rust
// src/ui/mod.rs
pub mod text;
```

And delete file:

```text
src/ui/text.rs
```

## Shell command if deletion is needed locally

If the connected workspace cannot delete the file directly, run locally:

```bash
git rm src/ui/text.rs
```

## Required tests

```text
cargo test text
cargo test ui::stations
cargo test ui::deck
```

## Pitfalls

This is pure path cleanup. Do not change the implementation of char counting or truncation.

---

# Implementation order

## Step 1: Add `UiModel`

Files:

```text
src/ui/model.rs
src/ui/mod.rs
```

Work:

```text
1. Add `pub mod model;`.
2. Add `UiModel<'a>` with shallow borrowed fields.
3. Add `UiModel::from_app` and `impl From<&App>`.
4. Add helper methods: `is_searching`, `show_command_palette`, overlay helpers.
5. Add model tests.
```

Validation:

```text
cargo test ui::model
cargo check
```

## Step 2: Convert top-level `ui::draw`

Files:

```text
src/ui/mod.rs
```

Work:

```text
1. Keep public `draw(frame, app)` signature.
2. Keep compact terminal guard before creating `UiModel`.
3. Create `UiModel` after compact guard.
4. Move current body into `draw_model(frame, size, app, model)` as a transitional helper.
5. Convert top-level layout/overlay/command-palette decisions to `model`.
6. Leave child renderers on `app` for this step if needed.
```

Validation:

```text
cargo test ui::tests
cargo check
```

## Step 3: Convert first renderer cluster

Files:

```text
src/ui/header.rs
src/ui/search.rs
src/ui/controls.rs
src/ui/mod.rs
```

Work:

```text
1. Change render signatures from `&App` to `&UiModel`.
2. Update top-level calls.
3. Replace direct app field access with model fields.
4. Keep tests green.
```

Validation:

```text
cargo test ui::controls
cargo test ui::search
cargo check
```

## Step 4: Convert station/detail/history cluster

Files:

```text
src/ui/stations.rs
src/ui/station_details.rs
src/ui/recent_tracks.rs
src/ui/mod.rs
```

Work:

```text
1. Change render/helper signatures to `&UiModel`.
2. Use `model.visible_stations`, `model.selected_station`, `model.now_playing`.
3. Fix iterator `&&Station` issues with `.iter().copied()`.
4. Update tests to create `UiModel` where helper functions require it.
```

Validation:

```text
cargo test ui::stations
cargo test ui::station_details
cargo test ui::recent_tracks
cargo check
```

## Step 5: Convert overlay/control cluster

Files:

```text
src/ui/help.rs
src/ui/settings.rs
src/ui/sleep_timer.rs
src/ui/playback_doctor.rs
src/ui/command_palette.rs
src/ui/mod.rs
```

Work:

```text
1. Convert simple overlays first: help, sleep_timer, playback_doctor.
2. Convert settings using `selected_setting_idx` and borrowed settings.
3. Convert command palette last, adding `command_palette_commands` to `UiModel` if needed.
```

Validation:

```text
cargo test ui::settings
cargo test ui::sleep_timer
cargo test ui::playback_doctor
cargo test ui::command_palette
cargo check
```

## Step 6: Convert deck/visualizer cluster

Files:

```text
src/ui/deck/mod.rs
src/ui/deck/meta.rs
src/ui/deck/cassette.rs
src/ui/deck/visualizer/mod.rs
src/ui/deck/visualizer/spectrum.rs
src/ui/deck/visualizer/oscilloscope.rs
src/ui/mod.rs
```

Work:

```text
1. Convert deck root to `&UiModel`.
2. Convert meta and cassette.
3. Convert visualizer modules carefully.
4. Preserve sample-buffer lock behavior.
```

Validation:

```text
cargo test ui::deck
cargo check
```

## Step 7: Remove transitional `&App` use from UI render modules

Goal search:

```text
rg "use crate::app::App|&App" src/ui
```

Allowed remaining matches after this release:

```text
src/ui/model.rs uses App to build UiModel
src/ui/mod.rs public draw adapter accepts &App
unit tests may construct App
```

Not allowed:

```text
render(frame, area, app: &App)
helper(app: &App)
```

Validation:

```text
cargo check
```

## Step 8: Optional text facade cleanup

Files:

```text
src/ui/stations.rs
src/ui/deck/cassette.rs
src/ui/mod.rs
src/ui/text.rs
```

Work:

```text
1. Replace `crate::ui::text::*` with `crate::text::*`.
2. Remove `pub mod text;`.
3. Delete `src/ui/text.rs`.
```

Validation:

```text
cargo test text
cargo test ui::stations
cargo test ui::deck
cargo check
```

## Step 9: Docs and release notes

Files:

```text
CHANGELOG.md
README.md, only if code-quality section should mention UiModel
```

Expected changelog entry:

```markdown
## [0.4.4] - Unreleased

### Changed
* **UI rendering boundary**: Introduced `src/ui/model.rs::UiModel` so render modules consume a read-only view model instead of direct access to the full `App` object.
* **Render module migration**: Migrated header, controls, search, station list, overlays, command palette, and deck rendering to `UiModel`.

### Removed
* **UI text facade**: Removed the `src/ui/text.rs` compatibility facade after updating UI modules to use root-level `crate::text` helpers directly.

### Internal
* Added regression coverage for `UiModel` selector wiring, overlay helpers, command palette visibility, and borrowed station data.
```

If `src/ui/text.rs` is not removed in this release, omit the `Removed` entry.

Validation:

```text
cargo check
cargo test
cargo clippy --all-targets --all-features
```

---

# Full validation gate

Before tagging 0.4.4:

```text
cargo fmt
cargo check
cargo test
cargo clippy --all-targets --all-features
```

If `cargo fmt` is blocked in the connected workspace, run locally before commit:

```bash
cargo fmt
```

Expected high-signal targeted tests:

```text
cargo test ui::model
cargo test ui::tests
cargo test ui::controls
cargo test ui::search
cargo test ui::stations
cargo test ui::station_details
cargo test ui::recent_tracks
cargo test ui::settings
cargo test ui::sleep_timer
cargo test ui::playback_doctor
cargo test ui::command_palette
cargo test ui::deck
```

Search checks:

```text
rg "render\(frame: &mut Frame, area: Rect, app: &App\)" src/ui
rg "use crate::app::App" src/ui
rg "crate::ui::text" src/ui
```

Expected after full migration:

```text
- `use crate::app::App` remains only in src/ui/model.rs and src/ui/mod.rs adapter/tests.
- No production renderer accepts `app: &App`.
- No production UI code calls `crate::ui::text::*` if optional text cleanup is completed.
```

---

# Manual smoke checklist

Because 0.4.4 changes render data plumbing, run a UI smoke pass even though behavior should be unchanged.

## Layout

```text
[ ] Start app in normal mode.
[ ] Split View renders header, library, deck, and controls.
[ ] `b` cycles Library Focus and Signal Focus.
[ ] Compact terminal warning still appears below 80x24.
```

## Search

```text
[ ] `/` opens search.
[ ] Typing `tag:ambient` shows debounce indicator and results.
[ ] Search result saved markers still appear.
[ ] Search empty/error/stale states still render correctly.
[ ] `Space` audition and `Enter` save-play still show correct footer hints.
```

## Station list and details

```text
[ ] Genre tabs still show and selection remains correct.
[ ] Current playing station marker still appears.
[ ] Selected row still highlights correctly.
[ ] Empty library onboarding still appears only in normal mode.
[ ] `i` opens Station Details with grouped sections.
```

## Overlays

```text
[ ] `h` help overlay renders.
[ ] `,` settings overlay renders selected row and description.
[ ] `d` Playback Doctor renders state/output/decoder/reconnect info.
[ ] `g` Recent Tracks or Listening History renders depending on setting.
[ ] `t` Sleep Timer renders remaining/waiting/off state.
[ ] Critical playback error banner still appears inside overlays.
```

## Deck

```text
[ ] Cassette/deck renders in Split View and Signal Focus.
[ ] Spectrum mode still animates while connecting.
[ ] Visualizer still reacts during playback.
[ ] Fade-out keeps deck visually active.
```

## Command palette

```text
[ ] `:` or `Ctrl+p` opens command palette above other UI.
[ ] Filtering still works.
[ ] Selected command highlight still works.
[ ] Commands still execute after selection.
```

---

# Edge cases to guard

## Edge case: `UiModel` lifetime with temporary visible list

This is valid:

```rust
let visible_stations = app.visible_stations();
let selected_station = visible_stations.get(app.nav.selected).copied();
```

because `visible_stations` stores references into `app`, and `selected_station` also references `app`. Both live inside `UiModel`.

Do not build selected station from a temporary that is dropped before the model:

```rust
// Bad pattern if not stored in model
let selected_station = app.visible_stations().get(app.nav.selected).copied();
```

The compiler may catch this, but avoid it.

## Edge case: command palette command vector ownership

If `UiModel` owns:

```rust
pub command_palette_commands: Vec<CommandPaletteCommand>
```

then helpers must borrow it:

```rust
let commands = &model.command_palette_commands;
```

Do not clone it repeatedly inside nested render functions.

## Edge case: settings selected row out of bounds

Keep current defensive behavior:

```rust
SettingRow::from_index(model.selected_setting_idx).unwrap_or(SettingRow::Notifications)
```

or whatever fallback the current code uses. Do not unwrap.

## Edge case: visible station list empty but nav selected nonzero

Keep `ListState` selection behavior safe:

```rust
if !model.visible_stations.is_empty() {
    state.select(Some(model.nav_selected));
}
```

Do not select row 0 when the list is empty.

## Edge case: visualizer lock poisoning

Keep:

```rust
if let Ok(buf) = model.sample_buffer.lock() {
    // render from samples
}
```

Do not use:

```rust
model.sample_buffer.lock().unwrap()
```

## Edge case: compact terminal path

Do not build full `UiModel` before returning compact warning unless you intentionally accept that extra work. Preferred path:

```rust
let size = frame.area();
render_background(frame, size);
if is_compact_terminal(size) {
    render_compact_terminal_warning(frame, size);
    return;
}
let model = UiModel::from(app);
```

---

# Rollback strategy

## If `UiModel` causes broad compile churn

Rollback to the first safe checkpoint:

```text
Keep src/ui/model.rs and its tests.
Keep ui::draw adapter if it compiles.
Revert individual renderer migrations.
```

The release can still ship with top-level `draw` using `UiModel` and child renderers using `&App` as a transitional step if needed.

## If station list behavior changes

Check first:

```text
src/ui/model.rs::visible_stations
src/ui/stations.rs iteration over model.visible_stations
src/ui/stations.rs station_list_title
src/ui/stations.rs ListState selection
```

Most bugs here will be `&&Station` iterator mistakes or using `visible_count` from the wrong source.

## If command palette behavior changes

Check:

```text
src/ui/model.rs command_palette_commands field
src/ui/command_palette.rs selected index clamp
src/app/command_palette.rs command_palette_commands implementation
```

Rollback command palette migration if needed. It can remain on `&App` longer than other modules.

## If visualizer behavior changes

Rollback only the deck/visualizer cluster. Keep `UiModel` and other migrated modules. The visualizer is the most timing-sensitive render area because it reads sample buffers and tick counters.

---

# Known non-goals for 0.4.4

Do not include:

```text
- No audio decoder changes.
- No stream compatibility changes.
- No persistence retry throttling.
- No keybinding changes.
- No UI redesign.
- No new layout mode.
- No station ranking or search API changes.
- No settings model redesign.
- No broad app-state split.
- No generic trait-based UI framework.
```

---

# Future work after 0.4.4

Good next releases:

```text
0.4.5: Persistence retry throttling so failed saves do not retry every tick.
0.4.6: Split `UiModel` into focused submodels once renderers depend on it consistently.
0.5.0: Dedicated audio compatibility release with explicit codec/stream QA matrix.
```

Potential post-0.4.4 submodel shape:

```rust
pub struct UiModel<'a> {
    pub layout: UiLayoutModel,
    pub playback: UiPlaybackModel<'a>,
    pub library: UiLibraryModel<'a>,
    pub search: UiSearchModel<'a>,
    pub overlays: UiOverlayModel<'a>,
    pub diagnostics: UiDiagnosticsModel<'a>,
}
```

Only do this after the first migration proves stable.

---

# Final release smell test

0.4.4 is successful if:

```text
- main.rs still calls ui::draw(frame, &app).
- ui::draw immediately adapts App into UiModel after the compact-terminal guard.
- production UI render modules no longer accept &App directly, except the transitional adapter if intentionally left.
- UI behavior looks unchanged.
- full tests and clippy pass.
- no audio files changed.
```

In human terms: the cockpit still looks identical, but the dashboard now gets a clean instrument feed instead of reaching through the firewall with a handful of wires.
