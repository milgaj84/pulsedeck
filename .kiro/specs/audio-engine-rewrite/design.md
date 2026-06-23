# Design Document: Audio Engine Rewrite (v0.5.0)

## Overview

PulseDeck's current audio engine (`src/audio.rs` + `src/audio/`) has become fragile. It is
MP3-only (it bypasses rodio's generic Symphonia probing by calling `Decoder::new_mp3` directly),
mixes connection, decoding, ICY-metadata stripping, output recovery, fade ramping, and concurrency
control inside one `AudioLoopState`, and has regressed repeatedly on stream startup, prebuffering,
reconnect loops, and hardware recovery. The control surface (`AudioCommand` in, `AudioStatus` out,
plus a shared `Arc<Mutex<VecDeque<f32>>>` visualizer tap) is sound, but the internals are not
resilient.

This rewrite keeps the external contract that the app layer already depends on
(`AudioEngine::spawn`, `AudioCommand`, `AudioStatus`, the sample buffer, codec capability gating,
output-device selection) and replaces the internals with a layered, testable pipeline built around
three principles:

1. **Isolation of failure.** Every fallible operation (DNS/connect, HTTP read, decode, output
   write) lives behind a typed boundary that converts errors into a single classified
   `EngineError` value. No layer can panic the engine thread; a poisoned mutex, a decoder error, or
   a vanished sound device degrades to a recoverable status, never a crash.
2. **One owner of state.** A single `EngineState` state machine owns all transitions
   (`Idle → Connecting → Buffering → Playing → Paused`, plus `Recovering` and `Failed`). Commands
   and internal events are the only inputs; status emissions are the only outputs. This removes the
   scattered ad-hoc flags (`reopen_output_on_next_connection`, `pending_action`,
   `current_fade_volume`) that caused regressions.
3. **Broaden codec support safely.** Decoding moves to a probe-based Symphonia front end with a
   bounded prebuffer, so AAC, OGG/Vorbis, Opus, FLAC, and WAV become playable while MP3 keeps its
   fast path. HLS remains explicitly out of scope for v0.5.0 (it needs a playlist/segment fetcher),
   but the capability policy is updated so the engine — not a hardcoded MP3-only gate — is the
   source of truth.

The engine continues to run on a dedicated OS thread with blocking I/O, communicating with the
async/TUI side via `std::sync::mpsc`. This preserves the existing threading boundary the rest of
the app is built around (`tokio` is used for the TUI/runtime, not for audio).

## Architecture

The engine is decomposed into a thin handle, a supervising control loop, and a set of single-
responsibility components. The control loop owns the state machine and is the only place that
sends `AudioStatus`. Worker threads (connection + decode) never touch shared UI state directly;
they report back through an internal event channel keyed by a generation id so stale work is
discarded deterministically.

```mermaid
graph TD
    subgraph UI/App Thread (tokio)
        APP[App / PlaybackRuntime]
    end

    subgraph Audio OS Thread
        HANDLE[AudioEngine handle]
        LOOP[EngineLoop + EngineState state machine]
        OUT[OutputManager - cpal/rodio device + Sink]
        SUP[ConnectionSupervisor - generation ids]
    end

    subgraph Worker Threads (per generation)
        CONN[Connector - HTTP connect + headers]
        SRC[StreamSource - byte reader + ICY demux]
        DEC[DecodePipeline - Symphonia probe + prebuffer]
    end

    VIZ[(Arc&lt;Mutex&lt;VecDeque&lt;f32&gt;&gt;&gt; sample buffer)]

    APP -- AudioCommand --> HANDLE
    HANDLE -- cmd mpsc --> LOOP
    LOOP -- AudioStatus mpsc --> APP
    LOOP --> OUT
    LOOP --> SUP
    SUP --> CONN
    CONN --> SRC
    SRC --> DEC
    DEC -- decoded PCM frames --> OUT
    DEC -- internal EngineEvent --> LOOP
    DEC -- sample batch --> VIZ
    OUT -- device error event --> LOOP
```

### Threading Model

- **UI/App thread**: owns `App`, polls `status_rx` each tick (`poll_audio_status`), sends commands.
  Unchanged from today.
- **Audio control thread** (`EngineLoop::run`): a single OS thread. Owns `EngineState`,
  `OutputManager`, and `ConnectionSupervisor`. Polls two channels with a short timeout: the command
  channel (from UI) and the internal event channel (from workers). It is the sole emitter of
  `AudioStatus`. It never performs blocking network or decode work itself.
- **Connection/decode worker** (per playback generation): a transient OS thread spawned by the
  supervisor. It connects, builds the stream source, probes the codec, fills a bounded prebuffer,
  then hands a decoded PCM `Source` to the control thread via an `EngineEvent::Connected`. After
  handoff it continues pulling/decoding inside the rodio `Sink` pull model (the decoded source is
  appended to the sink and driven by the cpal callback), exactly as today, but wrapped so read/
  decode errors surface as `EngineEvent::StreamEnded { reason }` rather than silent sink-empty.

### Generation IDs (stale work cancellation)

Every `Play` allocates a new monotonically increasing `Generation`. Workers carry their generation
and check a shared `AtomicU64` "active generation" before each blocking step and on every buffer
read (the existing `active_conn_id` pattern, generalized). Any result from a non-active generation
is dropped as `Abandoned`. This is what makes rapid station switching safe and is the backbone of
"no bad retry loops."

## Sequence Diagrams

### Successful play with prebuffer

```mermaid
sequenceDiagram
    participant App
    participant Loop as EngineLoop
    participant Sup as ConnectionSupervisor
    participant Wrk as Worker(gen=N)
    participant Out as OutputManager

    App->>Loop: AudioCommand::Play(url)
    Loop->>Loop: state Idle/Playing -> Connecting(gen=N)
    Loop-->>App: AudioStatus::Connecting
    Loop->>Sup: spawn(gen=N, url, opts)
    Sup->>Wrk: connect + headers
    Wrk-->>Loop: EngineEvent::Buffering(gen=N)
    Loop-->>App: AudioStatus::Buffering { percent }
    Wrk->>Wrk: probe codec + fill prebuffer
    Wrk-->>Loop: EngineEvent::Connected(gen=N, decoded_source, format)
    Loop->>Out: attach source to Sink, ramp volume in
    Loop->>Loop: state -> Playing(gen=N)
    Loop-->>App: AudioStatus::Playing
```

### Network drop with bounded reconnect (no loop)

```mermaid
sequenceDiagram
    participant App
    participant Loop as EngineLoop
    participant Wrk as Worker(gen=N)
    participant Out as OutputManager

    Wrk-->>Loop: EngineEvent::StreamEnded { reason: Network }
    Loop->>Loop: was intentional? no -> emit Error (app owns backoff)
    Loop-->>App: AudioStatus::Error("Connection lost: ...")
    Note over App: App-side Reconnect arms backoff (3/6/12s, max 3)
    App->>Loop: AudioCommand::Play(url)  (on backoff due)
    Loop->>Loop: gen=N+1, old gen abandoned
    Loop-->>App: AudioStatus::Connecting
```

### Hardware output recovery

```mermaid
sequenceDiagram
    participant Loop as EngineLoop
    participant Out as OutputManager
    participant App

    Out-->>Loop: EngineEvent::OutputLost
    Loop->>Loop: recovery_retries < limit?
    Loop->>Out: drop + reopen device (preferred name)
    alt reopen ok
        Loop->>Loop: re-attach current generation source
        Loop-->>App: AudioStatus::Connecting -> Playing
    else reopen fails
        Loop-->>App: AudioStatus::Error("Hardware output error: ...")
    end
```

## Components and Interfaces

### Component 1: `AudioEngine` (handle)

**Purpose**: Public, unchanged-facing handle the app holds. Spawns the engine thread and exposes
`send` + `status_rx`.

**Interface**:
```rust
pub struct AudioEngine {
    cmd_tx: mpsc::Sender<AudioCommand>,
    pub status_rx: mpsc::Receiver<AudioStatus>,
}

impl AudioEngine {
    /// Spawn the engine on a dedicated OS thread. Signature preserved.
    pub fn spawn(sample_buffer: Arc<Mutex<VecDeque<f32>>>) -> Self;

    /// Returns false if the engine command channel is closed. Signature preserved.
    pub fn send(&self, cmd: AudioCommand) -> bool;
}
```

**Responsibilities**:
- Own the command channel sender and status receiver.
- Spawn `EngineLoop::run` on an OS thread.
- Stay a pure message-passing facade (no audio logic) so it is trivially testable
  (`disconnected_for_test` is preserved).

### Component 2: `EngineLoop` + `EngineState`

**Purpose**: The single owner of engine state and the only `AudioStatus` emitter.

**Interface**:
```rust
pub(super) struct EngineLoop {
    state: EngineState,
    output: OutputManager,
    supervisor: ConnectionSupervisor,
    options: PlaybackOptions,        // metadata_enabled, target_volume, preferred_device
    volume: VolumeRamp,              // fade in/out, decoupled from state
    status_tx: mpsc::Sender<AudioStatus>,
    event_rx: mpsc::Receiver<EngineEvent>,
    event_tx: mpsc::Sender<EngineEvent>, // cloned into workers + output manager
    sample_buffer: Arc<Mutex<VecDeque<f32>>>,
}

impl EngineLoop {
    pub(super) fn run(
        cmd_rx: mpsc::Receiver<AudioCommand>,
        status_tx: mpsc::Sender<AudioStatus>,
        sample_buffer: Arc<Mutex<VecDeque<f32>>>,
    );

    fn handle_command(&mut self, cmd: AudioCommand);
    fn handle_event(&mut self, event: EngineEvent);
    fn tick(&mut self);              // drives volume ramps + buffering progress only
}
```

**Responsibilities**:
- Apply commands and internal events as the only transition inputs.
- Translate state transitions into `AudioStatus` emissions exactly once per change.
- Delegate device work to `OutputManager` and connection work to `ConnectionSupervisor`.
- Guarantee the run loop cannot panic: each `handle_*` call is total over its input and all worker
  results are `Result`-typed.

### Component 3: `ConnectionSupervisor`

**Purpose**: Own generation ids and the lifecycle of connection/decode workers.

**Interface**:
```rust
pub(super) struct ConnectionSupervisor {
    active_generation: Arc<AtomicU64>,
    current: Generation,
    worker: Option<JoinHandle<()>>,
}

impl ConnectionSupervisor {
    fn next_generation(&mut self) -> Generation;       // bump + store active
    fn spawn(&mut self, req: ConnectRequest, event_tx: mpsc::Sender<EngineEvent>);
    fn abandon(&mut self);                              // store 0, drop handle (non-blocking)
    fn is_active(&self, gen: Generation) -> bool;
}
```

**Responsibilities**:
- Allocate generations and publish the active one atomically.
- Spawn workers that self-cancel when their generation is no longer active.
- Never `join()` on the control thread's hot path (stale workers are detached and self-terminate on
  the next generation check), preserving instant station switching.

### Component 4: `OutputManager`

**Purpose**: Encapsulate cpal/rodio device selection, `Sink` lifecycle, and device recovery.

**Interface**:
```rust
pub(super) struct OutputManager {
    stream: Option<OutputStream>,
    handle: Option<OutputStreamHandle>,
    sink: Option<Sink>,
    preferred_device: Option<String>,
    recovery_retries: u8,
}

impl OutputManager {
    fn ensure_open(&mut self) -> Result<&OutputStreamHandle, EngineError>;
    fn set_preferred_device(&mut self, name: Option<String>); // marks reopen-needed
    fn attach(&mut self, source: DecodedSource) -> Result<(), EngineError>;
    fn set_volume(&mut self, v: f32);
    fn pause(&mut self);
    fn resume(&mut self);
    fn stop(&mut self);
    fn is_sink_drained(&self) -> bool;     // distinguishes natural end from device loss
    fn reopen(&mut self) -> Result<(), EngineError>;
}
```

**Responsibilities**:
- Lazily open the device on first playback (keeps browsing usable with no device), as today.
- Reopen on device change or recovery, honoring the preferred device name.
- Wrap all rodio/cpal calls so a failure becomes `EngineError::Output`, never a panic.
- Preserve the existing native-stderr suppression for ALSA/JACK probe chatter.

### Component 5: `DecodePipeline` + `StreamSource`

**Purpose**: Turn raw bytes into decoded PCM, broadening codec support and isolating ICY demux.

**Interface**:
```rust
pub(super) struct StreamSource<R: Read> {
    inner: R,
    icy: Option<IcyDemux>,            // None when metadata disabled or absent
    generation: Generation,
    active_generation: Arc<AtomicU64>,
    event_tx: mpsc::Sender<EngineEvent>, // for TrackChanged
}

pub(super) struct DecodePipeline;

impl DecodePipeline {
    /// Probe format from a bounded prebuffer, then build a decoded, visualizer-tapped source.
    fn build(
        source: StreamSource<impl Read + 'static>,
        prebuffer: PrebufferConfig,
        sample_buffer: Arc<Mutex<VecDeque<f32>>>,
    ) -> Result<(DecodedSource, StreamFormat), EngineError>;
}
```

**Responsibilities**:
- `StreamSource`: read live bytes, strip ICY metadata on the `metaint` boundary, emit
  `TrackChanged` via the event channel, refuse seeks (live streams are append-only). This is the
  hardened successor to today's `StreamReader`.
- `DecodePipeline`: read a bounded prebuffer into memory, probe the container/codec with Symphonia,
  select MP3 fast-path or general decoder, and wrap the decoded stream in the visualizer tap.
- Surface unsupported/garbage data as `EngineError::Decode` with a stable, classifiable message.

## Data Models

### `AudioCommand` (preserved, one addition)

```rust
#[derive(Debug, Clone)]
pub enum AudioCommand {
    Play(String),
    Pause,
    Resume,
    Stop,
    SetVolume(f32),
    SetOutputDevice(Option<String>),
    SetStreamMetadata(bool),
}
```
**Validation rules**: `SetVolume` is clamped to `[0.0, 1.0]` at the boundary. `Play` URL is trusted
as-is (already validated by capability gating in the app layer). No variants are removed, so
`src/app/*` callers compile unchanged.

### `AudioStatus` (extended, backward compatible)

```rust
#[derive(Debug, Clone)]
pub enum AudioStatus {
    Connecting,
    Buffering { percent: u8 },          // NEW: real prebuffer progress
    Playing,
    Paused,
    Stopped,
    FadingOut { current_volume: f32 },
    TrackChanged { url: String, title: String },
    Error(String),
}
```
**Validation rules**: `Buffering.percent` ∈ `[0, 100]`; `FadingOut.current_volume` ∈ `[0.0, 1.0]`.
`Buffering` is additive — the app's existing `poll_audio_status` match must add one arm (mapped to
the existing `Connecting`/buffer diagnostics fields), but no existing arm changes meaning. Error
strings keep the existing classifiable prefixes (`Hardware output error:`, `Decode error:`,
`Connection failed:`, `HTTP `) so `classify_playback_error` keeps working.

### Internal: `EngineState`

```rust
pub(super) enum EngineState {
    Idle,
    Connecting { generation: Generation, url: String },
    Buffering  { generation: Generation, url: String, percent: u8 },
    Playing    { generation: Generation, url: String },
    Paused     { generation: Generation, url: String },
    Recovering { generation: Generation, url: String, retries: u8 }, // hardware reopen
    Failed     { url: Option<String>, error: EngineError },
}
```
**Validation rules**: exactly one of these is active at a time. `generation` is strictly increasing.
`Recovering.retries <= MAX_HARDWARE_RECOVERY_RETRIES`.

### Internal: `EngineEvent` (worker/output -> loop)

```rust
pub(super) enum EngineEvent {
    Buffering   { generation: Generation, percent: u8 },
    Connected   { generation: Generation, source: DecodedSource, format: StreamFormat },
    TrackChanged{ generation: Generation, title: String },
    StreamEnded { generation: Generation, reason: EndReason }, // Network | Eof | Decode | Abandoned
    OutputLost,
    Failed      { generation: Generation, error: EngineError },
}
```

### Internal: `EngineError` (single classified error type)

```rust
pub(super) enum EngineError {
    Connect(String),   // DNS/TCP/TLS/connect-timeout
    Http(u16),         // non-success status
    Decode(String),    // probe/codec/corrupt data
    Output(String),    // device open / sink / cpal
    Abandoned,         // stale generation, not user-visible
}

impl EngineError {
    fn to_status_string(&self) -> String; // stable, classifiable prefixes
    fn is_recoverable_output(&self) -> bool;
}
```

### Internal: `PrebufferConfig`

```rust
pub(super) struct PrebufferConfig {
    min_bytes: usize,        // e.g. 32 KiB before probe
    max_bytes: usize,        // hard cap to bound memory + startup latency
    fill_timeout: Duration,  // give up -> EngineError::Connect (no infinite Connecting)
}
```
**Validation rules**: `0 < min_bytes <= max_bytes`; `fill_timeout` is finite and small (e.g. 8s) so
the engine can never sit in `Connecting`/`Buffering` forever — a core resilience guarantee.

## Codec Capability Policy (revised)

`src/audio/capability.rs` keeps its `PlaybackCapability { Supported, Unknown, Unsupported }` shape
and public `codec_capability` function, but the matrix changes to reflect the new decode pipeline:

| Codec | v0.4.7 | v0.5.0 target | Notes |
|-------|--------|---------------|-------|
| MP3   | Supported | Supported | Fast path retained |
| AAC / HE-AAC | Unsupported | Supported | Symphonia AAC |
| OGG/Vorbis | Unsupported | Supported | Symphonia |
| Opus  | Unsupported | Supported | Symphonia |
| FLAC  | Unsupported | Supported | Symphonia |
| WAV   | Unsupported | Supported | Symphonia |
| HLS / M3U8 | Unsupported | Unsupported | Needs playlist/segment fetcher — out of scope for 0.5.0 |
| missing / unknown | Unknown (try) | Unknown (try) | Probe decides at runtime |

**Validation rule**: the capability table and the set of decoders actually registered in
`DecodePipeline` must agree. A property test asserts that every `Supported` codec has a working
decode path and HLS stays `Unsupported`.

## Algorithmic Pseudocode

### Main control loop

```rust
fn run(cmd_rx, status_tx, sample_buffer) {
    let mut engine = EngineLoop::new(status_tx, sample_buffer);
    loop {
        // 1. Drain commands (highest priority, bounded per tick)
        match cmd_rx.recv_timeout(POLL_INTERVAL) {
            Ok(cmd)                 => engine.handle_command(cmd),
            Err(Timeout)            => {}
            Err(Disconnected)       => break, // UI gone: clean shutdown
        }
        // 2. Drain all pending internal events (non-blocking)
        while let Ok(ev) = engine.event_rx.try_recv() {
            engine.handle_event(ev);
        }
        // 3. Advance time-based concerns only (volume ramp, buffering UI)
        engine.tick();
    }
    engine.shutdown(); // stop sink, abandon generation, drop device
}
```
**Preconditions**: channels are open at spawn; `sample_buffer` is a valid shared handle.
**Postconditions**: on `Disconnected`, all audio resources are released and the thread exits; no
panic escapes `run`.
**Loop invariants**:
- Exactly one `EngineState` is active between iterations.
- Every emitted `AudioStatus` corresponds to a real state transition this iteration (no duplicate
  spam).
- Any in-flight worker's generation is `<=` the active generation.

### Command handling (transition table excerpt)

```rust
fn handle_command(&mut self, cmd: AudioCommand) {
    match (&self.state, cmd) {
        // Play from any state: bump generation, abandon old work, start fresh.
        (_, Play(url)) => {
            let gen = self.supervisor.next_generation();
            self.output.stop();                 // immediate, fade handled separately if playing
            self.transition(Connecting { gen, url: url.clone() });
            self.supervisor.spawn(ConnectRequest::new(gen, url, &self.options),
                                  self.event_tx.clone());
        }
        (Playing { .. }, Pause)  => { self.output.pause();  self.transition(Paused {..}); }
        (Paused  { .. }, Resume) => { self.output.resume(); self.volume.begin_fade_in();
                                      self.transition(Playing {..}); }
        (_, Stop)                => { self.supervisor.abandon(); self.output.stop();
                                      self.transition(Idle); }       // emits Stopped
        (_, SetVolume(v))        => { self.options.target_volume = v.clamp(0.0,1.0);
                                      self.volume.retarget(self.options.target_volume); }
        (_, SetOutputDevice(d))  => { self.options.preferred_device = normalize(d);
                                      self.output.set_preferred_device(self.options.preferred_device.clone()); }
        (_, SetStreamMetadata(e))=> { self.options.metadata_enabled = e; }
        _ => {} // command not meaningful in this state: ignored, never panics
    }
}
```
**Preconditions**: `self.state` is valid.
**Postconditions**: state is valid; at most one new worker spawned; idempotent for no-op pairs
(e.g. `Pause` while already paused is a no-op).
**Loop invariant (transition totality)**: `handle_command` is defined for every
`(state, command)` pair — the trailing `_ => {}` guarantees totality, eliminating the class of bugs
where an unexpected command in an unexpected state corrupts flags.

### Event handling (worker results)

```rust
fn handle_event(&mut self, ev: EngineEvent) {
    // Drop anything from a stale generation up front.
    if let Some(gen) = ev.generation() {
        if !self.supervisor.is_active(gen) { return; } // Abandoned, silent
    }
    match ev {
        Buffering { percent, .. }      => self.transition(Buffering { percent, .. })
                                          .emit(AudioStatus::Buffering { percent }),
        Connected { source, .. }       => {
            match self.output.attach(source) {
                Ok(())  => { self.volume.begin_fade_in(); self.transition(Playing {..}); }
                Err(e)  => self.fail_or_recover(e),
            }
        }
        TrackChanged { title, .. }     => self.emit(AudioStatus::TrackChanged { url, title }),
        StreamEnded { reason, .. }     => match reason {
            EndReason::Abandoned => {}                    // ignore
            EndReason::Eof       => self.transition(Idle),// natural end -> Stopped
            EndReason::Network|EndReason::Decode =>
                self.fail(EngineError::from(reason)),     // app owns reconnect/backoff
        },
        OutputLost                     => self.try_recover_output(),
        Failed { error, .. }           => self.fail_or_recover(error),
    }
}
```
**Preconditions**: event came from the internal channel.
**Postconditions**: stale-generation events have no observable effect; recoverable output errors
attempt at most `MAX_HARDWARE_RECOVERY_RETRIES` reopens before emitting `Error`.
**Loop invariant**: `Abandoned` results never change user-visible state.

### Bounded prebuffer + probe (worker)

```rust
fn worker_main(req: ConnectRequest, event_tx, sample_buffer) {
    guard_active(req.gen)?;                          // -> StreamEnded{Abandoned} on stale
    let resp = connect(req.url, req.opts)            // connect_timeout, finite
                   .map_err(EngineError::Connect)?;
    if !resp.ok() { fail(Http(resp.status)); return; }

    let metaint = parse_icy_metaint(&resp, req.opts.metadata_enabled);
    let src = StreamSource::new(resp, metaint, req.gen, active_gen, event_tx.clone());

    // Fill bounded prebuffer, reporting progress; bail on timeout.
    let mut pre = Vec::with_capacity(req.pre.min_bytes);
    let start = Instant::now();
    while pre.len() < req.pre.min_bytes {
        guard_active(req.gen)?;
        if start.elapsed() > req.pre.fill_timeout { fail(Connect("prebuffer timeout")); return; }
        let n = src.read_into(&mut pre, req.pre.max_bytes)?;     // network read
        if n == 0 { break; }                                     // short stream
        emit_buffering(event_tx, req.gen, percent(pre.len(), req.pre.min_bytes));
    }

    // Probe + build decoder over (prebuffer ++ remaining stream).
    let (decoded, fmt) = DecodePipeline::build(chain(pre, src), req.pre, sample_buffer)
                            .map_err(|e| { fail(e); })?;
    event_tx.send(Connected { gen: req.gen, source: decoded, format: fmt });
}
```
**Preconditions**: `req.gen` was the active generation at spawn time.
**Postconditions**: emits exactly one terminal event for its generation (`Connected`, `Failed`, or
`StreamEnded{Abandoned}`); never blocks past `fill_timeout`; never sends after becoming stale
(checked before each emit).
**Loop invariants**:
- `pre.len() <= max_bytes` (memory bound).
- Buffering percent is monotonically non-decreasing for a generation.
- Each iteration re-checks `guard_active`, so a `Stop`/new `Play` cancels promptly.

## Key Functions with Formal Specifications

### `EngineLoop::transition`

```rust
fn transition(&mut self, next: EngineState) -> &mut Self;
```
**Preconditions**: `next` is a state reachable from `self.state` per the transition table.
**Postconditions**: `self.state == next`; emits the `AudioStatus` mapped to `next` iff the
user-visible projection of the state changed (Connecting/Buffering/Playing/Paused/Stopped/Error).
No status is emitted for internal-only differences (e.g. generation bump within `Connecting`).
**Loop invariants**: N/A (no loop).

### `ConnectionSupervisor::is_active`

```rust
fn is_active(&self, gen: Generation) -> bool;
```
**Preconditions**: none.
**Postconditions**: returns `gen == active_generation.load(SeqCst)`; pure read, no mutation.
**Loop invariants**: N/A.

### `OutputManager::is_sink_drained`

```rust
fn is_sink_drained(&self) -> bool;
```
**Preconditions**: a sink may or may not exist.
**Postconditions**: returns `true` only when a sink exists and reports empty due to natural source
end — distinguished from device loss, which arrives as `EngineEvent::OutputLost`. This removes the
old ambiguity where `sink.empty()` could mean either "track ended" or "device died."
**Loop invariants**: N/A.

### `StreamSource::read` (ICY-aware)

```rust
fn read(&mut self, buf: &mut [u8]) -> io::Result<usize>;
```
**Preconditions**: `buf.len() > 0`.
**Postconditions**: returns audio bytes only (ICY metadata blocks are consumed and never written
into `buf`); on the `metaint` boundary, parses `StreamTitle` and emits `TrackChanged`; returns an
error (not partial audio) if the stream ends inside a metadata block; returns
`io::Error::other("Abandoned")` immediately if the generation is no longer active.
**Loop invariants** (internal read-exact for metadata): `filled` strictly increases until
`filled == len`, or an error is returned; never returns audio bytes as metadata or vice versa.

## Example Usage

```rust
// App side: unchanged construction and command sending.
let sample_buffer = Arc::new(Mutex::new(VecDeque::with_capacity(4096)));
let engine = AudioEngine::spawn(sample_buffer.clone());

engine.send(AudioCommand::SetOutputDevice(Some("BlueZ Headphones".into())));
engine.send(AudioCommand::SetVolume(0.8));
engine.send(AudioCommand::Play("https://example.com/stream".into()));

// App tick: drain status (one new arm for Buffering).
while let Ok(status) = engine.status_rx.try_recv() {
    match status {
        AudioStatus::Connecting          => { /* state = Connecting */ }
        AudioStatus::Buffering { percent }=> { /* diagnostics.buffer_percent = percent */ }
        AudioStatus::Playing             => { /* state = Playing; mark_station_success */ }
        AudioStatus::Paused              => { /* state = Paused */ }
        AudioStatus::Stopped             => { /* handle_audio_stopped */ }
        AudioStatus::FadingOut { .. }    => { /* state = FadingOut */ }
        AudioStatus::TrackChanged { .. } => { /* handle_track_changed */ }
        AudioStatus::Error(e)            => { /* classify + arm app reconnect backoff */ }
    }
}
```

## Correctness Properties

*A property is a characteristic or behavior that should hold true across all valid executions of a
system — essentially, a formal statement about what the system should do. Properties serve as the
bridge between human-readable specifications and machine-verifiable correctness guarantees.*

These are written as universally-quantified statements to seed property-based and regression tests
(the project uses `proptest`). `∀` ranges over arbitrary command/event sequences unless noted.

### Property 1: No-panic / liveness

∀ finite sequences of `AudioCommand`, `EngineLoop::run` never panics and always returns to polling
within a bounded time. (No blocking network/decode on the control thread; all `(state, command)` and
`(state, event)` pairs have a defined, non-panicking outcome.)

**Validates: Requirements 5.1, 5.2, 5.3, 5.4, 3.5, 3.6**

### Property 2: Single active state

∀ reachable points between loop iterations, exactly one `EngineState` variant is active.

**Validates: Requirements 3.1, 3.2**

### Property 3: Generation strict monotonicity and stale-event isolation

∀ two `Play` commands p1 before p2, `gen(p1) < gen(p2)`; and ∀ events with
`gen < active_generation`, the event produces no observable `AudioStatus` change.

**Validates: Requirements 4.1, 4.3**

### Property 4: Stop is terminal-and-clean

∀ states, `AudioCommand::Stop` leads to `EngineState::Idle` and emits exactly one
`AudioStatus::Stopped`; no worker from a prior generation can later move the engine out of `Idle`.

**Validates: Requirements 3.3, 11.5**

### Property 5: Bounded connecting

∀ unreachable/hanging URLs, the engine leaves `Connecting`/`Buffering` within
`fill_timeout + connect_timeout` and emits `AudioStatus::Error`, never waiting indefinitely.

**Validates: Requirements 6.2, 6.3**

### Property 6: No internal retry storm

∀ sequences of output-loss events, the engine performs at most `MAX_HARDWARE_RECOVERY_RETRIES`
automatic output reopens per generation and performs zero automatic network reconnects (network
backoff is owned solely by the app's `Reconnect`).

**Validates: Requirements 9.3, 11.5**

### Property 7: ICY safety

∀ byte streams with valid `metaint`, the audio bytes delivered to the decoder equal the input stream
with exactly the metadata blocks removed; no metadata byte is ever written into the audio output
buffer.

**Validates: Requirements 8.1, 8.3**

### Property 8: Volume clamp

∀ `SetVolume(v)` where `v` is any `f32` (including `NaN`, `inf`, `-inf`), the applied and reported
volume ∈ `[0.0, 1.0]`.

**Validates: Requirements 10.6**

### Property 9: Capability/decoder agreement

∀ codecs marked `PlaybackCapability::Supported`, `DecodePipeline` has a registered decoder path;
`HLS`/`M3U8` remain `Unsupported`; this invariant holds as a table-driven test over the complete
capability matrix.

**Validates: Requirements 7.3, 7.4, 7.7**

### Property 10: Status classifiability

∀ `AudioStatus::Error(e)` emitted by the engine, `classify_playback_error(e)` returns a non-`Unknown`
kind for output (`"Hardware output error:"`), decode (`"Decode error:"`), HTTP (`"HTTP "`), network
(`"Connection failed:"`), and timeout (`"timeout"`) causes.

**Validates: Requirements 12.1, 12.2, 12.3, 12.4, 12.5, 12.6, 12.7**

### Property 11: Sample tap is passive

∀ playback, a failed `try_lock` on the visualizer sample buffer drops the batch rather than
stalling the decode path; the audio thread is never blocked waiting for the UI to release the lock.

**Validates: Requirements 13.2**

### Property 12: Buffering percent is bounded

∀ prebuffer fill progress updates emitted by a worker, the `percent` field ∈ `[0, 100]`.

**Validates: Requirements 2.3**

### Property 13: Prebuffer memory is bounded

∀ stream read sequences, the bytes accumulated in the prebuffer never exceed
`PrebufferConfig::max_bytes`.

**Validates: Requirements 6.4**

### Property 14: Stale StreamSource read is immediately abandoned

∀ `StreamSource` instances whose generation is no longer active, the next call to `read` returns
`io::Error::other("Abandoned")` without performing any further network I/O.

**Validates: Requirements 8.5**

## Error Handling

### Network connect/read failures
**Condition**: DNS/TCP/TLS failure, connect timeout, or mid-stream read error.
**Response**: worker emits `Failed`/`StreamEnded{Network}`; loop transitions to `Failed`, emits
`AudioStatus::Error("Connection failed: ...")`.
**Recovery**: the app's `Reconnect` arms its backoff (unchanged). The engine does **not** auto-retry
network errors — this is the explicit fix for "bad retry loops."

### HTTP non-success
**Condition**: response status not 2xx.
**Response**: `EngineError::Http(status)` → `AudioStatus::Error("HTTP {status}")`.
**Recovery**: app-side, same as network.

### Decode / unsupported data
**Condition**: probe fails or codec unsupported by registered decoders.
**Response**: `AudioStatus::Error("Decode error: ...")`.
**Recovery**: app surfaces "search alternatives" hint (existing `playback_error_action_hint`).

### Hardware output loss
**Condition**: device unplugged, sink creation fails, cpal stream error.
**Response**: `EngineEvent::OutputLost`/`EngineError::Output`.
**Recovery**: `OutputManager.reopen()` up to `MAX_HARDWARE_RECOVERY_RETRIES`, re-attaching the
current generation's source; on exhaustion emit `AudioStatus::Error("Hardware output error: ...")`.

### Prebuffer timeout
**Condition**: stream connects but never delivers enough bytes within `fill_timeout`.
**Response**: `EngineError::Connect("prebuffer timeout")` → `Error`.
**Recovery**: app-side reconnect. Prevents the historical "stuck Connecting" regression.

### Poisoned mutex / worker panic
**Condition**: a worker thread panics or the sample-buffer mutex is poisoned.
**Response**: `join` results are inspected; a panicked worker maps to
`AudioStatus::Error("Connection thread panicked")`; poisoned locks are recovered via
`into_inner()` (as the output module already does for its stderr lock).
**Recovery**: the control thread continues running; one generation's failure never tears down the
engine.

## Testing Strategy

### Unit testing
- State machine transition table: table-driven tests over every `(state, command)` and
  `(state, event)` pair, asserting resulting state and emitted status.
- `OutputManager` device-name normalization, reopen bookkeeping, drained-vs-lost distinction.
- `StreamSource` ICY demux: exact-boundary reads, EOF inside metadata, seek refusal, abandon-on-
  stale-generation (port and extend existing `stream_reader` tests).
- `EngineError` → status-string prefix stability (locks the `classify_playback_error` contract).
- Capability matrix vs registered decoders.

### Property-based testing
**Library**: `proptest` (idiomatic Rust PBT; integrates with `cargo test`).
- Property 1 (no-panic): generate random `Vec<AudioCommand>`, drive a loop built over a mock
  transport/output, assert no panic and bounded settle.
- Property 3/4 (generations/stop): random interleavings of `Play`/`Stop` + delayed worker events,
  assert stale events are inert and `Stop` always reaches `Idle`.
- Property 7 (ICY): random `(audio_bytes, metaint, titles)` → assert reconstructed audio equals
  input minus metadata.
- Property 8 (volume clamp): random `f32` volumes (incl. NaN/inf) stay in range.

### Integration testing
- A local in-process HTTP server (e.g. `tiny_http` behind a `#[cfg(test)]` dep) serving canned MP3
  and an OGG/FLAC sample to validate the probe path end to end, plus a server that stalls to verify
  the prebuffer timeout, and one that drops mid-stream to verify clean `Error` + app reconnect.
- Mock `Transport` and `Output` traits so the control loop is testable without real sockets/devices
  (the abstraction boundary is the key enabler of "clean, testable boundaries").

## Performance Considerations

- **Startup latency vs stutter**: the historical reason for bypassing Symphonia probing was slow/
  stuttering startup. The bounded prebuffer (`min_bytes`, `fill_timeout`) plus an MP3 fast-path
  (skip full probe when ICY/codec metadata already says MP3) keeps startup snappy while enabling
  general codec support.
- **Memory bound**: prebuffer is capped by `max_bytes`; the visualizer buffer keeps its existing
  4096-sample cap.
- **No busy loops**: the control loop polls with a timeout and otherwise sleeps in `recv_timeout`;
  workers block on I/O, not spin.

## Security Considerations

- Stream URLs come from the user's library / Radio Browser; treat response bytes as untrusted.
  Decoders run on bounded buffers and all decode errors are caught — malformed audio cannot crash
  the engine.
- HTTP client keeps a finite `connect_timeout`; no following of redirects to non-http(s) schemes.
- ICY metadata is parsed defensively (length-prefixed, bounded) and only the `StreamTitle` field is
  extracted; never executed or used for filesystem paths.

## Dependencies

- **Retained**: `rodio` 0.20 (Sink + cpal output, OutputStream), `cpal` 0.15 (device enumeration),
  `reqwest` 0.12 blocking client, `libc` (unix stderr suppression), `anyhow`.
- **Newly relied upon**: Symphonia decoders (already pulled in via rodio's `symphonia-all` feature)
  used through a probe-based decode path rather than `Decoder::new_mp3` only.
- **Test-only (new)**: `proptest` for property tests; a lightweight in-process HTTP server such as
  `tiny_http` for integration tests (dev-dependencies).
- **Out of scope for 0.5.0**: HLS/segment fetching (would add a playlist parser + segment client);
  tracked as future work, capability stays `Unsupported`.
