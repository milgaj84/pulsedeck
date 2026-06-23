# Implementation Plan: Audio Engine Rewrite (v0.5.0)

## Overview

Replace the internals of `src/audio/` with a layered, state-machine-driven pipeline while keeping
the public `AudioEngine` contract (`spawn`, `send`, `status_rx`, `AudioCommand`, `AudioStatus`,
the visualizer sample buffer) byte-for-byte compatible so no app-layer call site needs to change.
The implementation proceeds bottom-up: data types → state machine → output manager →
stream/decode pipeline → capability update → wiring → integration tests.

## Tasks

- [x] 1. Add dev-dependencies and define core internal types
  - Add `proptest` to `[dev-dependencies]` in `Cargo.toml`
  - Add `tiny_http` (or equivalent lightweight HTTP server crate) to `[dev-dependencies]`
  - Create `src/audio/types.rs` defining:
    - `Generation` type alias (`u64`)
    - `EngineState` enum with all seven variants (`Idle`, `Connecting`, `Buffering`, `Playing`, `Paused`, `Recovering`, `Failed`)
    - `EngineEvent` enum with all six variants (`Buffering`, `Connected`, `TrackChanged`, `StreamEnded`, `OutputLost`, `Failed`)
    - `EngineError` enum (`Connect`, `Http`, `Decode`, `Output`, `Abandoned`) with `to_status_string()` and `is_recoverable_output()`
    - `EndReason` enum (`Network`, `Eof`, `Decode`, `Abandoned`)
    - `PrebufferConfig` struct with `min_bytes`, `max_bytes`, `fill_timeout`
    - `PlaybackOptions` struct with `metadata_enabled`, `target_volume`, `preferred_device`
    - `StreamFormat` struct or type alias
    - `DecodedSource` type alias for the boxed rodio `Source`
    - `ConnectRequest` struct
  - Add `AudioStatus::Buffering { percent: u8 }` variant to the public `AudioStatus` enum in `src/audio.rs`
  - Update `src/audio.rs` module declarations to expose the new internal modules
  - _Requirements: 1.1, 1.5, 1.6, 2.1, 3.2_

- [x] 2. Implement `EngineError` to status string mapping and error classification
  - Implement `EngineError::to_status_string()` using the stable prefixes:
    - `Connect` → `"Connection failed: ..."` (containing `"Connection failed:"`)
    - `Http(code)` → `"HTTP {code}"`
    - `Decode` → `"Decode error: ..."`
    - `Output` → `"Hardware output error: ..."`
    - `Abandoned` → not user-visible (internal only)
  - Write unit tests asserting that `classify_playback_error(err.to_status_string())` returns the correct `PlaybackErrorKind` for each `EngineError` variant
  - Ensure the `HARDWARE_OUTPUT_ERROR_PREFIX` constant in `src/audio.rs` is reused
  - _Requirements: 12.1, 12.2, 12.3, 12.4, 12.5, 12.7_

  - [ ]* 2.1 Write property test for error string classifiability
    - **Property 10: Status classifiability**
    - For any `EngineError` variant (excluding `Abandoned`), `classify_playback_error(err.to_status_string())` returns a non-`Unknown` kind
    - **Validates: Requirements 12.1, 12.2, 12.3, 12.4, 12.5, 12.7**

- [x] 3. Implement `VolumeRamp`
  - Create `src/audio/volume.rs`
  - Implement `VolumeRamp` struct with `begin_fade_in()`, `begin_fade_out()`, `retarget(target)`, `tick(sink)` methods
  - `tick` applies one exponential fade step toward `target_volume` using the existing 0.15 step factor
  - `SetVolume` input clamped to `[0.0, 1.0]` using `.clamp(0.0, 1.0)` before storing (guards against NaN/inf via `f32::clamp` which returns the bounds on NaN)
  - Write unit tests porting the existing `fade_out_next_volume`, `fade_out_complete`, and `clamp_status_volume` tests
  - _Requirements: 10.1, 10.2, 10.3, 10.5, 10.6_

  - [ ]* 3.1 Write property test for volume clamp
    - **Property 8: Volume clamp**
    - For any `f32` value `v` (generated including `f32::NAN`, `f32::INFINITY`, `f32::NEG_INFINITY`, and random normal values), the clamped result ∈ `[0.0, 1.0]`
    - **Validates: Requirements 10.6**

- [x] 4. Implement `OutputManager`
  - Create `src/audio/output_manager.rs`
  - Implement `OutputManager` struct with `stream`, `handle`, `sink`, `preferred_device`, `recovery_retries`
  - Implement all interface methods: `ensure_open`, `set_preferred_device`, `attach`, `set_volume`, `pause`, `resume`, `stop`, `is_sink_drained`, `reopen`
  - `is_sink_drained` returns `true` only when a sink exists and is empty — device loss arrives separately as `EngineEvent::OutputLost` (not via `sink.empty()`)
  - Lazy device open: device is not opened until `ensure_open` is first called
  - Preserve the existing native-stderr suppression for ALSA/JACK probe chatter (port from `src/audio/output.rs`)
  - All rodio/cpal call sites return `Result<_, EngineError::Output>` — no unwrap/panic
  - `reopen` resets `recovery_retries` on success, increments on failure
  - Write unit tests for: device-name normalization, reopen bookkeeping, drained-vs-lost distinction, `recovery_retries` counting
  - _Requirements: 9.1, 9.2, 9.3, 9.6, 9.7, 9.8_

- [x] 5. Implement `ConnectionSupervisor`
  - Create `src/audio/supervisor.rs`
  - Implement `ConnectionSupervisor` struct with `active_generation: Arc<AtomicU64>`, `current: Generation`, `worker: Option<JoinHandle<()>>`
  - Implement `next_generation()`: atomically bump and store the active generation
  - Implement `spawn(req, event_tx)`: spawns the worker thread (the worker body is implemented in task 7)
  - Implement `abandon()`: stores 0 to `active_generation`, drops the handle without joining
  - Implement `is_active(gen)`: pure read — `gen == active_generation.load(SeqCst)`
  - Write unit tests for: `next_generation` monotonicity, `is_active` correctness, `abandon` sets generation to 0
  - _Requirements: 4.1, 4.2, 4.3, 4.4, 4.5_

  - [ ]* 5.1 Write property test for generation strict monotonicity
    - **Property 3: Generation strict monotonicity**
    - For any sequence of `next_generation()` calls, each result is strictly greater than the previous
    - **Validates: Requirements 4.1**

- [x] 6. Implement `StreamSource` with ICY demux
  - Create `src/audio/stream_source.rs` as the hardened successor to `src/audio/stream_reader.rs`
  - Implement `StreamSource<R: Read>` struct with `inner`, `icy: Option<IcyDemux>`, `generation`, `active_generation`, `event_tx`
  - Implement `Read` for `StreamSource`:
    - Strip ICY metadata on `metaint` boundary before writing audio bytes to buf
    - Parse `StreamTitle` from metadata blocks and `send` `EngineEvent::TrackChanged`
    - If stream ends inside a metadata block, return an error — never partial audio/metadata
    - If `generation != active_generation.load(SeqCst)`, return `io::Error::other("Abandoned")` immediately
    - When `metadata_enabled` is `false`, bypass demux entirely
  - Return `Err` (not `Ok(0)`) when the stream ends inside a metadata block
  - Refuse `Seek` operations (implement `io::Seek` to always return `Err`)
  - Port and extend existing `stream_reader` tests: exact-boundary reads, EOF inside metadata, seek refusal, abandon-on-stale-generation
  - _Requirements: 8.1, 8.2, 8.3, 8.4, 8.5, 8.6, 8.7_

  - [ ]* 6.1 Write property test for ICY safety
    - **Property 7: ICY safety**
    - For any `(audio_bytes, metaint, titles)` triple, reconstructed audio bytes from `StreamSource::read` equal `audio_bytes` exactly (all metadata blocks removed, no metadata bytes appear in output)
    - Use `proptest` to generate random metaint values, audio payloads, and ICY title strings
    - **Validates: Requirements 8.1, 8.3**

  - [ ]* 6.2 Write property test for stale StreamSource abandonment
    - **Property 14: Stale StreamSource read is immediately abandoned**
    - For any `StreamSource` whose generation is made inactive before reading, the first `read` returns `Abandoned` error without blocking
    - **Validates: Requirements 8.5**

- [x] 7. Implement `DecodePipeline` and worker main function
  - Create `src/audio/decode.rs`
  - Implement `DecodePipeline::build(source, prebuffer, sample_buffer)`:
    - Read bytes into a bounded prebuffer Vec, emitting `EngineEvent::Buffering` progress updates
    - Bail with `EngineError::Connect("prebuffer timeout")` if `fill_timeout` elapses before `min_bytes` are read
    - Chain the prebuffer bytes ahead of the remaining `StreamSource` using `io::Cursor` + `io::Chain`
    - Probe container/codec with Symphonia (`symphonia::default::get_probe()`)
    - If ICY headers or prior knowledge indicate MP3, apply the MP3 fast-path (skip full probe)
    - Wrap the decoded stream in the visualizer tap (`VisualizerSource`) using `try_lock` (non-blocking; drop batch on failure)
    - Return `(DecodedSource, StreamFormat)` on success or `EngineError::Decode` on probe/codec failure
  - Implement worker main function that:
    1. Calls `guard_active` and returns `StreamEnded{Abandoned}` if stale
    2. Opens the HTTP connection via `reqwest` blocking client with finite `connect_timeout`
    3. Checks response status and emits `EngineError::Http` for non-2xx
    4. Parses ICY `metaint` header
    5. Constructs `StreamSource` and calls `DecodePipeline::build`
    6. Sends `EngineEvent::Connected` on success
    7. Maps all errors to appropriate `EngineEvent::Failed` or `EngineEvent::StreamEnded`
  - Write unit tests for: prebuffer timeout, short stream handling, probe failure maps to `Decode` error, visualizer try_lock non-blocking
  - _Requirements: 6.1, 6.2, 6.3, 6.4, 6.5, 6.6, 7.1, 7.2, 7.6, 13.1, 13.2, 13.3_

  - [ ]* 7.1 Write property test for prebuffer memory bound
    - **Property 13: Prebuffer memory is bounded**
    - For any stream that delivers bytes in arbitrary chunk sizes, the prebuffer Vec never exceeds `max_bytes` in length
    - **Validates: Requirements 6.4**

  - [ ]* 7.2 Write property test for visualizer passivity
    - **Property 11: Sample tap is passive**
    - For any simulated lock-contention scenario, the decode function completes without blocking (verifiable by running with a mutex held by another thread and asserting the decode call returns promptly)
    - **Validates: Requirements 13.2**

- [x] 8. Checkpoint — unit layer tests pass
  - Ensure all tests pass (`cargo test`), ask the user if questions arise.

- [x] 9. Implement `EngineLoop` and `EngineState` state machine
  - Create `src/audio/engine_loop_v2.rs` (rename to `engine_loop.rs` once it replaces the old one)
  - Implement `EngineLoop` struct containing: `state: EngineState`, `output: OutputManager`, `supervisor: ConnectionSupervisor`, `options: PlaybackOptions`, `volume: VolumeRamp`, `status_tx`, `event_rx`, `event_tx`, `sample_buffer`
  - Implement `handle_command(&mut self, cmd: AudioCommand)` as a total function (trailing `_ => {}` arm):
    - `Play(url)` from any state: bump generation, stop output immediately, transition to `Connecting`, spawn worker
    - `Pause` while `Playing`: pause output, transition to `Paused`
    - `Resume` while `Paused`: resume output, begin fade-in, transition to `Playing`
    - `Stop` from any state: abandon supervisor, stop output, transition to `Idle`, emit `Stopped`
    - `SetVolume(v)`: clamp and forward to `VolumeRamp`
    - `SetOutputDevice(d)`: normalize and forward to `OutputManager`
    - `SetStreamMetadata(e)`: store in `PlaybackOptions`
  - Implement `handle_event(&mut self, ev: EngineEvent)` as a total function:
    - Drop events from non-active generations immediately
    - `Buffering`: transition state, emit `AudioStatus::Buffering { percent }`
    - `Connected`: attach source to `OutputManager`, begin fade-in, transition to `Playing`; on output failure, call `fail_or_recover`
    - `TrackChanged`: emit `AudioStatus::TrackChanged`
    - `StreamEnded { Abandoned }`: no-op
    - `StreamEnded { Eof }`: transition to `Idle`, emit `Stopped`
    - `StreamEnded { Network | Decode }`: call `fail`; emit `Error`
    - `OutputLost`: call `try_recover_output`
    - `Failed`: call `fail_or_recover`
  - Implement `tick(&mut self)`: advance `VolumeRamp`, emit `FadingOut` progress updates during stop fade
  - Implement `transition(&mut self, next: EngineState)`: update state, emit `AudioStatus` only on user-visible change (no duplicate spam)
  - Implement `try_recover_output`: reopen `OutputManager` up to `MAX_HARDWARE_RECOVERY_RETRIES`, re-attach source on success, emit `Error` on exhaustion
  - Implement `EngineLoop::run`: poll `cmd_rx` with timeout, drain `event_rx` non-blocking, call `tick`; clean shutdown on `Disconnected`
  - Write table-driven unit tests covering every meaningful `(EngineState, AudioCommand)` and `(EngineState, EngineEvent)` pair
  - _Requirements: 3.1, 3.3, 3.4, 3.5, 3.6, 4.3, 4.4, 5.1, 5.4, 5.5, 9.3, 9.4, 9.5, 10.1, 10.2, 10.3, 10.4, 11.5, 11.6, 14.3, 14.4, 14.5_

  - [ ]* 9.1 Write property test for no-panic / liveness
    - **Property 1: No-panic / liveness**
    - For any randomly generated sequence of `AudioCommand` values, driving `EngineLoop::handle_command` with a mock `OutputManager` and `ConnectionSupervisor` never panics
    - Use `proptest` to generate arbitrary command sequences including edge cases (`SetVolume(f32::NAN)`, rapid `Play`/`Stop` interleaving)
    - **Validates: Requirements 5.1, 3.5, 3.6**

  - [ ]* 9.2 Write property test for stale-generation event isolation
    - **Property 3 (stale events): Stale-generation events produce no observable status change**
    - For any `EngineEvent` whose generation is not the active generation, no `AudioStatus` is emitted and state is unchanged
    - **Validates: Requirements 4.3**

  - [ ]* 9.3 Write property test for Stop terminal-and-clean
    - **Property 4: Stop is terminal-and-clean**
    - For any `EngineState`, sending `AudioCommand::Stop` results in `EngineState::Idle` and exactly one `AudioStatus::Stopped` emitted; subsequent stale-generation events do not change the state
    - **Validates: Requirements 3.3, 11.5**

  - [ ]* 9.4 Write property test for single AudioStatus per state change
    - **Property 2 (emission): Exactly one AudioStatus emitted per user-visible state change**
    - For any `(state, command)` or `(state, event)` pair that produces a user-visible transition, the status channel receives exactly one new value; pairs that are no-ops produce zero values
    - **Validates: Requirements 3.3**

- [x] 10. Update `AudioEngine` public handle and replace `engine_loop`
  - In `src/audio.rs`, add `AudioStatus::Buffering { percent: u8 }` if not already added in task 1
  - Replace the `engine_loop::audio_loop` call in `AudioEngine::spawn` with `EngineLoop::run`
  - Ensure `disconnected_for_test()` still compiles under `#[cfg(test)]`
  - Ensure `AudioEngine::send` still returns `false` when the command channel is closed
  - Update the `match status` example in app's `poll_audio_status` to handle the new `Buffering` arm (add to `src/app/playback_runtime.rs` or wherever status is consumed)
  - Confirm all existing `src/app/` call sites compile without changes
  - _Requirements: 1.1, 1.2, 1.3, 1.4, 1.5, 1.6, 2.1, 2.2, 2.4_

- [x] 11. Update codec capability policy
  - In `src/audio/capability.rs`, update `codec_capability` so AAC, OGG/Vorbis, Opus, FLAC, and WAV return `PlaybackCapability::Supported`
  - Update the `reason` strings for newly supported codecs to reflect they are now enabled
  - Keep HLS/M3U8 as `Unsupported`
  - Keep empty/unrecognized codecs as `Unknown`
  - Update existing unit tests: the current tests assert AAC/OGG/etc. are `Unsupported` — flip those assertions to `Supported`
  - Write a new test that iterates every `Supported` codec and asserts `DecodePipeline` accepts a real (minimal) encoded sample of that format (codec/decoder agreement)
  - _Requirements: 7.3, 7.4, 7.5, 7.7_

  - [ ]* 11.1 Write property test for capability/decoder agreement
    - **Property 9: Capability/decoder agreement**
    - For every codec string that maps to `PlaybackCapability::Supported` in `codec_capability`, `DecodePipeline::build` succeeds when given a minimal valid sample of that format; HLS/M3U8 remain `Unsupported`
    - **Validates: Requirements 7.3, 7.7**

- [x] 12. Checkpoint — full test suite passes after wiring
  - Run `cargo test --all` and resolve any failures
  - Ensure all property tests run with at least 100 iterations (default proptest configuration)
  - Ensure the capability/decoder agreement test covers all `Supported` codecs
  - Ask the user if questions arise.

- [ ] 13. Integration tests with in-process HTTP server
  - In `tests/audio_integration.rs` (or `src/audio/tests/`), using `tiny_http`:
    - [ ]* 13.1 Test successful MP3 playback: server streams a minimal MP3; assert engine transitions `Connecting → Buffering → Playing`
      - _Requirements: 2.2, 2.4, 7.2_
    - [ ]* 13.2 Test successful OGG playback: server streams a minimal OGG file; assert engine reaches `Playing` (validates Symphonia probe path)
      - _Requirements: 7.1, 7.3_
    - [ ]* 13.3 Test prebuffer timeout: server connects but stalls; assert engine emits `Error` within `fill_timeout` and does not stay in `Connecting` indefinitely
      - _Requirements: 6.2, 6.3_

  - [ ]* 13.4 Write property test for bounded connecting
    - **Property 5: Bounded connecting**
    - For any hanging/unreachable URL simulation, the engine exits `Connecting`/`Buffering` and emits `AudioStatus::Error` within `fill_timeout + connect_timeout`; the loop never runs indefinitely
    - **Validates: Requirements 6.2, 6.3**

  - [ ]* 13.5 Test mid-stream network drop: server disconnects after partial data; assert engine emits `AudioStatus::Error` and does NOT issue a reconnect command itself
    - _Requirements: 11.2, 11.5_
  - [ ]* 13.6 Test HTTP non-success (404): assert engine emits `AudioStatus::Error("HTTP 404")`
    - _Requirements: 11.3_
  - [ ]* 13.7 Test hardware output recovery: simulate `EngineEvent::OutputLost` via internal channel; assert engine attempts reopen and reaches `Playing` on success; assert engine emits `Error` after exhausting `MAX_HARDWARE_RECOVERY_RETRIES`
    - _Requirements: 9.3, 9.4, 9.5_

- [x] 14. Remove legacy audio internals and clean up
  - Delete `src/audio/session.rs` (replaced by the worker in `decode.rs` + `supervisor.rs`)
  - Delete or gut `src/audio/stream_reader.rs` (replaced by `stream_source.rs`)
  - Remove `src/audio/metadata.rs` if its functionality is absorbed into `stream_source.rs`
  - Remove or fold `src/audio/visualizer.rs` into `decode.rs`'s `VisualizerSource` wrapper
  - Remove the `AudioLoopState` struct and all ad-hoc flags from `engine_loop.rs`
  - Ensure `cargo build --release` succeeds with no warnings
  - _Requirements: 3.4, 5.5_

- [x] 15. Final checkpoint — all tests pass, build clean
  - Run `cargo test --all` and confirm no failures
  - Run `cargo clippy` and address any new lints introduced
  - Confirm `cargo build --release` produces a binary
  - Ask the user if questions arise.

## Notes

- Sub-tasks marked with `*` are optional and can be skipped for a faster MVP; all core
  implementation tasks (unnumbered or without `*`) must be completed.
- Each property test references the `proptest` library added in task 1; use the `proptest!` macro
  with `#[test]` (not `#[proptest]`) and configure at least 100 cases per property.
- The `tiny_http` integration tests run against real sockets — gate them with
  `#[cfg(feature = "integration")]` or a `#[ignore]` attribute if needed for CI speed.
- The `EngineLoop` implementation in task 9 uses `mock` variants of `OutputManager` and
  `ConnectionSupervisor` (via trait objects or test-only constructors) so the state machine can be
  tested without real audio devices.
- Generation IDs use `u64`; the `AtomicU64::store(0, SeqCst)` in `abandon()` is safe because
  generation 0 is never allocated by `next_generation` (which starts at 1).

## Task Dependency Graph

```json
{
  "waves": [
    { "wave": 1, "tasks": ["1"] },
    { "wave": 2, "tasks": ["2", "3", "4", "5", "6"] },
    { "wave": 3, "tasks": ["7"] },
    { "wave": 4, "tasks": ["8"] },
    { "wave": 5, "tasks": ["9"] },
    { "wave": 6, "tasks": ["10"] },
    { "wave": 7, "tasks": ["11"] },
    { "wave": 8, "tasks": ["12"] },
    { "wave": 9, "tasks": ["13"] },
    { "wave": 10, "tasks": ["14"] },
    { "wave": 11, "tasks": ["15"] }
  ]
}
```
