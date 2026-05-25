# Testing strategy

DriftFM uses fast, deterministic checks as the default quality gate. CI and local validation should remain useful even when a developer is offline, a stream host is down, or the Radio Browser network has certificate trouble.

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
- Radio Browser tests avoid live network calls. They verify URL construction, fallback ordering, API mapping, and error formatting.
- Runtime smoke tests cover the real terminal, audio backend, search, playback, recording toggles, and terminal restore.

## Why no live network tests by default

Live Radio Browser and stream tests would make CI depend on external certificates, DNS, routing, and stream availability. Those failures are useful during manual diagnosis but too noisy for normal pull request gates.

Use manual network smoke tests when touching:

- `src/radio.rs`
- `src/audio/session.rs`
- `src/audio/stream_reader.rs`
- stream retry/cancellation behavior
- TLS or HTTP fallback behavior

## Manual smoke checklist

1. Start the app with `cargo run`.
2. Search for `lofi`.
3. Confirm search results appear or show a useful compact error.
4. Press `Enter` on a search result and confirm it adds/plays.
5. Pause and resume playback.
6. Stop playback.
7. Switch stations.
8. Toggle recording while playing, then stop recording.
9. Open settings and press playback/layout/search keys to confirm they do not leak through.
10. Open help and settings to confirm overlays still mutually close.
11. Quit and confirm the terminal restores cleanly.
