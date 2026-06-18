# App architecture

PulseDeck routes input through a small action pipeline:

- `src/action.rs` defines user/application actions.
- `src/event.rs` maps terminal events to actions.
- `src/app.rs` owns the public `App` type and re-exports public app state enums.
- `src/app/update.rs` routes actions to focused reducer modules.
- `src/config.rs` owns shared config-directory lookup, legacy migration, and JSON load/save helpers.

## App modules

- `src/app/types.rs` owns public app enums and app-level constants.
- `src/app/lifecycle.rs` owns app construction, notices, and audio status polling.
- `src/app/nav.rs` owns navigation state, including visible selection, search snapshots, and per-genre cursor memory.
- `src/app/selectors.rs` owns read-only station selectors, selected-station lookup, and currently-playing lookup.
- `src/app/search.rs` owns `SearchState`, search mode, debounce state, stale-response handling, auditioning, search confirmation, and saved-result identity checks.
- `src/app/playback.rs` owns `PlaybackView`, playback commands, stream retry, and volume/mute state.
- `src/app/settings.rs` owns settings overlay behavior and action blocking while settings are open.
- `src/app/library.rs` owns library removal, genre navigation, and persistence notices.
- `src/app/overlays.rs` owns `ActiveOverlay`, overlay selection state, layout cycling, and visualizer mode cycling.
- `src/app/persist.rs` owns dirty flags and debounced persistence flushing for UI state, history, and library writes.
- `src/app/visualizer/` owns FFT and visualizer peak updates, with pure DSP helpers kept in `dsp.rs`.
- `src/app/idle.rs` owns platform-specific user idle detection for notifications.

## App state groups

`App` remains the UI-facing state root, but related fields are grouped into small structs:

- `Navigation` tracks selected rows, search/normal snapshots, and genre selection memory.
- `SearchState` tracks the query, result list, debounce status, and in-flight API query.
- `PlaybackView` tracks playback state, current URL, current track, buffer telemetry, and intentional stops.
- `Overlays` tracks the active overlay and focused settings row.
- `NoticeState` tracks transient notices and their remaining lifetime.
- `PersistFlags` tracks delayed saves that are flushed during ticks and shutdown.

## UI boundary rendering

`src/ui/mod.rs` owns the root terminal-size gate. When the frame is below the supported 80x24 minimum, PulseDeck renders a small diagnostic panel and skips deck, station, footer, help, settings, and context overlay composition. Help, settings, station details, recent tracks, and sleep timer also have local overlay-size guards so modal layouts do not draw broken borders inside cramped popup rectangles.

## Context overlays

PulseDeck has a small set of mutually exclusive overlays:

- Help shows the full shortcut reference.
- Settings owns persisted settings controls and blocks background actions while open.
- Station Details shows metadata for the highlighted visible station.
- Recent Tracks shows the in-memory session list of stream-provided titles.
- Sleep Timer owns timer preset and adjustment controls.

The active overlay is represented by `ActiveOverlay`, so help, settings, station details, recent tracks, and sleep timer cannot be open at the same time. Context overlays close or replace each other through `src/app/overlays.rs`, and `q` / `Esc` close open overlays before quitting the app.

## Theme styling

Full-screen clears and overlay canvas blocks must use semantic helpers from `src/ui/theme/`, especially `theme::clear()`. Raw background colors belong inside palette definitions only, so Catppuccin Latte, dark themes, and the terminal-native theme share the same rendering path.

The `Terminal` theme intentionally uses reset backgrounds plus ANSI colors such as terminal black, gray, magenta, cyan, yellow, green, and red. That lets PulseDeck follow the user's terminal emulator palette instead of forcing RGB colors.

## Audio status flow

The audio thread sends high-level `AudioStatus` messages into the app. Buffer telemetry is low-priority and is deduplicated in `src/audio/buffer_meter.rs` before it enters the channel, preventing repeated identical fill-level packets from delaying more important playback state changes.

## Audio output recovery

`src/audio/engine_loop.rs` owns output-stream handles and connection retries. Hardware-style sink failures are tagged with a dedicated prefix, stale output handles are dropped, and the active URL gets one guarded retry. Network and decode failures do not enter the hardware recovery path.

The app-level `RetryStream` action is separate from that hardware one-shot path. It lets the user manually retry the current stream URL from normal mode with plain `r` after an error, while leaving legacy `Ctrl+r` unmapped.

## Selection context

`App::nav.selected` remains the visible cursor index, while navigation state also tracks normal/search snapshots and per-genre library cursor memory. Entering search preserves the library cursor, leaving search restores it, and genre changes restore the last row visited in that category when possible.

`App::selected_station()` should be used by UI surfaces that need the highlighted row without duplicating normal/search filtering logic.

## Persistence

Library, history, and UI-state files use `src/config.rs` for config paths and legacy migration. Runtime mutations mark `PersistFlags` dirty instead of saving inline, and `App::flush_persistence()` writes dirty state from the tick loop and shutdown path.

## 0.2 focus boundary

PulseDeck 0.2 keeps the app centered on live radio playback, station discovery, saved library management, themes, visualizers, and resilient terminal UX.

Recording, local tape archive management, local file playback, and recovery-journal workflows are outside the 0.2 core product boundary.

## Refactor rules

- Keep `crate::app::App` as the public UI-facing state root.
- Keep `crate::app::{InputMode, SearchStatus, PlaybackState, LayoutMode, AppNotice}` re-exported.
- Keep `Action` and terminal event mapping separate from app reducers.
- Preserve settings overlay action blocking. This prevents background actions from leaking through settings.
- Keep context overlays lightweight and avoid introducing persistence for session-only Recent Tracks.
- Add new config-backed files through `src/config.rs` instead of duplicating path and migration logic.
- Keep behavior changes out of mechanical module-splitting PRs.
