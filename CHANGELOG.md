# Changelog

All notable changes to the DriftFM project will be documented in this file.

---

## [Unreleased]

### Improved
*   **Recording Feedback Notices**: Pressing `r` now gives clear footer feedback when recording cannot start without playback, when tape capture is armed for the next track boundary, and when recording stops.
*   **Undoable Station Removal**: Removing a station with `f` now stores the most recent removal and shows a footer prompt so users can restore it with `u`.
*   **Settings Row Model**: Replaced hardcoded settings row indices with a typed `SettingRow` list shared by the reducer and settings overlay, making future settings safer to add.
*   **Settings Disabled-State Consistency**: The minimum song duration row now stays inert when partial snippets are kept, matching its dimmed disabled UI state.
*   **WSL Notification Fallback**: Desktop notifications now fall back to a Windows PowerShell notification balloon when DriftFM is running under WSL and the normal Linux notification path is unavailable.

### Added
*   **Recording Feedback Tests**: Added app-level tests covering recording notices for stopped, pending, and stopped-again states.
*   **Undo Removal Tests**: Added shortcut and app-state tests covering undo restore behavior, empty undo feedback, replacement of older undo slots, and filtered genre restores.
*   **Settings Row Tests**: Added settings tests for row mapping, navigation wrapping, and disabled minimum-duration behavior.
*   **WSL Notification Tests**: Added notifier tests for WSL kernel detection and PowerShell string escaping.

---

## [0.1.4] - 2026-05-25

### Fixed
*   **Radio Browser HTTP Fallback**: Station search now falls back to Radio Browser HTTP mirrors when all HTTPS mirrors fail because of upstream certificate or TLS problems.
*   **Radio Browser Search Failover**: Station search now retries multiple Radio Browser mirrors and surfaces compact error details instead of only showing a generic connection failure.
*   **Security: TLS Certificate Validation**: Removed the insecure stream HTTP client certificate bypass so HTTPS streams now use normal certificate validation.
*   **Persistence Error Visibility**: Library save failures now return errors instead of being silently ignored, and user-triggered save failures are surfaced in the TUI status bar.
*   **Search Text Input Collision**: In search mode, bare `f` now remains normal text input so users can search for terms like `fm`, `funk`, and `lofi`.
*   **Search Add Flow Simplified**: Removed the add-without-play shortcut path after terminal compatibility testing showed it was unreliable on Ubuntu. Search results are now added only through `Enter`, which also starts playback.
*   **Stale Search Result Race**: Search API responses are now tied to the query that created them. Older responses are ignored if the user has already typed a newer query, preventing outdated results from replacing the current search.
*   **Terminal Restore Safety**: Added a terminal restore guard so the terminal is cleaned up even when the main loop exits early with an error.
*   **Search Failure Feedback**: Failed station searches now surface a clear search error state instead of silently doing nothing.

### Improved
*   **App State Module Split**: Split the monolithic app reducer into focused modules for types, lifecycle, selectors, search, playback, settings, library, recording, overlays, visualizer, and platform idle helpers while preserving the public app API.
*   **App Reducer Contract Tests**: Added state-level tests covering settings action blocking, search confirmation, playback state updates, overlay toggles, library removal, genre navigation, and recording toggles.
*   **Audio Session Extraction**: Moved stream connection, retry/backoff, decoder setup, downloader setup, and sink creation into a dedicated audio session module while preserving playback behavior.
*   **Lazy Audio Device Initialization**: DriftFM now opens the system output device on first playback instead of app startup, so browsing and search remain usable when no soundcard is immediately available.
*   **Audio Module Architecture**: Split audio internals into focused modules for buffering, metadata parsing, recording helpers, stream reading, and visualizer sample wrapping while preserving the public `crate::audio` API.
*   **Audio Architecture Documentation**: Added developer documentation describing the audio public boundary, module map, refactor rules, and follow-up work.
*   **CI Quality Gates**: Added GitHub Actions checks for formatting, clippy with warnings denied, tests, release build, RustSec audit, and a static guard against reintroducing invalid-certificate acceptance.
*   **README Quality Claims**: Replaced static test/warning badges with CI-backed quality documentation.
*   **Mode-Specific Shortcut Hints**: Updated the footer, Help HUD, and README to distinguish search actions from library actions: `Enter` adds and plays from search, while `f` removes from the library.
*   **Saved Result Feedback**: The search bar now shows `Saved to library` when the highlighted search result is already in the user's library.
*   **Debounced Search Requests**: Station search now waits briefly while the user types, reducing unnecessary API requests.
*   **Search Status Hints**: The search bar now distinguishes short queries, pending debounce, active searches, empty results, saved results, and failed searches.

### Added
*   **Phase 4 Test Hardening**: Added deterministic unit tests for Radio Browser fallback helpers, API station mapping, fallback station defaults, and audio buffer-level status math.
*   **Testing Strategy Documentation**: Added `docs/testing-strategy.md` describing local gates, test layers, network-test boundaries, and the manual runtime smoke checklist.
*   **Search Shortcut Tests**: Added keymap tests covering plain text entry in search, disabled add-only keys, library-mode `f`, and search-mode `Enter`.
*   **Search State Tests**: Added app-level tests for short-query reset, debounce state, accepted results, empty results, error responses, stale responses, and normal-mode response ignores.

---

## [0.1.3] - 2026-05-23

### Fixed
*   **Settings Panel Action Leak**: Pressing `b`, `v`, `r`, or other hotkeys while the settings overlay was open would incorrectly fire those actions in the background. Settings now fully blocks all unrelated input.
*   **Search Query Encoding**: Searches containing special characters (`&`, `=`, `#`, spaces, non-ASCII) now work correctly. Replaced raw URL string interpolation with `reqwest`'s `.query()` builder for proper percent-encoding.
*   **Win32 Idle Timer Rollover**: Upgraded `GetTickCount` to `GetTickCount64` for the notification sleep cascade suppression, preventing a brief malfunction every 49.7 days on long-running sessions.
*   **Theme Purity - 7 Hardcoded Colors Eliminated**: Recording indicators, the favorite star, and the search bar background all bypassed the theme system. These now route through semantic theme helpers so theme switching is consistent across UI elements.

### Improved
*   **Audio Connection DRY Refactor**: Extracted duplicated connection spawning boilerplate in `audio_loop` into a shared `spawn_connection` closure, reducing copy-paste.
*   **Station Count Performance**: Added `visible_count()` helper that counts filtered stations without allocating a `Vec`, replacing redundant `visible_stations().len()` calls per tick.
*   **Song History Data Structure**: Switched `song_history` from `Vec` to `VecDeque`.
*   **Spectrum Renderer Allocation**: Eliminated repeated heap allocations per frame in the spectrum analyzer by using static block character references directly.
*   **User-Agent Version Sync**: Replaced hardcoded User-Agent strings with compile-time `env!("CARGO_PKG_VERSION")` across both `audio.rs` and `radio.rs`.
*   **Zero Clippy Warnings**: Resolved all clippy warnings present in that release.
*   **Catppuccin Palette Verification**: All 4 Catppuccin flavors verified against the official Catppuccin spec.

### Added
*   **Expanded Test Suite (2 to 17 tests)**:
    *   `favorites::tests` covering genre resolution and Library CRUD operations.
    *   `ui::theme::tests` covering theme cycling, key serialization/deserialization, unknown key fallback, and label presence.

---

## [0.1.2] - 2026-05-22

### Added
*   **Real-Time Frequency-Based Spectrum Analyzer (RTA)**:
    *   Implemented pure-Rust recursive Cooley-Tukey Radix-2 FFT with a 512-sample Hanning window.
    *   Mapped 40 frequency bands logarithmically with high-frequency treble compensation.
    *   Vertical sub-character Unicode bar rendering provides 8-level height resolution per terminal cell.
    *   Premium three-tier retro neon gradient styling shifts with dynamic theme roles.
    *   Smooth gravity decay peak physics.
*   **Real-Time Oscilloscope Waveform**:
    *   Plotted actual time-domain audio samples from a thread-safe circular buffer directly onto a Braille canvas.
    *   Scaled waveform amplitude dynamically based on active player volume.
*   **Visualizer Multi-Mode Switcher**:
    *   Added Normal mode `v` hotkey to cycle modes: Spectrum Analyzer, Real Oscilloscope, and Simulated Oscilloscope.
    *   Updated the interactive Help HUD popup, bottom keybind status bar hints, and README instructions.
*   **Three-Way Bento Dashboard Layout**:
    *   Upgraded the standard right-panel toggle to a three-way `LayoutMode` cycle.
    *   Wired the `b` key in Normal mode to transition between split layout, closed Bento, and full-screen Bento.

### Fixed
*   **Windows Notification Sleep Cascade**: Implemented system idle-time detection via native Win32 `GetLastInputInfo` to suppress new track notifications if the system is inactive for more than 2 minutes.
*   **Visualizer Gradient Direction**: Corrected inverted visualizer gradient direction.
*   **Visualizer Saturated V-Shape**: Normalized FFT bin norms and implemented square root dynamic range compression on band averages.

---

## [0.1.1] - 2026-05-22

### Fixed
*   **Dynamic ICY Metadata Sync**: Replaced hardcoded metadata intervals with dynamic header extraction.
*   **Station Error Resiliency**: Added support for stations with self-signed or expired SSL certificates.
*   **Playback Stuttering**: Resolved intermittent pauses caused by metadata boundary desynchronization.

### Added
*   **Bitrate-Aware Buffer**: Improved buffer time accuracy in the UI via bitrate detection.

---

## [0.1.0] - 2026-05-21

Initial release of the DriftFM cyber-synthwave internet radio player and smart tape recorder.

### Added
*   **Decoupled Bounded Circular Resiliency Buffer**: Decoupled connection downloader socket from raw byte Symphonia decoders using an asynchronous consumer thread and a 1 MB thread-safe buffer.
*   **Volume Crossfade Transition Engine**: Smooth playback volume ramping on active playback transitions, pauses, resumes, and station switching.
*   **Smart Tape Recording & Category Organizer**:
    *   Boundary-perfect ICY metadata stream segmenter.
    *   Dynamic parent-genre directory resolver.
    *   Automatic metadata tagging into capture output.
*   **Smart Discarder & Sweep Filter**: Dynamic file purge discarding short audio fragments and commercial sweep tracks unless toggled otherwise in config.
*   **Catppuccin Theming System**: 5 built-in themes with semantic color architecture.
*   **Retrowave Bento TUI Graphics**:
    *   Spinning cassette reel animation deck.
    *   Real-time Braille Canvas audio stream oscilloscope.
    *   Interactive genre bento tabs and marquee ticker text displays.
    *   Centred Neon Configuration popups.
*   **System Notifications**: Desktop popups triggering alerts on fresh track changes with a silent notifier queue.
*   **Persistent Configuration**: Settings stored persistently inside JSON databases.
*   **Advanced Quality Suite**: Integrated modular unit testing systems for filename sanitizers and ICY metadata parsers.
