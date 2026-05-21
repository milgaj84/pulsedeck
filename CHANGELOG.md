# Changelog

All notable changes to the DriftFM project will be documented in this file.

---

## [0.2.0] - 2026-05-21

### Added
*   **Catppuccin Theming System**: 5 built-in themes — Retrowave (default), Catppuccin Mocha, Macchiato, Frappé, and Latte. Cycle live in the settings panel (`,` → Theme → `Space`). Theme persists between sessions.
*   **Semantic Color Architecture**: Replaced all 61 hardcoded color constants with a `ThemePalette` struct mapped to 14 semantic UI roles. Adding a new theme requires only one new function — zero UI file changes.

### Changed
*   Settings panel expanded from 4 to 5 rows to accommodate the theme selector.
*   Removed dead code: `ToggleFavoritesView`, `RefreshStations`, `ToggleSettingOption` action variants.

---

## [0.1.0] - 2026-05-20

Initial release of the DriftFM cyber-botanical retrowave audio deck and smart tape recorder.

### Added
*   **Decoupled Bounded Circular Resiliency Buffer**: Decoupled connection downloader socket from raw byte Symphonia decoders using an asynchronous consumer thread and a `1 MB` thread-safe `BufferQueue` ring buffer to neutralize stream stuttering.
*   **Volume Crossfade Transition Engine**: Smooth exponential playback volume ramping (fading out over `150ms` and swelling in over `250ms`) on active playback transitions, pauses, resumes, and station switching.
*   **Smart Tape Recording & Category Organizer**:
    *   Boundary-perfect ICY metadata stream segmenter.
    *   Dynamic parent-genre directory resolver, writing to structured paths: `recordings/<ParentGenre>/<Artist> - <Title>.mp3`.
    *   Automatic metadata tagging injecting ID3v2 Tags (Artist, Title, Station Album) into capture output.
*   **Smart Discarder & Sweep Filter**: Dynamic file purge discarding short audio fragments (under `90 seconds`) and commercial sweep tracks matching DJ speech or commercial metadata categories unless toggled otherwise in config.
*   **Retrowave Bento TUI Graphics**:
    *   Spinning cassette reel animation deck.
    *   Real-time Braille Canvas audio stream oscilloscope.
    *   Interactive genre bento tabs and marquee ticker text displays.
    *   Centred Neon Configuration popups.
*   **System Notifications**: Desktop popups triggering alerts on fresh track changes with a silent notifier queue.
*   **Persistent Configuration**: Settings stored persistently inside JSON databases to retain favorites, last played channels, startup parameters, and recording directories.
*   **Advanced Quality Suite**: Integrated modular unit testing systems for filename sanitizers and ICY metadata parsers.
