# Changelog

All notable changes to the PulseDeck project will be documented in this file.

---

## [Unreleased]

### Added
*   **Search Audition Shortcut**: Added `Space` / `Ctrl+Enter` in search mode to stream the highlighted global result without saving it to the Library.

### Improved
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
