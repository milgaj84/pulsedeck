# Testing strategy

PulseDeck uses fast, deterministic checks as the default quality gate. CI and local validation should remain useful even when a developer is offline, a stream host is down, or the Radio Browser network has certificate trouble.

## Required local gate

Run this before opening or updating a pull request:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo build --release
cargo run
```

## Test layers

- Unit tests cover pure helpers, reducers, parsing, theme serialization, library mutation, search state, and key mapping.
- Reducer contract tests protect behavior that previously regressed, especially settings-overlay action blocking and stale search response handling.
- Audio unit tests avoid opening sound devices or live streams. They test deterministic math and parsing helpers.
- Radio Browser tests avoid live network calls. They verify URL construction, fallback ordering, structured query parsing, API mapping, identity matching, dedupe/ranking, and error formatting.
- Runtime smoke tests cover the real terminal, audio backend, search, playback controls, settings, visualizers, and terminal restore.

## Why no live network tests by default

Live Radio Browser and stream tests would make CI depend on external certificates, DNS, routing, and stream availability. Those failures are useful during manual diagnosis but too noisy for normal pull request gates.

Use manual network or runtime smoke tests when touching:

- `src/radio.rs`
- `src/audio/session.rs`
- `src/audio/stream_reader.rs`
- `src/audio/output.rs`
- stream retry/cancellation behavior
- TLS or HTTP fallback behavior
- selectable audio output behavior
- PipeWire/PulseAudio/Bluetooth routing
- TUI safety around native ALSA/JACK stderr diagnostics
