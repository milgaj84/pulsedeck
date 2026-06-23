# Changelog

All notable changes to the PulseDeck project will be documented in this file.

---

## [0.6.1] - Unreleased

### Fixed
- **Unicode library filter matching**: Library filter now uses proper Unicode case folding (`to_lowercase()`) instead of ASCII-only. International station names with accented characters, Cyrillic, etc. now match correctly.
- **Number jump validates digit input**: `push_digit` now rejects non-digit characters at the domain layer, preventing silent logic errors if invalid input reaches the accumulator.
- **Number jump cleared on mode switch**: Entering library filter mode while digits are accumulated now clears the number jump state, preventing stale digits from causing unexpected jumps.
- **Integer-only digit_count**: Replaced floating-point `log10` with an integer loop in the row number width calculation, avoiding potential precision issues.

### Internal
- 693 tests pass, zero clippy warnings.
- Added property-based tests for `StationSlots` assign/get roundtrip and out-of-range rejection.
- Added `NotificationCooldown` multi-record test verifying window reset behavior.
- Added library filter edge-case tests for regex-special characters (`(`, `[`, `+`, `.*`).
- Added Unicode case-insensitivity test for library filter.

---

## [0.6.0] - Unreleased

### Added
- **Fuzzy library search**: Press `Ctrl+l` to activate an in-library substring filter. Type to instantly narrow your saved stations by name, genre, or tag — no network calls, purely in-memory. Navigate filtered results with j/k and press Enter to play. Press Esc to restore the previous view.
- **Station preset slots**: Press `Ctrl+1`–`Ctrl+5` to assign the currently playing station to a slot. Press `Alt+1`–`Alt+5` to instantly switch to that slot. Slots are fixed — they never shift until you explicitly reassign them. Slots persist across restarts.
- **Favorites / pinned stations**: Press `*` to star any station. Favorited stations display a ★ indicator and float to the top of their genre category (stable sort preserving insertion order within groups). Favorites persist in your library file.
- **Station quick-jump by number**: Row numbers now display next to stations in the library. Type digits followed by `G` or `Enter` to jump directly to that row (vim-style). A transient indicator shows your pending count. 1500ms timeout auto-cancels.
- **Tabbed help overlay**: The help screen (`h` / `?`) is now organized into 6 tabs — Playback, Library, Search, Visuals, Settings, App. Use `Tab` / `Shift+Tab` to cycle between tabs.

### Fixed
- **Notification swarm on WSL**: Desktop notifications no longer flood when multiple track titles arrive in quick succession (stream reconnection, station switch, chatty ICY metadata). A 5-second cooldown between notifications ensures at most one fires per burst while internal track state and song history continue to update for every event.

### Changed
- **Help overlay reorganized**: Controls are now grouped into logical tabs instead of one long scrollable list. Each tab focuses on a specific area (playback, library management, search, visuals, settings, app lifecycle).

### Internal
- 686 tests pass, zero clippy warnings.
- 4 new domain modules: `library_filter`, `recent_ring`, `favorites_set`, `number_jump`.
- `NotificationCooldown` struct enforces rate-limiting for desktop notifications.
- New `InputMode::LibraryFilter` variant with fully isolated key mapping.
- 11 new Action variants for the four library UX features.
- Property-based tests for `NotificationCooldown` boundary conditions.
- Persistence backward-compatible via `#[serde(default)]` — existing `library.json` files load cleanly.

---

## [0.5.1] - Unreleased

### Added
- **Comprehensive test coverage**: Added 60+ new tests across 11 modules covering previously untested public functions, error paths, state transitions, and data transformations.
  - Unit tests for `ThemeName` module (from_key, key round-trip, label, ALL)
  - Unit tests for `Station::enrich_from` covering all 9 field-path scenarios
  - Unit tests for `StationHealth::is_empty` covering each field trigger
  - Unit tests for FFT functions (`fft_rec`, `average_log_band_energy`) including pure-sine peak verification
  - Unit tests for `Library::mark_station_success` / `mark_station_failure` health tracking
  - Unit tests for `MetadataRefreshSummary::notice` format verification
  - State transition tests for `SleepTimer` pause/resume/clear behavior
  - Boundary tests for `Reconnect` backoff timing (exact deadline, 1ns before, exhaustion persistence)
  - Edge-case tests for `clean_tag_values` case-insensitive deduplication
  - Edge-case tests for command palette filtering (empty, no-match, multi-token, whitespace trimming)
  - Edge-case tests for ICY metadata parsing (multiple keys, missing delimiter, unicode, embedded quotes)
  - Property-based tests for station normalization functions (country code, codec, bitrate, URL, reflexivity)
  - Property-based tests for playlist serialization round-trips (JSON and M3U)
  - Property-based test for volume output fraction bounds and mute invariant
  - Property-based tests for genre filtering count/length consistency and "All"/None identity

### Changed
- **ThemeName extracted to cross-cutting module**: `ThemeName` now lives in `src/theme_name.rs`, removing the `app-state → ui-rendering` boundary violation (F-06).
- **UiModel sample buffer snapshot**: `UiModel` now holds a pre-copied `Vec<f32>` instead of `&Arc<Mutex<VecDeque<f32>>>`, eliminating interior-mutable shared state in the render path (F-07).
- **VisualizerMode enum**: Replaced `usize` with a proper `VisualizerMode` enum with exhaustive matching — adding a new mode now produces a compiler error (F-04).
- **Audio output manager error handling**: Replaced `expect()` calls with proper `Result`-returning error handling in `OutputManager` (F-01).
- **Stream source graceful fallback**: Replaced `expect()` calls in ICY demux with `let ... else` fallbacks that return IO errors instead of panicking (F-02).
- **Audio module visibility tightened**: `pub(crate) mod types` and `pub(crate) mod volume` narrowed to `pub(super)` (F-24).
- **Genre filtering consolidated**: Extracted `filter_stations_by_genre()` and `count_stations_by_genre()` shared helpers; `UiModel` now delegates to `app.visible_stations()` (F-12, F-23).
- **Station URL lookup helpers**: Added `find_station_by_url()` and `find_station_index_by_url()` to `src/radio/station.rs`, used across 4 files (F-17).
- **PlaybackView::reset_transient_status()**: Extracted repeated buffer/track reset triple into a single method (F-14).
- **PlaybackDiagnostics constructor**: Added `PlaybackDiagnostics::new()` for validated initial state (F-22).
- **Overlay chrome helper**: Added `render_overlay_chrome()` shared helper for overlay rendering boilerplate (F-13).
- **set_operation_error_notice()**: Added convenience method for context+error notice formatting (F-25).
- **step_choice monomorphized**: `step_choice<T>` replaced with `step_choice(ThemeName)` (F-33).
- **apply_directional_setting inlined**: Removed trivial forwarding wrapper (F-32).

### Fixed
- **WSL notifications delayed or missing**: On WSL, `notify_rust` (D-Bus) would silently accept notifications but never display them. Now skips D-Bus entirely on WSL and uses Windows toast notifications directly via PowerShell, with a registered AppUserModelID so Windows actually shows them.
- **Notification sounds during music**: Notifications are now silent on all platforms (no chime over your music). Uses `<audio silent="true"/>` on WSL/Windows and `SuppressSound` hint on Linux/macOS.
- **never_loop in decode.rs**: Fixed unconditional-break loop in prebuffer timeout test to properly loop until timeout fires (F-30).
- **Theme color unwrap safety**: Replaced 3 `unwrap()` calls on theme foreground colors with `unwrap_or_default()` (F-27).

### Removed
- **tiny_http dev-dependency**: Removed unused `tiny_http` from `[dev-dependencies]` (F-31).

### Internal
- Added 11 tests to `src/app/update.rs` covering mode-gating, action routing, and overlay isolation (F-03).
- Gated 6 dead-code functions with `#[cfg(test)]` instead of `#[allow(dead_code)]`: `load_json`, `is_codec_playback_supported`, `active_generation_arc`, `set_volume`, `reopen_needed`/`clear_reopen_needed`, `begin_fade_out`, `is_done` (F-28).
- 528 tests pass, zero warnings, `cargo build --release` clean.

---

## [0.5.0] - Unreleased

### Added
- **Multi-codec playback**: AAC, OGG/Vorbis, Opus, FLAC, and WAV streams are now playable alongside MP3 via Symphonia probe-based decoding.
- **Real-time prebuffer progress**: `AudioStatus::Buffering { percent }` provides visible buffering progress in the Playback Doctor and deck status.
- **Property-based test suite**: Added `proptest` coverage for ICY safety, volume clamping, generation monotonicity, prebuffer bounds, visualizer passivity, error classifiability, stale-generation isolation, and stop-is-clean guarantees.

### Changed
- **Audio engine rewrite**: Replaced the fragile `AudioLoopState` (12+ ad-hoc mutable flags) with a single-owner `EngineState` state machine, a `ConnectionSupervisor` with generation IDs for instant station switching, an `OutputManager` for lazy device open and bounded hardware recovery, a `VolumeRamp` for decoupled fade control, a `StreamSource` with hardened ICY demux, and a `DecodePipeline` with Symphonia probe + MP3 fast-path.
- **No more retry storms**: Generation IDs ensure rapid station switching discards stale workers deterministically. The engine performs zero automatic network reconnects — the app's existing 3/6/12s backoff is the sole reconnect mechanism.
- **Bounded connecting**: A prebuffer fill timeout (8s) makes it structurally impossible for the engine to sit in `Connecting`/`Buffering` indefinitely.
- **Total command/event handling**: Every `(state, command)` and `(state, event)` pair has a defined outcome — no panics, no corrupted flags.
- **Codec capability policy updated**: AAC, OGG/Vorbis, Opus, FLAC, and WAV flipped from `Unsupported` to `Supported`; HLS/M3U8 remains `Unsupported` (needs a segment fetcher).
- **Codec UI hints updated**: Previously-blocked codecs (AAC, OGG, Opus, FLAC, WAV) no longer show the `!` warning badge in search results or Station Details.

### Removed
- **Legacy audio internals**: Removed `src/audio/engine_loop.rs` (`AudioLoopState` and all ad-hoc flags), `src/audio/session.rs` (replaced by `decode.rs` + `supervisor.rs`), and `src/audio/stream_reader.rs` (replaced by `stream_source.rs`).

### Fixed
- **Stuck Connecting state**: The bounded prebuffer timeout guarantees the engine exits Connecting/Buffering within 8 seconds even for unreachable or hanging URLs.
- **Stale worker interference**: Generation-guarded events ensure a cancelled station's worker can never change visible state or trigger error notices after a new Play command.
- **Hardware recovery storms**: Output device recovery is capped at `MAX_HARDWARE_RECOVERY_RETRIES` per generation — no infinite reopen loops.
- **Metadata/audio byte mixing**: ICY metadata bytes are provably never delivered to the decoder (property-tested with arbitrary metaint and payload combinations).

### Internal
- Added `proptest` and `tiny_http` as dev-dependencies for property-based and integration testing.
- 445+ regression tests covering the state machine, ICY demux, volume clamping, generation monotonicity, error classification, and decoder pipeline.
- Architecturally separated concerns: `types.rs`, `volume.rs`, `output_manager.rs`, `supervisor.rs`, `stream_source.rs`, `decode.rs`, `engine_loop_v2.rs`.

---

## [0.4.7] - Unreleased

### Added
- Added a shared audio codec capability policy in `src/audio/capability.rs`.
- Added playback guardrails for known unsupported codecs before sending play commands to the audio engine.
- Added UI hints for playable, unknown, and unsupported codec metadata: `MP3` (supported), `AAC !` (unsupported), `codec ?` (unknown/empty).
- Added capability annotations in Station Details: `· playable`, `· playback will try`, `· not playable yet`.

### Changed
- Clarified that `codec:` search filters station metadata and does not guarantee playback support.
- Startup autoplay now skips known unsupported codecs instead of repeatedly trying them.
- Search empty-result hint for `codec:` now says "codec: filters metadata, playback is MP3-first" instead of suggesting AAC or OGG as equally playable options.
- Help overlay now includes a `Codec` row noting MP3 is the supported playback path.

### Fixed
- Prevented known unsupported codecs such as AAC, OGG/Vorbis, Opus, FLAC, WAV, and HLS from entering reconnect/retry loops as if they were MP3 streams.
- Blocked stations no longer persist as `last_played_url`, preventing autoplay from repeatedly trying an unsupported codec on next launch.

---

## [0.4.6] - Unreleased

### Changed
*   **App state split**: Added `src/app/ui_runtime.rs::UiRuntimeState` for navigation, overlays, notices, input mode, layout, tick count, and visualizer display state.
*   **Playback runtime split**: Added `src/app/playback_runtime.rs::PlaybackRuntime` for playback view state, volume/mute state, reconnect state, diagnostics, sleep timer, audio engine access, and the shared visualizer sample buffer.
*   **Main-loop boundary**: Updated `src/main.rs` to use `App::input_mode()` and `App::should_quit()` so the terminal loop no longer depends on the internal app field layout.

### Internal
*   Updated `App::from_parts` to construct grouped UI and playback runtime state while preserving startup warning, autoplay, diagnostics, and audio sync behavior.
*   Updated `UiModel` to snapshot grouped runtime state while keeping render modules on the existing read-model boundary.
*   Updated app, runtime, and UI tests from flat `App` fields to grouped state paths.
*   Added regression coverage for `UiRuntimeState` and `PlaybackRuntime` construction.

---

## [0.4.5] - Unreleased

### Fixed
*   **Persistence retry storms**: Failed UI state, history, or library saves now keep their dirty flags but retry after a cooldown instead of hammering the filesystem every UI tick.
*   **Repeated save-error notices**: Identical persistence failures no longer refresh the visible error notice on every retry window unless the notice cooldown has elapsed.

### Changed
*   **Scheduled vs forced saves**: Normal frame ticks use scheduled persistence flushes with backoff, while quit now uses a forced flush so PulseDeck still attempts one final save before stopping audio.
*   **Persistence save flow**: Extracted one-shot save attempts and retry bookkeeping inside `src/app/persist.rs`, preserving the existing UI state, history, and library file formats.

### Internal
*   Added regression coverage for dirty-flag preservation, retry scheduling, scheduled-skip behavior, forced flush behavior, retry reset after success, and duplicate notice throttling.

---

## [0.4.4] - Unreleased

### Changed
*   **UI read model**: Added `src/ui/model.rs::UiModel` as the read-only rendering snapshot derived from `App`, so TUI modules render from display-facing state instead of depending directly on the full app controller.
*   **Renderer boundaries**: Kept `ui::draw(frame, &app)` as the public adapter while routing header, controls, search, stations, deck, overlays, command palette, and diagnostics through `UiModel`.
*   **Visible station snapshots**: Moved visible-station and now-playing snapshots into the UI model so renderers do not recompute app selectors or reach into mutable runtime/audio/persistence internals.

### Removed
*   **UI text facade**: Removed the `src/ui/text.rs` compatibility facade after updating UI modules to use root-level `crate::text` helpers directly.

### Internal
*   Retired production use of several `App` overlay/selection helpers after the equivalent read-only behavior moved behind `UiModel`.
*   Updated UI helper tests to build `UiModel` snapshots where renderer helpers now expect the read model.
*   Added regression coverage for `UiModel` selector wiring, overlay helpers, command palette visibility, and borrowed station data.

---

## [0.4.3] - Unreleased

### Changed
*   **App construction**: Split production runtime loading from pure app state assembly with internal `AppParts` and `App::from_parts`, keeping `App::new(library)` as the public convenience constructor.
*   **Startup lifecycle wiring**: Extracted startup audio sync, warning aggregation, and autoplay setup into focused helpers so config/history loading, warning display, and auto-resume behavior are easier to test without changing playback semantics.
*   **Runtime orchestration**: Moved search debounce, Radio Browser search worker spawning, library metadata refresh worker spawning, and response draining out of `src/main.rs` into `src/runtime.rs::AppDriver`.
*   **Main loop clarity**: Reduced `src/main.rs` to CLI short-circuit, library/theme startup, terminal initialization, event polling, app updates, runtime ticking, and quit handling.

### Internal
*   Added regression coverage for injected app construction, startup warning behavior, autoplay failure visibility with a dead audio engine, runtime debounce state, and metadata refresh response draining.
*   Named the frame tick duration as `TICK_RATE` and moved search debounce timing into the runtime driver.

---

## [0.4.2] - Unreleased

### Changed
*   **Station persistence model**: Removed the duplicate `SavedStation` mirror type and now serialize `Station` directly while preserving load-time normalization for bitrate, UUID, country code, tags, language, codec, and homepage.
*   **Library loading flow**: Unified `Library::load` and `Library::load_existing` behind one internal missing-library policy so starter seeding and read-only empty-library behavior share the same parse/read path without changing their public behavior.
*   **Search prefix handling**: Search prefix help, aliases, API parameters, and display labels now come from one metadata table instead of repeated match chains.
*   **Playlist export boundary**: Moved M3U export filesystem work into `src/playlist_export.rs`, leaving `src/app/playback.rs::export_library` as UI-facing notice glue.
*   **Text helper boundary**: Moved unicode-aware text truncation helpers into root-level `src/text.rs`; `src/ui/text.rs` is now only a UI compatibility facade.

### Removed
*   **Unused dependency noise**: Removed the unused `crossterm` `event-stream` feature and stale commented tracing dependency placeholders from `Cargo.toml`.

### Internal
*   Added regression coverage for direct station persistence normalization, compact station serialization, missing-library fallback policies, playlist export paths/content, generated prefix examples, and alias-driven prefix lookup.

---

## [0.4.1] - Unreleased

### Fixed
*   **Silent audio-engine command failures**: Audio commands now report when the engine command channel is closed, and user-triggered playback actions surface a visible error instead of pretending playback changed.
*   **Station identity mismatches**: Library remove, contains, health updates, now-playing lookup, selection restoration, last-played selection, and track metadata matching now share normalized stream-URL matching instead of mixing raw URL equality with normalized identity logic.
*   **UUID whitespace edge cases**: Radio Browser UUID identity comparison now trims both sides before comparing, while still refusing to merge stations with conflicting non-empty UUIDs.

### Changed
*   **Audio engine loop structure**: Extracted `AudioLoopState` from `src/audio/engine_loop.rs::audio_loop` so command handling, fade ticks, connection completion, sink-end detection, and connection spawning are isolated without changing decoder or buffering behavior.
*   **Passive audio sync reporting**: Volume, output-device, and stream-metadata sync commands now return send status, allowing important call sites to detect failure without spamming notices from passive sync paths.

### Removed
*   **Dead audio prototypes**: Removed stale, uncompiled audio experiment files from `src/audio/`: `buffer.rs`, `buffer_meter.rs`, `decoded_source.rs`, `pcm_buffer.rs`, `pcm_buffer2.rs`, and `probe_reader.rs`.

### Internal
*   Added regression coverage for normalized station URL matching, UUID whitespace and conflict handling, normalized remove/contains/health/now-playing/select-playing behavior, track metadata URL matching, audio command-channel failure, and dead-engine app-state recovery.

---

## [0.4.0] - Unreleased

### Added
*   **Command palette**: Added `:` / `Ctrl+p` command search for common actions including station search, retry, stop, settings, theme changes, song-info metadata toggle, Playback Doctor, library metadata refresh, export, and help.
*   **Playback Doctor**: Added a diagnostics overlay for playback state, output device, song-info metadata mode, reconnect attempts, decoder state, recent events, and error hints.
*   **Stream Song Info Metadata setting**: Added a settings row for ICY now-playing metadata. It remains default-on and can be disabled per user preference.
*   **Library metadata refresh**: Added a command-palette-triggered refresh that enriches older saved stations with missing Radio Browser metadata while preserving saved-facing names, stream URLs, and genres.
*   **Import preview and enrich-only modes**: Added CLI import preview and enrich-only flows so users can inspect new stations, duplicates, enrichments, and skipped entries before saving.
*   **Station health memory**: Added local success/failure memory for saved stations, compact health badges in the library, and local health details in Station Details.

### Changed
*   **Direct MP3 live-stream playback path**: Playback now reads the HTTP response directly through the ICY-aware `StreamReader`, uses a small `BufReader`, and calls Rodio's MP3 decoder directly instead of the generic format-probing decoder.
*   **No internal playback queue in the active path**: The active module graph no longer includes the previous stream byte queue, buffer meter, prebuffer, downloader thread, or probe replay shim.
*   **Codec support is honest**: Search can still show codec metadata, but the active playback path is currently optimized for MP3 streams until explicit non-MP3 decoder selection is added.

### Improved
*   **Search explainability**: Highlighted search results now explain the strongest matching signals, such as exact tag, country code, codec, saved status, check health, HTTPS, votes, or clicks.
*   **Station Details organization**: Station Details now groups fields into Identity, Playback, Catalog, and Health sections with safer fallbacks for missing metadata.
*   **Playback diagnostics and recovery copy**: Playback errors now include more actionable retry, stop, output, metadata, decoder, and search guidance.
*   **CLI import reporting**: Import summaries now report added, enriched, and skipped counts separately.

### Fixed
*   **Slow or stuck stream startup on MP3 stations**: Bypassed Rodio's generic decoder probing path for the active stream path, avoiding the long-load/stutter behavior seen when live streams were treated too much like probeable files.
*   **Live-stream seek semantics**: The active stream reader still refuses real seeks instead of consuming or discarding live audio bytes.
*   **Health timestamp ordering**: Local station health compares numeric timestamps correctly, so newer failures and successes are classified accurately.
*   **Import duplicate handling**: Incoming import files are deduplicated against themselves as well as against the existing library.
*   **Metadata refresh safety**: Refresh matching rejects name-only candidates to avoid enriching a saved station from a similarly named Radio Browser neighbor.

### Internal
*   Added regression coverage for command palette routing, import preview, metadata refresh summaries, Playback Doctor rendering, station health, stream-reader metadata stripping, live-stream seek refusal, and grouped Station Details.
*   Stale audio experiment files still exist in the repository but are not wired into `src/audio.rs`; they should not be described as active playback behavior.

---

## [0.3.1] - 2026-06-18

### Added
*   **Structured Radio Browser search**: Search now supports focused prefixes for station names, tags/genres, country names or country codes, languages, and codecs: `name:`, `station:`, `tag:`, `genre:`, `country:`, `cc:`, `lang:`, `language:`, `codec:`, and `format:`. Plain text still searches station names.
*   **Richer station metadata**: Search results and saved stations can now carry Radio Browser UUIDs, country codes, tags, language, codec, homepage, last-check status, votes, and click counts.
*   **Expanded Station Details**: The `i` overlay now shows the richer station metadata when available, including trust/popularity fields and the station UUID.

### Fixed
*   **Structured search guidance**: Empty-result hints now explain what to try next based on the prefix being used, and unknown prefixes are clearly treated as plain station-name searches.
*   **Radio Browser metadata hardening**: Tags, codecs, country codes, bitrates, UUIDs, and last-check flags are normalized defensively before display or persistence.
*   **Search outage messages**: Radio Browser mirror failures now show a friendly user-facing summary while preserving server details internally.
*   **Playback startup and stutter**: Live radio streams no longer simulate forward seeks by reading and discarding real stream bytes during decoder probing, fixing long tuning delays and broken buffering on affected stations.
*   **Decoder read buffering**: Stream prebuffering now targets about two seconds of audio with a smaller 32 KiB floor and a shorter 2-second wait cap, while decoder reads go through a 64 KiB buffer to avoid tiny queue reads during stream probing.
*   **Visualizer audio path**: The visualizer is now a passive, non-blocking tap that copies small sample batches when the UI buffer is available instead of maintaining a separate decoded-PCM playback queue.
*   **Clean stream bytes**: Playback no longer requests ICY metadata by default, avoiding server-injected metadata bytes on streams where chunk timing can confuse decoders.
*   **Startup autoplay**: Last-played autoplay now starts the stored stream URL even when the saved-library URL no longer matches byte-for-byte after Radio Browser URL resolution.

### Improved
*   **Saved-station identity**: Saved search results are detected by Radio Browser UUID when available, falling back to normalized stream URL matching.
*   **Saved-station refresh**: Selecting an already-saved matching search result can refresh missing station metadata without duplicating the library entry or replacing the saved name/URL.
*   **Search ranking and dedupe**: Results are deduplicated and ranked locally so exact tag, country-code, language, and codec matches outrank loose popularity signals.
*   **In-app docs**: Help and README now surface structured-search examples and aliases.

### Internal
*   Split Radio Browser search into query, client, mapping, ranking, and station modules.
*   Expanded regression coverage for prefix parsing, metadata mapping, ranking, station identity, enrichment, legacy playlist compatibility, stream prebuffer targets, exact ICY metadata reads, and live-stream seek refusal.

---

## [0.2.4] - 2026-06-18

### Fixed
*   **CLI command handling**: Unknown CLI commands now report a clear error instead of silently launching the TUI.
*   **Release checklist**: Updated stale release commands and version references so future patch releases do not inherit 0.1.x examples.
*   **History wording**: Clarified Recent Tracks and persistent Listening History wording in user-facing docs and panel copy.

### Improved
*   **CLI export paths**: Export now creates missing parent directories for nested output paths.
*   **Config load warnings**: Malformed or unreadable persisted config now surfaces a startup warning while PulseDeck keeps running with safe defaults.
*   **Runtime hardening**: Buffer telemetry and theme palette access now avoid panics on poisoned locks.

### Internal
*   Added regression coverage for CLI unknown-command handling, nested export paths, config warning parsing, and history panel titling.

---

## [0.2.3] - 2026-06-16

### Changed
*   **Sleep timer timing**: Sleep timers now wait for active playback instead of silently counting down while no station is playing.

### Internal
*   Split shared UI text helpers, deck rendering, theme palettes, and visualizer DSP into smaller modules.
*   Replaced independent overlay booleans with a single `ActiveOverlay` state.
*   Grouped `App` navigation, search, playback, overlay, notice, and persistence state into focused structs.
*   Unified config-path handling for library, history, and UI-state persistence, with debounced writes flushed from the app tick.
*   Flattened audio status handling and split the audio engine loop into `src/audio/engine_loop.rs`.
*   Removed a dead audio-engine buffer field, hardened audio buffer lock errors, and named lifecycle timing constants.

---

## [0.2.2] - 2026-06-15

### Added
*   **Auto-reconnect**: Added resilient auto-reconnect backoff on unintended dropouts or errors.
*   **Persistent history**: Added opt-in history persistence for track history overlay.
*   **Sleep timer panel**: Added a sleep timer overlay (`t`) with 5-minute fine adjustments (`↑`/`+`, `↓`/`-`), one-key presets for 15/30/45/60/90/120 minutes, and an off toggle (`0`/`c`), all on an isolated input mode that cannot collide with other shortcuts.
*   **Import / export**: Added TUI (`e` key) and CLI import/export features for stations library.

---

## [0.2.1] - 2026-06-13

### Added
*   **Station Details Overlay**: Added an `i` shortcut in normal mode for inspecting the highlighted station's name, genre, country, bitrate, saved status, current track context, and stream URL.
*   **Recent Tracks Overlay**: Added a `g` shortcut for a session-only list of stream-provided track titles heard during the current run.
*   **Manual Stream Retry**: Added a plain `r` shortcut for retrying the current stream after playback errors without mapping legacy `Ctrl+r` behavior.
*   **Empty Library Onboarding**: Added an in-panel first-run guide that points new users toward search, auditioning, saving, settings, and help.
*   **Terminal Theme**: Added a terminal-native theme that uses ANSI colors and reset backgrounds for users who prefer their emulator palette.

### Changed
*   **Post-0.2 UX Language**: Reworded visible layout and visualizer labels around Split View, Library Focus, Signal Focus, RTA, Real Osc, and Sim Osc.
*   **Settings Overlay Copy**: Renamed the settings surface away from config-console language and added per-row descriptions plus saved-automatically guidance.

### Improved
*   **Adaptive Footer Hints**: The footer now emphasizes the most relevant actions for empty libraries, search, playback, errors, and open overlays instead of showing every shortcut at once.
*   **Search Action Clarity**: Search list titles and footer hints now make the `Space` preview versus `Enter` save-and-play distinction harder to miss.
*   **Overlay Behavior**: Help, settings, station details, and recent tracks now close or replace each other predictably, and `q` / `Esc` closes overlays before quitting.
*   **Playback Error Recovery**: Error-state hints now surface retry, stop, audio-output settings, and search as recovery actions.

### Removed
*   _Nothing yet._

---

## [0.2.0] - 2026-06-03

### Changed
*   **Focused 0.2 product reset**: PulseDeck now focuses on terminal radio playback, station discovery, saved library management, themes, visualizers, and audio reliability.

### Removed
*   Removed recording, local tape archive management, local tape playback, recording recovery, and tape file-management workflows.

### Added
*   _Nothing yet._

### Improved
*   **Stream Playback Prebuffer**: Added a short startup prebuffer and larger stream queue to smooth regular internet radio playback.

---

## [0.1.8] - 2026-06-02

### Added
*   **Recording Session Dashboard**: Added a live Tape Deck recording dashboard with station, elapsed time, capture path, file size, minimum duration, and snippet policy.
*   **Recording Recovery Journal**: Added a hidden session journal so pending or active recording sessions can be surfaced after an unexpected exit.
*   **Recording Recovery Actions**: Added keep, trash, and dismiss controls for abandoned recording recovery journals.
*   **Recording Duplicate Protection**: Added duplicate recording detection so existing artist/title captures are not overwritten.
*   **Richer Recording Metadata**: Added genre/category and source-stream context to completed MP3 ID3 tags when available.
*   **Local Tape Playback Modes**: Added Stop, Folder, All Recordings, Repeat, and Shuffle continuation modes for local tape playback.
*   **Local Tape Details Inspector**: Added an `i` shortcut for selected tape metadata and path details.
*   **Local Tape Rename and Move**: Added `Shift+R` rename and `Shift+M` move workflows for local recordings.
*   **Local Tape Progress Display**: Added elapsed / duration progress indicators for the currently playing local tape.
*   **Local Tape Library Browser**: Replaced the passive Tape History page with a disk-backed Local Tape Library for browsing captured recordings by folder.
*   **Local Tape Playback**: Added audio-engine support for playing recorded files directly through PulseDeck.
*   **Tape Archive Refresh and Delete Flow**: Added archive rescanning plus guarded `y`/`n` delete confirmation for selected local tape files.
*   **All Recordings Flat View**: Added a newest-first flat mode for browsing every local recording across folders.
*   **Local Tape Filtering**: Added tape-page filtering by title, folder, artist, extension, and filename.
*   **Local Tape Duration Labels**: Added best-effort `MM:SS` duration metadata for local recordings when decoders expose it.
*   **Local Tape Folder Opener**: Added an `o` shortcut for opening a selected recording's containing folder.
*   **Local Tape Playback Handoff**: Added automatic handoff to the next local recording in the same folder when a tape finishes.
*   **Compact Terminal Boundary Guard**: Added a root 80x24 terminal-size safety gate with a centered diagnostic screen before complex deck, station, help, or settings rendering runs.
*   **Search-Aware Name Truncation**: Added adaptive station-name truncation that pivots around the active search term when long result names would otherwise hide the match.
*   **Audio Output Recovery Retry**: Added a guarded one-shot recovery path for hardware-style sink failures by dropping stale output handles and retrying the current stream.

### Improved
*   **Recording Visibility**: Improved tape capture feedback so recording state is visible in the deck instead of only as transient footer notices.
*   **Recording Recovery Safety**: Failed recovery trash moves now keep the journal available for retry or a non-destructive keep/dismiss action.
*   **Recording Archive Safety**: Duplicate captures now produce a visible skip notice instead of silently replacing existing files.
*   **Local Tape Continuation Control**: Replaced hard-wired folder handoff with user-selectable local playback behavior.
*   **Local Tape File Management Safety**: Rename and move operations sanitize target names, prevent overwrite, stop active local playback first, and refresh the archive.
*   **Local Tape Playback Feedback**: Playing tape rows now surface progress without requiring a separate inspector.
*   **Recording Workflow Continuity**: Captured tracks are now visible and playable from inside the TUI after restart instead of only existing as external files.
*   **Local Tape Metadata Rows**: Improved local recording rows with format, duration when available, size, and folder context in All Recordings mode.
*   **Local Tape Help and Footer Hints**: Updated help and footer controls for local filtering, All Recordings mode, folder opening, refresh, and trash confirmation.
*   **Trash-Backed Tape Removal**: Local tape removal now moves files to the OS trash instead of permanently deleting them.
*   **Theme-Safe Clear Styling**: Routed root and overlay clear blocks through the semantic theme palette so light and dark themes keep consistent contrast.
*   **Buffer Status Backpressure**: Suppressed duplicate buffer-level status packets before they cross into the UI status queue.
*   **Selection Context Preservation**: Preserved library selection across search mode and remembered cursor position per genre category.
*   **Overlay Boundary Fallbacks**: Added compact fallback diagnostics for cramped help and settings overlays.

---

## [0.1.7] - 2026-06-01

### Fixed
*   **Documentation Parity**: Updated README, help overlay, and testing strategy docs for Audio Output settings, 15s–600s Min Song Duration controls, search-mode audio shortcuts, and audio-output smoke coverage.
*   **Quiet Audio Device Probing**: Suppressed native ALSA/JACK stderr diagnostics while enumerating or opening output devices so backend probe noise does not overwrite the terminal UI.

### Added
*   **Search Audio Escape Rails**: Added Ctrl/Alt volume and mute shortcuts inside search mode so core playback controls remain reachable without abandoning the active query.
*   **Selectable Audio Output Device**: Added a persisted settings row for choosing Default or a detected output device, allowing PipeWire/PulseAudio Bluetooth headphones to be targeted instead of always using Rodio's default sink.
*   **Search Audition Shortcut**: Added `Space` / `Ctrl+Enter` in search mode to stream the highlighted global result without saving it to the Library.

### Improved
*   **Fine Min Duration Settings**: Split Min Song Duration controls so Space cycles common presets including 45s and 300s, while directional controls nudge by 5 seconds within a 15s to 600s range.
*   **Overlay Critical Engine Alerts**: Mirrored playback engine errors inside help and settings overlays so stream failures remain visible even when modal screens cover the dashboard.
*   **Search Stale Response Telemetry**: Added a visible stale-response discard state so late async search payloads identify the ignored query instead of silently disappearing from the search bar.
*   **Adaptive Buffer Timing**: Replaced static bitrate-only buffer seconds with an EWMA consumption meter driven by real bytes popped from the playback queue, reducing VBR and burst-network status jitter.
*   **Treble Texture Spectrum Response**: Replaced the hard high-frequency gain shelf with a soft-knee curve and variance-aware treble expansion so crisp hats and synth arpeggios keep visual texture without reviving noisy final-band spikes.
*   **Audio Fade Visual Continuity**: Added an explicit fading-out status from the audio engine so deck status, reels, and visualizers remain active while the sink volume ramps down.
*   **Bounded Library Undo History**: Expanded station removal undo from a single volatile slot to a 10-entry history stack, letting repeated `u` restores roll back multiple recent deletions.
*   **Search Debounce Indicator**: Replaced the static `Searching soon` debounce message with a fast animated initializing indicator so search feels responsive immediately while debounce still protects API calls.
*   **Genre Cursor Preservation**: Kept the cursor anchored to the currently playing station when switching library genres, falling back to the first row only when the live station is absent.
*   **Disabled Settings Focus State**: Softened the selected state for disabled config rows so inactive settings no longer look fully adjustable when focused.
*   **Bidirectional Settings Cycling**: Added Left/Right and h/l/a/d directional shortcuts in the settings popup so cyclic values can step backward as well as forward.
*   **Progressive Volume Steps**: Changed `+` / `-` volume control to use finer 2% steps at low volume, 5% midrange steps, and faster 10% steps at high volume.
*   **Spectrum Tuning Feedback**: Kept the RTA Spectrum alive during `TUNING...` / connecting states with a subtle ambient pulse, matching the oscilloscope's existing interstitial feedback.

---

## [0.1.6] - 2026-05-26

### Improved
*   **Visual Enchantments Phase 1 - Cassette Geometry**: Stabilized the cassette deck illustration with fixed-width rows, fixed reel cells, cassette row-width tests, and theme-routed reel/status styling so animation no longer changes the cassette shape.
*   **Visual Enchantments Phase 2 - Deck Composition**: Reused the stable cassette design across split and full Bento layouts, framed the visualizer as a hardware signal screen, added visualizer mode titles, and upgraded the deck metadata into a bordered signal/status strip.
*   **Visual Enchantments Phase 3 - Spectrum Calibration**: Retuned the spectrum analyzer's visual response with a gentler gain curve, noise-floor gating, band smoothing, softer compression, and faster final-treble release so high-frequency bars look more natural without changing audio playback.
*   **Visual Enchantments Phase 4 - Footer and Help Polish**: Refined the footer into clearer status chips with layout/scope/recording labels, made shortcut hints more deck-aware, and refreshed the help overlay wording around Tape Deck, Tape History, and scope controls.
*   **Visual Enchantments Phase 5 - Library and Search Polish**: Refined station rows with clearer selected/playing markers, saved-result stars, compact country/bitrate metadata, safer name truncation, and calmer library/search titles.
*   **Tape History Naming**: Renamed the deck history page title from the sediment metaphor to `Tape History` so the visual language stays cassette-first.

---

## [0.1.5] - 2026-05-26

### Changed
*   **Project Rebrand**: Renamed DriftFM to PulseDeck to avoid name confusion with existing and historical radio-related uses of the old name. The crate, binary, app UI, notifications, README, and config paths now use PulseDeck naming.
*   **Config Migration**: Existing DriftFM config files are copied into the new PulseDeck config directory on first launch, preserving libraries and UI preferences while leaving the old files as a backup.

### Improved
*   **Recording Feedback Notices**: Pressing `r` now gives clear footer feedback when recording cannot start without playback, when tape capture is armed for the next track boundary, and when recording stops.
*   **Undoable Station Removal**: Removing a station with `f` now stores the most recent removal and shows a footer prompt so users can restore it with `u`.
*   **Settings Row Model**: Replaced hardcoded settings row indices with a typed `SettingRow` list shared by the reducer and settings overlay, making future settings safer to add.
*   **Settings Disabled-State Consistency**: The minimum song duration row now stays inert when partial snippets are kept, matching its dimmed disabled UI state.
*   **WSL Notification Fallback**: Desktop notifications now fall back to a Windows PowerShell notification balloon when PulseDeck is running under WSL and the normal Linux notification path is unavailable.
*   **Playback Pause Responsiveness**: Starting playback now marks the app as connecting immediately, and pause now acts directly on the audio sink to avoid delayed Space handling and visualizer/audio desync.
*   **Persisted UI State**: PulseDeck now remembers volume, mute state, layout mode, and visualizer mode across launches using a dedicated `ui-state.json` file.

### Added
*   **Recording Feedback Tests**: Added app-level tests covering recording notices for stopped, pending, and stopped-again states.
*   **Undo Removal Tests**: Added shortcut and app-state tests covering undo restore behavior, empty undo feedback, replacement of older undo slots, and filtered genre restores.
*   **Settings Row Tests**: Added settings tests for row mapping, navigation wrapping, and disabled minimum-duration behavior.
*   **WSL Notification Tests**: Added notifier tests for WSL kernel detection and PowerShell string escaping.
*   **Playback State Tests**: Added app-level tests covering immediate connecting state, stop state sync, and Space while connecting.
*   **UI State Tests**: Added tests for UI-state defaults, layout key round-tripping, visualizer clamping, and corrupted-value sanitizing.
