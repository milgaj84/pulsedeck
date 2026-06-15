# Changelog

All notable changes to the PulseDeck project will be documented in this file.

---

## [Unreleased]

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
