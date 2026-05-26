# Audio architecture

PulseDeck keeps audio playback on a dedicated blocking thread so the terminal UI can stay responsive.

## Public boundary

The public audio API remains exposed from `crate::audio`:

- `AudioEngine` is the UI-facing handle.
- `AudioCommand` is the command channel from the app to the audio thread.
- `AudioStatus` is the status channel from the audio thread back to the app.

Other audio modules are implementation details and should stay private or `pub(super)` unless there is a clear cross-module need.

## Current module map

- `src/audio.rs` owns the public API, audio thread loop, playback state, command handling, and lazy output-device initialization.
- `src/audio/buffer.rs` owns the bounded producer-consumer byte queue used between the network downloader and decoder.
- `src/audio/session.rs` owns stream connection, retry/backoff, downloader setup, decoder setup, and sink creation.
- `src/audio/stream_reader.rs` owns ICY metadata boundary stripping, recording segment lifecycle, and the `Read`/`Seek` adapter consumed by `rodio::Decoder`.
- `src/audio/metadata.rs` owns ICY metadata parsing helpers.
- `src/audio/recording.rs` owns recording filename sanitization and ID3 tagging helpers.
- `src/audio/visualizer.rs` owns sample interception for the visualizer buffer.

## Refactor rules

- Keep behavior changes out of mechanical extraction PRs.
- Preserve `crate::audio::{AudioCommand, AudioEngine, AudioStatus}` unless an app-level migration is planned.
- Keep networking, decoding, recording, and UI status updates testable through small helpers where possible.
- Prefer one subsystem movement per PR so regressions are easy to bisect.

## Known follow-ups

- Split the audio thread loop into a small playback state object if command handling grows further.
- Improve recording filename collision handling and stream format detection in a behavior-change PR.
