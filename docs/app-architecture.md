# App architecture

DriftFM routes input through a small action pipeline:

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
- `src/app/recording.rs` owns recording toggle state.
- `src/app/overlays.rs` owns help/settings visibility, layout cycling, deck page cycling, and visualizer mode cycling.
- `src/app/visualizer.rs` owns FFT and visualizer peak updates.
- `src/app/idle.rs` owns platform-specific user idle detection for notifications.

## Refactor rules

- Keep `crate::app::App` as the public UI-facing state root.
- Keep `crate::app::{InputMode, SearchStatus, PlaybackState, RecordingState, LayoutMode, AppNotice}` re-exported.
- Keep `Action` and terminal event mapping separate from app reducers.
- Preserve settings overlay action blocking. This prevents background actions from leaking through settings.
- Keep behavior changes out of mechanical module-splitting PRs.
