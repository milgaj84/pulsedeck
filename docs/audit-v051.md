# PulseDeck v0.5.1 — Code Quality Audit Action Plan

## Summary

This document presents the consolidated findings from a comprehensive code quality audit of the PulseDeck codebase (Rust TUI internet radio player). The audit covered dead code, DRY violations, KISS complexity, SOLID principles, modularity/separation of concerns, and long-term maintainability risks.

### Counts by Category

| Category | Count |
|----------|-------|
| Dead code / clippy warnings | 22 |
| Unused dependencies | 1 |
| Coupling hotspots | 1 |
| Large types (>10 fields/variants) | 5 |
| DRY violation patterns | 21 |
| KISS violations (long functions) | 22 functions, 16 deeply nested |
| KISS violations (unnecessary abstractions) | 8 |
| SRP violations | 8 |
| OCP violations | 2 (overlay, visualizer) |
| Boundary violations | 3 |
| Test coverage gaps | 1 critical |
| Encapsulation risks (pub fields, no constructor) | 9 structs |
| Error handling risks | 25 |
| Concurrency risks | 2 |

### Counts by Priority

| Priority | Distinct Findings |
|----------|-------------------|
| High | 7 |
| Medium | 12 |
| Low | 8 |
| **Total** | **27** |

### Requirement Area Coverage

| Area | Covered |
|------|---------|
| 1. Dead code / unused code | ✓ |
| 2. DRY violations | ✓ |
| 3. KISS violations | ✓ |
| 4. SOLID — Single Responsibility | ✓ |
| 5. SOLID — OCP, ISP, DIP | ✓ |
| 6. Modularity / Separation of Concerns | ✓ |
| 7. Maintainability risks | ✓ |

---

## High Priority

Findings that block future features, cause correctness risks, or create latent bugs. Ordered by number of modules affected, frequency, and risk of future bugs.

---

### H-1: Test Gap in Central Action Dispatcher (`update.rs`)

**ID:** TEST-001  
**Category:** Test coverage gap  
**Requirement:** Req 7, AC 7.3  
**Modules affected:** `src/app/update.rs` (central dispatcher for all 46 action variants)

**Description:** The `App::update()` function is the central state machine dispatcher (46-arm match) and `App::tick()` orchestrates all periodic updates (notice decay, audio polling, visualizer, reconnect, sleep timer, persistence). Neither has direct test coverage. If a match arm is accidentally removed or reordered, no test catches it.

**Dependencies:** None (can be addressed independently)

**Refactoring Plan:**

1. **Preconditions:** All existing tests pass. Understand the 46 action variants in `Action` enum.
2. **Step 1:** Create `src/app/update.rs` test module with `#[cfg(test)] mod tests`.
3. **Step 2:** Write one integration test per action category verifying the expected handler is reached (assert state changes for navigation, playback, volume, search, command palette, library, UI/layout, sleep timer, export, lifecycle actions).
4. **Step 3:** Write a test for `tick()` verifying it calls subsystems (notice decay, audio status poll, visualizer update, reconnect drive, sleep timer check, persistence flush).
5. **Step 4:** Add a compile-time exhaustiveness check — if using a helper that maps `Action` variants, ensure new variants cause a compile error if unhandled.
6. **Verification:** Run `cargo test`. All new tests pass. No modification to production code required.

---

### H-2: Concurrency Risk — `Arc<Mutex>` Exposed Through Public Field Chain

**ID:** CONC-001  
**Category:** Concurrency risk, encapsulation risk  
**Requirement:** Req 7, AC 7.4, 7.6  
**Modules affected:** `src/app/playback_runtime.rs`, `src/ui/model.rs`, `src/ui/deck/visualizer/mod.rs`

**Description:** The `sample_buffer: Arc<Mutex<VecDeque<f32>>>` is publicly accessible through `app.playback.sample_buffer` and exposed to renderers via `UiModel.sample_buffer`. The visualizer renderer calls blocking `.lock()` during frame rendering (line 61 of `src/ui/deck/visualizer/mod.rs`), which could stall the UI thread if the audio thread holds the lock during a batch write.

**Dependencies:** None (can be addressed independently)

**Refactoring Plan:**

1. **Preconditions:** Audio visualizer renders correctly. Tests pass.
2. **Step 1:** In `UiModel::from(app)` (in `src/ui/model.rs`), lock the buffer once and copy the most recent N samples into a `Vec<f32>` field (e.g., `sample_snapshot: Vec<f32>`). Remove the `&Arc<Mutex<_>>` field from `UiModel`.
3. **Step 2:** Update `src/ui/deck/visualizer/mod.rs` to read from `app.sample_snapshot` instead of calling `.lock()`.
4. **Step 3:** Change `PlaybackRuntime.sample_buffer` visibility from `pub` to `pub(super)` and add an accessor `pub(super) fn sample_buffer(&self) -> &Arc<Mutex<VecDeque<f32>>>`.
5. **Step 4:** Verify the audio-side `try_lock()` pattern in `src/audio/visualizer.rs` remains unchanged.
6. **Verification:** Run `cargo test`. Verify visualizer renders. No mutex access in rendering code path.

---

### H-3: Discarded Failure Events in Audio Decode Thread

**ID:** ERR-001  
**Category:** Error handling risk  
**Requirement:** Req 7, AC 7.5  
**Modules affected:** `src/audio/decode.rs` (6 locations), `src/audio/engine_loop_v2.rs` (1 location)

**Description:** The audio decode thread uses fire-and-forget channel sends (`let _ = event_tx.send(...)`) for critical failure events. Six locations discard `EngineEvent::Failed` or `EngineEvent::Connected` results. If the channel closes during shutdown races, the UI never learns about connection failures, prebuffer timeouts, or decode errors — playback appears stuck with no error indication.

**Affected lines in `src/audio/decode.rs`:** 189, 205, 225, 281, 342, 349  
**Affected line in `src/audio/engine_loop_v2.rs`:** 314

**Dependencies:** None (can be addressed independently)

**Refactoring Plan:**

1. **Preconditions:** Understand the fire-and-forget pattern is intentional (audio thread cannot block). Tests pass.
2. **Step 1:** Add a `log::debug!("Channel closed, could not deliver event: {:?}", event)` or `tracing::debug!` for each failed send of a `Failed` or `Connected` event.
3. **Step 2:** For the `status_tx.send(status)` in `engine_loop_v2.rs:314`, log at `warn!` level since all audio status updates flow through this path.
4. **Step 3:** Optionally, set an atomic flag (`is_channel_alive`) that the engine checks before spawning new decode workers, avoiding wasted work after receiver drops.
5. **Verification:** Run `cargo test`. Verify audio playback still works. Check that log output appears during `ctrl+c` shutdown.

---

### H-4: OCP Violation — Visualizer Numeric Dispatch

**ID:** OCP-002  
**Category:** OCP violation  
**Requirement:** Req 5, AC 5.3, 5.7  
**Modules affected:** `src/app/overlays.rs`, `src/app/ui_state.rs`, `src/ui/deck/visualizer/mod.rs`, `src/ui/controls.rs`

**Description:** Visualizer mode is represented as `usize` with hardcoded `% 3` modular arithmetic and integer dispatch (`match app.visualizer_mode { 0 => ..., 1 => ..., _ => ... }`). Adding a new visualizer mode requires updating 4 existing files. The `_ =>` wildcard catch-all means the compiler cannot warn about missing branches for new modes.

**Dependencies:** None (can be addressed independently)

**Refactoring Plan:**

1. **Preconditions:** Three visualizer modes exist (RTA spectrum, real oscilloscope, simulated oscilloscope). Tests pass.
2. **Step 1:** Define a `VisualizerMode` enum in `src/app/types.rs`:
   ```rust
   #[derive(Debug, Clone, Copy, PartialEq, Eq)]
   pub enum VisualizerMode { RtaSpectrum, RealOscilloscope, SimOscilloscope }
   ```
3. **Step 2:** Add `impl VisualizerMode { pub fn next(self) -> Self { ... } pub fn label(self) -> &'static str { ... } }`
4. **Step 3:** Replace `visualizer_mode: usize` with `visualizer_mode: VisualizerMode` in `UiRuntimeState` and `UiModel`.
5. **Step 4:** Replace `% 3` in `src/app/overlays.rs` with `.next()`. Remove `VISUALIZER_MODE_COUNT`.
6. **Step 5:** Replace all integer match arms in `src/ui/deck/visualizer/mod.rs` and `src/ui/controls.rs` with exhaustive enum matches.
7. **Verification:** Run `cargo test`. Verify all three modes cycle correctly. Confirm no `_ =>` wildcards remain for visualizer dispatch.

---

### H-5: DRY — Duplicated Genre Filtering Logic Across Boundaries

**ID:** DRY-UI-01  
**Category:** DRY violation, separation concern  
**Requirement:** Req 2, AC 2.1, 2.3; Req 6, AC 6.1  
**Modules affected:** `src/app/selectors.rs`, `src/ui/model.rs`

**Description:** Genre-based station filtering is implemented identically in three places: `App::visible_stations()`, `App::visible_count()` (both in `src/app/selectors.rs`), and `visible_stations_for()` in `src/ui/model.rs`. The logic (search mode short-circuit → genre index lookup → "All" check → case-insensitive genre filter) is byte-for-byte identical in control flow. This creates a cross-boundary DRY violation where app-state filtering logic is duplicated in the UI layer.

**Dependencies:** Must be resolved before H-2 (UiModel refactoring benefits from having a single filtering source of truth)

**Refactoring Plan:**

1. **Preconditions:** Tests pass. `visible_stations()` in `src/app/selectors.rs` is already the canonical implementation.
2. **Step 1:** In `src/ui/model.rs`, replace `visible_stations_for(app)` call with `app.visible_stations()`.
3. **Step 2:** Remove the `visible_stations_for()` function from `src/ui/model.rs`.
4. **Step 3:** Optionally, derive `visible_count()` from `visible_stations().len()` if the zero-alloc optimization is not performance-critical (it iterates the same collection).
5. **Verification:** Run `cargo test`. Verify genre filtering still works in the UI. No public API changes.

---

### H-6: SRP — `PlaybackRuntime` Conflates Audio, App-Features, and Infrastructure

**ID:** SRP-004  
**Category:** SRP violation  
**Requirement:** Req 4, AC 4.1, 4.3, 4.4  
**Modules affected:** `src/app/playback_runtime.rs`, `src/app/playback.rs`, `src/app/lifecycle.rs`, `src/app/reconnect.rs`, `src/app/sleep_timer.rs`

**Description:** `PlaybackRuntime` bundles three unrelated responsibilities: audio engine control (`audio`, `volume`, `muted`, `sample_buffer`), app-level features (`sleep_timer`), and infrastructure resilience (`reconnect`, `diagnostics`). Adding a new timer-like feature (alarm, scheduled recording) would require modifying this audio-focused struct.

**Dependencies:** Depends on H-5 (filtering consolidation) being resolved first so UiModel construction is stable before restructuring the playback subsystem.

**Refactoring Plan:**

1. **Preconditions:** Tests pass. Understand all access patterns to `PlaybackRuntime` fields across `src/app/`.
2. **Step 1:** Move `sleep_timer: SleepTimer` from `PlaybackRuntime` to `App` directly (it's an app-level feature, not an audio concern).
3. **Step 2:** Update all `self.playback.sleep_timer` references to `self.sleep_timer` across `src/app/sleep_timer.rs`, `src/app/update.rs`, `src/app/lifecycle.rs`.
4. **Step 3:** Optionally extract `reconnect` and `diagnostics` into a `PlaybackResilience` sub-struct for clearer separation.
5. **Step 4:** Update `UiModel::from(app)` to reference the new locations.
6. **Verification:** Run `cargo test`. All tests pass without modification. Verify sleep timer still functions.

---

### H-7: OCP Violation — Overlay Extension Requires 5+ File Modifications

**ID:** OCP-001  
**Category:** OCP violation  
**Requirement:** Req 5, AC 5.1  
**Modules affected:** `src/app/overlays.rs`, `src/ui/mod.rs`, `src/action.rs`, `src/event.rs`, `src/app/update.rs`

**Description:** Adding a new overlay type requires modifying 5 existing files beyond the enum definition and the new renderer module. The overlay dispatch in `src/ui/mod.rs` (exhaustive match on `ActiveOverlay`), centralized `Action` enum, event key-mapping, and update dispatcher all require manual wiring.

**Dependencies:** Depends on H-4 (visualizer enum) establishing the pattern for enum-based dispatch improvements.

**Refactoring Plan:**

1. **Preconditions:** Tests pass. 6 overlay types currently exist.
2. **Step 1:** Document the current overlay addition checklist (which files to modify) in a developer guide.
3. **Step 2:** Consolidate the overlay toggle pattern — instead of one `Action::Toggle<X>` per overlay plus one `toggle_<x>()` method each, consider a single `Action::ToggleOverlay(ActiveOverlay)` variant that takes the target overlay as data.
4. **Step 3:** Unify the `toggle_<x>` methods into a single `toggle_overlay(target: ActiveOverlay)` that handles mutual exclusion generically.
5. **Step 4:** Reduce the `draw_model()` overlay match to a trait-dispatch or map-lookup pattern.
6. **Verification:** Run `cargo test`. Verify all existing overlays still toggle correctly. Count files that would need modification for a hypothetical new overlay — target is ≤2 beyond enum + renderer.

---

## Medium Priority

Findings that increase maintenance cost, create architectural friction, or represent boundary violations that compound over time. Ordered by number of modules affected and pattern frequency.

---

### M-1: DRY — Playback State Reset Patterns (5 occurrences)

**ID:** DRY-PB-01  
**Category:** DRY violation  
**Requirement:** Req 2, AC 2.1, 2.2  
**Modules affected:** `src/app/playback.rs`, `src/app/lifecycle.rs`

**Description:** The buffer/track reset triple (`current_track = None; buffer_percent = 0; buffer_seconds = 0`) appears in 5 locations across playback and lifecycle modules. Related patterns include the diagnostics buffer reset (2 occurrences), play-and-sync-volume sequence (4 occurrences), transition-to-Connecting (4 occurrences), and reconnect-disarm + state reset (4 occurrences). Total: 8 distinct repeated patterns with 24+ occurrence sites in the playback subsystem.

**Proposed fix:** Extract `PlaybackView::reset_transient_status()`, `PlaybackDiagnostics::reset_buffer_state()`, and `App::start_stream(url)` helper methods. See detailed proposals in finding DRY-PB-01 through DRY-PB-08.

---

### M-2: DRY — Overlay Render Boilerplate (7 renderers)

**ID:** DRY-UI-02  
**Category:** DRY violation  
**Requirement:** Req 2, AC 2.1, 2.3  
**Modules affected:** `src/ui/help.rs`, `src/ui/station_details.rs`, `src/ui/recent_tracks.rs`, `src/ui/sleep_timer.rs`, `src/ui/settings.rs`, `src/ui/playback_doctor.rs`, `src/ui/command_palette.rs`

**Description:** All 7 overlay renderers repeat a 7-step initialization sequence: compute popup_area → compact check → Clear widget → boundary warning → Block construction → alert area split → render block + content + alert banner. The only variation is title, dimensions, and minimum size.

**Proposed fix:** Extract `prepare_overlay(frame, area, config, playback_state) -> Option<(Rect, Option<Rect>)>` in `src/ui/mod.rs` with an `OverlayConfig` struct parameterizing the variation points.

---

### M-3: SRP — Functions Mix State Transitions with Notice Formatting

**ID:** SRP-006  
**Category:** SRP violation  
**Requirement:** Req 4, AC 4.2, 4.3  
**Modules affected:** `src/app/playback.rs`, `src/app/lifecycle.rs`, `src/app/reconnect.rs`, `src/app/sleep_timer.rs`

**Description:** Multiple functions perform both playback state machine transitions (updating `PlaybackView` fields, sending `AudioCommand`s) and user-facing notice formatting. These are distinct responsibilities — state transition logic should be pure/deterministic, while notice presentation is a formatting concern.

**Proposed fix:** Have state transition functions return a `TransitionOutcome` enum with a `NoticeKind` variant. A single `App::apply_notice()` method maps `NoticeKind` to formatted strings, centralizing all notice text.

---

### M-4: SRP — `UiRuntimeState.visualizer_peaks` Crosses Audio→UI Boundary

**ID:** SRP-002  
**Category:** SRP violation, boundary violation  
**Requirement:** Req 4, AC 4.1, 4.4; Req 6, AC 6.3  
**Modules affected:** `src/app/ui_runtime.rs`, `src/app/visualizer/mod.rs`, `src/ui/model.rs`

**Description:** `visualizer_peaks: Vec<f32>` is raw audio FFT spectral data produced by the audio engine's pipeline, stored in a struct named "UiRuntimeState". This crosses the audio-engine → ui-rendering boundary. The data should flow through a dedicated visualizer state struct or be computed at render time.

**Proposed fix:** Extract to `VisualizerState { mode: VisualizerMode, peaks: Vec<f32> }` in `src/app/visualizer/state.rs`. Move ownership to `App` or `PlaybackRuntime`.

---

### M-5: Separation — UI Renderers Import Directly from Audio Engine

**ID:** BOUND-001  
**Category:** Boundary violation  
**Requirement:** Req 6, AC 6.1, 6.5  
**Modules affected:** `src/ui/stations.rs`, `src/ui/station_details.rs`, `src/ui/settings.rs`

**Description:** Three UI rendering modules import `crate::audio::PlaybackCapability`, `crate::audio::codec_capability`, and `crate::audio::output_device_display_name` directly. Per adjacency rules, `ui-rendering` should only access audio data via pre-computed fields in `UiModel`, not call into the audio-engine boundary.

**Proposed fix:** Pre-compute codec capability display strings and output device labels at UiModel construction time. Add fields like `codec_display: String` to the station display data.

---

### M-6: Separation — Codec Capability and Health Logic in UI Renderers

**ID:** BOUND-002  
**Category:** Boundary violation, business logic in UI  
**Requirement:** Req 6, AC 6.1, 6.3  
**Modules affected:** `src/ui/stations.rs` (codec_chip, station_health_chip, station_health_failure_is_current), `src/ui/station_details.rs` (codec_detail, failure_is_after_success)

**Description:** UI renderers perform domain-level decisions: codec capability lookup (calling `crate::audio::codec_capability()` to determine supported/unknown/unsupported status) and temporal health logic (parsing Unix timestamps to determine failure recency). These are business rules evaluated at render time rather than pre-computed in the domain layer.

**Proposed fix:** Pre-compute `codec_display_label` and `health_status` (enum: Healthy/Warning/Unknown) at the domain or app layer. Surface these as display-ready values through UiModel.

---

### M-7: DRY — Persistence Load/Save Scaffold (6 methods, 2 types)

**ID:** DRY-PERSIST-01  
**Category:** DRY violation  
**Requirement:** Req 2, AC 2.1, 2.2  
**Modules affected:** `src/history.rs`, `src/app/ui_state.rs`, `src/config.rs`

**Description:** `History` and `UiState` repeat identical four-method scaffolds using `#[cfg(not(test))]`/`#[cfg(test)]` pairs for `load()`, `load_with_warning()`, and `save()`. Each non-test path calls `load_json_with_warning::<Self>(FILE)` → `.sanitized()` → returns `(Self, Option<String>)`. The test path returns `(Self::default(), None)`.

**Proposed fix:** Define a `Persistable` trait in `src/config.rs` with associated const `FILE` and method `sanitized()`. Provide blanket `load/save` implementations that eliminate the `#[cfg]` duplication.

---

### M-8: Encapsulation — `PlaybackView` Fields Publicly Mutable

**ID:** ENCAP-001  
**Category:** Encapsulation risk  
**Requirement:** Req 7, AC 7.4  
**Modules affected:** `src/app/playback.rs`, `src/app/lifecycle.rs`, `src/app/reconnect.rs`

**Description:** `PlaybackView` exposes all 6 fields as `pub` with no public constructor beyond `Default`. Any module with `&mut App` can directly mutate playback state, bypassing controlled transition logic. This scatters state reset patterns (the DRY-PB-01 through DRY-PB-08 findings are a direct consequence of unrestricted field access).

**Proposed fix:** Reduce field visibility to `pub(super)`. Add transition methods (`reset_transient_status()`, `enter_connecting()`, `enter_error()`, `enter_playing()`) that enforce valid state changes.

---

### M-9: KISS — `run_worker()` 159 Logic Lines, Depth 6

**ID:** KISS-001  
**Category:** KISS violation (long function, deep nesting)  
**Requirement:** Req 3, AC 3.1, 3.2  
**Modules affected:** `src/audio/decode.rs`

**Description:** The `run_worker()` function spans 159 logic lines with nesting depth 6. It handles HTTP connection, prebuffer fill, codec detection, and decoder construction in a single flat imperative flow. This is the longest function in the codebase and the most complex in the audio engine.

**Proposed fix:** Decompose into `connect_stream()` (~25 lines), `parse_stream_headers()` (~15 lines), `fill_prebuffer()` (~25 lines), `detect_codec_and_build()` (~15 lines). Each returns `Result` with early-return error paths.

---

### M-10: KISS — `render_visualizer_signal()` 92 Lines, Depth 7

**ID:** KISS-002  
**Category:** KISS violation (deep nesting)  
**Requirement:** Req 3, AC 3.1, 3.2  
**Modules affected:** `src/ui/deck/visualizer/mod.rs`

**Description:** The visualizer renderer has the deepest nesting in the codebase (7 levels) from nested `match state → match mode → if let Ok(buf) → if n >= pixel_width`. This makes it difficult to add new rendering branches.

**Proposed fix:** Extract per-state rendering into dedicated functions: `render_oscilloscope_live()`, `render_oscilloscope_sim()`, `render_connecting_wave()`, `render_paused_ripple()`. Reduce main function nesting to 2.

---

### M-11: SRP — `nav`/`should_quit` (App-State) in `UiRuntimeState`

**ID:** SRP-003  
**Category:** SRP violation  
**Requirement:** Req 4, AC 4.1, 4.4  
**Modules affected:** `src/app/ui_runtime.rs`, ~15 access sites across `src/app/`

**Description:** `Navigation` and `should_quit` are application-level state (they determine business logic flow — which station is selected, whether the app exits) but live in a struct named "UiRuntimeState". Navigation selection drives which station gets played; `should_quit` controls the event loop. This makes the struct's name misleading and conflates responsibilities.

**Proposed fix:** Move `nav` and `should_quit` to `App` directly. `UiRuntimeState` would then contain only fields affecting rendering output.

---

### M-12: UiModel Interior Mutability — `Arc<Mutex>` Exposure

**ID:** BOUND-003  
**Category:** Separation concern, concurrency risk  
**Requirement:** Req 6, AC 6.3; Req 7, AC 7.6  
**Modules affected:** `src/ui/model.rs`, `src/ui/deck/visualizer/mod.rs`

**Description:** `UiModel` exposes `&Arc<Mutex<VecDeque<f32>>>` allowing renderers to call `.lock()`. While current code only reads, the type system doesn't enforce read-only access. A future change could accidentally mutate the buffer during rendering. This is the same underlying issue as H-2 viewed from the separation-of-concerns angle.

**Proposed fix:** Addressed by H-2 refactoring (pre-compute sample snapshot in `UiModel::from()`).

---

## Low Priority

Cosmetic improvements, minor organizational issues, and findings that don't block features or create correctness risks.

---

### L-1: Dead Code — 22 Clippy/Dead-Code Warnings

**ID:** DEAD-001  
**Category:** Dead code  
**Requirement:** Req 1, AC 1.1, 1.2  
**Modules affected:** Various (22 warnings across codebase)

**Description:** `cargo clippy` with `-W dead_code -W unused_variables -W unused_imports` reports 22 warnings. These include unused functions, unused parameters, and unused imports in non-test modules.

**Proposed fix:** Address each warning individually — remove unused items, prefix unused parameters with `_`, or add `#[cfg(test)]` annotations where appropriate.

---

### L-2: Unused Dependency — `tiny_http` as dev-dependency

**ID:** DEAD-002  
**Category:** Unused dependency  
**Requirement:** Req 1, AC 1.3  
**Modules affected:** `Cargo.toml`

**Description:** `tiny_http` is listed as a dev-dependency but is not referenced in any test code.

**Proposed fix:** Remove `tiny_http` from `[dev-dependencies]` in `Cargo.toml`.

---

### L-3: KISS — Unnecessary Single-Use Abstractions (5 functions)

**ID:** KISS-003  
**Category:** KISS violation (unnecessary abstraction)  
**Requirement:** Req 3, AC 3.3, 3.4  
**Modules affected:** `src/radio/station.rs` (`normalize_country_code`, `normalize_station_uuid`), `src/radio/map.rs` (`non_empty`, `fallback_trimmed`), `src/app/settings.rs` (`step_choice<T>`)

**Description:** Five functions are either trivial single-use conversions (trim + empty-check) or generics with a single concrete instantiation. They add indirection without providing validation or invariant enforcement.

**Proposed fix:** Inline `normalize_country_code`, `normalize_station_uuid`, `non_empty`, and `fallback_trimmed` at their call sites. Monomorphize `step_choice<T>` to `step_choice(choices: &[ThemeName], ...)`.

---

### L-4: DRY — Codec Display Formatting Across 3 Locations

**ID:** DRY-UI-05  
**Category:** DRY violation  
**Requirement:** Req 2, AC 2.1, 2.3  
**Modules affected:** `src/app/playback.rs` (`display_station_codec`), `src/ui/stations.rs` (`codec_chip`), `src/ui/station_details.rs` (`codec_detail`)

**Description:** Station codec formatting (trim → empty check → uppercase → optional capability suffix) exists in three locations. All share the normalize step; two also call `codec_capability()`.

**Proposed fix:** Consolidate into a `format_codec_display(codec, style: CodecDisplayStyle)` function in `src/audio/capability.rs` or a shared display module.

---

### L-5: DRY — Notice Construction Patterns (4+ sites)

**ID:** DRY-NOTICE-01  
**Category:** DRY violation  
**Requirement:** Req 2, AC 2.1, 2.2  
**Modules affected:** `src/app/search.rs`, `src/app/library.rs`, `src/app/playback.rs`, `src/app/settings.rs`, `src/app/sleep_timer.rs`

**Description:** Multiple patterns of repeated notice construction: "Could not {action}: {err}" (4 sites), sleep timer announcements (5 notices in 3 locations), settings change confirmations (3 occurrences).

**Proposed fix:** Extract `notify_operation_error(action, err)` helper; create `SleepTimerNotice` enum; extract `confirm_setting_change(label, value)`.

---

### L-6: Large Type — `Action` Enum with 46 Variants

**ID:** TYPE-001  
**Category:** Large type  
**Requirement:** Req 7, AC 7.2  
**Modules affected:** `src/action.rs`, `src/app/update.rs`, `src/event.rs`

**Description:** The `Action` enum has 46 variants, making it the largest type in the codebase. While functional for a TUI message-loop architecture, each new feature adds a variant here and a match arm in `update()`.

**Proposed fix:** Consider grouping into nested enums by domain (e.g., `Action::Search(SearchAction)`, `Action::Palette(PaletteAction)`) to reduce the flat dispatch surface. Low urgency — the current flat enum is idiomatic for this architecture size.

---

### L-7: Boundary — `audio/types.rs` Imports `crate::app` in Tests

**ID:** BOUND-004  
**Category:** Boundary violation (test-only)  
**Requirement:** Req 6, AC 6.4, 6.7  
**Modules affected:** `src/audio/types.rs` (lines 292, 444)

**Description:** The audio module's test code imports `classify_playback_error` and `PlaybackErrorKind` from the app-state boundary. This is an architectural smell — the lower-level module's test correctness depends on higher-level interpretation logic. Production binary is unaffected.

**Proposed fix:** Move `classify_playback_error` to the audio boundary (since it classifies audio errors), or duplicate the relevant assertions in audio tests without importing from app.

---

### L-8: Latent Encapsulation — `pub(crate) mod types/volume` in Audio

**ID:** ENCAP-002  
**Category:** Encapsulation risk (latent)  
**Requirement:** Req 5, AC 5.6; Req 7, AC 7.4  
**Modules affected:** `src/audio.rs`

**Description:** `pub(crate) mod types` and `pub(crate) mod volume` make these module paths visible crate-wide, even though all types inside are `pub(super)`. If a future change promotes an internal type to `pub(crate)` visibility, it would bypass the re-export boundary without compiler guard.

**Proposed fix:** Tighten to `pub(super) mod types` and `pub(super) mod volume`. No current consumers exist outside the audio module.

---

## High-Priority Dependency Graph

The following execution order ensures no circular dependencies among High-priority items:

```
H-1 (test gap)           ─── independent, can start immediately
H-2 (concurrency/mutex)  ─── independent, can start immediately
H-3 (error handling)     ─── independent, can start immediately
H-4 (visualizer enum)    ─── independent, can start immediately
H-5 (genre filtering)    ─── independent, can start immediately
          │
          ▼
H-6 (PlaybackRuntime SRP) ─── depends on H-5 (filtering stable before restructuring)
          │
          ▼
H-7 (overlay OCP)        ─── depends on H-4 (enum dispatch pattern established first)
```

**Recommended execution order:**
1. H-1, H-2, H-3, H-4, H-5 (all independent — can be parallelized)
2. H-6 (after H-5)
3. H-7 (after H-4)

---

## Traceability Matrix

| Finding | Requirement | Acceptance Criteria |
|---------|-------------|---------------------|
| H-1 | Req 7 | 7.3 (test coverage gaps in critical modules) |
| H-2 | Req 7 | 7.4 (pub fields expose internals), 7.6 (concurrency risks) |
| H-3 | Req 7 | 7.5 (discarded Result/Option values) |
| H-4 | Req 5 | 5.3 (visualizer OCP), 5.7 (numeric dispatch) |
| H-5 | Req 2, 6 | 2.1 (DRY ≥4 statements), 2.3 (cross-boundary), 6.1 (UI filtering) |
| H-6 | Req 4 | 4.1 (struct mixing), 4.3 (decomposition), 4.4 (PlaybackRuntime) |
| H-7 | Req 5 | 5.1 (overlay extension >2 files) |
| M-1 | Req 2 | 2.1, 2.2 (repeated playback reset patterns) |
| M-2 | Req 2 | 2.1, 2.3 (overlay boilerplate cross-UI) |
| M-3 | Req 4 | 4.2, 4.3 (functions span state + notice) |
| M-4 | Req 4, 6 | 4.1, 4.4 (audio data in UI struct), 6.3 |
| M-5 | Req 6 | 6.1, 6.5 (UI→audio adjacency violation) |
| M-6 | Req 6 | 6.1, 6.3 (business logic in renderers) |
| M-7 | Req 2 | 2.1, 2.2 (persistence scaffold DRY) |
| M-8 | Req 7 | 7.4 (pub fields without constructor) |
| M-9 | Req 3 | 3.1, 3.2 (159 lines, depth 6) |
| M-10 | Req 3 | 3.1, 3.2 (92 lines, depth 7) |
| M-11 | Req 4 | 4.1, 4.4 (nav in UiRuntimeState) |
| M-12 | Req 6, 7 | 6.3 (interior mutability), 7.6 |
| L-1 | Req 1 | 1.1, 1.2 (dead code warnings) |
| L-2 | Req 1 | 1.3 (unused dependency) |
| L-3 | Req 3 | 3.3, 3.4 (unnecessary abstractions) |
| L-4 | Req 2 | 2.1, 2.3 (codec formatting DRY) |
| L-5 | Req 2 | 2.1, 2.2 (notice patterns DRY) |
| L-6 | Req 7 | 7.2 (large types) |
| L-7 | Req 6 | 6.4, 6.7 (audio test imports app) |
| L-8 | Req 5, 7 | 5.6, 7.4 (latent pub(crate) risk) |

---

## Methodology Notes

- **Deduplication:** Overlapping findings from multiple passes were merged. For example, the genre filtering duplication appears in both DRY (task 4.2) and separation boundary (task 7.1) findings — consolidated as H-5. The mutex exposure appears in both concurrency (task 9.2) and boundary (task 7.1) findings — consolidated as H-2.
- **Priority classification follows the design document:** High = blocks features, causes bugs, correctness risks. Medium = maintenance cost. Low = cosmetic.
- **Test gaps and concurrency risks on critical paths are High** (per Property 3).
- **Coupling hotspots and encapsulation risks are Medium or higher** (per Property 3).
- **Minimum coverage:** 27 distinct findings across all 7 requirement areas (exceeds minimum of 10 per Req 8 AC 2).
- **Non-destructiveness:** No source files were modified during this audit. `cargo build --release` and `cargo test` pass unchanged.
