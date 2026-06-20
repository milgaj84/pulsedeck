# PulseDeck 0.4.3 Runtime Separation Plan

Release theme: **Constructor Detox, Runtime Wiring, and Testable Background Work**.

0.4.1 stabilized the active playback path. 0.4.2 removed persistence and search-prefix duplication. 0.4.3 should continue the same maintenance thread by separating pure app state from runtime side effects and moving background task orchestration out of `src/main.rs`.

This release is deliberately structural. It should make PulseDeck easier to test and harder to break, without changing decoder behavior, reconnect behavior, search ranking, UI layout, keybindings, or library file format.

---

## Current baseline

Important current facts from the 0.4.2 tree:

```text
src/app/lifecycle.rs::App::new currently:
- loads UI state from disk
- creates the visualizer sample buffer
- spawns AudioEngine
- loads History from disk
- builds PlaybackDiagnostics
- constructs App
- sends output-device, metadata, and volume commands
- aggregates startup warnings
- optionally autoplays the last stream

src/main.rs::main currently:
- handles CLI short-circuit
- loads Library
- applies saved theme
- constructs App
- initializes Ratatui
- owns search channels
- owns metadata refresh channels
- owns search debounce timing
- spawns Radio Browser search tasks
- spawns library metadata refresh tasks
- drains worker responses
- owns the terminal frame loop
```

Validation before writing this plan:

```text
cargo check                         passed
cargo test                          passed, 321 tests
cargo clippy --all-targets --all-features passed
```

0.4.3 should preserve that baseline and add tests around the new seams.

---

## Non-negotiable rules for 0.4.3

### Rule 1: Do not change audio semantics

The active playback path remains:

```text
src/app/playback.rs::play_selected
  -> src/app/playback.rs::send_audio_command
  -> src/audio.rs::AudioEngine::send
  -> src/audio/engine_loop.rs::audio_loop
  -> src/audio/engine_loop.rs::AudioLoopState
  -> src/audio/session.rs::connect_and_decode
  -> src/audio/session.rs::try_connect_and_decode_once
  -> src/audio/stream_reader.rs::StreamReader
  -> rodio::Sink
```

0.4.3 may change how the audio engine is injected into `App`, but it must not change:

```text
- AudioCommand variants
- AudioStatus variants
- Decoder::new_mp3 usage
- StreamReader behavior
- fade timing
- reconnect limits
- hardware output retry count
- output device switching semantics
```

### Rule 2: Keep `App::new` as the public convenience constructor

Users of the app code should still be able to call:

```rust
let app = App::new(library);
```

But internally `App::new` should delegate to a testable constructor that accepts already-loaded dependencies.

### Rule 3: Avoid fake trait kingdoms in 0.4.3

A full dependency-inversion pass can wait. The immediate win is to introduce concrete `AppParts` and `AppDriver` seams. Do not create a giant `Services` trait or broad trait-object framework.

Good 0.4.3 abstractions:

```text
AppParts
AppRuntimeLoader
StartupWarnings
AppDriver
SearchWorkerResponse
MetadataRefreshResponse
```

Bad 0.4.3 abstractions:

```text
trait EverythingService
Box<dyn AppWorld>
generic lifetime-heavy App<'a, TAudio, THistory, TUiState, TClock, TSearch, TMetadata>
```

### Rule 4: Main loop extraction must be behavior-preserving

`src/main.rs::main` can become smaller, but the frame loop order must remain effectively the same:

```text
1. draw UI
2. poll input or tick
3. update app
4. run search debounce/background task driver
5. drain search responses
6. start metadata refresh if requested
7. drain metadata refresh responses
8. break on app.should_quit
```

Changing this order can create subtle UX regressions, especially for search debounce, stale search responses, notices, and quit handling.

### Rule 5: Tests first where possible

Prefer tests around pure helpers and driver state. For anything involving real terminal rendering or live audio, keep the change mechanical and verify with `cargo check`, `cargo test`, and a manual smoke checklist.

---

# Fix A: Split `App::new` into runtime loading and pure state construction

## Goal

Make `App` construction testable without always touching disk and spawning the audio thread.

`App::new(library)` should remain as the ergonomic production constructor, but the actual state assembly should move into `App::from_parts(parts)`.

## Files and symbols

Primary files:

```text
src/app/lifecycle.rs
src/app.rs
src/audio.rs
src/history.rs
src/app/ui_state.rs
```

Primary symbols:

```text
src/app/lifecycle.rs::App::new
src/app/lifecycle.rs::App::from_parts
src/app/lifecycle.rs::AppParts
src/app/lifecycle.rs::StartupWarnings
src/audio.rs::AudioEngine
src/app/ui_state.rs::UiState
src/history.rs::History
```

## Current problem

`src/app/lifecycle.rs::App::new` is doing construction and runtime side effects in one function:

```rust
pub fn new(library: Library) -> Self {
    let (ui_state, ui_state_warning) = super::ui_state::UiState::load_with_warning();
    let sample_buffer = Arc::new(Mutex::new(VecDeque::with_capacity(4096)));
    let audio = AudioEngine::spawn(sample_buffer.clone());
    let (history, history_warning) = crate::history::History::load_with_warning();
    // builds diagnostics
    // constructs App
    // syncs audio settings
    // aggregates warnings
    // autoplays last station
    app
}
```

This makes tests pay for runtime side effects even when they only need state transitions. It also makes startup behavior harder to reason about because loading, construction, sync, warning display, and autoplay all happen inside one constructor.

## Desired shape

Add a concrete `AppParts` struct in `src/app/lifecycle.rs`:

```rust
pub(crate) struct AppParts {
    pub library: Library,
    pub ui_state: super::ui_state::UiState,
    pub ui_state_warning: Option<String>,
    pub history: crate::history::History,
    pub history_warning: Option<String>,
    pub audio: AudioEngine,
    pub sample_buffer: Arc<Mutex<VecDeque<f32>>>,
}
```

Add a production loader:

```rust
impl AppParts {
    pub(crate) fn load(library: Library) -> Self {
        let (ui_state, ui_state_warning) = super::ui_state::UiState::load_with_warning();
        let sample_buffer = Arc::new(Mutex::new(VecDeque::with_capacity(4096)));
        let audio = AudioEngine::spawn(sample_buffer.clone());
        let (history, history_warning) = crate::history::History::load_with_warning();

        Self {
            library,
            ui_state,
            ui_state_warning,
            history,
            history_warning,
            audio,
            sample_buffer,
        }
    }
}
```

Then shrink `App::new`:

```rust
impl App {
    pub fn new(library: Library) -> Self {
        Self::from_parts(AppParts::load(library))
    }
}
```

Add the pure constructor:

```rust
impl App {
    pub(crate) fn from_parts(parts: AppParts) -> Self {
        let diagnostics_output_device = crate::audio::output_device_display_name(
            parts.library.settings.output_device_name.as_deref(),
        );
        let diagnostics_metadata_enabled = parts.library.settings.stream_metadata_enabled;

        let mut app = Self {
            library: parts.library,
            nav: Navigation::default(),
            search: SearchState::default(),
            command_palette: CommandPaletteState::default(),
            player: PlaybackView::default(),
            volume: parts.ui_state.volume(),
            muted: parts.ui_state.muted(),
            should_quit: false,
            notice: NoticeState::default(),
            input_mode: InputMode::Normal,
            tick_count: 0,
            layout_mode: parts.ui_state.layout_mode(),
            overlays: Overlays::default(),
            song_history: VecDeque::new(),
            undo_history: VecDeque::new(),
            reconnect: Reconnect::default(),
            diagnostics: PlaybackDiagnostics {
                output_device: diagnostics_output_device,
                metadata_enabled: diagnostics_metadata_enabled,
                reconnect_limit: 3,
                ..PlaybackDiagnostics::default()
            },
            sleep_timer: SleepTimer::default(),
            history: parts.history,
            metadata_refresh_pending: false,
            metadata_refresh_running: false,
            persist: persist::PersistFlags::default(),
            audio: parts.audio,
            sample_buffer: parts.sample_buffer,
            visualizer_mode: parts.ui_state.visualizer_mode(),
            visualizer_peaks: Vec::new(),
        };

        app.sync_startup_audio_settings();
        app.apply_startup_warnings(parts.ui_state_warning, parts.history_warning);
        app.apply_startup_autoplay();
        app
    }
}
```

## Extract startup helpers

### `sync_startup_audio_settings`

Current lines in `App::new`:

```rust
app.sync_output_device();
app.sync_stream_metadata();
app.sync_volume();
```

Move to:

```rust
impl App {
    fn sync_startup_audio_settings(&mut self) {
        self.sync_output_device();
        self.sync_stream_metadata();
        self.sync_volume();
    }
}
```

Keep this private. It is startup glue, not a public API.

### `apply_startup_warnings`

Current warning logic:

```rust
let mut startup_warnings = app.library.load_warnings.clone();
if let Some(warning) = ui_state_warning {
    startup_warnings.push(warning);
}
if let Some(warning) = history_warning {
    startup_warnings.push(warning);
}

match startup_warnings.len() {
    0 => {}
    1 => app.set_error_notice(startup_warnings.remove(0)),
    count => app.set_error_notice(format!(
        "{count} config files had load warnings; using safe defaults where needed"
    )),
}
```

Move to:

```rust
impl App {
    fn apply_startup_warnings(
        &mut self,
        ui_state_warning: Option<String>,
        history_warning: Option<String>,
    ) {
        let mut startup_warnings = self.library.load_warnings.clone();
        if let Some(warning) = ui_state_warning {
            startup_warnings.push(warning);
        }
        if let Some(warning) = history_warning {
            startup_warnings.push(warning);
        }

        match startup_warnings.len() {
            0 => {}
            1 => self.set_error_notice(startup_warnings.remove(0)),
            count => self.set_error_notice(format!(
                "{count} config files had load warnings; using safe defaults where needed"
            )),
        }
    }
}
```

### `apply_startup_autoplay`

Current autoplay logic:

```rust
if app.library.settings.autoplay_last {
    if let Some(url) = app.library.settings.last_played_url.clone() {
        if let Some(pos) = last_played_station_position(&app.library.stations, &url) {
            app.nav.selected = pos;
        }
        app.player.playing_url = Some(url.clone());
        app.player.state = PlaybackState::Connecting;
        if app.send_audio_command(AudioCommand::Play(url)) {
            app.sync_volume();
        }
    }
}
```

Move to:

```rust
impl App {
    fn apply_startup_autoplay(&mut self) {
        if !self.library.settings.autoplay_last {
            return;
        }

        let Some(url) = self.library.settings.last_played_url.clone() else {
            return;
        };

        if let Some(pos) = last_played_station_position(&self.library.stations, &url) {
            self.nav.selected = pos;
        }

        self.player.playing_url = Some(url.clone());
        self.player.state = PlaybackState::Connecting;
        if self.send_audio_command(AudioCommand::Play(url)) {
            self.sync_volume();
        }
    }
}
```

## Test support

Add a test-only constructor for connected audio without spawning a thread, if needed.

Current `src/audio.rs` already has:

```rust
#[cfg(test)]
impl AudioEngine {
    pub fn disconnected_for_test() -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel::<AudioCommand>();
        drop(cmd_rx);
        let (_status_tx, status_rx) = mpsc::channel::<AudioStatus>();
        Self { cmd_tx, status_rx }
    }
}
```

Add a connected inert engine for tests that need sends to succeed but no real audio thread:

```rust
#[cfg(test)]
impl AudioEngine {
    pub fn connected_for_test() -> Self {
        let (cmd_tx, _cmd_rx) = mpsc::channel::<AudioCommand>();
        let (_status_tx, status_rx) = mpsc::channel::<AudioStatus>();
        Self { cmd_tx, status_rx }
    }
}
```

Important pitfall: `_cmd_rx` must stay alive inside the returned `AudioEngine` if sends should succeed. The snippet above drops `_cmd_rx` at function return, so it is wrong for a connected engine.

Correct shape requires keeping the receiver alive. Use a test-only field or wrapper only if necessary. Prefer avoiding this unless tests need successful sends.

Better 0.4.3 option: use `AudioEngine::spawn` in existing broad tests, and only use `disconnected_for_test` for failure tests. Do not contort production `AudioEngine` just for ideal test purity in this release.

## Required tests

Add tests in `src/app/lifecycle.rs` where private helpers are visible.

### Pure constructor uses injected state

```rust
#[test]
fn from_parts_uses_loaded_ui_state_and_history_without_loading_runtime_files() {
    let mut ui_state = super::ui_state::UiState::default();
    ui_state.volume = Some(37);
    ui_state.muted = Some(true);
    ui_state.visualizer_mode = Some(2);

    let app = App::from_parts(AppParts {
        library: Library::in_memory(vec![]),
        ui_state,
        ui_state_warning: None,
        history: crate::history::History::default(),
        history_warning: None,
        audio: AudioEngine::disconnected_for_test(),
        sample_buffer: Arc::new(Mutex::new(VecDeque::with_capacity(4096))),
    });

    assert_eq!(app.volume, 37);
    assert!(app.muted);
    assert_eq!(app.visualizer_mode, 2);
}
```

Adjust fields based on the actual `UiState` API. If fields are private, build using existing constructors or add a small `#[cfg(test)]` helper in `ui_state.rs`.

### Startup warnings aggregate safely

```rust
#[test]
fn from_parts_shows_single_startup_warning_verbatim() {
    let mut library = Library::in_memory(vec![]);
    library.load_warnings.push("bad library".to_string());

    let app = App::from_parts(test_parts(library)
        .with_ui_state_warning(None)
        .with_history_warning(None));

    assert!(matches!(
        app.notice.current,
        Some(AppNotice::Error(ref message)) if message == "bad library"
    ));
}
```

### Multiple startup warnings use summary copy

```rust
#[test]
fn from_parts_summarizes_multiple_startup_warnings() {
    let mut parts = test_parts(Library::in_memory(vec![]));
    parts.ui_state_warning = Some("bad ui".to_string());
    parts.history_warning = Some("bad history".to_string());

    let app = App::from_parts(parts);

    assert!(matches!(
        app.notice.current,
        Some(AppNotice::Error(ref message))
            if message.contains("2 config files had load warnings")
    ));
}
```

### Autoplay selects normalized last-played station

Existing tests cover `last_played_station_position`. Add a higher-level constructor test only if it does not require a successful audio send.

```rust
#[test]
fn from_parts_autoplay_sets_selected_station_and_error_when_audio_engine_is_dead() {
    let mut library = Library::in_memory(vec![Station::basic(
        "Saved",
        "HTTP://STREAM/",
        "Radio",
        "US",
        128,
    )]);
    library.settings.autoplay_last = true;
    library.settings.last_played_url = Some("http://stream".to_string());

    let app = App::from_parts(test_parts(library));

    assert_eq!(app.nav.selected, 0);
    assert_eq!(app.player.playing_url.as_deref(), Some("http://stream"));
    assert!(matches!(app.player.state, PlaybackState::Error(_)));
}
```

This uses `AudioEngine::disconnected_for_test`, so the test proves failure is visible without real audio.

## Pitfalls

### Pitfall: accidentally changing startup audio command order

Keep the order:

```text
1. sync output device
2. sync stream metadata
3. sync volume
4. process warnings
5. autoplay if enabled
6. sync volume after autoplay send succeeds
```

If you change this order, output-device and metadata settings can land after playback starts, which may change real behavior.

### Pitfall: tests that spawn audio threads forever

Most existing tests call `App::new`, which spawns an audio thread. 0.4.3 should reduce that over time, but do not try to fix every test in one pass. Start by making new tests use `App::from_parts`.

### Pitfall: making `AppParts` public API

Use `pub(crate)`, not `pub`. This is internal wiring, not a stable API.

### Pitfall: hiding startup warnings behind autoplay errors

If autoplay fails because the test uses `AudioEngine::disconnected_for_test`, it will set an audio error notice after startup warnings. That means warning tests should disable autoplay. Autoplay tests should not assert startup warning notices.

## Definition of done for Fix A

```text
[ ] App::new delegates to App::from_parts(AppParts::load(library)).
[ ] AppParts exists and owns loaded UI state, history, audio, and sample buffer.
[ ] App::from_parts performs pure state assembly from injected parts.
[ ] Startup warning logic is extracted and tested.
[ ] Startup autoplay logic is extracted and tested.
[ ] Existing App::new callers still compile.
[ ] New tests can construct App without config/history loading.
```

---

# Fix B: Extract background search and metadata task orchestration from `main.rs`

## Goal

Make `src/main.rs::main` a small terminal shell instead of the owner of all background work.

The new object should own:

```text
- search debounce state
- search response channel
- metadata refresh response channel
- spawning search tasks
- spawning metadata refresh tasks
- draining responses into App
```

It should not own:

```text
- terminal initialization
- drawing
- keyboard polling
- app update semantics
- audio playback internals
```

## Files and symbols

Primary files:

```text
src/main.rs
new src/runtime.rs or src/app_driver.rs
src/app/search.rs
src/app/library.rs
src/radio.rs
```

Recommended new file:

```text
src/runtime.rs
```

Recommended new symbols:

```text
src/runtime.rs::AppDriver
src/runtime.rs::SearchWorkerResponse
src/runtime.rs::MetadataRefreshWorkerResponse
src/runtime.rs::SEARCH_DEBOUNCE
src/runtime.rs::AppDriver::new
src/runtime.rs::AppDriver::tick
src/runtime.rs::AppDriver::update_search_debounce
src/runtime.rs::AppDriver::spawn_ready_search
src/runtime.rs::AppDriver::drain_search_responses
src/runtime.rs::AppDriver::spawn_metadata_refresh_if_requested
src/runtime.rs::AppDriver::drain_metadata_refresh_responses
```

## Current problem

`src/main.rs::main` currently has the complete background runtime inline:

```rust
let (search_tx, mut search_rx) =
    tokio::sync::mpsc::unbounded_channel::<(String, Result<Vec<radio::Station>, String>)>();
let (metadata_tx, mut metadata_rx) =
    tokio::sync::mpsc::unbounded_channel::<Result<(usize, Vec<radio::Station>, usize), String>>();
let tick_rate = Duration::from_millis(66);
let mut search_debounce: Option<(String, Instant)> = None;
```

Then inside the frame loop it manages debounce, spawns tasks, and drains responses. This will grow every time PulseDeck adds background work.

## Desired design

Add a runtime module declaration:

```rust
// src/main.rs
mod runtime;
```

Move debounce constant from `main.rs` into `runtime.rs`:

```rust
// src/runtime.rs
use std::time::{Duration, Instant};

const SEARCH_DEBOUNCE: Duration = Duration::from_millis(250);
```

Add response aliases:

```rust
// src/runtime.rs
use crate::{app::App, radio};

type SearchWorkerResponse = (String, Result<Vec<radio::Station>, String>);
type MetadataRefreshWorkerResponse = Result<(usize, Vec<radio::Station>, usize), String>;
```

Define the driver:

```rust
pub struct AppDriver {
    search_tx: tokio::sync::mpsc::UnboundedSender<SearchWorkerResponse>,
    search_rx: tokio::sync::mpsc::UnboundedReceiver<SearchWorkerResponse>,
    metadata_tx: tokio::sync::mpsc::UnboundedSender<MetadataRefreshWorkerResponse>,
    metadata_rx: tokio::sync::mpsc::UnboundedReceiver<MetadataRefreshWorkerResponse>,
    search_debounce: Option<(String, Instant)>,
}

impl AppDriver {
    pub fn new() -> Self {
        let (search_tx, search_rx) = tokio::sync::mpsc::unbounded_channel();
        let (metadata_tx, metadata_rx) = tokio::sync::mpsc::unbounded_channel();
        Self {
            search_tx,
            search_rx,
            metadata_tx,
            metadata_rx,
            search_debounce: None,
        }
    }

    pub fn tick(&mut self, app: &mut App) {
        self.update_search_debounce(app);
        self.spawn_ready_search(app);
        self.drain_search_responses(app);
        self.spawn_metadata_refresh_if_requested(app);
        self.drain_metadata_refresh_responses(app);
    }
}
```

Then `src/main.rs::main` becomes:

```rust
let mut driver = runtime::AppDriver::new();
let tick_rate = Duration::from_millis(66);

loop {
    terminal.draw(|frame| ui::draw(frame, &app))?;

    if let Some(action) = event::poll_action(tick_rate, &app.input_mode) {
        app.update(action);
    } else {
        app.update(action::Action::Tick);
    }

    driver.tick(&mut app);

    if app.should_quit {
        break;
    }
}
```

## Method details

### `update_search_debounce`

Move current logic mechanically:

```rust
fn update_search_debounce(&mut self, app: &App) {
    if let Some(query) = app.current_debounce_query().map(str::to_string) {
        match &self.search_debounce {
            Some((pending_query, _deadline)) if pending_query == &query => {}
            _ => {
                self.search_debounce = Some((query, Instant::now() + SEARCH_DEBOUNCE));
            }
        }
    } else {
        self.search_debounce = None;
    }
}
```

Pitfall: do not use `app.search.query` directly. Use the public `App::current_debounce_query` helper because it encodes status semantics.

### `spawn_ready_search`

Move current logic mechanically:

```rust
fn spawn_ready_search(&mut self, app: &mut App) {
    let Some((query, deadline)) = self.search_debounce.as_ref() else {
        return;
    };

    if Instant::now() < *deadline {
        return;
    }

    let query = query.clone();
    self.search_debounce = None;

    if app.mark_search_started(&query) {
        let tx = self.search_tx.clone();
        tokio::spawn(async move {
            let result = radio::search_stations(&query)
                .await
                .map_err(|err| err.to_string());
            let _ = tx.send((query, result));
        });
    }
}
```

Pitfall: `mark_search_started` must be called before spawning the task. If the app has left search mode or the query changed, do not spawn.

### `drain_search_responses`

```rust
fn drain_search_responses(&mut self, app: &mut App) {
    while let Ok((query, result)) = self.search_rx.try_recv() {
        app.apply_search_response(query, result);
    }
}
```

Pitfall: keep draining all available responses, not just one. Stale response handling already lives in `App::apply_search_response`.

### `spawn_metadata_refresh_if_requested`

```rust
fn spawn_metadata_refresh_if_requested(&mut self, app: &mut App) {
    let Some(stations) = app.take_metadata_refresh_request() else {
        return;
    };

    let tx = self.metadata_tx.clone();
    tokio::spawn(async move {
        let checked = stations.len();
        let mut matches = Vec::new();
        let mut failed = 0;

        for station in stations {
            match radio::lookup_station_metadata(&station).await {
                Ok(Some(metadata)) => matches.push(metadata),
                Ok(None) => {}
                Err(_) => failed += 1,
            }
        }

        let _ = tx.send(Ok((checked, matches, failed)));
    });
}
```

Pitfall: keep `checked = stations.len()` before consuming the vector.

### `drain_metadata_refresh_responses`

```rust
fn drain_metadata_refresh_responses(&mut self, app: &mut App) {
    while let Ok(result) = self.metadata_rx.try_recv() {
        app.apply_metadata_refresh_response(result);
    }
}
```

## Optional testability hook

Because `AppDriver` uses `Instant::now`, testing debounce timing is awkward. Do not overengineer a clock trait yet. Instead, add a narrow constructor for tests if needed:

```rust
#[cfg(test)]
impl AppDriver {
    fn with_search_debounce_for_test(query: impl Into<String>, deadline: Instant) -> Self {
        let mut driver = Self::new();
        driver.search_debounce = Some((query.into(), deadline));
        driver
    }
}
```

Then test pure state transitions where possible.

## Required tests

### Search debounce resets when app has no debounce query

```rust
#[test]
fn driver_clears_search_debounce_when_app_is_not_debouncing() {
    let mut driver = AppDriver::with_search_debounce_for_test(
        "lofi",
        Instant::now() + Duration::from_secs(1),
    );
    let app = test_app();

    driver.update_search_debounce(&app);

    assert!(driver.search_debounce.is_none());
}
```

This requires the test module to access private driver fields. Put tests in `src/runtime.rs`.

### Search debounce keeps same pending query deadline

```rust
#[test]
fn driver_keeps_existing_deadline_for_same_debounce_query() {
    let mut app = test_app_in_search_with_debouncing_query("lofi");
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut driver = AppDriver::with_search_debounce_for_test("lofi", deadline);

    driver.update_search_debounce(&app);

    assert_eq!(driver.search_debounce.as_ref().map(|(_, d)| *d), Some(deadline));
}
```

If building `test_app_in_search_with_debouncing_query` needs private access, either put a small `#[cfg(test)]` helper in `src/app/search.rs` or test through `app.update(Action::EnterSearch)` and `app.update(Action::SearchInput(...))`.

### Metadata refresh response drain clears running state

This can be tested by sending directly into `metadata_tx`:

```rust
#[test]
fn driver_applies_metadata_refresh_responses() {
    let mut driver = AppDriver::new();
    let mut app = test_app();
    driver
        .metadata_tx
        .send(Ok((0, Vec::new(), 0)))
        .unwrap();

    driver.drain_metadata_refresh_responses(&mut app);

    // Assert through visible notice copy or public state if available.
}
```

If `metadata_refresh_running` remains private and no user-visible assertion is easy, skip this micro-test. The app-level metadata refresh tests already cover response application.

## Main loop after extraction

`src/main.rs` should shrink to roughly:

```rust
mod runtime;

use anyhow::Result;
use std::time::Duration;

const TICK_RATE: Duration = Duration::from_millis(66);

#[tokio::main]
async fn main() -> Result<()> {
    if let cli::CliOutcome::Handled = cli::run(std::env::args())? {
        return Ok(());
    }

    let library = Library::load(fallback_stations());
    let saved_theme = ui::theme::ThemeName::from_key(&library.settings.theme);
    ui::theme::set_active(saved_theme);

    let mut app = App::new(library);
    let mut driver = runtime::AppDriver::new();
    let mut terminal = ratatui::init();
    let _terminal_restore = TerminalRestoreGuard;

    loop {
        terminal.draw(|frame| ui::draw(frame, &app))?;

        if let Some(action) = event::poll_action(TICK_RATE, &app.input_mode) {
            app.update(action);
        } else {
            app.update(action::Action::Tick);
        }

        driver.tick(&mut app);

        if app.should_quit {
            break;
        }
    }

    Ok(())
}
```

## Pitfalls

### Pitfall: spawning duplicate searches

If `search_debounce` is not cleared before `mark_search_started`, the next tick may spawn the same query again. Preserve this ordering:

```text
clone query
clear self.search_debounce
call app.mark_search_started
spawn only if true
```

### Pitfall: stale responses are normal

Do not try to filter stale responses in `AppDriver`. `App::apply_search_response` intentionally handles stale responses and sets user-facing state.

### Pitfall: metadata refresh task has no cancellation

This is current behavior. Do not add cancellation in 0.4.3 unless a separate design exists. The task loops through cloned stations and reports a summary when done.

### Pitfall: `tokio::spawn` requires runtime context

`AppDriver::tick` is called from `#[tokio::main]`, so `tokio::spawn` is valid. Do not call `AppDriver::tick` from non-Tokio tests that force spawn paths unless using `#[tokio::test]`.

## Definition of done for Fix B

```text
[ ] src/runtime.rs exists.
[ ] AppDriver owns search and metadata channels.
[ ] SEARCH_DEBOUNCE moves out of main.rs.
[ ] main.rs no longer owns search_debounce or worker channels.
[ ] main.rs frame loop order remains draw, input/tick, driver tick, quit check.
[ ] Existing search and metadata tests still pass.
[ ] Any new runtime tests avoid live network calls.
```

---

# Fix C: Introduce focused startup/runtime constants and helpers

## Goal

Name the remaining magic runtime values and keep time behavior easy to inspect.

## Files and symbols

```text
src/main.rs::TICK_RATE
src/runtime.rs::SEARCH_DEBOUNCE
src/app/lifecycle.rs::NOTICE_INFO_TICKS
src/app/lifecycle.rs::NOTICE_ERROR_TICKS
src/app/lifecycle.rs::SONG_HISTORY_CAP
src/app/lifecycle.rs::NOTIFY_IDLE_MS
```

## Current situation

Some constants already exist in `src/app/lifecycle.rs`:

```rust
const NOTICE_INFO_TICKS: u16 = 90;
const NOTICE_ERROR_TICKS: u16 = 150;
const SONG_HISTORY_CAP: usize = 100;
const NOTIFY_IDLE_MS: u64 = 120_000;
```

`src/main.rs` currently has:

```rust
const SEARCH_DEBOUNCE: Duration = Duration::from_millis(250);
let tick_rate = Duration::from_millis(66);
```

After Fix B, use:

```rust
// src/main.rs
const TICK_RATE: Duration = Duration::from_millis(66);

// src/runtime.rs
const SEARCH_DEBOUNCE: Duration = Duration::from_millis(250);
```

## Why this matters

These values are part of the app's feel. They should not be hidden inside loop bodies.

## Required tests

No direct tests required. This is compile-only cleanup. Existing search debounce and notice tests act as behavior coverage.

## Pitfalls

### Pitfall: moving app constants into runtime

Do not move notice or song-history constants into `runtime.rs`. They belong to app lifecycle behavior, not the terminal driver.

### Pitfall: using a public constant too early

Keep `TICK_RATE` and `SEARCH_DEBOUNCE` private unless another module truly needs them.

---

# Fix D: Prepare for future UI model extraction without doing it yet

## Goal

Leave small comments or helper seams that make a future `UiModel` extraction easier, but do not convert UI rendering in 0.4.3.

## Current situation

`src/ui/mod.rs::draw` takes the full `&App`:

```rust
pub fn draw(frame: &mut Frame, app: &App) {
    // all UI modules read App directly
}
```

This is convenient but creates tight coupling between rendering and `App` internals. A full UI model is a larger refactor and should not be mixed with startup/runtime separation.

## 0.4.3 action

Do not implement `UiModel` yet. Instead, add a future-work note in `plan.md` and leave code untouched.

Future shape for 0.4.4 or later:

```rust
pub struct UiModel<'a> {
    pub input_mode: InputMode,
    pub layout_mode: LayoutMode,
    pub overlay: ActiveOverlay,
    pub selected_station: Option<&'a Station>,
    pub now_playing: Option<&'a Station>,
    pub playback: &'a PlaybackView,
    pub notice: Option<&'a AppNotice>,
}

impl<'a> From<&'a App> for UiModel<'a> {
    fn from(app: &'a App) -> Self {
        Self {
            input_mode: app.input_mode,
            layout_mode: app.layout_mode,
            overlay: app.overlays.active,
            selected_station: app.selected_station(),
            now_playing: app.now_playing(),
            playback: &app.player,
            notice: app.notice.current.as_ref(),
        }
    }
}
```

## Pitfalls

### Pitfall: doing UI model too soon

After Fix A and Fix B, the diff will already be meaningful. Do not mix a rendering-wide signature migration into 0.4.3.

### Pitfall: creating a partial UI model with mixed patterns

Half the UI using `&App` and half using `&UiModel` can make code more confusing. Defer until there is time to do it consistently.

---

# Implementation order

## Step 1: Plan only

Write this `plan.md` and do not change code in the same commit if using small commits.

Files:

```text
plan.md
```

Validation:

```text
No compile needed for plan-only commit.
```

## Step 2: Extract `AppParts` and `App::from_parts`

Files:

```text
src/app/lifecycle.rs
src/app.rs, only if re-exports are needed
src/audio.rs, only if a test helper is genuinely needed
```

Work:

```text
1. Add AppParts.
2. Add AppParts::load.
3. Move current App::new body into App::from_parts.
4. Change App::new to delegate.
5. Extract sync_startup_audio_settings.
6. Extract apply_startup_warnings.
7. Extract apply_startup_autoplay.
8. Add constructor tests.
```

Targeted validation:

```text
cargo test app::lifecycle
cargo check
```

## Step 3: Convert new or fragile tests to `App::from_parts`

Files:

```text
src/app/lifecycle.rs
src/app/playback.rs, only if tests need helper migration
src/app/search.rs, only if tests need helper migration
```

Work:

```text
1. Add a local test_parts helper in lifecycle tests.
2. Use from_parts for new tests.
3. Do not churn every existing test unless needed.
```

Validation:

```text
cargo test app
```

## Step 4: Extract `AppDriver` into `src/runtime.rs`

Files:

```text
src/main.rs
src/runtime.rs
```

Work:

```text
1. Add mod runtime.
2. Add AppDriver with channels and debounce state.
3. Move search debounce update.
4. Move search spawn.
5. Move search response drain.
6. Move metadata refresh spawn.
7. Move metadata response drain.
8. Shrink main loop.
```

Targeted validation:

```text
cargo check
cargo test app::search
cargo test app::library
```

## Step 5: Add runtime tests where they are cheap

Files:

```text
src/runtime.rs
```

Work:

```text
1. Test debounce clearing.
2. Test same-query debounce deadline is preserved if building test app is easy.
3. Avoid tests that hit Radio Browser.
```

Validation:

```text
cargo test runtime
```

## Step 6: Full validation and docs

Files:

```text
CHANGELOG.md
README.md, only if user-visible startup behavior changed
```

Expected changelog entry:

```markdown
## [0.4.3] - Unreleased

### Changed
* **App construction**: Split runtime dependency loading from pure `App` state assembly through `AppParts` and `App::from_parts`.
* **Runtime orchestration**: Moved search debounce, search workers, metadata refresh workers, and response draining out of `src/main.rs` into `src/runtime.rs::AppDriver`.

### Internal
* Added focused tests around startup warning aggregation, autoplay state setup, and driver debounce behavior.
```

Full validation:

```text
cargo check
cargo test
cargo clippy --all-targets --all-features
```

---

# Manual smoke checklist

0.4.3 should not require deep audio QA because it does not touch decoder or engine-loop behavior. Still run a light smoke test because startup wiring and autoplay are touched.

```text
[ ] Start PulseDeck normally.
[ ] Confirm saved theme still applies before first draw.
[ ] Search opens with `/`.
[ ] Type `tag:ambient`; results debounce and load.
[ ] Stale search behavior still feels sane when typing quickly.
[ ] Open command palette and trigger metadata refresh on a small library.
[ ] Export library still creates an M3U file.
[ ] Play an MP3 station.
[ ] Stop playback.
[ ] Quit cleanly.
```

Autoplay smoke if enabled locally:

```text
[ ] Enable autoplay last station.
[ ] Play a station and quit.
[ ] Restart PulseDeck.
[ ] Last station selection is restored.
[ ] Playback attempts to start.
[ ] Any audio-engine failure is visible, not silent.
```

---

# Rollback strategy

## If `AppParts` extraction causes startup regressions

Revert only the constructor split commit. Keep any pure helper tests that still make sense.

High-risk areas:

```text
src/app/lifecycle.rs::App::new
src/app/lifecycle.rs::apply_startup_autoplay
src/app/lifecycle.rs::apply_startup_warnings
```

Check for changed order of startup actions first. Most regressions will be ordering bugs, not type bugs.

## If `AppDriver` extraction causes search regressions

Revert `src/runtime.rs` and restore the previous inline block in `src/main.rs`. Search task orchestration is isolated enough that rollback should be straightforward.

High-risk areas:

```text
src/runtime.rs::update_search_debounce
src/runtime.rs::spawn_ready_search
src/runtime.rs::drain_search_responses
```

Compare line-by-line with the previous main loop before inventing new behavior.

## If metadata refresh stops completing

Check:

```text
src/runtime.rs::spawn_metadata_refresh_if_requested
src/runtime.rs::drain_metadata_refresh_responses
src/app/library.rs::take_metadata_refresh_request
src/app/library.rs::apply_metadata_refresh_response
```

The most likely bug is forgetting to drain `metadata_rx` every tick or moving drain before spawn in a way that delays completion by a tick. A one-tick delay is fine. Never draining is not.

---

# Known non-goals for 0.4.3

Do not include these:

```text
- No decoder changes.
- No AAC/M4A compatibility work.
- No audio buffering architecture.
- No UI model migration.
- No keybinding changes.
- No Radio Browser ranking changes.
- No library JSON format migration.
- No cancellation system for metadata refresh tasks.
- No broad trait-based dependency injection framework.
- No async terminal input rewrite.
```

---

# Future work after 0.4.3

Good next candidates:

```text
0.4.4: UI read model extraction so rendering no longer depends directly on the full App object.
0.4.5: Focused LibraryStore abstraction for config/history persistence tests.
0.5.0: Audio compatibility release with explicit MP3/AAC/M4A/OGG stream matrix and decoder strategy.
```

Potential `UiModel` path:

```text
src/ui/model.rs
src/ui/mod.rs::draw(frame, &UiModel::from(&app))
render modules gradually take &UiModel instead of &App
```

Potential persistence abstraction path:

```text
src/storage.rs::LibraryStore
src/storage.rs::HistoryStore
src/config.rs remains path resolver
favorites.rs remains domain plus serialization
```

Potential audio compatibility path:

```text
src/audio/session.rs::choose_decoder
explicit stream codec hints from Station.codec
manual stream matrix in docs/releases/0.5.0.md
```

---

# Final 0.4.3 release gate

Do not tag 0.4.3 until all are true:

```text
[ ] App::new remains available and production-safe.
[ ] App::from_parts exists for testable state construction.
[ ] Startup warnings still surface correctly.
[ ] Autoplay behavior is preserved.
[ ] main.rs no longer owns search/metadata worker channels.
[ ] runtime driver does not perform network calls in unit tests.
[ ] cargo check passes.
[ ] cargo test passes.
[ ] cargo clippy --all-targets --all-features passes.
[ ] Light manual smoke test passes.
[ ] CHANGELOG.md has a 0.4.3 entry.
```

The success smell for 0.4.3: `main.rs` reads like a doorway, `App::new` reads like a recipe, and the audio path remains untouched enough that the speaker gremlin keeps sleeping.
