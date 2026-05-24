# Changelog

All notable changes to the DriftFM project will be documented in this file.

---

## [Unreleased]

### Fixed
*   **Search Add Shortcut Collision**: In search mode, bare `f` now remains normal text input so users can search for terms like `fm`, `funk`, and `lofi`. Adding a highlighted search result without playing now uses `Ctrl+A`.

### Improved
*   **Mode-Specific Shortcut Hints**: Updated the footer, Help HUD, and README to distinguish search actions from library actions: `Ctrl+A` adds from search, while `f` removes from the library.
*   **Saved Result Feedback**: The search bar now shows `★ Saved to library` when the highlighted search result is already in the user's library.

### Added
*   **Search Shortcut Tests**: Added keymap tests covering plain `f` text entry in search, `Ctrl+A` search add, library-mode `f`, and search-mode `Enter`.

---

## [0.1.3] - 2026-05-23

### Fixed
*   **Settings Panel Action Leak**: Pressing `b`, `v`, `r`, or other hotkeys while the settings overlay was open would incorrectly fire those actions in the background. Settings now fully blocks all unrelated input.
*   **Search Query Encoding**: Searches containing special characters (`&`, `=`, `#`, spaces, non-ASCII) now work correctly. Replaced raw URL string interpolation with `reqwest`'s `.query()` builder for proper percent-encoding.
*   **Win32 Idle Timer Rollover**: Upgraded `GetTickCount` to `GetTickCount64` for the notification sleep cascade suppression, preventing a brief malfunction every 49.7 days on long-running sessions.
*   **Theme Purity — 7 Hardcoded Colors Eliminated**: Recording indicators (`Color::Red`, `Color::Yellow`), the favorite star (`Color::Rgb(255,200,50)`), and the search bar background (`Color::Rgb(15,10,30)`) all bypassed the theme system. These now route through `theme::error()`, `theme::warm()`, and `theme::surface_color()` — making theme switching fully consistent across every UI element.

### Improved
*   **Audio Connection DRY Refactor**: Extracted the 3× duplicated connection spawning boilerplate in `audio_loop` into a shared `spawn_connection` closure, reducing ~40 lines of copy-paste.
*   **Station Count Performance**: Added `visible_count()` helper that counts filtered stations without allocating a `Vec`, replacing four redundant `visible_stations().len()` calls per tick.
*   **Song History Data Structure**: Switched `song_history` from `Vec` (O(n) front removal) to `VecDeque` (O(1) front removal).
*   **Spectrum Renderer Allocation**: Eliminated ~1,600 `String` heap allocations per frame in the spectrum analyzer by using `&'static str` block character references directly.
*   **User-Agent Version Sync**: Replaced hardcoded `DriftFM/0.1.0` User-Agent strings with compile-time `env!("CARGO_PKG_VERSION")` across both `audio.rs` and `radio.rs`.
*   **Zero Clippy Warnings**: Resolved all 3 clippy warnings (2× `needless_range_loop`, 1× `upper_case_acronyms`).
*   **Catppuccin Palette Verification**: All 4 Catppuccin flavors (Mocha, Macchiato, Frappé, Latte) verified pixel-perfect against the [official Catppuccin spec](https://catppuccin.com/palette) — 52 color values confirmed correct.

### Added
*   **Expanded Test Suite (2 → 17 tests)**:
    *   `favorites::tests` — 10 tests covering all `resolve_parent_genre` branches (Synthwave, Ambient, Rock, Vaporwave, Other), case-insensitive matching, and Library CRUD operations (add/dedup, remove, contains, genre rebuild).
    *   `ui::theme::tests` — 5 tests covering theme cycling wrap-around, advancement, key serialization/deserialization roundtrip, unknown key fallback, and label presence.

---

## [0.1.2] - 2026-05-22

### Added
*   **Real-Time Frequency-Based Spectrum Analyzer (RTA)**:
    *   Implemented high-performance pure-Rust recursive Cooley-Tukey Radix-2 FFT with a 512-sample Hanning window to minimize spectral leakage.
    *   Mapped 40 frequency bands logarithmically (matching human auditory perception) with high-frequency treble compensation.
    *   Vertical sub-character Unicode bar rendering (` `, `▂`, `▃`, `▄`, `▅`, `▆`, `▇`, `█`) providing 8-level height resolution per terminal cell.
    *   Premium three-tier retro neon gradient styling (Sunset Orange ➔ Hot Pink ➔ Neon Cyan) automatically shifting with dynamic theme roles.
    *   Smooth gravity decay peak physics (`peak - 0.08` step-down).
*   **Real-Time Oscilloscope Waveform**:
    *   Plotted actual time-domain audio samples from a thread-safe circular buffer directly onto a micro-dot Braille canvas.
    *   Scaled waveform amplitude dynamically based on active player volume.
*   **Visualizer Multi-Mode Switcher**:
    *   Added Normal mode `'v'` hotkey to cycle modes: `0 = Spectrum Analyzer`, `1 = Real Oscilloscope`, `2 = Simulated Oscilloscope`.
    *   Updated the interactive Help HUD popup, bottom keybind status bar hints, and README.md instructions.
*   **Three-Way Bento Dashboard Layout**:
    *   Upgraded the standard right-panel toggle to an elegant three-way `LayoutMode` cycle (`Split` ➔ `LeftOnly` ➔ `RightOnly`).
    *   Wired the `'b'` key in Normal mode to transition between: dual-split layout (55% list, 45% deck), closed Bento (100% station list), and full-screen Bento (100% tape deck focusing purely on the high-fidelity spectrum visualizer, spinning cassette animation, and active recording capture indicators).

### Fixed
*   **Windows Notification Sleep Cascade**: Implemented system idle-time detection via native Win32 `GetLastInputInfo` to suppress new track notifications if the system is inactive for more than 2 minutes, preventing rapid notification cascades upon monitor wake.
*   **Visualizer Gradient Direction**: Corrected an inverted `y_factor` evaluation that drew visualizer peaks in Neon Cyan and base columns in Sunset Orange, aligning layout with premium hardware styling (Cyan base, Hot Pink mids, Sunset Orange peaks).
*   **Visualizer Saturated V-Shape**: Normalized raw FFT bin norms by the window size ($N$) and implemented square root dynamic range compression on band averages. This prevents microscopic high-frequency static noise and low-frequency DC offset from clamping the equalizer columns to full height, yielding a perfectly fluid, dancing RTA spectrum.

---

## [0.1.1] - 2026-05-22

### Fixed
*   **Dynamic ICY Metadata Sync**: Replaced hardcoded metadata intervals with dynamic header extraction (`icy-metaint`), resolving audio corruption and "analog antenna" distortion on non-standard streams.
*   **Station Error Resiliency**: Added support for stations with self-signed or expired SSL certificates.
*   **Playback Stuttering**: Resolved intermittent pauses caused by metadata boundary desynchronization.

### Added
*   **Bitrate-Aware Buffer**: Improved buffer time accuracy in the UI via `icy-br` bitrate detection.

---

## [0.1.0] - 2026-05-21

Initial release of the DriftFM cyber-synthwave internet radio player and smart tape recorder.

### Added
*   **Decoupled Bounded Circular Resiliency Buffer**: Decoupled connection downloader socket from raw byte Symphonia decoders using an asynchronous consumer thread and a `1 MB` thread-safe `BufferQueue` ring buffer to neutralize stream stuttering.
*   **Volume Crossfade Transition Engine**: Smooth exponential playback volume ramping (fading out over `150ms` and swelling in over `250ms`) on active playback transitions, pauses, resumes, and station switching.
*   **Smart Tape Recording & Category Organizer**:
    *   Boundary-perfect ICY metadata stream segmenter.
    *   Dynamic parent-genre directory resolver, writing to structured paths: `recordings/<ParentGenre>/<Artist> - <Title>.mp3`.
    *   Automatic metadata tagging injecting ID3v2 Tags (Artist, Title, Station Album) into capture output.
*   **Smart Discarder & Sweep Filter**: Dynamic file purge discarding short audio fragments (under `90 seconds`) and commercial sweep tracks matching DJ speech or commercial metadata categories unless toggled otherwise in config.
*   **Catppuccin Theming System**: 5 built-in themes — Retrowave (default), Catppuccin Mocha, Macchiato, Frappé, and Latte. Cycle live in the settings panel (`,` → Theme → `Space`). Theme persists between sessions. Semantic color architecture with 14 UI roles.
*   **Retrowave Bento TUI Graphics**:
    *   Spinning cassette reel animation deck.
    *   Real-time Braille Canvas audio stream oscilloscope.
    *   Interactive genre bento tabs and marquee ticker text displays.
    *   Centred Neon Configuration popups.
*   **System Notifications**: Desktop popups triggering alerts on fresh track changes with a silent notifier queue.
*   **Persistent Configuration**: Settings stored persistently inside JSON databases to retain favorites, last played channels, startup parameters, themes, and recording directories.
*   **Advanced Quality Suite**: Integrated modular unit testing systems for filename sanitizers and ICY metadata parsers.
