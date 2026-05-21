# Changelog

All notable changes to the DriftFM project will be documented in this file.

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

