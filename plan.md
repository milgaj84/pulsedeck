# PulseDeck 0.4.5 Plan: Persistence Backoff and Save Safety

## Release intent

0.4.5 is a small reliability release focused on persistence behavior. The goal is to make failed saves boring, visible, and non-destructive instead of letting them retry on every UI tick like a tiny disk-writing woodpecker.

This release must not change audio playback, stream decoding, station search, station identity, UI layout, or file formats. It only changes when dirty state is flushed after failures and how those failures are reported.

## Current problem

`App::tick` calls `flush_persistence()` every frame:

```rust
// src/app/update.rs
pub(super) fn tick(&mut self) {
    let now = std::time::Instant::now();
    self.tick_count += 1;
    self.tick_notice();
    self.poll_audio_status();
    self.update_visualizer();
    self.drive_reconnect(now);
    self.check_sleep_timer(now);
    self.flush_persistence();
}
```

`flush_persistence()` immediately retries every dirty save on every tick:

```rust
// src/app/persist.rs
pub(super) fn flush_persistence(&mut self) {
    if self.persist.ui_state_dirty {
        let state = super::ui_state::UiState::from_app_values(
            self.volume,
            self.muted,
            self.layout_mode,
            self.visualizer_mode,
        );
        match state.save() {
            Ok(()) => self.persist.ui_state_dirty = false,
            Err(err) => self.set_error_notice(format!("Could not save UI state: {err}")),
        }
    }

    if self.persist.history_dirty {
        match self.history.save() {
            Ok(()) => self.persist.history_dirty = false,
            Err(err) => self.set_error_notice(format!("Could not save history: {err}")),
        }
    }

    if self.persist.library_dirty {
        match self.library.save() {
            Ok(()) => self.persist.library_dirty = false,
            Err(err) => self.set_error_notice(format!("Could not save library: {err}")),
        }
    }
}
```

If saving fails because the config directory is missing, permissions are wrong, the disk is full, or an antivirus/file-lock tantrum occurs, dirty flags remain true. That is correct. The problem is retry cadence: the next tick tries again, and the next tick tries again, roughly every 66ms.

## Release goals

1. Keep dirty flags set after failed saves.
2. Retry failed persistence after a cooldown, not every tick.
3. Avoid repeating the same error notice every frame.
4. Let new user changes request a save without waiting forever behind stale failure state.
5. Preserve `stop_audio_before_quit()` behavior: quit should still attempt one final flush.
6. Add focused tests for retry timing, dirty flag preservation, and notice behavior.
7. Keep the JSON formats unchanged:
   - UI state file format unchanged.
   - history file format unchanged.
   - library file format unchanged.

## Non-goals

Do not do these in 0.4.5:

- Do not change audio engine code.
- Do not change `AudioEngine::send` behavior.
- Do not change stream decoder selection.
- Do not change `LibraryFile` JSON shape.
- Do not make persistence asynchronous.
- Do not add a database.
- Do not add background save threads.
- Do not change UI layout or notice rendering.
- Do not move config paths.

This release is a brake pedal, not a new vehicle.

## Current connections

### Save producers

Dirty flags are set by these paths:

```text
src/app/playback.rs::volume_up
src/app/playback.rs::volume_down
src/app/playback.rs::toggle_mute
src/app/overlays.rs::toggle_station_details
src/app/overlays.rs::toggle_recent_tracks
src/app/library.rs::remove_library_selection
src/app/library.rs::undo_remove_library_selection
src/app/search.rs::confirm_search
src/app/settings.rs settings mutations
src/app/sleep_timer.rs sleep timer mutations, if persisted later
```

Search for:

```text
mark_ui_state_dirty
mark_history_dirty
mark_library_dirty
```

### Save consumer

The consumer is:

```text
src/app/update.rs::tick
  -> src/app/persist.rs::flush_persistence
```

Quit path also calls persistence manually:

```text
src/app/playback.rs::stop_audio_before_quit
  -> self.flush_persistence()
  -> self.audio.send(AudioCommand::Stop)
```

0.4.5 must treat the quit path carefully. A cooldown that prevents normal tick retries should not prevent an explicit final flush before quit.

## Design summary

Add retry bookkeeping to `PersistFlags`:

```rust
// src/app/persist.rs
#[derive(Default)]
pub(super) struct PersistFlags {
    ui_state_dirty: bool,
    history_dirty: bool,
    library_dirty: bool,
    retry: PersistenceRetry,
}

#[derive(Debug, Clone)]
pub(super) struct PersistenceRetry {
    next_retry_at: Option<std::time::Instant>,
    last_error_key: Option<PersistenceErrorKey>,
    last_error_notice_at: Option<std::time::Instant>,
}

impl Default for PersistenceRetry {
    fn default() -> Self {
        Self {
            next_retry_at: None,
            last_error_key: None,
            last_error_notice_at: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PersistenceErrorKey {
    target: PersistTarget,
    message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PersistTarget {
    UiState,
    History,
    Library,
}
```

Use a cooldown constant:

```rust
// src/app/persist.rs
const PERSIST_RETRY_COOLDOWN: std::time::Duration = std::time::Duration::from_secs(5);
const PERSIST_NOTICE_COOLDOWN: std::time::Duration = std::time::Duration::from_secs(30);
```

The 5-second retry cooldown prevents frame-by-frame disk writes. The 30-second notice cooldown prevents a visual error flood if the same failure persists. These values should be constants so later releases can tune them without hunting literals.

## Implementation phase 1: split persistence flush API

### Files

```text
src/app/persist.rs
src/app/update.rs
src/app/playback.rs
```

### Add flush modes

Introduce an explicit mode so `tick()` and quit can behave differently.

```rust
// src/app/persist.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PersistenceFlushMode {
    Scheduled,
    Force,
}
```

Change the current method from:

```rust
pub(super) fn flush_persistence(&mut self) {
    // immediate save attempts
}
```

to:

```rust
pub(super) fn flush_persistence(&mut self) {
    self.flush_persistence_with_mode(PersistenceFlushMode::Scheduled);
}

pub(super) fn force_flush_persistence(&mut self) {
    self.flush_persistence_with_mode(PersistenceFlushMode::Force);
}

fn flush_persistence_with_mode(&mut self, mode: PersistenceFlushMode) {
    let now = std::time::Instant::now();

    if mode == PersistenceFlushMode::Scheduled && !self.persist.retry_due(now) {
        return;
    }

    match self.try_flush_persistence_once() {
        Ok(()) => self.persist.clear_retry_state(),
        Err(error) => self.handle_persistence_error(error, now),
    }
}
```

### Update callers

Keep `tick()` as scheduled:

```rust
// src/app/update.rs
pub(super) fn tick(&mut self) {
    let now = std::time::Instant::now();
    self.tick_count += 1;
    self.tick_notice();
    self.poll_audio_status();
    self.update_visualizer();
    self.drive_reconnect(now);
    self.check_sleep_timer(now);
    self.flush_persistence();
}
```

Change quit path to forced:

```rust
// src/app/playback.rs
pub(super) fn stop_audio_before_quit(&mut self) {
    self.player.intentional_stop = true;
    self.force_flush_persistence();
    self.audio.send(AudioCommand::Stop);
}
```

### Pitfall

Do not make `flush_persistence()` always force. The whole point is that normal frame ticks respect cooldown.

Do not make `force_flush_persistence()` clear dirty flags on failure. It only bypasses the cooldown gate.

## Implementation phase 2: extract one-shot save attempt

### File

```text
src/app/persist.rs
```

Add a small error type:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PersistenceError {
    target: PersistTarget,
    message: String,
}

impl PersistenceError {
    fn new(target: PersistTarget, err: impl std::fmt::Display) -> Self {
        Self {
            target,
            message: err.to_string(),
        }
    }

    fn notice_message(&self) -> String {
        match self.target {
            PersistTarget::UiState => format!("Could not save UI state: {}", self.message),
            PersistTarget::History => format!("Could not save history: {}", self.message),
            PersistTarget::Library => format!("Could not save library: {}", self.message),
        }
    }

    fn key(&self) -> PersistenceErrorKey {
        PersistenceErrorKey {
            target: self.target,
            message: self.message.clone(),
        }
    }
}
```

Extract `try_flush_persistence_once`:

```rust
fn try_flush_persistence_once(&mut self) -> Result<(), PersistenceError> {
    if self.persist.ui_state_dirty {
        let state = super::ui_state::UiState::from_app_values(
            self.volume,
            self.muted,
            self.layout_mode,
            self.visualizer_mode,
        );

        state
            .save()
            .map_err(|err| PersistenceError::new(PersistTarget::UiState, err))?;

        self.persist.ui_state_dirty = false;
    }

    if self.persist.history_dirty {
        self.history
            .save()
            .map_err(|err| PersistenceError::new(PersistTarget::History, err))?;

        self.persist.history_dirty = false;
    }

    if self.persist.library_dirty {
        self.library
            .save()
            .map_err(|err| PersistenceError::new(PersistTarget::Library, err))?;

        self.persist.library_dirty = false;
    }

    Ok(())
}
```

### Behavior

This preserves current ordering:

1. UI state
2. history
3. library

If UI state fails, history and library are not attempted in that tick. That matches a conservative fail-fast model. It is also easier to test.

### Edge case

A failure in UI state can temporarily block history/library saves until UI state succeeds. This is already effectively true if the current code repeatedly errors loudly, but the current implementation actually attempts later targets even after earlier failures.

There are two acceptable designs:

#### Option A: fail fast

Simpler, fewer moving parts. Use the snippet above.

#### Option B: attempt all dirty targets and collect first error

More complete but more code. Recommended if we want history/library to save even when UI state fails.

```rust
fn try_flush_persistence_once(&mut self) -> Result<(), PersistenceError> {
    let mut first_error = None;

    if self.persist.ui_state_dirty {
        let state = super::ui_state::UiState::from_app_values(
            self.volume,
            self.muted,
            self.layout_mode,
            self.visualizer_mode,
        );

        match state.save() {
            Ok(()) => self.persist.ui_state_dirty = false,
            Err(err) => first_error.get_or_insert_with(|| {
                PersistenceError::new(PersistTarget::UiState, err)
            }),
        };
    }

    if self.persist.history_dirty {
        match self.history.save() {
            Ok(()) => self.persist.history_dirty = false,
            Err(err) => first_error.get_or_insert_with(|| {
                PersistenceError::new(PersistTarget::History, err)
            }),
        };
    }

    if self.persist.library_dirty {
        match self.library.save() {
            Ok(()) => self.persist.library_dirty = false,
            Err(err) => first_error.get_or_insert_with(|| {
                PersistenceError::new(PersistTarget::Library, err)
            }),
        };
    }

    first_error.map_or(Ok(()), Err)
}
```

Recommended for 0.4.5: **Option B**. It best preserves the current behavior where one broken target does not stop later dirty targets from saving.

## Implementation phase 3: retry and notice bookkeeping

### File

```text
src/app/persist.rs
```

Add helper methods:

```rust
impl PersistFlags {
    fn has_dirty_work(&self) -> bool {
        self.ui_state_dirty || self.history_dirty || self.library_dirty
    }

    fn retry_due(&self, now: std::time::Instant) -> bool {
        if !self.has_dirty_work() {
            return false;
        }

        self.retry
            .next_retry_at
            .map_or(true, |deadline| now >= deadline)
    }

    fn schedule_retry(&mut self, now: std::time::Instant) {
        self.retry.next_retry_at = Some(now + PERSIST_RETRY_COOLDOWN);
    }

    fn clear_retry_state(&mut self) {
        self.retry = PersistenceRetry::default();
    }
}
```

Add notice throttling:

```rust
impl App {
    fn handle_persistence_error(&mut self, error: PersistenceError, now: std::time::Instant) {
        self.persist.schedule_retry(now);

        let key = error.key();
        let should_notice = self.persist.retry.last_error_key.as_ref() != Some(&key)
            || self
                .persist
                .retry
                .last_error_notice_at
                .map_or(true, |last| now.duration_since(last) >= PERSIST_NOTICE_COOLDOWN);

        self.persist.retry.last_error_key = Some(key);

        if should_notice {
            self.persist.retry.last_error_notice_at = Some(now);
            self.set_error_notice(error.notice_message());
        }
    }
}
```

### Pitfall: borrowing `self.persist` and `self.set_error_notice`

Avoid this bad shape:

```rust
let retry = &mut self.persist.retry;
self.set_error_notice(...); // borrow checker may complain because retry is still borrowed
```

Prefer computing booleans and assigning before calling `set_error_notice`, or limit mutable borrows with blocks:

```rust
let should_notice = {
    let retry = &self.persist.retry;
    // compute only
};

self.persist.retry.last_error_key = Some(key);

if should_notice {
    self.persist.retry.last_error_notice_at = Some(now);
    self.set_error_notice(message);
}
```

## Implementation phase 4: new dirty changes should remain prompt

### Decision

When a dirty flag is newly marked while a retry cooldown exists, should it reset `next_retry_at`?

Example:

1. History save fails at 12:00:00.
2. Next retry scheduled for 12:00:05.
3. User changes volume at 12:00:01.
4. Should UI state wait until 12:00:05, or try immediately?

Recommended 0.4.5 behavior: **do not bypass cooldown automatically**.

Reason: if the underlying problem is a bad config path, trying immediately just recreates the storm. A 5-second wait is acceptable, and quit still forces one final flush.

But add a helper that makes this policy explicit:

```rust
impl PersistFlags {
    fn mark_ui_state_dirty(&mut self) {
        self.ui_state_dirty = true;
    }

    fn mark_history_dirty(&mut self) {
        self.history_dirty = true;
    }

    fn mark_library_dirty(&mut self) {
        self.library_dirty = true;
    }
}
```

Then app methods become:

```rust
impl App {
    pub(super) fn mark_ui_state_dirty(&mut self) {
        self.persist.mark_ui_state_dirty();
    }

    pub(super) fn mark_history_dirty(&mut self) {
        self.persist.mark_history_dirty();
    }

    pub(super) fn mark_library_dirty(&mut self) {
        self.persist.mark_library_dirty();
    }
}
```

This makes the policy easy to change later if desired.

## Implementation phase 5: test seam for time

### Problem

`flush_persistence()` uses `Instant::now()`. Tests need deterministic time.

### Add internal method

```rust
// src/app/persist.rs
fn flush_persistence_at(&mut self, now: std::time::Instant, mode: PersistenceFlushMode) {
    if mode == PersistenceFlushMode::Scheduled && !self.persist.retry_due(now) {
        return;
    }

    match self.try_flush_persistence_once() {
        Ok(()) => self.persist.clear_retry_state(),
        Err(error) => self.handle_persistence_error(error, now),
    }
}

pub(super) fn flush_persistence(&mut self) {
    self.flush_persistence_at(std::time::Instant::now(), PersistenceFlushMode::Scheduled);
}

pub(super) fn force_flush_persistence(&mut self) {
    self.flush_persistence_at(std::time::Instant::now(), PersistenceFlushMode::Force);
}
```

Tests inside `src/app/persist.rs` can call `flush_persistence_at` directly.

## Implementation phase 6: tests

### Files

```text
src/app/persist.rs
```

Add tests in the same file under `#[cfg(test)] mod tests` so private fields and helpers are accessible.

### Test helper app

Use the existing lifecycle test construction style from `src/app/lifecycle.rs`. If a reusable app builder already exists in tests, use it. Otherwise add a local helper:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::lifecycle::AppParts;
    use crate::favorites::Library;
    use crate::history::History;
    use crate::radio::Station;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    fn test_app() -> App {
        let library = Library::in_memory(vec![Station::new("Test", "http://example.com/stream")]);
        App::from_parts(AppParts {
            library,
            ui_state: super::super::ui_state::UiState::default(),
            ui_state_warning: None,
            history: History::default(),
            history_warning: None,
            audio: crate::audio::AudioEngine::disconnected_for_test(),
            sample_buffer: Arc::new(Mutex::new(VecDeque::new())),
        })
    }
}
```

If `AppParts` is `pub(super)` or lives in `lifecycle.rs`, tests in `persist.rs` are sibling modules and may need a visibility adjustment:

```rust
// src/app/lifecycle.rs
pub(crate) struct AppParts { ... }
```

or add a test-only constructor in `src/app.rs`:

```rust
#[cfg(test)]
impl App {
    pub(crate) fn test_app_with_library(library: Library) -> Self {
        // build via AppParts
    }
}
```

Prefer the smallest visibility change that keeps production internals private.

### Test 1: no dirty work does nothing

```rust
#[test]
fn flush_persistence_with_no_dirty_work_does_not_schedule_retry() {
    let mut app = test_app();
    let now = std::time::Instant::now();

    app.flush_persistence_at(now, PersistenceFlushMode::Scheduled);

    assert!(app.persist.retry.next_retry_at.is_none());
}
```

### Test 2: failure keeps dirty flag and schedules retry

This requires a save failure. Use a library path that cannot be written, or construct a temporary file path whose parent is a file. Avoid relying on `/root` or OS-specific permission behavior.

Recommended helper:

```rust
fn unwritable_child_path() -> std::path::PathBuf {
    let dir = tempfile::tempdir().expect("tempdir");
    let parent_file = dir.path().join("not_a_directory");
    std::fs::write(&parent_file, "blocker").expect("write blocker file");
    parent_file.join("library.json")
}
```

Pitfall: returning a path inside a `TempDir` drops the tempdir. Keep the `TempDir` alive in the test.

```rust
#[test]
fn failed_library_save_keeps_dirty_flag_and_schedules_retry() {
    let temp = tempfile::tempdir().expect("tempdir");
    let parent_file = temp.path().join("not_a_directory");
    std::fs::write(&parent_file, "blocker").expect("write blocker");

    let mut library = Library::in_memory(vec![Station::new("Test", "http://example.com")]);
    library.set_path_for_test(Some(parent_file.join("library.json")));

    let mut app = test_app_with_library(library);
    app.mark_library_dirty();

    let now = std::time::Instant::now();
    app.flush_persistence_at(now, PersistenceFlushMode::Scheduled);

    assert!(app.persist.library_dirty);
    assert_eq!(app.persist.retry.next_retry_at, Some(now + PERSIST_RETRY_COOLDOWN));
}
```

If `Library` does not have `set_path_for_test`, add one behind `#[cfg(test)]`:

```rust
// src/favorites.rs
#[cfg(test)]
impl Library {
    pub(crate) fn set_path_for_test(&mut self, path: Option<std::path::PathBuf>) {
        self.path = path;
    }
}
```

### Test 3: scheduled flush before retry deadline does nothing

```rust
#[test]
fn scheduled_flush_before_retry_deadline_skips_disk_work() {
    let temp = tempfile::tempdir().expect("tempdir");
    let parent_file = temp.path().join("not_a_directory");
    std::fs::write(&parent_file, "blocker").expect("write blocker");

    let mut app = test_app_with_unwritable_library(parent_file.join("library.json"));
    app.mark_library_dirty();

    let now = std::time::Instant::now();
    app.flush_persistence_at(now, PersistenceFlushMode::Scheduled);

    let first_notice = app.notice.current.clone();
    app.flush_persistence_at(now + std::time::Duration::from_secs(1), PersistenceFlushMode::Scheduled);

    assert!(app.persist.library_dirty);
    assert_eq!(app.notice.current, first_notice);
}
```

### Test 4: scheduled flush at retry deadline retries

```rust
#[test]
fn scheduled_flush_at_retry_deadline_retries() {
    let temp = tempfile::tempdir().expect("tempdir");
    let parent_file = temp.path().join("not_a_directory");
    std::fs::write(&parent_file, "blocker").expect("write blocker");

    let mut app = test_app_with_unwritable_library(parent_file.join("library.json"));
    app.mark_library_dirty();

    let now = std::time::Instant::now();
    app.flush_persistence_at(now, PersistenceFlushMode::Scheduled);

    let retry_at = now + PERSIST_RETRY_COOLDOWN;
    app.flush_persistence_at(retry_at, PersistenceFlushMode::Scheduled);

    assert!(app.persist.library_dirty);
    assert_eq!(app.persist.retry.next_retry_at, Some(retry_at + PERSIST_RETRY_COOLDOWN));
}
```

### Test 5: force flush ignores retry deadline

```rust
#[test]
fn force_flush_ignores_retry_deadline() {
    let temp = tempfile::tempdir().expect("tempdir");
    let parent_file = temp.path().join("not_a_directory");
    std::fs::write(&parent_file, "blocker").expect("write blocker");

    let mut app = test_app_with_unwritable_library(parent_file.join("library.json"));
    app.mark_library_dirty();

    let now = std::time::Instant::now();
    app.flush_persistence_at(now, PersistenceFlushMode::Scheduled);
    app.flush_persistence_at(now + std::time::Duration::from_secs(1), PersistenceFlushMode::Force);

    assert_eq!(
        app.persist.retry.next_retry_at,
        Some(now + std::time::Duration::from_secs(1) + PERSIST_RETRY_COOLDOWN)
    );
}
```

### Test 6: successful save clears retry state

```rust
#[test]
fn successful_save_clears_retry_state_and_dirty_flag() {
    let temp = tempfile::tempdir().expect("tempdir");
    let library_path = temp.path().join("library.json");

    let mut app = test_app_with_library_path(library_path);
    app.mark_library_dirty();

    let now = std::time::Instant::now();
    app.flush_persistence_at(now, PersistenceFlushMode::Scheduled);

    assert!(!app.persist.library_dirty);
    assert!(app.persist.retry.next_retry_at.is_none());
    assert!(app.persist.retry.last_error_key.is_none());
}
```

### Test 7: notice throttles repeated identical errors

```rust
#[test]
fn repeated_same_persistence_error_does_not_refresh_notice_before_notice_cooldown() {
    let temp = tempfile::tempdir().expect("tempdir");
    let parent_file = temp.path().join("not_a_directory");
    std::fs::write(&parent_file, "blocker").expect("write blocker");

    let mut app = test_app_with_unwritable_library(parent_file.join("library.json"));
    app.mark_library_dirty();

    let now = std::time::Instant::now();
    app.flush_persistence_at(now, PersistenceFlushMode::Scheduled);
    let first_notice = app.notice.current.clone();

    app.flush_persistence_at(now + PERSIST_RETRY_COOLDOWN, PersistenceFlushMode::Scheduled);

    assert_eq!(app.notice.current, first_notice);
}
```

## Implementation phase 7: optional test helper for save destinations

### File

```text
src/favorites.rs
```

If needed, add this test-only helper:

```rust
#[cfg(test)]
impl Library {
    pub(crate) fn with_path_for_test(mut self, path: Option<std::path::PathBuf>) -> Self {
        self.path = path;
        self
    }
}
```

This avoids making `Library::path` public.

### Pitfall

Do not add production setters just for tests. Keep test seams under `#[cfg(test)]`.

## Implementation phase 8: changelog and README

### CHANGELOG.md

Add a new section above 0.4.4:

```markdown
## [0.4.5] - Unreleased

### Fixed
- Throttled failed persistence retries so UI state, history, and library save failures no longer retry every frame.
- Preserved dirty flags after failed saves while scheduling a later retry.
- Prevented repeated identical persistence errors from refreshing the visible notice every tick.

### Changed
- Added explicit scheduled vs forced persistence flush modes so normal ticks respect retry cooldowns while quit still attempts a final save.
- Extracted one-shot persistence save attempts into a testable helper.

### Tests
- Added coverage for retry scheduling, forced flush behavior, dirty flag preservation, retry reset after success, and notice throttling.
```

### README.md

Update the Code Quality section with a concise note:

```markdown
- Persistence writes use a retry backoff, so transient save failures keep dirty state intact without hammering the filesystem every UI tick.
```

Do not mention internal constants in README unless the existing style is very implementation-heavy.

## Validation commands

Run focused tests first:

```bash
cargo test app::persist
```

Then full validation:

```bash
cargo check
cargo test
cargo clippy --all-targets --all-features
```

Run formatting locally if the connected workspace still blocks it:

```bash
cargo fmt
```

## Manual smoke checklist

Because this release touches persistence timing, manually check these behaviors:

1. Start PulseDeck normally.
2. Change volume.
3. Quit and restart.
4. Confirm volume persisted.
5. Toggle mute.
6. Quit and restart.
7. Confirm mute persisted.
8. Add a station from search.
9. Quit and restart.
10. Confirm station persisted.
11. Remove a station.
12. Quit and restart.
13. Confirm removal persisted.
14. Export library remains unaffected.
15. Playback start/stop remains unaffected.

Optional failure test:

1. Make the config directory temporarily unwritable.
2. Change volume or library.
3. Confirm one visible error appears.
4. Confirm UI remains responsive.
5. Restore permissions.
6. Wait for retry or quit.
7. Confirm changes eventually save.

## Edge cases

### Disk becomes writable after failure

Dirty flags must remain true after the failed save. When retry cooldown expires, the save should succeed and clear the dirty flag.

### App quits during cooldown

`stop_audio_before_quit()` must call `force_flush_persistence()`, not scheduled flush. Otherwise a recently failed save could block the final quit-time save attempt.

### Multiple dirty targets

If UI state, history, and library are all dirty, the flush should attempt all three. Successful targets should clear even if another target fails.

Recommended behavior:

```text
UI state save succeeds -> ui_state_dirty = false
History save fails -> history_dirty = true
Library save succeeds -> library_dirty = false
Retry scheduled because at least one target failed
```

### Repeated identical failure

The retry should still occur after each retry cooldown, but the notice should not refresh every retry unless `PERSIST_NOTICE_COOLDOWN` has elapsed.

### Different failure after previous failure

If the error target or message changes, show a new notice immediately. Example: first UI state fails, then library fails. Those are different problems.

### No dirty flags

No dirty flags means no retry work. `retry_due` should return false, and `flush_persistence_at` should not schedule anything.

### Time arithmetic

Use `now + Duration`, not `duration_since` unless you handle earlier timestamps. Tests can pass synthetic `Instant`s safely as long as they derive from one base `Instant`.

## Rollback strategy

If this release causes trouble, rollback should be easy:

1. Revert `src/app/persist.rs` to the old direct flush implementation.
2. Revert `src/app/playback.rs::stop_audio_before_quit` from `force_flush_persistence()` back to `flush_persistence()`.
3. Keep docs/changelog rollback in the same commit.

No file format migrations are involved, so rollback has no data compatibility risk.

## Done criteria

0.4.5 is complete when:

- `src/app/persist.rs::PersistFlags` tracks retry state.
- `src/app/persist.rs::flush_persistence` respects retry cooldown.
- `src/app/persist.rs::force_flush_persistence` bypasses retry cooldown.
- Failed saves keep relevant dirty flags set.
- Successful saves clear relevant dirty flags.
- Repeated identical save errors do not refresh notices every tick.
- Quit still attempts one final save.
- `cargo check` passes.
- `cargo test` passes.
- `cargo clippy --all-targets --all-features` passes.
- `CHANGELOG.md` has a 0.4.5 entry.
- `README.md` mentions persistence backoff in the code-quality/maintainability section.

## Next release candidate after 0.4.5

After persistence backoff, the next worthwhile release is likely **0.4.6 App State Split**:

- Extract `PlaybackController` from `App`.
- Extract `UiRuntimeState` from `App`.
- Keep `UiModel` as the read-only renderer boundary.

That should wait until persistence is calm. One knot at a time, lest the codebase become a bowl of headphones.
