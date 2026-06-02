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
- `src/app/recording.rs` owns recording toggle state.
- `src/app/overlays.rs` owns help/settings visibility, layout cycling, deck page cycling, and visualizer mode cycling.
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

## Local tape archive

`src/tape_archive.rs` owns the disk-backed model for recorded files. The scanner uses blocking filesystem APIs, so `src/main.rs` runs scans through `tokio::task::spawn_blocking` and applies results back into `App` through `apply_tape_archive_scan`.

The archive page lives behind `active_deck_page == 1`. While focused, normal navigation actions are routed to tape rows instead of station rows. `Enter` plays a selected local tape, `Space` expands folders or controls local playback, `Ctrl+r` refreshes the archive, and deletion requires an explicit confirmation step.

Local file playback is represented separately from live stream playback with `AudioCommand::PlayLocalFile` and `AudioStatus::LocalFilePlaying`. Recording remains live-stream-only.

## Local tape archive enhancements

Local tape archive state now includes a filter query and an All Recordings flat-view flag. Filtering is owned by `src/tape_archive.rs` so UI, reducer, and tests share one row model. The archive can render the folder tree, a newest-first All Recordings view, or a filtered flat view without duplicating row selection logic.

Duration labels are best-effort metadata hints gathered during the blocking archive scan. If a local decoder exposes total duration, the UI renders `FORMAT · MM:SS · SIZE`; otherwise it falls back to `FORMAT · SIZE`.

The tape filter uses `InputMode::TapeFilter`, separate from global radio search. `/` opens local tape filtering when the tape archive is focused, while global station search remains available outside the tape page.

## Local tape file management

Local tape file management is intentionally conservative. Opening a containing folder goes through `src/system_open.rs`, which builds platform-specific command specs and launches the host file manager without mutating the archive.

Trash deletion goes through `src/system_trash.rs`. PulseDeck attempts platform trash commands and does not fall back to permanent deletion if trash is unavailable. This keeps the guarded `y` confirmation flow recoverable by the user's OS.

Local playback completion is surfaced as `AudioStatus::LocalFileFinished`. The app reducer uses the tape archive model to find the next recording in the same folder and starts it automatically. At the end of a folder, playback stops and the footer reports the end of the tape folder.

## Recording session dashboard

Recording session visibility lives in app state rather than the audio thread. `src/app/recording.rs` starts, updates, and clears the session fields, while `src/app/lifecycle.rs` delegates `AudioStatus::RecordingStateChanged` into that reducer.

`src/recording_journal.rs` owns the lightweight recovery journal. The app writes it when recording is pending or active and removes it when recording stops cleanly. On startup, `App::new` checks the configured recording directory for an abandoned journal and exposes a recovery notice to the Tape Deck.

The Tape Deck renders the session dashboard from app state: station, elapsed time, active capture path, file size, minimum duration, and snippet policy. The dashboard intentionally reads file size from the filesystem at render time so the audio thread does not need to send high-frequency byte counters.

## Recording recovery actions

When startup detects a recovery journal, app state stores both the full `RecordingRecovery` payload and a user-facing recovery notice. The Tape Deck and footer expose three explicit actions: keep the partial file and remove the journal, move the partial file to OS trash, or dismiss the journal only.

Recovery actions are handled in `src/app/recording.rs` so the reducer owns the lifecycle: journal removal, trash attempts through `src/system_trash.rs`, archive refresh requests, and notice updates. Failed trash moves keep recovery state intact so the user can retry or choose a non-destructive action.

## Recording intelligence

The stream reader owns track-boundary recording decisions because it sees Icecast metadata changes before decoded samples reach the sink. It now refuses to overwrite an existing target file for the same sanitized artist/title path, reporting a duplicate-skip notice rather than replacing a user's archive.

Completed MP3 captures receive richer ID3 metadata through `src/audio/recording.rs`: artist/title splitting, PulseDeck album context, genre/category, and source stream URL when available. This keeps portable local recordings useful outside the TUI without adding transcoding complexity.

## Local tape playback modes

Local tape playback continuation is controlled by `TapePlaybackMode`, owned by app state. The audio thread only reports `AudioStatus::LocalFileFinished`; the app reducer decides whether to stop, continue through the current folder, continue through all recordings, repeat the current file, or choose a deterministic shuffle target.

The archive model owns track lookup and next-track helpers so playback modes do not duplicate folder traversal logic in UI code. Shuffle mode intentionally uses deterministic path hashing with the app tick counter as salt, avoiding an additional runtime RNG dependency.

## Refactor rules

- Keep `crate::app::App` as the public UI-facing state root.
- Keep `crate::app::{InputMode, SearchStatus, PlaybackState, RecordingState, LayoutMode, AppNotice}` re-exported.
- Keep `Action` and terminal event mapping separate from app reducers.
- Preserve settings overlay action blocking. This prevents background actions from leaking through settings.
- Keep behavior changes out of mechanical module-splitting PRs.
