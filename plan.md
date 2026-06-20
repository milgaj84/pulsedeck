# PulseDeck 0.4.2 Maintainability Plan

Release theme: **Make the boring parts actually boring**.

0.4.1 stabilized the scary audio seam: dead audio prototypes are gone, station identity is centralized, and audio engine command-channel failures are visible. 0.4.2 should continue the code-review cleanup without yanking on the speaker wire. The goal is to remove duplication, clarify ownership boundaries, and shrink the number of places future changes can accidentally fork behavior.

This is a maintenance release. It should not change playback semantics, decoder selection, station ranking behavior, or the terminal interaction model unless a test proves the old and new behavior are equivalent.

---

## Non-negotiable rules for 0.4.2

### Rule 1: Do not touch active audio decoding

The active stream path remains:

```text
src/app/playback.rs::play_selected
  -> src/app/playback.rs::send_audio_command
  -> src/audio.rs::AudioEngine::send
  -> src/audio/engine_loop.rs::audio_loop
  -> src/audio/engine_loop.rs::AudioLoopState
  -> src/audio/session.rs::connect_and_decode
  -> src/audio/session.rs::try_connect_and_decode_once
  -> src/audio/stream_reader.rs::StreamReader
  -> rodio::Sink
```

0.4.2 may compile against audio code and may adjust documentation around audio, but it must not replace `Decoder::new_mp3`, buffering, reconnect timing, fade timing, output device selection, or connection-thread behavior. Those belong in a dedicated audio QA release.

### Rule 2: Prefer data tables over repeated match chains

When behavior is duplicated across help text, parsing, API parameters, and labels, 0.4.2 should collapse it into one metadata table.

### Rule 3: Domain modules must not depend on UI modules

`favorites`, `radio`, `playlist`, `history`, and `config` are domain/service code. They must not call `crate::ui::*`. Generic helpers belong in root-level modules such as `src/text.rs`.

### Rule 4: Serialization changes must be backward-compatible

`library.json` is user data. Any cleanup to `src/favorites.rs` must keep current JSON readable. New serialization should preserve field names, defaults, and `skip_serializing_if` behavior.

### Rule 5: Every refactor gets a regression test or an explicit reason it is compile-only

The preferred gate is:

```text
cargo check
cargo test
cargo clippy --all-targets --all-features
```

For each implemented cluster, run at least the relevant focused tests first, then full validation at the end.

---

## Scope for 0.4.2

### Fix A: Remove `SavedStation` duplication and serialize `Station` directly

#### Files and symbols

- `src/radio/station.rs::Station`
- `src/radio/station.rs::StationHealth`
- `src/favorites.rs::LibraryFile`
- `src/favorites.rs::SavedStation`
- `src/favorites.rs::parse_library_file`
- `src/favorites.rs::Library::save`

#### Current problem

`Station` already derives `Serialize` and `Deserialize` and has the exact serde field attributes needed for compact `library.json` output. `src/favorites.rs` still defines a second `SavedStation` struct that mirrors `Station` field-for-field, then converts back and forth:

```rust
// src/favorites.rs
struct SavedStation {
    name: String,
    url: String,
    genre: String,
    country: String,
    bitrate: u32,
    station_uuid: Option<String>,
    country_code: String,
    tags: Vec<String>,
    language: String,
    codec: String,
    homepage: String,
    last_check_ok: Option<bool>,
    votes: Option<u32>,
    click_count: Option<u32>,
    health: StationHealth,
}

impl From<&Station> for SavedStation { /* field copy */ }
impl From<SavedStation> for Station { /* field copy + normalization */ }
```

This creates a future bug hatch. Every new station field must be added in three places:

1. `src/radio/station.rs::Station`
2. `src/favorites.rs::SavedStation`
3. the two conversion impls

#### Desired design

`LibraryFile` should store `Vec<Station>` directly:

```rust
// src/favorites.rs
#[derive(Serialize, Deserialize)]
struct LibraryFile {
    #[serde(default = "default_library_version")]
    version: u32,
    #[serde(default)]
    stations: Vec<Station>,
    #[serde(default)]
    settings: Settings,
}
```

Normalization after load should be explicit and named:

```rust
fn normalize_loaded_station(mut station: Station) -> Station {
    station.country = station.country.trim().to_string();
    station.bitrate = sanitize_bitrate(station.bitrate);
    station.station_uuid = station.station_uuid.and_then(normalize_station_uuid);
    station.country_code = normalize_country_code(&station.country_code);
    station.tags = clean_tag_values(station.tags);
    station.language = station.language.trim().to_string();
    station.codec = normalize_codec(&station.codec);
    station.homepage = station.homepage.trim().to_string();
    station
}
```

Then parsing becomes:

```rust
fn parse_library_file(
    contents: &str,
) -> serde_json::Result<(Vec<Station>, Settings, Option<String>)> {
    let file = serde_json::from_str::<LibraryFile>(contents)?;
    let warning = if file.version == 1 {
        None
    } else {
        Some(format!(
            "Library file version {} is newer than supported version 1",
            file.version
        ))
    };

    Ok((
        file.stations.into_iter().map(normalize_loaded_station).collect(),
        file.settings,
        warning,
    ))
}
```

Saving becomes:

```rust
let file = LibraryFile {
    version: 1,
    stations: self.stations.clone(),
    settings: self.settings.clone(),
};
```

#### Required tests

Add or keep tests proving:

- missing optional station fields still deserialize via `Station` defaults
- invalid/high bitrate is sanitized on load
- UUIDs are trimmed on load
- country codes are uppercased on load
- tags are trimmed and deduplicated on load
- JSON output still omits empty optional fields because `Station` owns the serde attributes

#### Pitfalls

- Do not remove serde attributes from `Station`. They are now the persistence contract.
- Do not normalize user-facing `name`, `url`, or `genre` during load. Those are saved-facing fields.
- Do not collapse `StationHealth::default()` behavior. Existing libraries without health must still load.

---

### Fix B: Deduplicate `Library::load` and `Library::load_existing`

#### Files and symbols

- `src/favorites.rs::Library::load`
- `src/favorites.rs::Library::load_existing`
- `src/favorites.rs::parse_library_file`
- `src/favorites.rs::config_path`

#### Current problem

`Library::load` and `Library::load_existing` repeat the same disk-read and parse logic. Their only policy difference is missing/corrupt fallback behavior:

- `Library::load(seed_stations)` seeds starter stations and writes `library.json` on first launch.
- `Library::load_existing()` returns an empty library without seeding or writing, for read-only/CLI use.

Repeated parse/read logic makes future migrations twice as fragile.

#### Desired design

Introduce an internal policy enum:

```rust
#[derive(Debug, Clone)]
enum MissingLibraryPolicy {
    SeedAndSave(Vec<Station>),
    Empty,
}
```

Then route both public constructors through one private implementation:

```rust
impl Library {
    pub fn load(seed_stations: Vec<Station>) -> Self {
        Self::load_with_policy(MissingLibraryPolicy::SeedAndSave(seed_stations))
    }

    pub fn load_existing() -> Self {
        Self::load_with_policy(MissingLibraryPolicy::Empty)
    }

    fn load_with_policy(policy: MissingLibraryPolicy) -> Self {
        let path = config_path();
        let mut load_warnings = Vec::new();

        let (stations, settings, should_save_seed) = match path.as_ref() {
            Some(path) if path.exists() => load_existing_file(path, &policy, &mut load_warnings),
            Some(_) | None => fallback_for_missing_library(&policy),
        };

        let mut library = Self {
            stations,
            available_genres: Vec::new(),
            settings,
            path,
            load_warnings,
        };
        library.rebuild_genres();

        if should_save_seed {
            if let Err(err) = library.save() {
                library
                    .load_warnings
                    .push(format!("Could not save starter library: {err}"));
            }
        }

        library
    }
}
```

The helper should be private and boring:

```rust
fn fallback_for_missing_library(policy: &MissingLibraryPolicy) -> (Vec<Station>, Settings, bool) {
    match policy {
        MissingLibraryPolicy::SeedAndSave(stations) => {
            (stations.clone(), Settings::default(), true)
        }
        MissingLibraryPolicy::Empty => (Vec::new(), Settings::default(), false),
    }
}
```

#### Required tests

Use existing parse tests plus add targeted unit helpers if possible:

- `Library::load_existing` does not seed starter stations when no file exists
- corrupt file fallback warning wording remains different for starter vs empty policy
- first-launch seed still calls save best-effort when a config path exists

If config-path injection is not currently possible, do not add brittle environment tests. Keep the implementation private and rely on existing behavior plus pure helper tests.

#### Pitfalls

- Do not make `load_existing()` write a starter library.
- Do not lose load warnings from parse/read failures.
- Do not return before `rebuild_genres()`.
- Do not move `config_path()` behavior in this release.

---

### Fix C: Make search prefixes metadata-driven

#### Files and symbols

- `src/radio/query.rs::SearchField`
- `src/radio/query.rs::SearchPrefixHelp`
- `src/radio/query.rs::SEARCH_PREFIX_HELP`
- `src/radio/query.rs::prefix_examples_inline`
- `src/radio/query.rs::known_search_prefix`
- `src/radio/query.rs::StationSearchQuery::parse`
- `src/radio/query.rs::StationSearchQuery::api_params`
- `src/radio/query.rs::StationSearchQuery::display_label`
- `src/ui/help.rs` if it consumes `SEARCH_PREFIX_HELP`

#### Current problem

Prefix behavior is repeated in several forms:

```rust
pub const SEARCH_PREFIX_HELP: &[SearchPrefixHelp] = &[ /* help only */ ];

match prefix.trim().to_ascii_lowercase().as_str() {
    "name" | "station" => SearchField::Name,
    "tag" | "genre" => SearchField::Tag,
    "country" | "cc" => SearchField::Country or CountryCode,
    "lang" | "language" => SearchField::Language,
    "codec" | "format" => SearchField::Codec,
    _ => plain,
}

match self.field {
    SearchField::Name => params.push(("name", ...)),
    SearchField::Tag => params.push(("tag", ...)),
    SearchField::Country => params.push(("country", ...)),
    SearchField::CountryCode => params.push(("countrycode", ...)),
    SearchField::Language => params.push(("language", ...)),
    SearchField::Codec => params.push(("codec", ...)),
}
```

Adding a new prefix currently requires editing help, parser, API mapping, display labels, and tests.

#### Desired design

Turn help metadata into behavior metadata:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchPrefixHelp {
    pub prefix: &'static str,
    pub aliases: &'static [&'static str],
    pub field: SearchField,
    pub api_param: &'static str,
    pub label_prefix: &'static str,
    pub label: &'static str,
    pub example: &'static str,
}
```

Use a lookup helper:

```rust
fn search_prefix(prefix: &str) -> Option<&'static SearchPrefixHelp> {
    let prefix = prefix.trim();
    SEARCH_PREFIX_HELP.iter().find(|help| {
        help.prefix.eq_ignore_ascii_case(prefix)
            || help.aliases.iter().any(|alias| alias.eq_ignore_ascii_case(prefix))
    })
}
```

Handle country-code detection as the only special case because it changes the API param from `country` to `countrycode`:

```rust
fn field_for_prefix(help: &SearchPrefixHelp, value: &str) -> (SearchField, String) {
    if help.field == SearchField::Country && is_country_code(value) {
        (SearchField::CountryCode, value.to_ascii_uppercase())
    } else {
        (help.field, value.to_string())
    }
}
```

Generate examples from the same table:

```rust
pub fn prefix_examples_inline() -> String {
    let examples = SEARCH_PREFIX_HELP
        .iter()
        .map(|help| help.example)
        .collect::<Vec<_>>()
        .join(", ");
    format!("try {examples}")
}
```

#### Required tests

- existing prefix tests remain green
- `known_search_prefix` recognizes aliases through the metadata table
- `prefix_examples_inline` contains all table examples and has no separately hard-coded list
- `display_label` continues to display `country:BA` for `countrycode`

#### Pitfalls

- Keep `SearchField::CountryCode`; it represents API behavior, not user syntax.
- Do not expose `countrycode:` as a user prefix unless intentionally added.
- `prefix_examples_inline` changes return type from `&'static str` to `String`; update callers if any expect a static string.

---

### Fix D: Move playlist export out of playback state logic

#### Files and symbols

- `src/app/playback.rs::App::export_library`
- `src/playlist.rs::to_m3u`
- new `src/playlist_export.rs::export_library_m3u`
- `src/main.rs` module declarations

#### Current problem

`App::export_library` lives in `src/app/playback.rs`, but it is not playback behavior. It performs filesystem export:

```rust
std::fs::create_dir_all(&dir)
std::fs::write(&filepath, m3u_content)
```

That tangles UI/app state with low-level persistence details and makes `playback.rs` a junk drawer.

#### Desired design

Create a tiny service module:

```rust
// src/playlist_export.rs
use std::path::{Path, PathBuf};

use crate::radio::Station;

pub fn export_library_m3u(
    stations: &[Station],
    dir: &Path,
    unix_time: u64,
) -> anyhow::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let filepath = dir.join(format!("pulsedeck-export-{unix_time}.m3u"));
    std::fs::write(&filepath, crate::playlist::to_m3u(stations))?;
    Ok(filepath)
}
```

Then `App::export_library` becomes UI-facing glue:

```rust
pub(super) fn export_library(&mut self) {
    let Some(dir) = self.export_directory() else {
        self.set_error_notice("Could not resolve config directory for export");
        return;
    };

    match crate::playlist_export::export_library_m3u(
        &self.library.stations,
        &dir,
        current_unix_time(),
    ) {
        Ok(path) => self.set_info_notice(format!("Library exported to {}", path.display())),
        Err(err) => self.set_error_notice(format!("Export failed: {err}")),
    }
}
```

Optionally add small helpers:

```rust
fn current_unix_time() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
```

#### Required tests

- `export_library_m3u` creates the directory if missing
- exported file name includes the supplied timestamp
- exported content comes from `playlist::to_m3u`

#### Pitfalls

- Keep `App::export_library` responsible for notices. Service functions should return `Result` and not know about UI.
- Keep timestamp injectable in the service for deterministic tests.
- Do not introduce `dirs` dependency into `playlist_export.rs`; directory selection remains app/config glue.

---

### Fix E: Move generic text helpers out of `ui`

#### Files and symbols

- new `src/text.rs`
- `src/ui/text.rs`
- `src/favorites.rs::compact_error_summary`
- `src/ui/stations.rs`
- `src/main.rs` module declarations

#### Current problem

`src/favorites.rs::compact_error_summary` calls:

```rust
crate::ui::text::truncate_with_ellipsis(error.trim(), 96)
```

That makes domain/library code depend on UI code.

#### Desired design

Move the real implementation into root-level `src/text.rs`:

```rust
pub fn visible_len(text: &str) -> usize {
    text.chars().count()
}

pub fn truncate_to_chars(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

pub fn truncate_with_ellipsis(value: &str, max_chars: usize) -> String {
    let value_len = visible_len(value);
    if value_len <= max_chars {
        return value.to_string();
    }

    if max_chars <= 1 {
        return "…".to_string();
    }

    let mut truncated = value.chars().take(max_chars - 1).collect::<String>();
    truncated.push('…');
    truncated
}
```

Then `favorites.rs` uses:

```rust
crate::text::truncate_with_ellipsis(error.trim(), 96)
```

`src/ui/text.rs` can either be deleted or become a compatibility facade:

```rust
pub use crate::text::{truncate_to_chars, truncate_with_ellipsis, visible_len};
```

Preferred long-term end state: delete `src/ui/text.rs` and point UI callers to `crate::text`.

#### Required tests

- existing text truncation tests move with the implementation to `src/text.rs`
- `compact_error_summary` behavior remains unchanged through existing failure-health tests

#### Pitfalls

- Do not use byte length for truncation. Keep char-aware behavior.
- Keep the `max_chars <= 1` behavior returning only `…`.

---

### Fix F: Remove unused dependency/config noise

#### Files and symbols

- `Cargo.toml`
- `Cargo.lock`

#### Current problem

`Cargo.toml` enables `crossterm` feature `event-stream`, but `src/event.rs::poll_action` uses blocking `event::poll` and `event::read`, not `EventStream`.

It also has commented tracing dependencies:

```toml
# Logging (optional, useful for debugging)
# tracing = "0.1"
# tracing-subscriber = "0.3"
```

#### Desired design

Use plain crossterm:

```toml
crossterm = "0.29"
```

Remove commented dependency tombstones. If tracing returns later, add it through an actual feature.

#### Required tests

- `cargo check`
- `cargo clippy --all-targets --all-features`

#### Pitfalls

- If a future async input refactor lands, it can re-enable `event-stream` deliberately.
- Let `cargo` update `Cargo.lock`; do not hand-edit lockfile.

---

## Implementation order

### Step 1: Text helper boundary

1. Add `mod text;` to `src/main.rs`.
2. Create `src/text.rs` with the current helper implementations and tests.
3. Turn `src/ui/text.rs` into a re-export facade or update all UI callers directly.
4. Change `src/favorites.rs::compact_error_summary` to call `crate::text::truncate_with_ellipsis`.
5. Run:

```text
cargo test text
cargo check
```

### Step 2: Playlist export service

1. Add `mod playlist_export;` to `src/main.rs`.
2. Create `src/playlist_export.rs`.
3. Move filesystem export mechanics out of `src/app/playback.rs::export_library`.
4. Keep user notices inside `App::export_library`.
5. Run:

```text
cargo test playlist_export
cargo check
```

### Step 3: Direct `Station` persistence

1. Change `LibraryFile.stations` from `Vec<SavedStation>` to `Vec<Station>`.
2. Delete `SavedStation` and both conversion impls.
3. Add `normalize_loaded_station`.
4. Change `parse_library_file` to map through `normalize_loaded_station`.
5. Change `Library::save` to clone `self.stations` directly.
6. Add/adjust serialization tests.
7. Run:

```text
cargo test favorites
cargo check
```

### Step 4: Deduplicate library load policy

1. Add `MissingLibraryPolicy`.
2. Add `load_with_policy`.
3. Add `read_library_file_or_fallback` and `fallback_for_missing_library` private helpers.
4. Route `load` and `load_existing` through the shared implementation.
5. Run:

```text
cargo test favorites
cargo check
```

### Step 5: Metadata-driven search prefixes

1. Extend `SearchPrefixHelp` with `field`, `api_param`, and `label_prefix`.
2. Add `search_prefix` lookup helper.
3. Rewrite `known_search_prefix`, `parse`, `api_params`, and `display_label` around metadata.
4. Change `prefix_examples_inline` to return `String`.
5. Update call sites that expect `&'static str`.
6. Run:

```text
cargo test radio::query
cargo check
```

### Step 6: Cargo cleanup

1. Change `crossterm` to plain version string.
2. Remove commented tracing dependencies.
3. Run:

```text
cargo check
cargo test
cargo clippy --all-targets --all-features
```

---

## Final validation checklist

Before tagging 0.4.2:

```text
cargo check
cargo test
cargo clippy --all-targets --all-features
```

Manual smoke check:

```text
pulsedeck
/ search still opens
name:lofi searches by name
station:lofi aliases name
tag:ambient searches by tag
genre:ambient aliases tag
country:BA searches by country code
country:Bosnia searches by country name
lang:english searches by language
language:serbian aliases lang
codec:mp3 searches by codec
format:mp3 aliases codec
export library creates an M3U file
play/pause/stop still show visible state changes
```

Audio caution: because 0.4.2 deliberately avoids decoder changes, a basic play/stop smoke test is enough. Full stream compatibility testing belongs to the next audio-specific release.
