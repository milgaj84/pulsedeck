# PulseDeck 0.4.1 Stabilization Plan

Release theme: **Stop Breaking Audio, Remove Dead Audio Debris, Make Station Identity Boringly Correct**.

This release is deliberately small. It is not the release for new UI features, new providers, local files, recording, podcasts, plugins, cloud sync, or a major playback rewrite. PulseDeck has broken audio too many times. For 0.4.1, the work must move like a bomb technician in wool socks: small cuts, measured tests, no clever detonations.

## Current baseline observed during planning

Repository: `pulsedeck`

Important current facts:

```text
src/audio.rs declares only:
- src/audio/engine_loop.rs
- src/audio/metadata.rs
- src/audio/output.rs
- src/audio/session.rs
- src/audio/stream_reader.rs
- src/audio/visualizer.rs

src/audio/ contains additional stale files that are not declared by src/audio.rs:
- src/audio/buffer.rs
- src/audio/buffer_meter.rs
- src/audio/decoded_source.rs
- src/audio/pcm_buffer.rs
- src/audio/pcm_buffer2.rs
- src/audio/probe_reader.rs
```

Validation from the review pass:

```text
cargo check                         passed
cargo clippy --all-targets --all-features passed
cargo test                          passed, 303 tests
```

This plan intentionally focuses on the top three fixes from the code review:

1. Remove or deliberately quarantine dead audio prototype files.
2. Centralize station identity and replace raw URL equality with one normalized identity path.
3. Stabilize the active audio engine loop through behavior-preserving extraction, tests, and manual playback gates.

The third item is intentionally framed as **stabilize**, not “rewrite.” If a change touches active playback behavior, it must ship with a regression test or an explicit manual verification checklist item.

---

# Non-negotiable audio rules for 0.4.1

These rules exist because audio has been broken repeatedly. Do not skip them.

## Rule 1: No silent audio failures

Any command sent from app state into `AudioEngine` must eventually become either:

- an `AudioStatus` from the engine, or
- a visible app notice / playback error if the engine command channel is dead.

Current risk:

```rust
// src/audio.rs
impl AudioEngine {
    pub fn send(&self, cmd: AudioCommand) {
        let _ = self.cmd_tx.send(cmd);
    }
}
```

`send` currently discards failure. That means the audio thread can die and UI state can still pretend commands worked. This is unacceptable for a stabilization release.

## Rule 2: Active playback path is sacred

The active path is:

```text
src/app/playback.rs::play_selected
  -> src/audio.rs::AudioEngine::send(AudioCommand::Play)
  -> src/audio/engine_loop.rs::audio_loop
  -> src/audio/engine_loop.rs::start_connection
  -> src/audio/engine_loop.rs::spawn_connection
  -> src/audio/session.rs::connect_and_decode
  -> src/audio/session.rs::try_connect_and_decode_once
  -> src/audio/stream_reader.rs::StreamReader
  -> rodio::Sink
```

Do not replace this path wholesale in 0.4.1. Only isolate, test, and clarify it.

## Rule 3: No zombie audio modules in `src/audio/`

If a file under `src/audio/` is not declared in `src/audio.rs`, it must either be:

- deleted, or
- moved out of `src/` into documentation / archive space.

Leaving uncompiled audio experiments beside active code is how the next release accidentally copies from a corpse.

## Rule 4: No broad decoder changes without manual playback checks

`src/audio/session.rs::try_connect_and_decode_once` currently uses `rodio::Decoder::new_mp3`. This may be too narrow for `.m4a` and AAC-ish streams, but changing decoder behavior can break working MP3 playback. For 0.4.1, decoder changes are allowed only if they are isolated and manually verified with known streams.

Manual stream checks must include at least:

```text
Nightride FM              https://stream.nightride.fm/nightride.m4a
NightWave Plaza           https://radio.plaza.one/mp3
SomaFM Groove Salad       https://ice2.somafm.com/groovesalad-128-mp3
SomaFM DEF CON            https://ice2.somafm.com/defcon-128-mp3
```

If `.m4a` still does not play, do not hide it. Surface it as a known limitation or filter/warn before playback.

---

# Desired release outcome

By the end of 0.4.1:

```text
1. The active audio module graph is obvious.
2. Station remove/select/now-playing/health lookup all use the same URL identity rules.
3. Audio command send failure is observable.
4. audio_loop.rs is easier to reason about without changing playback behavior.
5. Tests prove station identity edge cases and engine command failure semantics.
6. Manual playback checklist is documented and completed before release.
```

---

# Fix 1: Remove or quarantine dead audio prototype files

## Goal

Delete stale, uncompiled audio code from `src/audio/` so maintainers cannot confuse abandoned buffering/PCM experiments with the active playback stack.

This is the least glamorous fix, but it is the first because audio has already been fragile. A confusing source tree is not harmless. It is a trapdoor with comments.

## Files involved

Active module root:

```text
src/audio.rs
```

Active audio modules declared by `src/audio.rs`:

```text
src/audio/engine_loop.rs
src/audio/metadata.rs
src/audio/output.rs
src/audio/session.rs
src/audio/stream_reader.rs
src/audio/visualizer.rs
```

Dead / uncompiled files currently under `src/audio/`:

```text
src/audio/buffer.rs
src/audio/buffer_meter.rs
src/audio/decoded_source.rs
src/audio/pcm_buffer.rs
src/audio/pcm_buffer2.rs
src/audio/probe_reader.rs
```

## Why these files are dead

Rust does not compile a sibling `.rs` file just because it exists. A file must be declared via `mod`, included, or otherwise referenced.

Current `src/audio.rs` begins with:

```rust
mod engine_loop;
mod metadata;
mod output;
mod session;
mod stream_reader;
mod visualizer;
```

That means these files are not compiled:

```text
src/audio/buffer.rs
src/audio/buffer_meter.rs
src/audio/decoded_source.rs
src/audio/pcm_buffer.rs
src/audio/pcm_buffer2.rs
src/audio/probe_reader.rs
```

Specific stale-code evidence:

```rust
// src/audio/buffer_meter.rs
// This file attempts to send a status variant that does not exist in src/audio.rs.
AudioStatus::BufferLevel { percent, seconds }
```

Current `src/audio.rs::AudioStatus` contains:

```rust
pub enum AudioStatus {
    Playing,
    Paused,
    Stopped,
    Error(String),
    Connecting,
    FadingOut { current_volume: f32 },
    TrackChanged { url: String, title: String },
}
```

There is no `BufferLevel` variant. So `src/audio/buffer_meter.rs` is not merely unused. It is stale relative to the active public audio status contract.

## Implementation choice

Prefer deletion.

```text
Delete:
- src/audio/buffer.rs
- src/audio/buffer_meter.rs
- src/audio/decoded_source.rs
- src/audio/pcm_buffer.rs
- src/audio/pcm_buffer2.rs
- src/audio/probe_reader.rs
```

If someone insists on keeping history, move them outside active source:

```text
docs/architecture/audio-prototypes/buffer.rs
docs/architecture/audio-prototypes/buffer_meter.rs
docs/architecture/audio-prototypes/decoded_source.rs
docs/architecture/audio-prototypes/pcm_buffer.rs
docs/architecture/audio-prototypes/pcm_buffer2.rs
docs/architecture/audio-prototypes/probe_reader.rs
```

But the preferred 0.4.1 path is deletion because Git already preserves history.

## Exact commands

```bash
git rm src/audio/buffer.rs
git rm src/audio/buffer_meter.rs
git rm src/audio/decoded_source.rs
git rm src/audio/pcm_buffer.rs
git rm src/audio/pcm_buffer2.rs
git rm src/audio/probe_reader.rs
```

Do not edit `src/audio.rs` for this step unless a deleted file was unexpectedly declared. It currently is not.

## Required verification

Run:

```bash
cargo check
cargo test
cargo clippy --all-targets --all-features
```

Expected result:

```text
No compile behavior should change.
No tests should disappear except tests that were never compiled in the first place.
```

## Pitfalls

### Pitfall: accidentally deleting active audio files

Do not delete:

```text
src/audio/engine_loop.rs
src/audio/metadata.rs
src/audio/output.rs
src/audio/session.rs
src/audio/stream_reader.rs
src/audio/visualizer.rs
```

Those are declared by `src/audio.rs` and are active.

### Pitfall: reintroducing buffer code to “fix” audio quickly

Do not revive `buffer.rs`, `pcm_buffer.rs`, or `pcm_buffer2.rs` as a panic patch. The active playback path currently uses `StreamReader` and `rodio::Sink`. Reviving a decoded PCM queue is a larger architecture change and is not a patch-release move.

### Pitfall: trusting uncompiled tests

Any tests inside deleted files were not running. Do not count them as coverage. If any test scenario is valuable, port it into an active module test.

## Edge cases

### Edge case: future buffering metrics

`PlaybackView` still has:

```rust
// src/app/playback.rs
pub buffer_percent: u8,
pub buffer_seconds: u32,
```

And diagnostics still track buffer-like fields. Deleting `buffer_meter.rs` does not remove those UI fields. It only removes the stale, uncompiled producer. If buffer metrics are revived later, add a fresh `AudioStatus::BufferLevel` variant and wire it through active code intentionally.

### Edge case: release notes mention buffering

Search README and CHANGELOG for claims about active buffering metrics. If docs claim a live buffer meter exists, update them or defer the claim.

## Definition of done for Fix 1

```text
[ ] Dead src/audio prototype files removed or moved out of src/.
[ ] src/audio.rs still declares only active modules.
[ ] cargo check passes.
[ ] cargo test passes.
[ ] cargo clippy --all-targets --all-features passes.
[ ] No release notes claim deleted prototypes are active features.
```

---

# Fix 2: Centralize station identity and remove raw URL equality

## Goal

Make station identity consistent everywhere. A station URL should match even when casing, whitespace, or trailing slashes differ. Radio Browser UUID should remain the preferred identity when both sides have UUIDs.

This fix prevents ghost-station behavior:

- remove says station is not present even though UI shows it,
- now-playing cannot find the active station,
- health metadata attaches to one spelling of a URL but not another,
- autoplay restores selection differently from now-playing lookup.

## Files involved

Identity source of truth:

```text
src/radio/station.rs
```

Callers to update:

```text
src/favorites.rs
src/app/selectors.rs
src/app/lifecycle.rs
src/app/lifecycle.rs::handle_track_changed
```

Also inspect for raw comparisons:

```text
src/app/playback.rs
src/app/library.rs
src/app/update.rs
src/radio/rank.rs
src/radio/map.rs
```

## Current identity situation

`src/radio/station.rs::Station::identity` already normalizes URLs when no UUID is present:

```rust
pub fn identity(&self) -> StationIdentity {
    self.station_uuid
        .as_deref()
        .map(str::trim)
        .filter(|uuid| !uuid.is_empty())
        .map(|uuid| StationIdentity::Uuid(uuid.to_ascii_lowercase()))
        .unwrap_or_else(|| StationIdentity::Url(normalized_station_url(&self.url)))
}
```

`src/radio/station.rs::station_identity_matches` already prefers UUID and falls back to normalized URL:

```rust
pub fn station_identity_matches(a: &Station, b: &Station) -> bool {
    match (a.station_uuid.as_deref(), b.station_uuid.as_deref()) {
        (Some(left), Some(right)) if !left.trim().is_empty() && !right.trim().is_empty() => {
            left.eq_ignore_ascii_case(right)
        }
        _ => normalized_station_url(&a.url) == normalized_station_url(&b.url),
    }
}
```

But the URL normalizer is private:

```rust
fn normalized_station_url(value: &str) -> String {
    value.trim().trim_end_matches('/').to_ascii_lowercase()
}
```

So other modules duplicate it or skip it.

## Raw / duplicated URL comparison sites to fix

### `src/favorites.rs::remove`

Current:

```rust
pub fn remove(&mut self, url: &str) -> anyhow::Result<bool> {
    let before = self.stations.len();
    self.stations.retain(|s| s.url != url);
    let removed = self.stations.len() < before;
    if removed {
        self.rebuild_genres();
    }
    Ok(removed)
}
```

Problem: exact raw URL equality.

### `src/favorites.rs::contains`

Current:

```rust
pub fn contains(&self, url: &str) -> bool {
    self.stations.iter().any(|s| s.url == url)
}
```

Problem: exact raw URL equality.

### `src/favorites.rs::normalized_url_match`

Current:

```rust
fn normalized_url_match(left: &str, right: &str) -> bool {
    left.trim().trim_end_matches('/').eq_ignore_ascii_case(right.trim().trim_end_matches('/'))
}
```

Problem: duplicate logic. It should live in `src/radio/station.rs`.

### `src/app/selectors.rs::now_playing`

Current:

```rust
pub fn now_playing(&self) -> Option<&Station> {
    self.player.playing_url.as_ref().and_then(|url| {
        self.library
            .stations
            .iter()
            .find(|s| s.url == *url)
            .or_else(|| self.search.results.iter().find(|s| s.url == *url))
            .or_else(|| {
                self.undo_history.iter().rev().find_map(|(station, _, _)| {
                    if station.url == *url {
                        Some(station)
                    } else {
                        None
                    }
                })
            })
    })
}
```

Problem: exact raw URL equality in all three lookup paths.

### `src/app/selectors.rs::select_playing`

Current:

```rust
pub(super) fn select_playing(&mut self) {
    if let Some(ref url) = self.player.playing_url {
        if let Some(pos) = self.visible_stations().iter().position(|s| s.url == *url) {
            self.nav.selected = pos;
        }
    }
}
```

Problem: exact raw URL equality.

### `src/app/lifecycle.rs::last_played_station_position`

Current:

```rust
fn last_played_station_position(stations: &[Station], last_played_url: &str) -> Option<usize> {
    let needle = normalized_playback_url(last_played_url);
    stations
        .iter()
        .position(|station| normalized_playback_url(&station.url) == needle)
}

fn normalized_playback_url(url: &str) -> String {
    url.trim().trim_end_matches('/').to_ascii_lowercase()
}
```

Problem: duplicate logic.

### `src/app/lifecycle.rs::handle_track_changed`

Current:

```rust
fn handle_track_changed(&mut self, url: String, title: String) {
    if Some(&url) != self.player.playing_url.as_ref() {
        return;
    }

    // ...
}
```

Problem: exact raw URL equality means ICY metadata from a normalized-equivalent stream can be ignored.

## Implementation plan

### Step 2.1: Export URL normalization helpers from `src/radio/station.rs`

Change:

```rust
fn normalized_station_url(value: &str) -> String {
    value.trim().trim_end_matches('/').to_ascii_lowercase()
}
```

To:

```rust
pub fn normalized_station_url(value: &str) -> String {
    value.trim().trim_end_matches('/').to_ascii_lowercase()
}

pub fn station_url_matches(left: &str, right: &str) -> bool {
    normalized_station_url(left) == normalized_station_url(right)
}
```

Then update `station_identity_matches` to use the public helper:

```rust
pub fn station_identity_matches(a: &Station, b: &Station) -> bool {
    match (a.station_uuid.as_deref(), b.station_uuid.as_deref()) {
        (Some(left), Some(right)) if !left.trim().is_empty() && !right.trim().is_empty() => {
            left.trim().eq_ignore_ascii_case(right.trim())
        }
        _ => station_url_matches(&a.url, &b.url),
    }
}
```

Note the `trim()` on UUID comparison. Current code checks trimmed emptiness but compares untrimmed values. Fix that while touching this function.

### Step 2.2: Re-export helper from `src/radio.rs` if needed

Check current `src/radio.rs`. It likely re-exports station helpers. Add:

```rust
pub use station::{
    clean_tag_values,
    fallback_stations,
    normalize_codec,
    normalize_country_code,
    normalize_station_uuid,
    normalized_station_url,
    sanitize_bitrate,
    station_identity_matches,
    station_url_matches,
    Station,
    StationHealth,
    StationIdentity,
};
```

Keep names sorted consistently with existing style.

### Step 2.3: Update `src/favorites.rs` imports

Current import:

```rust
use crate::radio::{
    clean_tag_values, normalize_codec, normalize_country_code, normalize_station_uuid,
    sanitize_bitrate, station_identity_matches, Station, StationHealth,
};
```

Change to:

```rust
use crate::radio::{
    clean_tag_values, normalize_codec, normalize_country_code, normalize_station_uuid,
    sanitize_bitrate, station_identity_matches, station_url_matches, Station, StationHealth,
};
```

Then update `remove`:

```rust
pub fn remove(&mut self, url: &str) -> anyhow::Result<bool> {
    let before = self.stations.len();
    self.stations
        .retain(|station| !station_url_matches(&station.url, url));

    let removed = self.stations.len() < before;
    if removed {
        self.rebuild_genres();
    }

    Ok(removed)
}
```

Update `contains`:

```rust
pub fn contains(&self, url: &str) -> bool {
    self.stations
        .iter()
        .any(|station| station_url_matches(&station.url, url))
}
```

Update health methods:

```rust
pub fn mark_station_success(&mut self, url: &str, now: String) -> bool {
    if let Some(station) = self
        .stations
        .iter_mut()
        .find(|station| station_url_matches(&station.url, url))
    {
        station.health.last_success_at = Some(now);
        station.health.last_error_summary.clear();
        return true;
    }
    false
}

pub fn mark_station_failure(&mut self, url: &str, now: String, error: &str) -> bool {
    if let Some(station) = self
        .stations
        .iter_mut()
        .find(|station| station_url_matches(&station.url, url))
    {
        station.health.last_failure_at = Some(now);
        station.health.failure_count = Some(
            station
                .health
                .failure_count
                .unwrap_or(0)
                .saturating_add(1),
        );
        station.health.last_error_summary = compact_error_summary(error);
        return true;
    }
    false
}
```

Delete local duplicate:

```rust
fn normalized_url_match(left: &str, right: &str) -> bool {
    left.trim().trim_end_matches('/').eq_ignore_ascii_case(right.trim().trim_end_matches('/'))
}
```

### Step 2.4: Update `src/app/selectors.rs`

Add import:

```rust
use crate::radio::station_url_matches;
```

Update `now_playing`:

```rust
pub fn now_playing(&self) -> Option<&Station> {
    self.player.playing_url.as_ref().and_then(|url| {
        self.library
            .stations
            .iter()
            .find(|station| station_url_matches(&station.url, url))
            .or_else(|| {
                self.search
                    .results
                    .iter()
                    .find(|station| station_url_matches(&station.url, url))
            })
            .or_else(|| {
                self.undo_history.iter().rev().find_map(|(station, _, _)| {
                    station_url_matches(&station.url, url).then_some(station)
                })
            })
    })
}
```

Update `select_playing`:

```rust
pub(super) fn select_playing(&mut self) {
    if let Some(ref url) = self.player.playing_url {
        if let Some(pos) = self
            .visible_stations()
            .iter()
            .position(|station| station_url_matches(&station.url, url))
        {
            self.nav.selected = pos;
        }
    }
}
```

### Step 2.5: Update `src/app/lifecycle.rs`

Add helper import if not already available via `super::*`:

```rust
use crate::radio::station_url_matches;
```

Replace:

```rust
fn last_played_station_position(stations: &[Station], last_played_url: &str) -> Option<usize> {
    let needle = normalized_playback_url(last_played_url);
    stations
        .iter()
        .position(|station| normalized_playback_url(&station.url) == needle)
}

fn normalized_playback_url(url: &str) -> String {
    url.trim().trim_end_matches('/').to_ascii_lowercase()
}
```

With:

```rust
fn last_played_station_position(stations: &[Station], last_played_url: &str) -> Option<usize> {
    stations
        .iter()
        .position(|station| station_url_matches(&station.url, last_played_url))
}
```

Update `handle_track_changed`:

```rust
fn handle_track_changed(&mut self, url: String, title: String) {
    if !self
        .player
        .playing_url
        .as_deref()
        .is_some_and(|playing_url| station_url_matches(playing_url, &url))
    {
        return;
    }

    // existing body unchanged
}
```

## Tests to add

### `src/radio/station.rs` tests

Add or extend tests for helpers:

```rust
#[test]
fn station_url_matches_ignores_case_whitespace_and_trailing_slash() {
    assert!(station_url_matches(
        " HTTP://Example.COM/stream/ ",
        "http://example.com/stream"
    ));
}

#[test]
fn station_identity_matches_trims_uuid_before_comparing() {
    let mut a = Station::basic("A", "http://a", "Radio", "US", 128);
    let mut b = Station::basic("B", "http://b", "Radio", "US", 128);
    a.station_uuid = Some(" UUID-1 ".to_string());
    b.station_uuid = Some("uuid-1".to_string());

    assert!(station_identity_matches(&a, &b));
}

#[test]
fn station_identity_falls_back_to_normalized_url_when_uuid_missing() {
    let a = Station::basic("A", "HTTP://STREAM/", "Radio", "US", 128);
    let b = Station::basic("B", "http://stream", "Radio", "US", 128);

    assert!(station_identity_matches(&a, &b));
}
```

### `src/favorites.rs` tests

Add tests near existing `Library` tests:

```rust
#[test]
fn remove_matches_normalized_station_url() {
    let mut library = Library::in_memory(vec![Station::basic(
        "A",
        " HTTP://STREAM/ ",
        "Radio",
        "US",
        128,
    )]);

    assert!(library.remove("http://stream").unwrap());
    assert!(library.stations.is_empty());
}

#[test]
fn contains_matches_normalized_station_url() {
    let library = Library::in_memory(vec![Station::basic(
        "A",
        "HTTP://STREAM/",
        "Radio",
        "US",
        128,
    )]);

    assert!(library.contains("http://stream"));
}

#[test]
fn station_health_matches_normalized_url() {
    let mut library = Library::in_memory(vec![Station::basic(
        "A",
        "HTTP://STREAM/",
        "Radio",
        "US",
        128,
    )]);

    assert!(library.mark_station_failure(
        "http://stream",
        "123".to_string(),
        "temporary failure",
    ));

    assert_eq!(library.stations[0].health.failure_count, Some(1));
}
```

### `src/app/selectors.rs` tests

Add tests for lookup and selection:

```rust
#[test]
fn now_playing_matches_normalized_library_url() {
    let mut app = App::new(Library::in_memory(vec![station(
        "A",
        " HTTP://STREAM/ ",
    )]));
    app.player.playing_url = Some("http://stream".to_string());

    assert_eq!(app.now_playing().map(|station| station.name.as_str()), Some("A"));
}

#[test]
fn select_playing_matches_normalized_visible_url() {
    let mut app = App::new(Library::in_memory(vec![
        station("A", "http://a"),
        station("B", " HTTP://STREAM/ "),
    ]));
    app.player.playing_url = Some("http://stream".to_string());

    app.select_playing();

    assert_eq!(app.nav.selected, 1);
}
```

### `src/app/lifecycle.rs` tests

Existing test already covers `last_played_station_position_matches_normalized_urls`. Update it so it indirectly uses `station_url_matches` and delete references to `normalized_playback_url` if that helper is removed.

Add test for metadata URL normalization if `handle_track_changed` can be exercised through public status polling. One path:

```rust
#[test]
fn track_changed_uses_normalized_playing_url() {
    let mut app = App::new(Library::in_memory(vec![Station::basic(
        "A",
        "HTTP://STREAM/",
        "Radio",
        "US",
        128,
    )]));

    app.player.playing_url = Some("http://stream".to_string());
    app.handle_track_changed("HTTP://STREAM/".to_string(), "Artist - Title".to_string());

    assert_eq!(app.player.current_track.as_deref(), Some("Artist - Title"));
}
```

If `handle_track_changed` remains private and test placement makes this awkward, test via `poll_audio_status` only if injecting status is easy. Do not contort the design for this one test in 0.4.1.

## Pitfalls

### Pitfall: URL normalization is not URL canonicalization

This helper intentionally does only:

```text
trim whitespace
trim trailing slash
lowercase ASCII
```

Do not add query sorting, percent-decoding, default-port removal, or HTTP-to-HTTPS equivalence in this patch. Those are semantic changes and can merge different stations incorrectly.

### Pitfall: UUID comparison with empty strings

A blank UUID must not override URL matching. Keep this guard:

```rust
(Some(left), Some(right)) if !left.trim().is_empty() && !right.trim().is_empty()
```

### Pitfall: Station UUID conflict

If both stations have non-empty UUIDs and they differ, `station_identity_matches` should return false even if URLs happen to match. This preserves Radio Browser identity semantics.

Add test:

```rust
#[test]
fn station_identity_prefers_uuid_mismatch_over_url_match() {
    let mut a = Station::basic("A", "http://same", "Radio", "US", 128);
    let mut b = Station::basic("B", "http://same/", "Radio", "US", 128);
    a.station_uuid = Some("uuid-a".to_string());
    b.station_uuid = Some("uuid-b".to_string());

    assert!(!station_identity_matches(&a, &b));
}
```

### Pitfall: now-playing may search undo history

`src/app/selectors.rs::now_playing` searches library, search results, then `undo_history`. Keep that ordering. The station should prefer the current library copy because it may have enriched metadata and health fields.

### Pitfall: extra allocation in `visible_stations`

`select_playing` currently calls `visible_stations()` which allocates a `Vec<&Station>`. Do not optimize this in 0.4.1 unless needed. The identity fix is already enough.

## Definition of done for Fix 2

```text
[ ] src/radio/station.rs exposes station_url_matches.
[ ] src/radio/station.rs exposes normalized_station_url only if needed outside tests.
[ ] station_identity_matches trims UUIDs before comparing.
[ ] src/favorites.rs remove/contains/health use station_url_matches.
[ ] src/favorites.rs duplicate normalized_url_match removed.
[ ] src/app/selectors.rs now_playing/select_playing use station_url_matches.
[ ] src/app/lifecycle.rs no longer has normalized_playback_url duplicate.
[ ] Track metadata URL check uses station_url_matches or is consciously left exact with a comment.
[ ] Tests cover URL case/whitespace/trailing slash for remove, contains, now-playing, and health.
[ ] cargo test passes.
```

---

# Fix 3: Stabilize active audio engine loop without changing playback semantics

## Goal

Make the active audio engine harder to break by extracting state and adding observable failure behavior, while preserving existing playback behavior.

This fix is not a rewrite. The mission is to make `src/audio/engine_loop.rs::audio_loop` understandable enough that the next audio fix is not a blindfolded knife throw.

## Files involved

Primary:

```text
src/audio.rs
src/audio/engine_loop.rs
src/audio/session.rs
src/audio/stream_reader.rs
src/app/playback.rs
src/app/lifecycle.rs
src/app/settings.rs
```

Secondary verification:

```text
src/audio/output.rs
src/audio/metadata.rs
src/app/reconnect.rs
src/app/playback_error.rs
src/ui/playback_doctor.rs
```

## Current active command/status contracts

Commands:

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

Statuses:

```rust
// src/audio.rs
pub enum AudioStatus {
    Playing,
    Paused,
    Stopped,
    Error(String),
    Connecting,
    FadingOut { current_volume: f32 },
    TrackChanged { url: String, title: String },
}
```

Important: do not add a new `AudioStatus` variant in this fix unless it is fully handled in `src/app/lifecycle.rs::poll_audio_status` and tested.

## Current audio loop responsibilities

`src/audio/engine_loop.rs::audio_loop` currently owns all of these local variables:

```rust
let mut output_stream: Option<OutputStream> = None;
let mut output_handle: Option<rodio::OutputStreamHandle> = None;
let mut preferred_output_device_name: Option<String> = None;
let mut stream_metadata_enabled = true;
let mut reopen_output_on_next_connection = false;

let mut current_sink: Option<Sink> = None;
let mut connect_thread: Option<std::thread::JoinHandle<Result<Sink, String>>> = None;

let active_conn_id = Arc::new(AtomicU64::new(0));
let mut current_conn_id: u64 = 0;
let mut current_url: Option<String> = None;
let mut hardware_recovery_retries: u8 = 0;

let mut target_volume: f32 = 0.8;
let mut current_fade_volume: Option<f32> = None;
let mut pending_action: Option<AudioCommand> = None;
```

This function also handles:

```text
- command receive loop
- Play/Pause/Resume/Stop semantics
- volume updates
- output device switching
- metadata setting
- fade-out before switching/stopping
- fade-in after connection
- output stream lazy opening
- connection thread spawning
- stale connection cancellation via active_conn_id
- hardware output recovery retry
- sink-ended detection
- test-mode fake command handling
```

This is too much for one function, but it is also fragile. Refactor only in behavior-preserving slices.

---

## Step 3.1: Make `AudioEngine::send` observable

### Problem

Current:

```rust
// src/audio.rs
pub fn send(&self, cmd: AudioCommand) {
    let _ = self.cmd_tx.send(cmd);
}
```

If the audio thread dies, command send failure is swallowed.

### Change

Change `send` to return a boolean:

```rust
// src/audio.rs
impl AudioEngine {
    pub fn send(&self, cmd: AudioCommand) -> bool {
        self.cmd_tx.send(cmd).is_ok()
    }
}
```

### Update call sites

Call sites currently include:

```text
src/app/playback.rs::play_selected
src/app/playback.rs::retry_stream
src/app/playback.rs::toggle_pause
src/app/playback.rs::stop_playback
src/app/playback.rs::stop_audio_before_quit
src/app/playback.rs::sync_volume
src/app/lifecycle.rs::App::new autoplay branch
src/app/settings.rs::sync_output_device
src/app/settings.rs::sync_stream_metadata
```

For 0.4.1, use a small app helper so failure messaging is consistent.

Add in a suitable app module, preferably `src/app/playback.rs` or new `src/app/audio_commands.rs`:

```rust
impl App {
    pub(super) fn send_audio_command(&mut self, command: AudioCommand) -> bool {
        if self.audio.send(command) {
            true
        } else {
            self.player.current_track = None;
            self.player.buffer_percent = 0;
            self.player.buffer_seconds = 0;
            self.player.state = PlaybackState::Error("Audio engine stopped".to_string());
            self.diagnostics.decoder_state = DecoderState::Failed;
            self.diagnostics.last_error = Some("Audio engine command channel closed".to_string());
            self.set_error_notice("Audio engine is not available");
            false
        }
    }
}
```

Then replace important command sends:

```rust
// src/app/playback.rs::play_selected
if self.send_audio_command(AudioCommand::Play(station.url)) {
    self.sync_volume();
}
```

```rust
// src/app/playback.rs::retry_stream
if self.send_audio_command(AudioCommand::Play(url)) {
    self.sync_volume();
    self.set_info_notice("Retrying stream");
}
```

```rust
// src/app/playback.rs::toggle_pause
PlaybackState::Playing => {
    self.send_audio_command(AudioCommand::Pause);
}
PlaybackState::Paused => {
    self.send_audio_command(AudioCommand::Resume);
}
```

```rust
// src/app/playback.rs::stop_playback
self.player.intentional_stop = true;
let sent = self.send_audio_command(AudioCommand::Stop);

if !sent {
    self.player.playing_url = None;
    self.player.state = PlaybackState::Error("Audio engine stopped".to_string());
    return;
}
```

For passive sync commands, use a quieter helper if needed:

```rust
impl App {
    pub(super) fn try_send_audio_command(&mut self, command: AudioCommand) -> bool {
        self.audio.send(command)
    }
}
```

But be careful: if every `sync_volume()` failure pops a notice every tick or startup path, it can annoy users. Prefer visible failure only for user-initiated playback commands.

### Pitfalls

#### Pitfall: `sync_volume(&self)` cannot call `set_error_notice`

Current:

```rust
pub(super) fn sync_volume(&self) {
    self.audio.send(AudioCommand::SetVolume(
        self.current_output_volume_fraction(),
    ));
}
```

It takes `&self`, not `&mut self`. Do not mutate notices from here unless you change the signature carefully. For 0.4.1, it is acceptable to ignore volume sync send failure or return `bool`:

```rust
pub(super) fn sync_volume(&self) -> bool {
    self.audio.send(AudioCommand::SetVolume(
        self.current_output_volume_fraction(),
    ))
}
```

Then callers that care can decide what to do.

#### Pitfall: autoplay happens inside `App::new`

`src/app/lifecycle.rs::App::new` currently sends `AudioCommand::Play` during construction:

```rust
if app.library.settings.autoplay_last {
    if let Some(url) = app.library.settings.last_played_url.clone() {
        if let Some(pos) = last_played_station_position(&app.library.stations, &url) {
            app.nav.selected = pos;
        }
        app.player.playing_url = Some(url.clone());
        app.player.state = PlaybackState::Connecting;
        app.audio.send(AudioCommand::Play(url));
        app.sync_volume();
    }
}
```

Since `app` is mutable here, update it to use the same command helper or handle failure inline:

```rust
if !app.audio.send(AudioCommand::Play(url)) {
    app.player.state = PlaybackState::Error("Audio engine stopped".to_string());
    app.set_error_notice("Could not start autoplay: audio engine is not available");
} else {
    app.sync_volume();
}
```

Do not move autoplay out of `App::new` in 0.4.1 unless doing the larger constructor split deliberately.

### Tests

Because `AudioEngine` currently spawns a real thread, directly forcing send failure may require a small test-only constructor.

Add to `src/audio.rs`:

```rust
#[cfg(test)]
impl AudioEngine {
    pub fn disconnected_for_test() -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel::<AudioCommand>();
        drop(cmd_rx);
        let (_status_tx, status_rx) = mpsc::channel::<AudioStatus>();
        Self { cmd_tx, status_rx }
    }
}
```

Then in app tests, replace the app's audio engine:

```rust
#[test]
fn play_selected_reports_dead_audio_engine() {
    let mut app = App::new(Library::in_memory(vec![station("A", "http://a")]));
    app.audio = AudioEngine::disconnected_for_test();

    app.play_selected();

    assert!(matches!(app.player.state, PlaybackState::Error(_)));
    assert!(matches!(app.notice.current, Some(AppNotice::Error(_))));
}
```

If `App::audio` visibility makes this test awkward, put the test inside the appropriate `src/app` submodule where private fields are accessible, or add a `#[cfg(test)]` setter.

---

## Step 3.2: Extract `AudioLoopState` without changing behavior

### Current smell

`SpawnConnectionState<'a>` currently carries many mutable references:

```rust
struct SpawnConnectionState<'a> {
    conn_id_ref: &'a mut u64,
    active_ref: &'a Arc<AtomicU64>,
    connect_ref: &'a mut Option<std::thread::JoinHandle<Result<Sink, String>>>,
    output_stream: &'a mut Option<OutputStream>,
    output_handle: &'a mut Option<rodio::OutputStreamHandle>,
    status_tx: &'a mpsc::Sender<AudioStatus>,
    sample_buffer: &'a Arc<Mutex<VecDeque<f32>>>,
    preferred_output_device_name: &'a Option<String>,
    stream_metadata_enabled: bool,
    reopen_output_on_next_connection: &'a mut bool,
}
```

This is an argument-bag, which means the real state object is missing.

### Target structure

Create a private state object inside `src/audio/engine_loop.rs`:

```rust
struct AudioLoopState {
    output_stream: Option<OutputStream>,
    output_handle: Option<rodio::OutputStreamHandle>,
    preferred_output_device_name: Option<String>,
    stream_metadata_enabled: bool,
    reopen_output_on_next_connection: bool,

    current_sink: Option<Sink>,
    connect_thread: Option<std::thread::JoinHandle<Result<Sink, String>>>,

    active_conn_id: Arc<AtomicU64>,
    current_conn_id: u64,
    current_url: Option<String>,
    hardware_recovery_retries: u8,

    target_volume: f32,
    current_fade_volume: Option<f32>,
    pending_action: Option<AudioCommand>,
}

impl AudioLoopState {
    fn new() -> Self {
        Self {
            output_stream: None,
            output_handle: None,
            preferred_output_device_name: None,
            stream_metadata_enabled: true,
            reopen_output_on_next_connection: false,
            current_sink: None,
            connect_thread: None,
            active_conn_id: Arc::new(AtomicU64::new(0)),
            current_conn_id: 0,
            current_url: None,
            hardware_recovery_retries: 0,
            target_volume: 0.8,
            current_fade_volume: None,
            pending_action: None,
        }
    }
}
```

Then shrink `audio_loop`:

```rust
pub(super) fn audio_loop(
    cmd_rx: mpsc::Receiver<AudioCommand>,
    status_tx: mpsc::Sender<AudioStatus>,
    sample_buffer: Arc<Mutex<VecDeque<f32>>>,
) {
    let mut state = AudioLoopState::new();

    loop {
        match cmd_rx.recv_timeout(Duration::from_millis(10)) {
            Ok(cmd) => {
                if cfg!(test) {
                    handle_test_audio_command(cmd, &status_tx);
                    continue;
                }

                state.handle_command(cmd, &status_tx, &sample_buffer);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }

        state.tick_pending_action(&status_tx, &sample_buffer);
        state.tick_fade_in();
        state.tick_connection(&status_tx, &sample_buffer);
        state.tick_sink_end(&status_tx);
    }
}
```

### Extract methods one at a time

Do not extract everything in one commit. Recommended commit slices:

```text
Commit A: Introduce AudioLoopState::new and move fields only.
Commit B: Move Play/Pause/Resume/Stop command handling into handle_command.
Commit C: Move fade-out pending action into tick_pending_action.
Commit D: Move fade-in into tick_fade_in.
Commit E: Move connection completion into tick_connection.
Commit F: Move sink empty check into tick_sink_end.
```

Run tests after each commit or at least after A, C, and E.

### Method details

#### `AudioLoopState::handle_command`

Skeleton:

```rust
impl AudioLoopState {
    fn handle_command(
        &mut self,
        cmd: AudioCommand,
        status_tx: &mpsc::Sender<AudioStatus>,
        sample_buffer: &Arc<Mutex<VecDeque<f32>>>,
    ) {
        match cmd {
            AudioCommand::Play(url) => {
                if self.current_sink.is_some() {
                    self.pending_action = Some(AudioCommand::Play(url));
                } else {
                    self.start_connection(url, true, status_tx, sample_buffer);
                }
            }
            AudioCommand::Pause => {
                if let Some(ref sink) = self.current_sink {
                    self.pending_action = None;
                    self.current_fade_volume = None;
                    sink.pause();
                    let _ = status_tx.send(AudioStatus::Paused);
                }
            }
            AudioCommand::Resume => {
                if let Some(ref sink) = self.current_sink {
                    self.pending_action = None;
                    sink.play();
                    let _ = status_tx.send(AudioStatus::Playing);
                    self.current_fade_volume = Some(0.0);
                }
            }
            AudioCommand::Stop => {
                if self.current_sink.is_some() {
                    self.pending_action = Some(AudioCommand::Stop);
                } else {
                    self.active_conn_id.store(0, Ordering::SeqCst);
                    self.connect_thread = None;
                    let _ = status_tx.send(AudioStatus::Stopped);
                }
            }
            AudioCommand::SetVolume(vol) => {
                self.target_volume = vol;
                if self.current_fade_volume.is_none() && self.pending_action.is_none() {
                    if let Some(ref sink) = self.current_sink {
                        sink.set_volume(vol);
                    }
                }
            }
            AudioCommand::SetOutputDevice(device_name) => {
                self.preferred_output_device_name =
                    output::normalize_output_device_name(device_name.as_deref());

                if self.current_sink.is_some() {
                    self.reopen_output_on_next_connection = true;
                } else {
                    self.output_stream = None;
                    self.output_handle = None;
                    self.reopen_output_on_next_connection = false;
                }
            }
            AudioCommand::SetStreamMetadata(enabled) => {
                self.stream_metadata_enabled = enabled;
            }
        }
    }
}
```

This should be a mechanical move. Do not “improve” behavior while extracting.

#### `AudioLoopState::start_connection`

Move `start_connection` and `spawn_connection` into methods later:

```rust
impl AudioLoopState {
    fn start_connection(
        &mut self,
        url: String,
        reset_hardware_retries: bool,
        status_tx: &mpsc::Sender<AudioStatus>,
        sample_buffer: &Arc<Mutex<VecDeque<f32>>>,
    ) {
        self.current_url = Some(url.clone());
        if reset_hardware_retries {
            self.hardware_recovery_retries = 0;
        }
        self.spawn_connection(url, status_tx, sample_buffer);
    }

    fn spawn_connection(
        &mut self,
        url: String,
        status_tx: &mpsc::Sender<AudioStatus>,
        sample_buffer: &Arc<Mutex<VecDeque<f32>>>,
    ) {
        if self.reopen_output_on_next_connection {
            self.output_stream = None;
            self.output_handle = None;
            self.reopen_output_on_next_connection = false;
        }

        let Some(handle) = ensure_output_handle(
            &mut self.output_stream,
            &mut self.output_handle,
            self.preferred_output_device_name.as_deref(),
            status_tx,
        ) else {
            return;
        };

        self.current_conn_id += 1;
        self.active_conn_id.store(self.current_conn_id, Ordering::SeqCst);
        let _ = status_tx.send(AudioStatus::Connecting);

        let context = ConnectionContext {
            status_tx: status_tx.clone(),
            conn_id: self.current_conn_id,
            active_conn_id: self.active_conn_id.clone(),
            sample_buffer: sample_buffer.clone(),
            request_stream_metadata: self.stream_metadata_enabled,
        };

        drop(self.connect_thread.take());
        self.connect_thread = Some(std::thread::spawn(move || {
            connect_and_decode(url, handle, context)
        }));
    }
}
```

Once this exists, `SpawnConnectionState<'a>` can be deleted.

### Preserve current semantics exactly

The following behavior must remain unchanged:

```text
Play while current_sink exists:
- set pending_action = Some(AudioCommand::Play(url))
- fade out current sink before new connection

Stop while current_sink exists:
- set pending_action = Some(AudioCommand::Stop)
- fade out before stopping

Stop while no current_sink exists:
- active_conn_id = 0
- connect_thread = None
- send AudioStatus::Stopped

Pause:
- clear pending action
- clear fade-in state
- pause sink
- send AudioStatus::Paused

Resume:
- play sink
- send AudioStatus::Playing
- set fade-in from 0.0

SetOutputDevice while current_sink exists:
- do not tear down immediately
- set reopen_output_on_next_connection = true

SetOutputDevice while no current_sink exists:
- drop output stream and handle immediately

Hardware output error:
- retry once when error starts with HARDWARE_OUTPUT_ERROR_PREFIX
- reset output stream/handle before retry
```

## Step 3.3: Add behavior tests around helper functions and command failure

Current helper tests cover:

```text
fade_out_next_volume_uses_exponential_step
fade_out_complete_triggers_at_low_volume
clamp_status_volume_keeps_ui_payload_normalized
hardware_output_error_uses_recovery_prefix
non_hardware_error_does_not_trigger_recovery
reset_output_handle_accepts_empty_handles
```

Keep those tests.

Add tests that do not require real audio hardware:

### Test `AudioEngine::send` failure

```rust
#[test]
fn audio_engine_send_returns_false_when_command_channel_is_closed() {
    let engine = AudioEngine::disconnected_for_test();

    assert!(!engine.send(AudioCommand::Stop));
}
```

### Test app-level dead-engine handling

```rust
#[test]
fn play_selected_surfaces_dead_audio_engine() {
    let mut app = App::new(Library::in_memory(vec![station("A", "http://a")]));
    app.audio = AudioEngine::disconnected_for_test();

    app.play_selected();

    assert!(matches!(app.player.state, PlaybackState::Error(_)));
    assert!(matches!(app.notice.current, Some(AppNotice::Error(_))));
}
```

### Test volume sync does not mutate UI state

If `sync_volume` becomes `-> bool`:

```rust
#[test]
fn sync_volume_reports_dead_audio_engine_without_changing_playback_state() {
    let mut app = App::new(Library::in_memory(vec![station("A", "http://a")]));
    app.audio = AudioEngine::disconnected_for_test();
    app.player.state = PlaybackState::Playing;

    assert!(!app.sync_volume());
    assert_eq!(app.player.state, PlaybackState::Playing);
}
```

This prevents passive sync failures from stomping playback state.

## Step 3.4: Add an explicit manual playback checklist

Automated tests cannot fully prove live audio. Before tagging 0.4.1, manually run PulseDeck and verify:

```text
[ ] Start app with default library.
[ ] Play NightWave Plaza mp3.
[ ] Stop playback.
[ ] Play SomaFM Groove Salad mp3.
[ ] Switch from Groove Salad to DEF CON while playing; fade-out/fade-in should work.
[ ] Pause playing station.
[ ] Resume paused station.
[ ] Change volume while playing.
[ ] Mute/unmute while playing.
[ ] Stop while connecting.
[ ] Retry after a forced bad URL.
[ ] Toggle Stream Song Info Metadata off, play MP3 station, confirm playback still works.
[ ] Toggle Stream Song Info Metadata on, play MP3 station, confirm playback still works.
[ ] If an output device setting exists, select default output and confirm playback still works.
[ ] Quit while playing; app exits cleanly.
```

Optional but recommended:

```text
[ ] Play Nightride FM .m4a.
[ ] If it fails, confirm error is visible and not a silent spinner.
[ ] If it succeeds, add it to release notes as verified.
```

## Pitfalls

### Pitfall: `cfg!(test)` branch hides real engine behavior in tests

`audio_loop` currently contains:

```rust
if cfg!(test) {
    handle_test_audio_command(cmd, &status_tx);
    continue;
}
```

This means tests that exercise `AudioEngine::spawn` do not test the real connection path. That is fine for command/status smoke tests, but it is not playback coverage.

Do not claim automated tests prove live audio. They prove state transitions and helpers.

### Pitfall: dropping a join handle does not stop a thread

Current code uses `active_conn_id` to abandon stale connection threads. Keep that mechanism. Do not assume `drop(self.connect_thread.take())` cancels blocking HTTP or decode work. It only drops the handle.

The cancellation semantics are in `ConnectionContext`:

```text
src/audio/session.rs::ConnectionContext
- conn_id
- active_conn_id
```

Do not remove or bypass those fields.

### Pitfall: output handle lifetime

Rodio output requires the stream object to stay alive. Current state stores both:

```rust
output_stream: Option<OutputStream>
output_handle: Option<rodio::OutputStreamHandle>
```

Do not “simplify” by storing only the handle. That can kill output unexpectedly.

### Pitfall: hardware recovery retry loops

Current retry limit comes from:

```rust
// src/audio.rs
const MAX_HARDWARE_RECOVERY_RETRIES: u8 = 1;
```

Keep one retry. Do not increase this in 0.4.1. Infinite or repeated retries can create an error storm.

### Pitfall: Stop while connecting

When there is no current sink but a connection thread exists, `Stop` currently does:

```rust
active_conn_id.store(0, Ordering::SeqCst);
connect_thread = None;
let _ = status_tx.send(AudioStatus::Stopped);
```

Preserve this. Users need to be able to cancel a slow connection.

### Pitfall: stream metadata is not audio correctness

Do not diagnose every playback failure as ICY metadata. The setting is:

```text
src/favorites.rs::Settings::stream_metadata_enabled
```

And command is:

```rust
AudioCommand::SetStreamMetadata(bool)
```

Metadata can corrupt playback if stripped incorrectly, but the active failure surface also includes network, decoder, sink, output device, and stale connection cancellation.

## Definition of done for Fix 3

```text
[ ] AudioEngine::send returns bool.
[ ] User-initiated playback commands surface dead-engine failure.
[ ] Passive sync commands do not spam notices.
[ ] AudioLoopState exists and owns previous audio_loop locals.
[ ] SpawnConnectionState is removed or reduced to a temporary step during refactor.
[ ] audio_loop top-level loop is readable and delegates to methods.
[ ] Existing helper tests still pass.
[ ] New tests cover send failure and app-level dead-engine handling.
[ ] Manual playback checklist completed before tag.
```

---

# Release sequencing

## Recommended commit order

### Commit 1: Plan only

```text
plan.md
```

No code changes.

### Commit 2: Remove dead audio files

```text
src/audio/buffer.rs                 deleted
src/audio/buffer_meter.rs           deleted
src/audio/decoded_source.rs         deleted
src/audio/pcm_buffer.rs             deleted
src/audio/pcm_buffer2.rs            deleted
src/audio/probe_reader.rs           deleted
```

Run:

```bash
cargo check
cargo test
cargo clippy --all-targets --all-features
```

### Commit 3: Centralize station URL identity

```text
src/radio/station.rs
src/radio.rs
src/favorites.rs
src/app/selectors.rs
src/app/lifecycle.rs
```

Run:

```bash
cargo test station_url
cargo test station_identity
cargo test remove_matches_normalized
cargo test now_playing
cargo test
```

### Commit 4: Make audio send failure observable

```text
src/audio.rs
src/app/playback.rs
src/app/lifecycle.rs
src/app/settings.rs
```

Run:

```bash
cargo test audio_engine_send
cargo test dead_audio_engine
cargo test playback
cargo test
```

### Commit 5: Extract AudioLoopState fields only

```text
src/audio/engine_loop.rs
```

No semantic change. Run:

```bash
cargo test engine_loop
cargo test
```

### Commit 6: Move command handling into AudioLoopState

```text
src/audio/engine_loop.rs
```

Run:

```bash
cargo test engine_loop
cargo test playback
cargo test
```

### Commit 7: Move pending-action, fade-in, connection completion, and sink-end ticks

```text
src/audio/engine_loop.rs
```

Run full validation:

```bash
cargo check
cargo test
cargo clippy --all-targets --all-features
```

### Commit 8: Manual playback QA and release notes

```text
CHANGELOG.md
README.md, only if behavior/docs changed
```

Do manual playback checklist before this commit.

---

# Test strategy

## Automated tests required

Run all of these before merging:

```bash
cargo check
cargo test
cargo clippy --all-targets --all-features
```

Targeted tests to add and keep:

```text
src/radio/station.rs
- station_url_matches_ignores_case_whitespace_and_trailing_slash
- station_identity_matches_trims_uuid_before_comparing
- station_identity_falls_back_to_normalized_url_when_uuid_missing
- station_identity_prefers_uuid_mismatch_over_url_match

src/favorites.rs
- remove_matches_normalized_station_url
- contains_matches_normalized_station_url
- station_health_matches_normalized_url

src/app/selectors.rs
- now_playing_matches_normalized_library_url
- select_playing_matches_normalized_visible_url

src/audio.rs or src/audio/engine_loop.rs
- audio_engine_send_returns_false_when_command_channel_is_closed

src/app/playback.rs
- play_selected_surfaces_dead_audio_engine
- sync_volume_reports_dead_audio_engine_without_changing_playback_state, if sync_volume returns bool
```

## Manual playback tests required

Use real terminal and real audio output.

```text
[ ] MP3 station starts within reasonable time.
[ ] Stop works while playing.
[ ] Stop works while connecting.
[ ] Switching stations fades out old station and starts new station.
[ ] Pause and resume work.
[ ] Volume changes while playing.
[ ] Mute and unmute work.
[ ] Metadata on/off does not break MP3 playback.
[ ] Bad URL produces visible error and reconnect behavior is understandable.
[ ] Quit while playing does not hang.
```

Use at least these stations:

```text
NightWave Plaza           https://radio.plaza.one/mp3
SomaFM Groove Salad       https://ice2.somafm.com/groovesalad-128-mp3
SomaFM DEF CON            https://ice2.somafm.com/defcon-128-mp3
Nightride FM              https://stream.nightride.fm/nightride.m4a
```

If Nightride `.m4a` fails, record it honestly. Do not silently ship a default station that cannot play unless the release notes say MP3-only or the default list is adjusted.

---

# Rollback strategy

## If deletion of dead audio files causes trouble

This should not happen because they are uncompiled. If it does:

```bash
git restore src/audio/buffer.rs
git restore src/audio/buffer_meter.rs
git restore src/audio/decoded_source.rs
git restore src/audio/pcm_buffer.rs
git restore src/audio/pcm_buffer2.rs
git restore src/audio/probe_reader.rs
```

Then investigate why an uncompiled file mattered. That would mean tooling outside Rust compilation depends on it.

## If identity centralization causes duplicate behavior changes

Revert only the call-site changes first, not the helper addition.

High-risk spots:

```text
src/favorites.rs::remove
src/favorites.rs::contains
src/app/selectors.rs::now_playing
src/app/lifecycle.rs::handle_track_changed
```

The helper itself is safe. Behavior changes come from adopting it.

## If AudioLoopState extraction breaks playback

Revert the extraction commits, but keep:

```text
AudioEngine::send -> bool
app-level dead-engine handling
```

Those are stabilization improvements independent of the engine-loop refactor.

Do not attempt to fix broken extraction by adding more state flags. If playback breaks after extraction, compare behavior against pre-extraction code and restore exact order of operations.

---

# Known non-goals for 0.4.1

Do not include these unless they are required to fix a regression introduced by the plan:

```text
- No new buffering architecture.
- No decoded PCM queue revival.
- No async rewrite of the audio thread.
- No broad Rodio replacement.
- No new UI overlay.
- No new search prefixes.
- No library file format migration except accidental compatibility fixes.
- No station ranking overhaul.
- No new Radio Browser server strategy.
- No large App constructor split unless needed for tests.
```

The review suggested splitting `App::new` eventually. That is a good future refactor, but for 0.4.1 audio is the burning room. Split `App::new` later unless tests require a tiny test-only injection point.

---

# Future work after 0.4.1

After this release is stable, consider these for 0.4.2 or 0.5.0:

```text
1. Split App::new into pure construction and runtime wiring.
2. Move playlist export out of src/app/playback.rs.
3. Remove SavedStation duplication and serialize Station directly.
4. Convert search prefix handling to metadata-driven specs.
5. Move generic text helpers out of src/ui/text.rs.
6. Investigate generic decoder support for AAC/M4A/OGG with explicit stream compatibility tests.
7. Add an AudioPort trait so app tests do not spawn real audio threads.
```

---

# Final 0.4.1 release gate

Do not tag 0.4.1 until all are true:

```text
[ ] Dead audio prototype files are gone from src/audio/.
[ ] cargo check passes.
[ ] cargo test passes.
[ ] cargo clippy --all-targets --all-features passes.
[ ] Station identity tests cover normalized URL matching.
[ ] Dead audio engine send failure is visible to app state.
[ ] Manual playback checklist completed on a real machine.
[ ] CHANGELOG.md says this is a stabilization release.
[ ] Any known codec limitation is documented honestly.
```

If any manual playback item fails, 0.4.1 is not ready. No vibes-based shipping. No “works on the CI goblin.” Sound must actually come out of speakers.
