# App architecture

PulseDeck routes input through a small action pipeline:

- `src/action.rs` defines user/application actions.
- `src/event.rs` maps terminal events to actions.
- `src/app.rs` owns the public `App` type and re-exports public app state enums.
- `src/app/update.rs` routes actions to focused reducer modules.

## App modules

- `src/app/types.rs` owns public app enums and app-level constants.
- `src/app/lifecycle.rs` owns app construction, notices, and audio status polling.
- `src/app/selectors.rs` owns read-only station selectors and currently-playing lookup.
- `src/app/search.rs` owns search mode, debounce state, stale-response handling, and search confirmation.
- `src/app/playback.rs` owns playback commands and volume/mute state.
- `src/app/settings.rs` owns settings overlay behavior and action blocking while settings are open.
- `src/app/library.rs` owns library removal, genre navigation, and persistence notices.
- `src/app/overlays.rs` owns help/settings visibility, layout cycling, and visualizer mode cycling.
- `src/app/visualizer.rs` owns FFT and visualizer peak updates.
- `src/app/idle.rs` owns platform-specific user idle detection for notifications.

## UI boundary rendering

`src/ui/mod.rs` owns the root terminal-size gate. When the frame is below the supported 80x24 minimum, PulseDeck renders a small diagnostic panel and skips deck, station, footer, help, and settings composition. Help and settings also have local overlay-size guards so modal layouts do not draw broken borders inside cramped popup rectangles.

## Theme styling

Full-screen clears and overlay canvas blocks must use semantic helpers from `src/ui/theme.rs`, especially `theme::clear()`. Raw background colors belong inside palette definitions only, so Catppuccin Latte and dark themes share the same rendering path.

## Audio status flow

The audio thread sends high-level `AudioStatus` messages into the app. Buffer telemetry is low-priority and is deduplicated in `src/audio/buffer_meter.rs` before it enters the channel, preventing repeated identical fill-level packets from delaying more important playback state changes.

## Audio output recovery

`src/audio.rs` owns output-stream handles and connection retries. Hardware-style sink failures are tagged with a dedicated prefix, stale output handles are dropped, and the active URL gets one guarded retry. Network and decode failures do not enter the hardware recovery path.

## Selection context

`App::selected` remains the visible cursor index, while app state also tracks normal/search snapshots and per-genre library cursor memory. Entering search preserves the library cursor, leaving search restores it, and genre changes restore the last row visited in that category when possible.

## 0.2 focus boundary

PulseDeck 0.2 keeps the app centered on live radio playback, station discovery, saved library management, themes, visualizers, and resilient terminal UX.

Recording, local tape archive management, local file playback, and recovery-journal workflows are outside the 0.2 core product boundary.

## Refactor rules

- Keep `crate::app::App` as the public UI-facing state root.
- Keep `crate::app::{InputMode, SearchStatus, PlaybackState, LayoutMode, AppNotice}` re-exported.
- Keep `Action` and terminal event mapping separate from app reducers.
- Preserve settings overlay action blocking. This prevents background actions from leaking through settings.
- Keep behavior changes out of mechanical module-splitting PRs.
