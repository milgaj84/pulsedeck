# Requirements Document

## Introduction

PulseDeck v0.5.0 rewrites the audio engine (`src/audio/`) to eliminate fragility caused by an
MP3-only decode path, ad-hoc mutable flags, and an unstructured retry loop. The rewrite introduces
a layered, testable pipeline governed by a single state machine, broadens codec support via
Symphonia probe-based decoding (AAC, OGG/Vorbis, Opus, FLAC, WAV alongside MP3), and hardens
resilience against network drops, hardware output changes, and malformed streams — all while
preserving the external contract (`AudioEngine::spawn`, `AudioCommand`, `AudioStatus`, the
visualizer sample buffer) that the rest of the application already depends on.

## Glossary

- **AudioEngine**: The public handle that the app layer holds. Exposes `spawn`, `send`, and
  `status_rx`.
- **EngineLoop**: The single OS thread that owns `EngineState` and is the sole emitter of
  `AudioStatus`.
- **EngineState**: The typed state machine enum (`Idle`, `Connecting`, `Buffering`, `Playing`,
  `Paused`, `Recovering`, `Failed`).
- **EngineEvent**: Internal channel messages from workers and `OutputManager` to `EngineLoop`.
- **EngineError**: A single classified error type replacing scattered string errors.
- **ConnectionSupervisor**: Manages generation IDs and the lifecycle of connection/decode workers.
- **Generation**: A monotonically increasing `u64` allocated per `Play` command. Used to discard
  stale worker results.
- **OutputManager**: Encapsulates cpal/rodio device selection, `Sink` lifecycle, and device
  recovery.
- **DecodePipeline**: Probes container/codec with Symphonia and builds a decoded PCM source.
- **StreamSource**: A byte reader that strips ICY metadata and emits `TrackChanged` events.
- **PrebufferConfig**: Configures minimum bytes, maximum bytes, and fill timeout for the bounded
  prebuffer.
- **VolumeRamp**: Handles fade-in and fade-out logic, decoupled from `EngineState`.
- **Reconnect**: The app-layer backoff policy (3/6/12 s, max 3 attempts). Unchanged.
- **classify_playback_error**: The app-layer function that maps error strings to
  `PlaybackErrorKind`. Must remain working after the rewrite.

---

## Requirements

### Requirement 1: Preserve the Public AudioEngine Contract

**User Story:** As an app developer, I want the public `AudioEngine` API to remain unchanged, so
that no call sites in `src/app/` need modification.

#### Acceptance Criteria

1. THE `AudioEngine` SHALL expose `spawn(sample_buffer: Arc<Mutex<VecDeque<f32>>>) -> Self` with
   the identical signature as today.
2. THE `AudioEngine` SHALL expose `send(&self, cmd: AudioCommand) -> bool` returning `false` when
   the command channel is closed.
3. THE `AudioEngine` SHALL expose `pub status_rx: mpsc::Receiver<AudioStatus>` as a public field.
4. THE `AudioEngine` SHALL expose `disconnected_for_test() -> Self` under `#[cfg(test)]` so
   existing test helpers compile unchanged.
5. THE `AudioEngine` SHALL preserve all seven `AudioCommand` variants (`Play`, `Pause`, `Resume`,
   `Stop`, `SetVolume`, `SetOutputDevice`, `SetStreamMetadata`) with their existing signatures.
6. THE `AudioEngine` SHALL preserve all pre-existing `AudioStatus` variants
   (`Playing`, `Paused`, `Stopped`, `Connecting`, `FadingOut`, `TrackChanged`, `Error`) with their
   existing semantics.

---

### Requirement 2: Add `AudioStatus::Buffering` Variant

**User Story:** As a user, I want to see real-time prebuffer progress in the UI, so that I know
the engine is actively loading and haven't stalled silently.

#### Acceptance Criteria

1. THE `AudioEngine` SHALL add `AudioStatus::Buffering { percent: u8 }` as a new variant.
2. WHEN the worker is filling the prebuffer, THE `EngineLoop` SHALL emit
   `AudioStatus::Buffering { percent }` on each progress update.
3. THE `percent` field SHALL be in the range `[0, 100]` inclusive.
4. THE `AudioEngine` SHALL emit `AudioStatus::Buffering` between `Connecting` and `Playing`, never
   before `Connecting` and never after `Playing` for the same generation.
5. IF the prebuffer completes instantly (stream delivers bytes before the first tick),
   THE `EngineLoop` SHALL still transition through `Buffering` before emitting `Playing`.

---

### Requirement 3: Single-Owner State Machine (`EngineState`)

**User Story:** As a maintainer, I want a single, explicit state machine to own all engine
transitions, so that scattered mutable flags can no longer cause regressions.

#### Acceptance Criteria

1. THE `EngineLoop` SHALL own exactly one `EngineState` value at all times.
2. `EngineState` SHALL cover the variants `Idle`, `Connecting`, `Buffering`, `Playing`, `Paused`,
   `Recovering`, and `Failed`.
3. WHEN a transition occurs, THE `EngineLoop` SHALL emit the corresponding `AudioStatus` exactly
   once per user-visible state change (no duplicate spam; internal generation bumps within the same
   variant are silent).
4. THE `EngineLoop` SHALL NOT store ad-hoc flags equivalent to `reopen_output_on_next_connection`,
   `pending_action`, or `current_fade_volume` outside their designated components (`OutputManager`
   and `VolumeRamp`).
5. THE `handle_command` function SHALL be total: every `(EngineState, AudioCommand)` pair SHALL
   have a defined, non-panicking outcome (with a no-op `_ => {}` arm for combinations that are not
   meaningful).
6. THE `handle_event` function SHALL be total: every `EngineEvent` SHALL have a defined,
   non-panicking outcome.

---

### Requirement 4: Generation IDs and Stale-Work Cancellation

**User Story:** As a user, I want rapid station switching to work correctly, so that switching
stations quickly never plays audio from a previous station or produces retry storms.

#### Acceptance Criteria

1. WHEN `Play` is sent, THE `ConnectionSupervisor` SHALL allocate a new `Generation` that is
   strictly greater than all prior generations.
2. THE `ConnectionSupervisor` SHALL publish the active generation atomically via `AtomicU64` so
   workers can read it without locking.
3. WHEN `EngineLoop` receives an `EngineEvent` from a non-active generation,
   THE `EngineLoop` SHALL discard the event and produce no `AudioStatus` change.
4. WHEN `Play` is sent while a prior worker is running, THE `ConnectionSupervisor` SHALL abandon
   the prior worker without blocking the control thread (no `join()` on the hot path).
5. THE `ConnectionSupervisor::is_active` function SHALL be a pure read: `gen == active.load(SeqCst)`.
6. WHEN a worker's generation becomes inactive, THE worker SHALL exit promptly on its next
   `guard_active` check rather than sending any further events.

---

### Requirement 5: No-Panic Engine Loop

**User Story:** As a user, I want the audio engine to never crash the application, so that
any audio error degrades gracefully to an error state rather than a panic.

#### Acceptance Criteria

1. THE `EngineLoop::run` function SHALL NOT panic for any sequence of `AudioCommand` inputs.
2. WHEN a worker thread panics, THE `EngineLoop` SHALL emit
   `AudioStatus::Error("Connection thread panicked")` and continue running.
3. WHEN the sample-buffer mutex is poisoned, THE `EngineLoop` SHALL recover via `into_inner()` and
   continue running.
4. WHEN the UI command channel closes (application exit), THE `EngineLoop` SHALL release all audio
   resources and return from `run` cleanly without panicking.
5. THE `EngineLoop` control thread SHALL NOT perform blocking network I/O or blocking decode work
   itself; all blocking work SHALL be delegated to worker threads.

---

### Requirement 6: Bounded Prebuffer with Timeout

**User Story:** As a user, I want streams that never deliver enough bytes to fail with a clear
error rather than hanging forever in "Connecting", so that I am never stuck waiting indefinitely.

#### Acceptance Criteria

1. THE `DecodePipeline` SHALL read at least `PrebufferConfig::min_bytes` before probing the codec,
   up to a cap of `PrebufferConfig::max_bytes`.
2. IF the stream does not deliver `min_bytes` within `PrebufferConfig::fill_timeout`,
   THE worker SHALL emit `EngineError::Connect("prebuffer timeout")` and terminate.
3. THE `fill_timeout` value SHALL be finite (e.g. 8 s) so the engine can never remain in
   `Connecting`/`Buffering` indefinitely.
4. THE prebuffer memory usage SHALL not exceed `max_bytes` at any point.
5. IF the stream ends before `min_bytes` are received (short/empty stream),
   THE worker SHALL stop reading and attempt to probe whatever bytes are available.
6. WHEN prebuffer fills successfully, THE worker SHALL chain the prebuffer bytes ahead of the
   remaining live stream before handing off to the decoder, so no audio data is lost.

---

### Requirement 7: Symphonia Probe-Based Codec Support

**User Story:** As a user, I want to play AAC, OGG/Vorbis, Opus, FLAC, and WAV streams in
addition to MP3, so that more radio stations are accessible.

#### Acceptance Criteria

1. THE `DecodePipeline` SHALL probe the container and codec using Symphonia rather than calling
   `Decoder::new_mp3` directly.
2. THE `DecodePipeline` SHALL retain an MP3 fast-path that skips full probing when the codec is
   already known to be MP3.
3. THE `codec_capability` function SHALL return `PlaybackCapability::Supported` for AAC, OGG/
   Vorbis, Opus, FLAC, and WAV in v0.5.0.
4. THE `codec_capability` function SHALL continue to return `PlaybackCapability::Unsupported` for
   HLS/M3U8, as HLS remains out of scope for v0.5.0.
5. THE `codec_capability` function SHALL continue to return `PlaybackCapability::Unknown` for
   unrecognized or empty codec strings, allowing a probe attempt.
6. WHEN probe fails or the codec is not registered in `DecodePipeline`,
   THE worker SHALL emit `EngineError::Decode` with a descriptive message.
7. THE set of codecs returning `PlaybackCapability::Supported` SHALL exactly match the set of
   codecs for which `DecodePipeline` has a registered decoder path.

---

### Requirement 8: ICY Metadata Demux (`StreamSource`)

**User Story:** As a user, I want track titles to appear correctly and ICY metadata bytes to
never be decoded as audio, so that the audio output is clean and metadata is accurate.

#### Acceptance Criteria

1. THE `StreamSource` SHALL read bytes from the underlying stream and strip ICY metadata blocks
   before passing bytes to the decoder.
2. WHEN a `StreamTitle` is present in an ICY metadata block, THE `StreamSource` SHALL emit
   `EngineEvent::TrackChanged { title }` via the internal event channel.
3. THE `StreamSource::read` function SHALL never write metadata bytes into the audio output buffer.
4. IF the stream ends inside an ICY metadata block, THE `StreamSource` SHALL return an error rather
   than partial audio or partial metadata.
5. WHEN the generation becomes inactive, THE `StreamSource::read` function SHALL return
   `io::Error::other("Abandoned")` immediately without further reading.
6. THE `StreamSource` SHALL refuse seek operations (live streams are append-only).
7. WHERE `metadata_enabled` is `false`, THE `StreamSource` SHALL skip ICY demux entirely and pass
   all bytes through unchanged.

---

### Requirement 9: Output Device Management (`OutputManager`)

**User Story:** As a user, I want the audio engine to lazily open the output device and recover
from device changes, so that browsing works without a device and device unplugs do not crash
playback.

#### Acceptance Criteria

1. THE `OutputManager` SHALL open the output device lazily on first playback, not at engine
   startup.
2. WHEN `SetOutputDevice` is sent, THE `OutputManager` SHALL mark that a reopen is needed and apply
   the new preferred device on the next connection attempt.
3. WHEN an output device error occurs during playback, THE `EngineLoop` SHALL attempt at most
   `MAX_HARDWARE_RECOVERY_RETRIES` automatic reopens of the output device.
4. WHEN a reopen succeeds, THE `EngineLoop` SHALL re-attach the current generation's decoded source
   and emit `AudioStatus::Connecting` then `AudioStatus::Playing`.
5. WHEN reopen exhausts `MAX_HARDWARE_RECOVERY_RETRIES`,
   THE `EngineLoop` SHALL emit `AudioStatus::Error("Hardware output error: ...")`.
6. THE `OutputManager::is_sink_drained` function SHALL return `true` only when the sink reports
   empty due to natural source end, not due to device loss (device loss arrives as
   `EngineEvent::OutputLost`).
7. THE `OutputManager` SHALL suppress ALSA/JACK probe chatter on stderr using the same
   native-stderr suppression mechanism as the current implementation.
8. ALL rodio/cpal calls inside `OutputManager` SHALL be wrapped so that failures produce
   `EngineError::Output` rather than panicking.

---

### Requirement 10: Volume Ramping (`VolumeRamp`)

**User Story:** As a user, I want smooth fade-in and fade-out when switching or stopping stations,
so that audio transitions are not jarring.

#### Acceptance Criteria

1. WHEN a new source is attached after `Connected`, THE `VolumeRamp` SHALL begin a fade-in from
   0.0 to `target_volume`.
2. WHEN `Resume` is sent, THE `VolumeRamp` SHALL begin a fade-in from the current volume to
   `target_volume`.
3. WHEN `Stop` is sent while playing, THE `EngineLoop` SHALL emit `AudioStatus::FadingOut`
   progress updates and stop the sink only after the fade completes.
4. WHEN `Play` is sent while playing, THE `EngineLoop` SHALL stop the current sink immediately
   (no fade) and start connecting for the new station.
5. THE `VolumeRamp` SHALL be a separate struct, not embedded as ad-hoc fields in `EngineLoop`.
6. WHEN `SetVolume(v)` is sent, THE applied volume SHALL be clamped to `[0.0, 1.0]` before being
   stored or sent to the sink, including when `v` is NaN or infinite.

---

### Requirement 11: Network Error Handling and App-Side Reconnect

**User Story:** As a user, I want network errors to be reported clearly to the app so the existing
reconnect backoff can retry appropriately, without the engine creating its own retry loops.

#### Acceptance Criteria

1. WHEN a network connect fails (DNS/TCP/TLS/timeout),
   THE `EngineLoop` SHALL emit `AudioStatus::Error("Connection failed: ...")`.
2. WHEN a mid-stream network read error occurs, THE worker SHALL emit `EngineEvent::StreamEnded`
   with `EndReason::Network`, and THE `EngineLoop` SHALL emit `AudioStatus::Error`.
3. WHEN an HTTP non-success status is received, THE `EngineLoop` SHALL emit
   `AudioStatus::Error("HTTP {status}")` where `{status}` is the numeric HTTP status code.
4. WHEN a decode error occurs, THE `EngineLoop` SHALL emit `AudioStatus::Error("Decode error: ...")`.
5. THE `EngineLoop` SHALL NOT perform automatic network reconnect; the app-layer `Reconnect` policy
   (3/6/12 s, max 3 attempts) is the sole reconnect mechanism.
6. WHEN a stream ends naturally (EOF), THE `EngineLoop` SHALL transition to `Idle` and emit
   `AudioStatus::Stopped`.

---

### Requirement 12: Error String Stability for `classify_playback_error`

**User Story:** As a maintainer, I want `AudioStatus::Error` strings to keep their classifiable
prefixes, so that the app's error classification and UI hint logic continues to work without
modification.

#### Acceptance Criteria

1. `AudioStatus::Error` strings for output errors SHALL start with `"Hardware output error:"`.
2. `AudioStatus::Error` strings for decode errors SHALL start with `"Decode error:"`.
3. `AudioStatus::Error` strings for HTTP errors SHALL start with `"HTTP "` followed by the numeric
   status code.
4. `AudioStatus::Error` strings for network/connect errors SHALL contain `"Connection failed:"`.
5. `AudioStatus::Error` strings for prebuffer timeout SHALL satisfy the network/timeout classifier
   (containing `"timeout"` or `"Connection failed:"`).
6. `AudioStatus::Error("Connection thread panicked")` SHALL be emitted when a worker thread panics.
7. FOR ALL error strings emitted by the engine, `classify_playback_error(e)` SHALL return a value
   other than `PlaybackErrorKind::Unknown` for output, decode, HTTP, network, and timeout causes.

---

### Requirement 13: Visualizer Sample Tap (Passive)

**User Story:** As a user, I want the spectrum visualizer to continue working without ever
blocking audio playback, so that a slow UI tick never causes audio stutter.

#### Acceptance Criteria

1. THE `DecodePipeline` SHALL push decoded PCM sample batches into the shared
   `Arc<Mutex<VecDeque<f32>>>` sample buffer.
2. WHEN the sample buffer mutex cannot be acquired immediately (`try_lock` fails),
   THE `DecodePipeline` SHALL drop the batch and continue decoding without blocking.
3. THE sample buffer capacity SHALL be bounded (e.g. 4096 samples) as in the current
   implementation.
4. THE visualizer tap SHALL have no effect on `AudioStatus` or `EngineState`.

---

### Requirement 14: Control Thread Concurrency Model

**User Story:** As a maintainer, I want the engine's threading model to remain compatible with the
rest of the application, so that tokio and the TUI are not disrupted.

#### Acceptance Criteria

1. THE `EngineLoop` SHALL run on a dedicated OS thread using `std::thread::spawn`, not inside the
   tokio runtime.
2. THE `EngineLoop` SHALL communicate with the app via `std::sync::mpsc` channels only.
3. THE `EngineLoop` SHALL poll the command channel with a bounded timeout (`recv_timeout`) and the
   internal event channel non-blocking (`try_recv`), so the control thread never blocks longer than
   one poll interval.
4. THE `EngineLoop` SHALL process commands before events in each iteration (commands have higher
   priority).
5. THE `EngineLoop` SHALL call `tick()` once per iteration to advance time-based concerns (volume
   ramp, buffering progress UI) after commands and events are drained.

---

### Requirement 15: Testing Infrastructure

**User Story:** As a developer, I want a clear testing strategy implemented so that regressions
can be caught automatically.

#### Acceptance Criteria

1. THE project SHALL add `proptest` as a dev-dependency for property-based tests.
2. THE project SHALL add a lightweight in-process HTTP server (e.g. `tiny_http`) as a dev-
   dependency for integration tests.
3. THE `EngineLoop` state machine transitions SHALL be covered by table-driven unit tests over every
   meaningful `(state, command)` and `(state, event)` pair.
4. THE `StreamSource` ICY demux logic SHALL be covered by unit tests including: exact-boundary
   reads, EOF inside metadata, seek refusal, and abandon-on-stale-generation.
5. THE `EngineError`-to-status-string mapping SHALL be covered by unit tests that lock the stable
   prefix contract.
6. THE `codec_capability` matrix and the set of decoders registered in `DecodePipeline` SHALL be
   validated together so additions to one that are missing from the other cause a test failure.
7. THE `OutputManager` device-name normalization, reopen bookkeeping, and drained-vs-lost
   distinction SHALL be covered by unit tests.
