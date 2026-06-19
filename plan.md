# PulseDeck 0.4.0 Implementation Plan

Release theme: **Playback Confidence, Search Confidence, UI Explainability**.

PulseDeck 0.4.0 should make focused internet radio feel stable, inspectable, and recoverable. This is not the release for recording, local tape playback, plugins, accounts, podcasts, cloud sync, or a broad media-suite turn. The app should stay small, sharp, and radio-first.

Current branch observed during planning:

```text
feature/0.3.0-radio-prefixes...origin/feature/0.3.0-radio-prefixes
```

Recent groundwork already implemented in this workspace:

```text
src/favorites.rs          Settings::stream_metadata_enabled, default true
src/app/types.rs          SettingRow::StreamMetadata
src/app/settings.rs       toggles and syncs stream metadata
src/app/lifecycle.rs      syncs stream metadata on startup
src/audio.rs              AudioCommand::SetStreamMetadata(bool)
src/audio/engine_loop.rs  forwards metadata setting into ConnectionContext
src/audio/session.rs      requests/parses ICY metadata only when enabled
src/ui/settings.rs        renders Stream Song Info Metadata row
```

Validation after metadata groundwork:

```text
cargo test                 272 passed
cargo clippy --all-targets passed
```

Important merge caveat: this branch had a mismatch where `favorites.rs` and `radio.rs` expected richer Radio Browser helpers that were missing in `src/radio/query.rs` and `src/radio/station.rs`. These were restored here. Before merging into a 0.4.0 branch, verify the intended base already contains or receives:

```rust
has_unknown_prefix
prefix_examples_inline
normalize_codec
normalize_country_code
normalize_station_uuid
sanitize_bitrate
Station::enrich_from
```

---

## Release principles

### Preserve

- Focused terminal public-radio playback.
- No API keys.
- No cloud dependency.
- No fake file semantics for live streams.
- Optional song metadata, never mandatory for playback.
- Simple search for casual users, powerful prefixes for advanced users.
- Compact terminal protection.
- Testable pure helpers wherever possible.

### Avoid

- Reintroducing a separate decoded-PCM playback queue.
- Reintroducing fake forward seeking that consumes live stream bytes.
- Treating ICY metadata as the audio problem.
- Turning settings into a giant cockpit.
- Letting docs describe abandoned implementation attempts.

---

## Phase 0: repository cleanup

### Goal

Remove stale experiments so future audio work starts from a clear map instead of a haunted drawer.

### Remove unused files

These files are not included by `src/audio.rs` and should be removed from the repository:

```text
src/audio/decoded_source.rs
src/audio/pcm_buffer.rs
src/audio/pcm_buffer2.rs
```

Keep this file, `plan.md`, as the 0.4.0 implementation plan.

Do not remove:

```text
src/radio/
```

That directory is intentional and is wired through `src/radio.rs`.

### Verify module graph

`src/audio.rs` should only include active audio modules:

```rust
mod buffer;
mod buffer_meter;
mod engine_loop;
mod metadata;
mod output;
mod session;
mod stream_reader;
mod visualizer;
```

`src/radio.rs` should include:

```rust
mod client;
mod map;
mod query;
mod rank;
mod station;
```

### Tests

Run after cleanup:

```text
cargo test
cargo clippy --all-targets
```

---

## Phase 1: Stream Song Info Metadata setting

### Status

Already implemented as groundwork. Keep it in 0.4.0 and document it.

### Behavior

Default is on.

When enabled:

- `src/audio/session.rs` sends `Icy-MetaData: 1`.
- `icy-metaint` response header is parsed.
- `StreamReader` strips metadata blocks from audio bytes.
- `AudioStatus::TrackChanged` updates current track, recent tracks, saved history, and notifications.

When disabled:

- No metadata request header is sent.
- `metaint` is `None`.
- Playback receives clean stream bytes only.

### Important files

```text
src/favorites.rs
src/app/types.rs
src/app/settings.rs
src/app/lifecycle.rs
src/audio.rs
src/audio/engine_loop.rs
src/audio/session.rs
src/audio/stream_reader.rs
src/ui/settings.rs
```

### Current connection path

```rust
// src/app/lifecycle.rs
app.sync_output_device();
app.sync_stream_metadata();
app.sync_volume();
```

```rust
// src/audio.rs
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

```rust
// src/audio/session.rs
let mut request = client.get(url);
if context.request_stream_metadata {
    request = request.header("Icy-MetaData", "1");
}

let metaint = if context.request_stream_metadata {
    response
        .headers()
        .get("icy-metaint")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<usize>().ok())
} else {
    None
};
```

### Follow-up tests to add

Add direct settings serialization tests in `src/favorites.rs`:

```rust
#[test]
fn settings_default_enables_stream_metadata() {
    assert!(Settings::default().stream_metadata_enabled);
}

#[test]
fn settings_deserializes_missing_stream_metadata_as_enabled() {
    let json = r#"{"notifications_enabled":true}"#;
    let settings: Settings = serde_json::from_str(json).unwrap();
    assert!(settings.stream_metadata_enabled);
}
```

Pitfall: do not call this setting only “metadata” in UI. Users may confuse Radio Browser station metadata with ICY song-title metadata. Preferred UI label:

```text
Stream Song Info Metadata
```

---

## Phase 2: Playback Doctor overlay

### Goal

Add a diagnostic overlay that explains current playback state and recovery options.

### User experience

Open with `d` in normal mode.

Example overlay:

```text
Playback Doctor

State: Playing
Station: SomaFM: Groove Salad
Track: Tycho - Awake
URL: https://ice2.somafm.com/groovesalad-128-mp3
Output: Default
Song info metadata: On
Decoder: Playing
Buffer: 68% / 4s
Reconnects: 0 / 3
Last event: Playback started
Last error: N/A
Last recovery: N/A

Actions: r retry  s stop  , output  / search  Esc close
```

### New app state

File: `src/app/types.rs`

```rust
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PlaybackDiagnostics {
    pub output_device: String,
    pub metadata_enabled: bool,
    pub reconnect_attempts: u8,
    pub reconnect_limit: u8,
    pub buffer_percent: u8,
    pub buffer_seconds: u32,
    pub last_event: Option<String>,
    pub last_error: Option<String>,
    pub last_recovery: Option<String>,
    pub decoder_state: DecoderState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecoderState {
    Idle,
    Connecting,
    Probing,
    Playing,
    Ended,
    Failed,
}

impl Default for DecoderState {
    fn default() -> Self {
        Self::Idle
    }
}
```

File: `src/app.rs`

```rust
pub diagnostics: PlaybackDiagnostics,
```

File: `src/app/lifecycle.rs`

Initialize in `App::new`:

```rust
diagnostics: PlaybackDiagnostics::default(),
```

Update in `poll_audio_status()`:

```rust
AudioStatus::BufferLevel { percent, seconds } => {
    self.player.buffer_percent = percent;
    self.player.buffer_seconds = seconds;
    self.diagnostics.buffer_percent = percent;
    self.diagnostics.buffer_seconds = seconds;
}
AudioStatus::Connecting => {
    self.player.current_track = None;
    self.player.state = PlaybackState::Connecting;
    self.diagnostics.decoder_state = DecoderState::Connecting;
    self.diagnostics.last_event = Some("Connecting to stream".to_string());
}
AudioStatus::Playing => {
    self.player.state = PlaybackState::Playing;
    self.reconnect.disarm();
    self.diagnostics.decoder_state = DecoderState::Playing;
    self.diagnostics.last_event = Some("Playback started".to_string());
}
AudioStatus::Error(error) => {
    self.diagnostics.decoder_state = DecoderState::Failed;
    self.diagnostics.last_error = Some(error.clone());
    self.handle_audio_error(error);
}
```

### Optional audio diagnostics event

File: `src/audio.rs`

```rust
#[derive(Debug, Clone)]
pub enum AudioStatus {
    Playing,
    Paused,
    Stopped,
    Error(String),
    Connecting,
    FadingOut { current_volume: f32 },
    TrackChanged { url: String, title: String },
    BufferLevel { percent: u8, seconds: u32 },
    Diagnostic(AudioDiagnostic),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioDiagnostic {
    OutputSelected { name: String },
    StreamConnected { url: String },
    DecoderProbing,
    DecoderReady,
    HardwareRecoveryAttempt { attempt: u8, limit: u8 },
    MetadataMode { enabled: bool },
}
```

Pitfall: diagnostics must never be required for playback. If a diagnostic send fails, ignore it.

### Overlay routing

Add `PlaybackDoctor` to the active overlay enum, likely in `src/app/overlays.rs` or `src/app/types.rs`, depending where `ActiveOverlay` lives.

Add action:

```rust
// src/action.rs
TogglePlaybackDoctor,
```

Map key:

```rust
// src/event.rs, normal mode
KeyCode::Char('d') => Some(Action::TogglePlaybackDoctor),
```

Pitfall: `d` may already mean directional forward inside settings. That is fine if routed only in normal mode.

### UI module

Create:

```text
src/ui/playback_doctor.rs
```

Register in `src/ui/mod.rs`:

```rust
pub mod playback_doctor;
```

Render in overlay match:

```rust
ActiveOverlay::PlaybackDoctor => playback_doctor::render(frame, size, app),
```

Skeleton:

```rust
use crate::app::{App, PlaybackState};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use super::theme;

const MIN_DOCTOR_WIDTH: u16 = 64;
const MIN_DOCTOR_HEIGHT: u16 = 18;

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let popup_area = super::centered_rect(64, 72, area);
    frame.render_widget(Clear, popup_area);

    if popup_area.width < MIN_DOCTOR_WIDTH || popup_area.height < MIN_DOCTOR_HEIGHT {
        super::render_boundary_warning(
            frame,
            popup_area,
            "Playback Doctor Too Compact",
            format!("Expand terminal or close doctor (overlay: {}x{})", popup_area.width, popup_area.height),
        );
        return;
    }

    let block = Block::default()
        .title(Span::styled(" Playback Doctor ", theme::title()))
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(Style::default().fg(theme::highlight()))
        .style(theme::clear());

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let station = app.now_playing().map(|s| s.name.as_str()).unwrap_or("N/A");
    let url = app.player.playing_url.as_deref().unwrap_or("N/A");
    let track = app.player.current_track.as_deref().unwrap_or("N/A");

    let lines = vec![
        row("State", playback_state_label(&app.player.state)),
        row("Station", station),
        row("Track", track),
        row("URL", url),
        row("Buffer", &format!("{}% / {}s", app.player.buffer_percent, app.player.buffer_seconds)),
        row("Metadata", if app.library.settings.stream_metadata_enabled { "On" } else { "Off" }),
        row("Last error", app.diagnostics.last_error.as_deref().unwrap_or("N/A")),
        Line::from(""),
        Line::from(vec![
            Span::styled(" r ", theme::cyan()), Span::raw("retry  "),
            Span::styled(" s ", theme::cyan()), Span::raw("stop  "),
            Span::styled(" , ", theme::cyan()), Span::raw("output  "),
            Span::styled(" / ", theme::cyan()), Span::raw("search  "),
            Span::styled(" Esc ", theme::cyan()), Span::raw("close"),
        ]),
    ];

    frame.render_widget(Paragraph::new(lines).style(theme::clear()), inner);
}

fn row(label: &'static str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:>12}: "), theme::dim()),
        Span::styled(value.to_string(), theme::text()),
    ])
}

fn playback_state_label(state: &PlaybackState) -> &'static str {
    match state {
        PlaybackState::Stopped => "Stopped",
        PlaybackState::Connecting => "Connecting",
        PlaybackState::Playing => "Playing",
        PlaybackState::FadingOut { .. } => "Fading out",
        PlaybackState::Paused => "Paused",
        PlaybackState::Error(_) => "Error",
    }
}
```

Pitfall: if `app.now_playing()` is not public to UI modules, expose an existing selector through `src/app/selectors.rs` instead of duplicating station lookup.

### Tests

- `event::tests::normal_mode_d_opens_playback_doctor`
- `app::overlays::tests::playback_doctor_is_mutually_exclusive`
- `ui::playback_doctor::tests::doctor_overlay_rejects_tiny_area`
- `ui::playback_doctor::tests::playback_state_label_formats_all_states`

---

## Phase 3: actionable playback errors

### Goal

Error messages should guide recovery. Different failures need different next actions.

### Error classification

Create:

```text
src/app/playback_error.rs
```

Register in `src/app.rs` or module tree:

```rust
mod playback_error;
pub use playback_error::{classify_playback_error, playback_error_action_hint, PlaybackErrorKind};
```

Implementation:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackErrorKind {
    Network,
    Http,
    Decode,
    Output,
    Timeout,
    Unknown,
}

pub fn classify_playback_error(error: &str) -> PlaybackErrorKind {
    let lower = error.to_ascii_lowercase();
    if lower.contains("soundcard") || lower.contains("hardware output") || lower.contains("sink error") {
        PlaybackErrorKind::Output
    } else if lower.contains("decode") || lower.contains("unsupported") {
        PlaybackErrorKind::Decode
    } else if lower.contains("http ") {
        PlaybackErrorKind::Http
    } else if lower.contains("timeout") || lower.contains("timed out") {
        PlaybackErrorKind::Timeout
    } else if lower.contains("connection") || lower.contains("network") {
        PlaybackErrorKind::Network
    } else {
        PlaybackErrorKind::Unknown
    }
}

pub fn playback_error_action_hint(error: &str) -> &'static str {
    match classify_playback_error(error) {
        PlaybackErrorKind::Output => "r retry output  , choose device  s stop",
        PlaybackErrorKind::Decode => "r retry  / search alternatives  s stop",
        PlaybackErrorKind::Http | PlaybackErrorKind::Network | PlaybackErrorKind::Timeout => {
            "r retry  / search alternatives  d inspect"
        }
        PlaybackErrorKind::Unknown => "r retry  d inspect  s stop",
    }
}
```

### UI integration

Files likely involved:

```text
src/ui/controls.rs
src/ui/header.rs
src/ui/critical.rs
```

Wherever `PlaybackState::Error(e)` is rendered, append or replace generic hints with:

```rust
let hint = crate::app::playback_error_action_hint(error);
```

Pitfall: keep the hint short and truncate on compact widths. Use `src/ui/text.rs` helpers if available.

### Tests

- output errors classify as `Output`.
- decode errors classify as `Decode`.
- HTTP errors classify as `Http`.
- timeout and connection errors classify separately.
- footer/critical hint remains compact.

---

## Phase 4: initial probe replay buffer

### Goal

Improve decoder compatibility without bringing back fake seek.

0.3.1 fixed a major bug by refusing to seek through live audio. For 0.4.0, support limited decoder probe rewinds inside an initial byte window.

### New module

Create:

```text
src/audio/probe_reader.rs
```

Register in `src/audio.rs`:

```rust
mod probe_reader;
```

### Rules

- Buffer first 256 KiB of stream bytes.
- Allow `SeekFrom::Start(n)` only when `n <= buffered_len`.
- Allow `SeekFrom::Current(0)` as position report.
- Allow small seeks only inside replay buffer.
- Refuse `SeekFrom::End`.
- Refuse seeks beyond buffered bytes.
- Never implement seeking by reading and discarding live bytes.

### Sketch

```rust
use std::io::{Read, Seek, SeekFrom};

const INITIAL_PROBE_BYTES: usize = 256 * 1024;

pub struct ProbeReplayReader<R> {
    inner: R,
    replay: Vec<u8>,
    pos: u64,
}

impl<R: Read> ProbeReplayReader<R> {
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            replay: Vec::with_capacity(INITIAL_PROBE_BYTES),
            pos: 0,
        }
    }
}

impl<R: Read> Read for ProbeReplayReader<R> {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        let replay_len = self.replay.len() as u64;
        if self.pos < replay_len {
            let available = (replay_len - self.pos) as usize;
            let n = available.min(out.len());
            let start = self.pos as usize;
            out[..n].copy_from_slice(&self.replay[start..start + n]);
            self.pos += n as u64;
            return Ok(n);
        }

        let n = self.inner.read(out)?;
        if n > 0 && self.replay.len() < INITIAL_PROBE_BYTES {
            let remaining = INITIAL_PROBE_BYTES - self.replay.len();
            self.replay.extend_from_slice(&out[..n.min(remaining)]);
        }
        self.pos += n as u64;
        Ok(n)
    }
}

impl<R: Read> Seek for ProbeReplayReader<R> {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let target = match pos {
            SeekFrom::Start(n) => n,
            SeekFrom::Current(0) => return Ok(self.pos),
            SeekFrom::Current(offset) if offset < 0 => {
                self.pos.checked_sub(offset.unsigned_abs()).ok_or_else(unsupported_seek)?
            }
            SeekFrom::Current(offset) => self.pos.saturating_add(offset as u64),
            SeekFrom::End(_) => return Err(unsupported_seek()),
        };

        if target <= self.replay.len() as u64 {
            self.pos = target;
            Ok(self.pos)
        } else {
            Err(unsupported_seek())
        }
    }
}

fn unsupported_seek() -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "live radio stream can only seek inside the initial probe buffer",
    )
}
```

Pitfall: this sketch needs careful review. The invariant is: seeking must never advance the live stream by consuming bytes. It may only replay bytes that were already captured.

### Integration

File: `src/audio/session.rs`

Current:

```rust
let reader = StreamReader::new(...);
let buffered_reader = BufReader::with_capacity(DECODER_READ_BUFFER_SIZE, reader);
let source = Decoder::new(buffered_reader)?;
```

Target:

```rust
let reader = StreamReader::new(...);
let probe_reader = ProbeReplayReader::new(reader);
let buffered_reader = BufReader::with_capacity(DECODER_READ_BUFFER_SIZE, probe_reader);
let source = Decoder::new(buffered_reader)?;
```

### Tests

- replay reader can read bytes, seek to 0, read same bytes again.
- seeking beyond replay returns `Unsupported`.
- `SeekFrom::End` returns `Unsupported`.
- forward seek beyond replay does not consume inner reader bytes.
- integration test preserves old `StreamReader` live seek refusal.

---

## Phase 5: station health memory

### Goal

Saved stations should remember local playback reliability.

### Data model

Prefer nested health struct in `src/radio/station.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct StationHealth {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_success_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_failure_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_count: Option<u32>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub last_error_summary: String,
}

impl StationHealth {
    pub fn is_empty(&self) -> bool {
        self.last_success_at.is_none()
            && self.last_failure_at.is_none()
            && self.failure_count.unwrap_or(0) == 0
            && self.last_error_summary.is_empty()
    }
}
```

In `Station`:

```rust
#[serde(default, skip_serializing_if = "StationHealth::is_empty")]
pub health: StationHealth,
```

Update `Station::basic`:

```rust
health: StationHealth::default(),
```

### Library update helpers

File: `src/favorites.rs`

```rust
impl Library {
    pub fn mark_station_success(&mut self, url: &str, now: String) -> bool {
        if let Some(station) = self.stations.iter_mut().find(|s| normalized_url_match(&s.url, url)) {
            station.health.last_success_at = Some(now);
            station.health.last_error_summary.clear();
            return true;
        }
        false
    }

    pub fn mark_station_failure(&mut self, url: &str, now: String, error: &str) -> bool {
        if let Some(station) = self.stations.iter_mut().find(|s| normalized_url_match(&s.url, url)) {
            station.health.last_failure_at = Some(now);
            station.health.failure_count = Some(station.health.failure_count.unwrap_or(0).saturating_add(1));
            station.health.last_error_summary = compact_error_summary(error);
            return true;
        }
        false
    }
}
```

Pitfall: if URL has been resolved differently since save, URL matching may miss. Later improvement can mark by station UUID if the audio layer carries UUID. For 0.4.0, URL is acceptable.

### UI badges

File: `src/ui/stations.rs`

Add compact labels:

```text
OK
FAIL
NEW
```

Keep visual noise low. Health should be a hint, not a siren.

### Tests

- old libraries without health load.
- success stores timestamp and clears error.
- failure increments count.
- compact row stays inside width.

---

## Phase 6: search result explanations

### Goal

A highlighted result should explain why it ranks well.

### Add explanation model

File: `src/radio/rank.rs`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RankExplanation {
    pub signals: Vec<RankSignal>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RankSignal {
    ExactName,
    ExactTag,
    CountryCode,
    Language,
    Codec,
    LastCheckOk,
    HighVotes,
    HighClicks,
    AlreadySaved,
    Https,
}

pub fn explain_station_match(
    query: &StationSearchQuery,
    station: &Station,
    is_saved: bool,
) -> RankExplanation {
    let mut signals = Vec::new();

    match query.field() {
        SearchField::Tag if station.tags.iter().any(|t| t.eq_ignore_ascii_case(query.value())) => {
            signals.push(RankSignal::ExactTag);
        }
        SearchField::CountryCode if station.country_code.eq_ignore_ascii_case(query.value()) => {
            signals.push(RankSignal::CountryCode);
        }
        SearchField::Language if station.language.eq_ignore_ascii_case(query.value()) => {
            signals.push(RankSignal::Language);
        }
        SearchField::Codec if station.codec.eq_ignore_ascii_case(query.value()) => {
            signals.push(RankSignal::Codec);
        }
        SearchField::Name if station.name.eq_ignore_ascii_case(query.value()) => {
            signals.push(RankSignal::ExactName);
        }
        _ => {}
    }

    if station.last_check_ok == Some(true) {
        signals.push(RankSignal::LastCheckOk);
    }
    if station.url.starts_with("https://") {
        signals.push(RankSignal::Https);
    }
    if is_saved {
        signals.push(RankSignal::AlreadySaved);
    }

    RankExplanation { signals }
}
```

### UI display

File: `src/ui/search.rs`

```rust
fn highlighted_result_explanation(app: &App) -> Option<String> {
    let station = app.search.results.get(app.nav.selected)?;
    let query = StationSearchQuery::parse(&app.search.query);
    let is_saved = app.library.contains_station(station);
    let explanation = crate::radio::explain_station_match(&query, station, is_saved);
    Some(explanation_label(&explanation))
}
```

Example labels:

```text
Exact tag + Last check OK + Saved
Country BA + MP3 + High clicks
```

Pitfall: keep explanation one line. The search list already carries metadata.

### Tests

- exact tag explanation.
- country-code explanation.
- saved explanation.
- explanation label truncates safely.

---

## Phase 7: command palette

### Goal

Make features discoverable without adding permanent UI clutter.

### User story

Press `:` or `Ctrl+p`. Type a command. Press Enter.

Commands:

```text
search stations
retry stream
stop playback
open settings
change theme
toggle song info metadata
open playback doctor
export library
open help
```

### State and actions

File: `src/app/types.rs`

```rust
pub enum InputMode {
    Normal,
    Search,
    SleepTimer,
    CommandPalette,
}
```

File: `src/app/command_palette.rs`

```rust
#[derive(Default)]
pub struct CommandPaletteState {
    pub query: String,
    pub selected: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteCommand {
    SearchStations,
    RetryStream,
    StopPlayback,
    OpenSettings,
    ToggleHelp,
    TogglePlaybackDoctor,
    ToggleHistory,
    ToggleMetadata,
    CycleTheme,
    ExportLibrary,
}

pub fn command_label(cmd: PaletteCommand) -> &'static str { ... }
pub fn command_action(cmd: PaletteCommand) -> Action { ... }
pub fn filtered_commands(query: &str, app: &App) -> Vec<PaletteCommand> { ... }
```

File: `src/action.rs`

```rust
OpenCommandPalette,
CommandPaletteConfirm,
CommandPaletteClose,
CommandPaletteBackspace,
CommandPaletteInput(char),
```

Pitfall: commands like retry should be hidden or disabled when unavailable. Do not let the palette execute nonsense.

### UI

Create:

```text
src/ui/command_palette.rs
```

Render input and filtered list in centered overlay.

### Tests

- opens only in normal mode.
- filters commands case-insensitively.
- Enter executes selected command.
- Esc closes cleanly.
- disabled context commands are not executed.

---

## Phase 8: library metadata refresh

### Goal

Users with older saved stations can enrich metadata without re-adding stations.

### Radio lookup function

File: `src/radio.rs`

```rust
pub async fn lookup_station_metadata(station: &Station) -> anyhow::Result<Option<Station>> {
    // Prefer UUID lookup if supported by client.
    // Fallback to name search.
    // Return best identity match or highest-ranked candidate.
}
```

If Radio Browser UUID endpoint is not already supported, start with name search and conservative matching.

### Library helper already exists

File: `src/favorites.rs`

```rust
pub fn enrich_matching_station(&mut self, station: &Station) -> bool
```

Do not replace user-facing name, URL, or genre.

### Refresh summary

```rust
pub struct MetadataRefreshSummary {
    pub checked: usize,
    pub enriched: usize,
    pub unchanged: usize,
    pub failed: usize,
}
```

Notice:

```text
Metadata refresh: 12 checked, 5 enriched, 6 unchanged, 1 failed
```

Pitfall: do not run this automatically at startup. Make it user-triggered through command palette first.

### Tests

- fills missing metadata.
- preserves name, URL, genre.
- duplicate UUID does not create duplicate station.
- summary counts changed, unchanged, failed.

---

## Phase 9: import preview

### Goal

Make import safe and explainable.

### Core model

File: `src/favorites.rs` or `src/playlist.rs`

```rust
pub struct ImportPreview {
    pub new_stations: Vec<Station>,
    pub duplicates: Vec<Station>,
    pub enrichments: Vec<Station>,
    pub skipped: Vec<ImportSkip>,
}

pub struct ImportSkip {
    pub name: String,
    pub reason: String,
}

pub enum ImportMode {
    All,
    EnrichExistingOnly,
}
```

Library methods:

```rust
impl Library {
    pub fn preview_import(&self, stations: Vec<Station>) -> ImportPreview { ... }

    pub fn apply_import_preview(
        &mut self,
        preview: ImportPreview,
        mode: ImportMode,
    ) -> anyhow::Result<ImportSummary> { ... }
}
```

### CLI behavior

Do not break current automation. Existing command should still import directly.

Add optional modes later:

```text
pulsedeck import file.m3u --preview
pulsedeck import file.m3u --enrich-only
```

### TUI behavior

Expose through command palette first:

```text
Import library
Preview import
```

### Tests

- preview classifies new, duplicate, enrichment.
- preview mode does not write library.
- enrich-only does not add new stations.
- broken entries get stable skip reasons.

---

## Phase 10: Station Details grouping

### Goal

Make the rich metadata readable.

### Layout

Group rows:

```text
Identity
  Name
  UUID
  Saved

Playback
  Stream
  Codec
  Bitrate
  Now playing

Catalog
  Genre
  Tags
  Country
  Country ID
  Language
  Homepage

Health
  Last check
  Local health
  Votes
  Clicks
```

File: `src/ui/station_details.rs`

```rust
struct DetailSection {
    title: &'static str,
    rows: Vec<DetailRow>,
}

struct DetailRow {
    label: &'static str,
    value: String,
}

fn station_detail_sections(app: &App) -> Vec<DetailSection> { ... }
```

Pitfall: overlay height. Keep compact warning. Consider compact mode that shows only Identity, Playback, and Health.

### Tests

- sections contain expected fields.
- missing metadata displays `N/A`.
- long homepage and UUID truncate safely.

---

## Phase 11: README and CHANGELOG updates

### README settings text

Add under Settings:

```markdown
- **Stream Song Info Metadata**: request ICY now-playing metadata when stations support it. Turn this off if a rare stream behaves better with clean audio bytes only.
```

Update playback model:

```markdown
PulseDeck can request ICY song-title metadata when enabled in settings. Metadata is optional and can be disabled without changing saved stations or playback controls.
```

### Help overlay

File: `src/ui/help.rs`

Update settings line:

```rust
shortcut(",", "Settings: output, theme, autoplay, metadata, history"),
```

### Changelog skeleton

```markdown
## [0.4.0] - YYYY-MM-DD

### Added
* **Playback Doctor**: Added a diagnostic overlay for stream state, buffer health, decoder status, output device, reconnect attempts, metadata mode, and recovery actions.
* **Command Palette**: Added a searchable action palette for core commands, overlays, settings, and recovery actions.
* **Stream Song Info Metadata setting**: Added a settings row for ICY now-playing metadata, defaulting on with a clean-audio opt-out.
* **Search Result Explanations**: Highlighted search results now explain matching signals such as exact tag, country code, codec, health, and saved status.
* **Library Metadata Refresh**: Saved stations can be refreshed with richer Radio Browser metadata without replacing saved names or stream URLs.
* **Import Preview**: Library imports now summarize new stations, duplicates, metadata refreshes, and skipped entries before committing.

### Improved
* **Live Stream Decoder Compatibility**: Decoder probing can replay an initial buffered stream window without pretending live radio is fully seekable.
* **Playback Recovery UX**: Stream, decoder, and audio-output errors now show contextual recovery actions.
* **Station Details**: Metadata is grouped for faster scanning and clearer trust/health context.
* **Station Health**: Saved stations remember local success/failure signals for better recovery hints and future ranking.

### Fixed
* **Repository hygiene**: Removed unused legacy audio experiment files that were not part of the active playback path.
```

---

## Implementation order

1. Cleanup unused audio experiment files.
2. Finish metadata setting tests and docs.
3. Playback Doctor diagnostics state and overlay.
4. Actionable playback error hints.
5. Initial probe replay buffer.
6. Search result explanations.
7. Station health memory.
8. Library metadata refresh.
9. Import preview.
10. Command palette.
11. Station Details grouping.
12. README, CHANGELOG, release notes.

Why this order:

- Cleanup removes confusion.
- Metadata setting is already mostly done.
- Doctor and error hints improve confidence before deeper audio changes.
- Probe replay is risky and should be isolated.
- Search/library features build on stable metadata helpers.
- Command palette can expose the new features after they exist.

---

## Known pitfalls from 0.3.1

### Fake seek bug

Never implement live stream seek by consuming bytes from the live stream. Decoder probing must not eat audio.

Bad pattern:

```rust
let n = self.read(&mut discard[..to_read])?;
```

### Decoded PCM queue experiment

Do not resurrect a separate decoded-PCM playback queue unless there is a complete scheduler design and heavy tests. The current passive visualizer tap is the correct architecture.

### Metadata blame

ICY metadata was not the final root cause of the audio bug. Keep it optional, default on, and easy to disable.

### Docs drift

README and CHANGELOG must describe the final design only.

---

## Audio stability gate

Live stream bytes are sacred. Any future change to `src/audio/session.rs`, `src/audio/stream_reader.rs`, `src/audio/probe_reader.rs`, `src/audio/engine_loop.rs`, buffering, decoder setup, reconnect behavior, or ICY metadata stripping must preserve this contract:

- `StreamReader` may report its current position, but it must not implement seek by reading and discarding live bytes.
- `SeekFrom::End` is always unsupported for live radio.
- Forward seek outside already captured probe bytes is unsupported and must not consume from the queue.
- Rewind is only allowed inside `ProbeReplayReader`'s captured initial probe window.
- ICY metadata remains supported and default-on; metadata must not be treated as the root audio scapegoat.
- Tests must cover the composed `StreamReader -> ProbeReplayReader` path, not only the individual wrappers.

Required regression coverage for audio byte-path work:

- safe initial rewind replays captured bytes without draining the live queue twice.
- unsafe forward seek fails without consuming queued bytes.
- ICY metadata stripping still yields clean audio bytes after a safe start rewind.
- metadata-on and metadata-off manual smoke tests pass on real stations.

---

## Acceptance criteria

0.4.0 is ready when:

- Dead audio experiment files are removed.
- Metadata setting is documented and tested.
- Playback Doctor opens, closes, and shows useful state.
- Playback errors provide contextual recovery actions.
- Probe replay allows safe initial rewinds and refuses unsafe live seeks.
- Composed audio-reader tests prove safe rewind, unsafe forward-seek refusal, and ICY stripping on the active reader path.
- Search result explanations exist for highlighted results.
- Station health is stored without breaking old libraries.
- Metadata refresh enriches saved stations safely.
- Import preview classifies new, duplicate, enrich, skipped.
- Command palette exposes recovery and discovery actions.
- Station Details is grouped and readable.
- README and CHANGELOG match reality.
- `cargo test` passes.
- `cargo clippy --all-targets` passes.
- Manual metadata-on and metadata-off playback smoke tests pass.

---

## Manual smoke checklist

- Start with metadata on.
- Play a known ICY station.
- Verify current track updates.
- Verify Recent Tracks updates.
- Enable saved history and verify history updates.
- Turn metadata off.
- Stop and replay station.
- Verify playback still works.
- Verify no new track titles arrive while metadata is off.
- Open Playback Doctor while connecting.
- Open Playback Doctor while playing.
- Open Playback Doctor after an error.
- Search `tag:ambient`.
- Search `country:BA`.
- Search `lang:english`.
- Search `codec:mp3`.
- Verify search explanations are short and sensible.
- Preview import of M3U and JSON sample files.
- Test 80x24 terminal.
- Test below-minimum terminal dimensions.

---

## Final product shape

PulseDeck 0.4.0 should feel like this:

- If playback works, it is smooth and informative.
- If playback fails, PulseDeck says why and offers the next action.
- If search returns results, the user understands why they are good.
- If the library ages, it can refresh and remember health.
- If the user forgets a shortcut, the command palette catches them.
- If a station supports song titles, PulseDeck shows them.
- If a station behaves better without metadata, the user can turn metadata off.

This is the confidence release.
