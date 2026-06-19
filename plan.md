# PulseDeck 0.3.1 Release Plan

Target release: `0.3.1`
Base branch: `feature/0.3.0-radio-prefixes`
SemVer scope: patch release for the `0.3.0` structured-search upgrade
Date drafted: 2026-06-18

## Release theme

PulseDeck `0.3.1` should polish the structured Radio Browser search work introduced in `0.3.0` without turning into a disguised `0.4.0` feature crate. The release theme is:

> Structured search, now smoother.

`0.3.0` introduced:

- Structured search prefixes such as `tag:ambient`, `country:BA`, `lang:english`, `codec:mp3`, and `name:lofi`.
- Richer station metadata from Radio Browser.
- Station Details trust metadata.
- Result deduplication and ranking.
- UUID-aware saved station detection.
- A split radio module structure: query parsing, client, mapping, ranking, station model.

`0.3.1` should make those same features more robust, more discoverable, and safer around odd real-world API data. No recording features, no new playback architecture, no multi-filter query language, and no new major UX surface.

## Non-goals

Keep these out of `0.3.1` unless they are strictly necessary to fix a regression:

- Multi-prefix search such as `tag:jazz country:BA codec:mp3`.
- Boolean search syntax.
- Search history.
- Filter sidebars or new settings panels.
- Recording or local tape workflows.
- Audio engine rewrites.
- Breaking changes to `library.json`.
- A schema version bump unless absolutely required.

If any implementation starts requiring broad behavior changes, split it into a later `0.4.0` plan.

## Current 0.3.0 code map

Important files on the `feature/0.3.0-radio-prefixes` branch:

```text
Cargo.toml                         version = 0.3.0
CHANGELOG.md                       0.3.0 changelog exists, Unreleased is empty
docs/releases/0.3.0.md             0.3.0 release notes
src/radio.rs                       public search entrypoint and server fallback orchestration
src/radio/query.rs                 StationSearchQuery parser and API parameter builder
src/radio/client.rs                reqwest client and mirror search attempts
src/radio/map.rs                   Radio Browser API model to Station mapping
src/radio/rank.rs                  dedupe and local ranking
src/radio/station.rs               Station model, identity matching, fallback stations
src/favorites.rs                   Library persistence, add/import/remove, contains_station
src/app/search.rs                  search input lifecycle, confirm, audition, response application
src/ui/search.rs                   search overlay rendering and empty/error hints
src/ui/station_details.rs          metadata/trust rendering
src/ui/help.rs                     in-app help overlay
README.md                          user docs
```

Current `0.3.0` search flow:

```text
User types in search
    -> src/app/search.rs refresh_search_state
    -> debounce marks pending query
    -> event loop calls radio::search_stations(raw_query)
    -> src/radio.rs parses StationSearchQuery
    -> src/radio/client.rs tries HTTPS mirrors, then HTTP mirrors
    -> src/radio/map.rs maps API rows to Station
    -> src/radio/rank.rs dedupes and ranks
    -> App::apply_search_response stores results or error/empty status
    -> src/ui/search.rs renders list, saved stars, hints, errors
```

Current persistence flow:

```text
Search result selected
    -> App::confirm_search
    -> Library::add(station)
    -> Library::contains_station checks UUID first, then normalized URL
    -> mark_library_dirty if newly added
    -> background/debounced persistence writes library.json
```

The `0.3.1` work should mostly refine the edges of those flows.

## Implementation order

Recommended order keeps risk low and makes tests useful quickly:

1. Extend query parsing metadata and aliases in `src/radio/query.rs`.
2. Improve empty-result and prefix guidance in `src/ui/search.rs`, `src/ui/help.rs`, and README.
3. Harden station metadata normalization in `src/radio/station.rs` and `src/radio/map.rs`.
4. Add saved-station metadata enrichment in `src/radio/station.rs`, `src/favorites.rs`, and `src/app/search.rs`.
5. Tune ranking in `src/radio/rank.rs`.
6. Improve Radio Browser error presentation in `src/radio/client.rs`, `src/radio.rs`, and `src/ui/search.rs`.
7. Add compatibility and regression tests across query, map, rank, favorites, app search, and UI helper modules.
8. Update docs, changelog, release notes, and version number.
9. Run release checks.

## Global acceptance criteria

`0.3.1` is done when:

- `cargo fmt --check` passes.
- `cargo clippy --all-targets -- -D warnings` passes, or existing warnings are documented if clippy is not currently clean.
- `cargo test` passes.
- Old `library.json` files without new metadata still load.
- New saved stations preserve metadata across save/load.
- Existing saved stations can be enriched from matching search results.
- Prefixes are easier to discover from the app, README, and release notes.
- Failed search mirrors produce friendly user-facing messages without deleting detailed debugging context from code/tests.

---

# 1. Better prefix guidance in the UI

## Goal

Make structured search self-teaching. Users should not need to read the release notes to guess that `tag:`, `country:`, `lang:`, `codec:`, and `name:` exist.

## Files

```text
src/radio/query.rs
src/ui/search.rs
src/ui/help.rs
README.md
docs/releases/0.3.1.md
```

## Design

Add a tiny query help model in `src/radio/query.rs` so UI and docs can reuse canonical prefix labels instead of scattering strings.

Suggested API:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchPrefixHelp {
    pub prefix: &'static str,
    pub aliases: &'static [&'static str],
    pub label: &'static str,
    pub example: &'static str,
}

pub const SEARCH_PREFIX_HELP: &[SearchPrefixHelp] = &[
    SearchPrefixHelp {
        prefix: "name",
        aliases: &["station"],
        label: "station name",
        example: "name:lofi",
    },
    SearchPrefixHelp {
        prefix: "tag",
        aliases: &["genre"],
        label: "genre or tag",
        example: "tag:ambient",
    },
    SearchPrefixHelp {
        prefix: "country",
        aliases: &["cc"],
        label: "country name or code",
        example: "country:BA",
    },
    SearchPrefixHelp {
        prefix: "lang",
        aliases: &["language"],
        label: "language",
        example: "lang:english",
    },
    SearchPrefixHelp {
        prefix: "codec",
        aliases: &["format"],
        label: "stream codec",
        example: "codec:mp3",
    },
];

pub fn prefix_examples_inline() -> &'static str {
    "Try tag:ambient, country:BA, lang:english, codec:mp3, or name:lofi"
}
```

Use this in `src/ui/search.rs` instead of hardcoded examples.

Suggested footer copy while search input is active:

```text
Search prefixes: tag:ambient  country:BA  lang:english  codec:mp3
```

If the footer is already crowded, use a compact version in the search title or empty-result area.

## Implementation notes

Current `src/ui/search.rs` already has `empty_search_hint(query: &str)`. Move prefix examples behind a helper in `src/radio/query.rs` and import it:

```rust
use crate::radio::{prefix_examples_inline, StationSearchQuery};
```

If `prefix_examples_inline` feels too UI-specific for `radio`, put it in `src/ui/search.rs`, but keep a single canonical list of prefixes in `query.rs` so future docs and UI do not drift apart.

## Help overlay

Add a short section to `src/ui/help.rs` near search controls:

```text
Search prefixes
  tag:ambient       find stations by genre/tag
  country:BA        country name or two-letter code
  lang:english      station language
  codec:mp3         stream codec
  name:lofi         station name search
```

Keep the wording compact. Help already has many shortcuts, so this should be a small block rather than a tutorial mural.

## Pitfalls

- Do not add a new input mode or settings row just for prefix help.
- Do not show a huge prefix wall in tiny terminals. Compact-screen protection already exists, but the normal UI still needs restraint.
- Do not duplicate prefix lists manually in many files. One canonical list, small formatting helpers.

## Edge cases

- Empty library plus search open: onboarding card should not fight the search help.
- Very narrow terminals: hints must truncate cleanly.
- Prefix examples should not imply multi-prefix support yet.

## Tests

Add tests around helper formatting if non-trivial:

```rust
#[test]
fn prefix_examples_include_all_supported_prefixes() {
    let examples = prefix_examples_inline();
    for expected in ["tag:", "country:", "lang:", "codec:", "name:"] {
        assert!(examples.contains(expected));
    }
}
```

---

# 2. More forgiving prefix aliases

## Goal

Accept natural alias guesses without changing the core search model.

New aliases proposed for `0.3.1`:

```text
station:lofi   -> name:lofi
cc:BA          -> country:BA / countrycode
format:mp3     -> codec:mp3
```

Already supported in `0.3.0`:

```text
genre:jazz     -> tag:jazz
language:en    -> lang:en / language
```

## Files

```text
src/radio/query.rs
src/radio.rs
README.md
src/ui/help.rs
```

## Current parser

`StationSearchQuery::parse` currently does:

```rust
match prefix.trim().to_ascii_lowercase().as_str() {
    "name" => Self::with_field(raw_trimmed, SearchField::Name, value),
    "tag" | "genre" => Self::with_field(raw_trimmed, SearchField::Tag, value),
    "country" => { ... }
    "lang" | "language" => Self::with_field(raw_trimmed, SearchField::Language, value),
    "codec" => Self::with_field(raw_trimmed, SearchField::Codec, value),
    _ => Self::plain(raw_trimmed),
}
```

## Suggested change

```rust
match prefix.trim().to_ascii_lowercase().as_str() {
    "name" | "station" => Self::with_field(raw_trimmed, SearchField::Name, value),
    "tag" | "genre" => Self::with_field(raw_trimmed, SearchField::Tag, value),
    "country" | "cc" => {
        if is_country_code(&value) {
            Self::with_field(raw_trimmed, SearchField::CountryCode, value.to_ascii_uppercase())
        } else {
            Self::with_field(raw_trimmed, SearchField::Country, value)
        }
    }
    "lang" | "language" => Self::with_field(raw_trimmed, SearchField::Language, value),
    "codec" | "format" => Self::with_field(raw_trimmed, SearchField::Codec, value),
    _ => Self::plain(raw_trimmed),
}
```

## Optional parser metadata

It may be useful to preserve whether a prefix was recognized so the UI can explain unknown prefixes:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StationSearchQuery {
    raw: String,
    value: String,
    field: SearchField,
    recognized_prefix: Option<String>,
    unknown_prefix: Option<String>,
}
```

But avoid overengineering. A simpler helper is enough:

```rust
pub fn known_search_prefix(prefix: &str) -> bool {
    matches!(
        prefix.trim().to_ascii_lowercase().as_str(),
        "name" | "station" | "tag" | "genre" | "country" | "cc" |
        "lang" | "language" | "codec" | "format"
    )
}
```

Then UI can detect `foo:bar` where `foo` is not known.

## Pitfalls

- `cc:` should not map to country name for a long value unless intentionally allowed. Either support both or document that `cc:` is for two-letter country code. Recommendation: allow both for simplicity, but if value is not two letters, map to `Country` exactly like `country:`.
- Do not parse every colon as structured syntax if the prefix is unknown. Current fallback to plain search is good.
- Keep plain text search unchanged.
- Do not make prefix names case-sensitive.

## Edge cases

Add tests for:

```text
station:lofi
STATION:lofi
cc:ba
cc:Bosnia
format:mp3
FORMAT:AAC
unknown:value
http://example  // should remain plain name search if typed somehow
```

## Tests

Add to `src/radio/query.rs`:

```rust
#[test]
fn station_alias_maps_to_name() {
    let query = StationSearchQuery::parse("station:lofi");
    assert_eq!(query.field(), SearchField::Name);
    assert_eq!(param_value(&query, "name").as_deref(), Some("lofi"));
}

#[test]
fn cc_alias_maps_to_country_code_when_two_letters() {
    let query = StationSearchQuery::parse("cc:ba");
    assert_eq!(query.field(), SearchField::CountryCode);
    assert_eq!(query.value(), "BA");
    assert_eq!(param_value(&query, "countrycode").as_deref(), Some("BA"));
}

#[test]
fn format_alias_maps_to_codec() {
    let query = StationSearchQuery::parse("format:mp3");
    assert_eq!(query.field(), SearchField::Codec);
    assert_eq!(param_value(&query, "codec").as_deref(), Some("mp3"));
}
```

---

# 3. Improve empty-result messages

## Goal

Make empty search results actionable and prefix-aware.

`0.3.0` added a basic hint:

```text
No results; try tag:ambient, country:BA, lang:english, or a shorter name
```

For `0.3.1`, tailor the hint to the active field.

## Files

```text
src/ui/search.rs
src/radio/query.rs
```

## Suggested UI helper

In `src/ui/search.rs`:

```rust
fn empty_search_hint(query: &str) -> String {
    let parsed = StationSearchQuery::parse(query);
    let value = compact_search_label(parsed.value());

    match parsed.field() {
        SearchField::Name => {
            if query.contains(':') && !known_search_prefix_before_colon(query) {
                format!(
                    "  No results for {}; unknown prefix, treated as station name",
                    compact_search_label(query)
                )
            } else {
                "  No results; try tag:ambient, country:BA, lang:english, codec:mp3, or a shorter name".to_string()
            }
        }
        SearchField::Tag => format!("  No tag results for {value}; try a broader genre"),
        SearchField::Country => format!("  No country results for {value}; try a country code like country:BA"),
        SearchField::CountryCode => format!("  No country results for {value}; try the full country name"),
        SearchField::Language => format!("  No language results for {value}; try english, bosnian, or serbian"),
        SearchField::Codec => format!("  No codec results for {value}; try codec:MP3, codec:AAC, or codec:OGG"),
    }
}
```

This requires re-exporting `SearchField` from `src/radio.rs`:

```rust
pub use query::{SearchField, StationSearchQuery};
```

## Unknown prefix helper

In `src/radio/query.rs`:

```rust
pub fn query_prefix(raw: &str) -> Option<&str> {
    raw.trim().split_once(':').map(|(prefix, _)| prefix.trim())
}

pub fn has_unknown_prefix(raw: &str) -> bool {
    query_prefix(raw).is_some_and(|prefix| !known_search_prefix(prefix))
}
```

If lifetime complexity annoys the compiler, return `Option<String>` instead. This helper is not performance-sensitive.

## Pitfalls

- Avoid making the hint too long. Search status text appears inside a constrained TUI line.
- Do not hardcode country-specific examples everywhere. `country:BA` is fine as one example, but do not make it look like the app is Bosnia-only.
- Do not produce scary wording for unknown prefixes. It should be helpful, not punitive.

## Edge cases

- `tag:` and `tag:a` are currently considered short. They should stay in waiting state or show a short-input hint, not send API requests.
- A value containing a colon after a known prefix, such as `name:http://radio`, should preserve the value after the first colon.
- Unknown prefixes remain plain search. That is compatibility-friendly.

## Tests

In `src/ui/search.rs`:

```rust
#[test]
fn empty_search_hint_suggests_country_code_for_country_name() {
    assert_eq!(
        empty_search_hint("country:Bosna"),
        "  No country results for Bosna; try a country code like country:BA"
    );
}

#[test]
fn empty_search_hint_suggests_codec_values_for_codec_query() {
    assert_eq!(
        empty_search_hint("codec:aacplus"),
        "  No codec results for aacplus; try codec:MP3, codec:AAC, or codec:OGG"
    );
}

#[test]
fn empty_search_hint_explains_unknown_prefix_fallback() {
    assert_eq!(
        empty_search_hint("mood:rain"),
        "  No results for mood:rain; unknown prefix, treated as station name"
    );
}
```

Use `compact_search_label` in tests when strings may truncate.

---

# 4. Harden station metadata mapping

## Goal

Radio Browser data is useful, but it can be messy. Normalize and guard metadata so station rows, details, ranking, saving, and import/export stay stable.

## Files

```text
src/radio/station.rs
src/radio/map.rs
src/ui/station_details.rs
src/ui/stations.rs
src/favorites.rs
```

## Current station fields

`src/radio/station.rs`:

```rust
pub struct Station {
    pub name: String,
    pub url: String,
    pub genre: String,
    pub country: String,
    pub bitrate: u32,
    pub station_uuid: Option<String>,
    pub country_code: String,
    pub tags: Vec<String>,
    pub language: String,
    pub codec: String,
    pub homepage: String,
    pub last_check_ok: Option<bool>,
    pub votes: Option<u32>,
    pub click_count: Option<u32>,
}
```

## Add normalization helpers

In `src/radio/station.rs`:

```rust
const MAX_REASONABLE_BITRATE: u32 = 1024;

pub fn normalize_station_uuid(value: String) -> Option<String> {
    let trimmed = value.trim().to_string();
    (!trimmed.is_empty()).then_some(trimmed)
}

pub fn normalize_country_code(value: &str) -> String {
    value.trim().to_ascii_uppercase()
}

pub fn normalize_codec(value: &str) -> String {
    let normalized = value.trim().to_ascii_uppercase();
    match normalized.as_str() {
        "AUDIO/MPEG" | "MPEG" => "MP3".to_string(),
        "AAC+" | "HE-AAC" | "HEAAC" => "AAC".to_string(),
        "OGG VORBIS" | "VORBIS" => "OGG".to_string(),
        _ => normalized,
    }
}

pub fn sanitize_bitrate(value: u32) -> u32 {
    if value > MAX_REASONABLE_BITRATE {
        0
    } else {
        value
    }
}

pub fn clean_tag_values(values: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut tags = Vec::new();

    for value in values {
        let tag = value.trim();
        if tag.is_empty() {
            continue;
        }
        let key = tag.to_ascii_lowercase();
        if seen.insert(key) {
            tags.push(tag.to_string());
        }
    }

    tags
}
```

Note: `clean_tag_values` already exists. Change it in place and update its tests.

## Harden API mapping

In `src/radio/map.rs`, replace scattered normalization with helpers:

```rust
use super::station::{
    clean_tag_values, normalize_codec, normalize_country_code, normalize_station_uuid,
    sanitize_bitrate,
};
```

Then map fields:

```rust
Some(Station {
    name: fallback_trimmed(api.name, "Unnamed station"),
    url,
    genre,
    country: api.country.trim().to_string(),
    bitrate: sanitize_bitrate(api.bitrate),
    station_uuid: normalize_station_uuid(api.stationuuid),
    country_code: normalize_country_code(&api.countrycode),
    tags,
    language: api.language.trim().to_string(),
    codec: normalize_codec(&api.codec),
    homepage: api.homepage.trim().to_string(),
    last_check_ok: normalize_last_check_ok(api.lastcheckok),
    votes: api.votes,
    click_count: api.clickcount,
})
```

Add:

```rust
fn normalize_last_check_ok(value: Option<u8>) -> Option<bool> {
    match value {
        Some(1) => Some(true),
        Some(0) => Some(false),
        _ => None,
    }
}
```

## Station Details layout safety

Inspect `src/ui/station_details.rs` before editing. Make sure long fields use existing truncation helpers from `src/ui/text.rs` if available. If not, add local helpers.

Example:

```rust
fn detail_value(value: &str, max_chars: usize) -> String {
    crate::ui::text::truncate_graphemes(value, max_chars)
}
```

If no grapheme helper exists, use a char-based helper consistent with the rest of the UI. Do not hand-roll several different truncators.

Fields that must not overflow:

- `station.homepage`
- `station.url`
- `station.station_uuid`
- tags joined as comma-separated string
- language strings with multiple comma-separated values

## Pitfalls

- Do not strip useful metadata just because it is unusual. Normalize only obvious cases.
- Do not mutate URLs beyond trim and identity normalization. Playback URLs should remain exactly what the API resolved.
- Do not make bitrate clamping too aggressive. Some streams report 320 kbps, which is fine. Values above 1024 kbps for internet radio are suspicious.
- Preserve old library compatibility. New fields must remain serde-defaulted.

## Edge cases

- Duplicate tags with different case: `Jazz,jazz,JAZZ` should become one tag.
- Empty tags between commas: `jazz, ,pop,,rock` should drop empties.
- Codecs: `mp3`, `MP3`, `audio/mpeg`, `aac+`, `ogg vorbis`.
- `lastcheckok = 2` or `255` should become `None`, not true.
- Blank UUID should be `None`.
- Very long homepage should display safely but save fully.

## Tests

In `src/radio/station.rs`:

```rust
#[test]
fn clean_tag_values_dedupes_case_insensitively() {
    assert_eq!(
        clean_tag_values(vec![" Jazz ".into(), "jazz".into(), "ROCK".into()]),
        vec!["Jazz".to_string(), "ROCK".to_string()]
    );
}

#[test]
fn normalize_codec_cleans_common_values() {
    assert_eq!(normalize_codec("mp3"), "MP3");
    assert_eq!(normalize_codec("audio/mpeg"), "MP3");
    assert_eq!(normalize_codec("aac+"), "AAC");
}

#[test]
fn sanitize_bitrate_drops_absurd_values() {
    assert_eq!(sanitize_bitrate(128), 128);
    assert_eq!(sanitize_bitrate(9999), 0);
}
```

In `src/radio/map.rs`:

```rust
#[test]
fn map_api_station_ignores_unknown_lastcheckok_values() {
    let mut api = api_station("Meta", "http://stream", "jazz");
    api.lastcheckok = Some(2);

    let station = map_api_station(api).expect("station should map");
    assert_eq!(station.last_check_ok, None);
}
```

---

# 5. Saved-station metadata enrichment

## Goal

When a search result matches an existing saved station by Radio Browser UUID or normalized URL, enrich the saved station with new metadata without duplicating it.

This helps older libraries benefit from the `0.3.0` metadata model.

## Files

```text
src/radio/station.rs
src/favorites.rs
src/app/search.rs
src/playlist.rs
src/ui/search.rs
```

## Design

Add a merge method to `Station`. It should copy missing or stale metadata from a richer station while preserving user-important basics.

Rules:

- Preserve saved station `name`, `url`, and `genre` by default. Users may recognize their saved labels.
- Fill missing metadata fields from the incoming search result.
- Update trust/popularity fields because they are fresh API observations.
- Do not replace a working saved URL just because Radio Browser returned a different resolved URL, unless there is a future explicit migration design.
- Return `true` only if something changed, so callers can mark the library dirty.

## Suggested `Station` method

In `src/radio/station.rs`:

```rust
impl Station {
    pub fn enrich_from(&mut self, incoming: &Station) -> bool {
        let mut changed = false;

        changed |= fill_option(&mut self.station_uuid, incoming.station_uuid.clone());
        changed |= fill_string(&mut self.country_code, &incoming.country_code);
        changed |= fill_string(&mut self.language, &incoming.language);
        changed |= fill_string(&mut self.codec, &incoming.codec);
        changed |= fill_string(&mut self.homepage, &incoming.homepage);

        if self.tags.is_empty() && !incoming.tags.is_empty() {
            self.tags = incoming.tags.clone();
            changed = true;
        }

        if self.bitrate == 0 && incoming.bitrate > 0 {
            self.bitrate = incoming.bitrate;
            changed = true;
        }

        if self.country.trim().is_empty() && !incoming.country.trim().is_empty() {
            self.country = incoming.country.clone();
            changed = true;
        }

        // Trust and popularity are observations from Radio Browser. Refresh them.
        changed |= set_if_different(&mut self.last_check_ok, incoming.last_check_ok);
        changed |= set_if_different(&mut self.votes, incoming.votes);
        changed |= set_if_different(&mut self.click_count, incoming.click_count);

        changed
    }
}

fn fill_string(target: &mut String, incoming: &str) -> bool {
    if target.trim().is_empty() && !incoming.trim().is_empty() {
        *target = incoming.trim().to_string();
        true
    } else {
        false
    }
}

fn fill_option<T: PartialEq>(target: &mut Option<T>, incoming: Option<T>) -> bool {
    if target.is_none() && incoming.is_some() {
        *target = incoming;
        true
    } else {
        false
    }
}

fn set_if_different<T: PartialEq + Copy>(target: &mut Option<T>, incoming: Option<T>) -> bool {
    if incoming.is_some() && *target != incoming {
        *target = incoming;
        true
    } else {
        false
    }
}
```

If helper generics get noisy, write simple explicit blocks. Clarity beats trait gymnastics here.

## Library API

In `src/favorites.rs`:

```rust
impl Library {
    pub fn enrich_matching_station(&mut self, station: &Station) -> bool {
        if let Some(saved) = self
            .stations
            .iter_mut()
            .find(|saved| crate::radio::station_identity_matches(saved, station))
        {
            return saved.enrich_from(station);
        }
        false
    }
}
```

## App connection

In `src/app/search.rs`, update `confirm_search`.

Current behavior:

```rust
match self.library.add(station.clone()) {
    Ok(true) => {
        self.mark_library_dirty();
        self.set_info_notice("Station saved to library");
    }
    Ok(false) => {}
    Err(err) => self.set_error_notice(format!("Could not add station: {err}")),
}
```

Suggested behavior:

```rust
match self.library.add(station.clone()) {
    Ok(true) => {
        self.mark_library_dirty();
        self.set_info_notice("Station saved to library");
    }
    Ok(false) => {
        if self.library.enrich_matching_station(&station) {
            self.mark_library_dirty();
            self.set_info_notice("Saved station metadata refreshed");
        }
    }
    Err(err) => self.set_error_notice(format!("Could not add station: {err}")),
}
```

Alternative: enrich inside `Library::add` when duplicate found. That can be cleaner:

```rust
pub fn add(&mut self, station: Station) -> anyhow::Result<AddOutcome> {
    if let Some(saved) = self.matching_station_mut(&station) {
        return Ok(if saved.enrich_from(&station) {
            AddOutcome::Enriched
        } else {
            AddOutcome::Duplicate
        });
    }
    self.stations.push(station);
    self.rebuild_genres();
    Ok(AddOutcome::Added)
}
```

But this changes the return type from `bool` to enum and touches more callers. For `0.3.1`, prefer adding `enrich_matching_station` separately to reduce blast radius.

## Import behavior

Consider enrichment during CLI import too. Current `import_stations` counts duplicates as skipped. For `0.3.1`, keep the summary stable unless changing it is worth the docs change.

Minimal option:

```rust
if self.contains_station(&s) {
    if self.enrich_matching_station(&s) {
        // Still count as skipped to avoid changing CLI semantics.
    }
    skipped += 1;
} else {
    self.stations.push(s);
    added += 1;
}
```

If enrichment happens in import, make sure the library saves even when `added == 0` but enrichment changed data. The current `import_stations` rebuilds genres only when added. It returns only added/skipped, so callers may save regardless. Verify `src/cli.rs` or equivalent import command path before relying on that.

## Pitfalls

- Do not auto-enrich on every search response unless you are ready to mark the library dirty during browsing. That could cause unexpected disk writes while users merely search.
- Do not change saved URLs silently.
- Do not replace user-facing names without a deliberate migration policy.
- Do not rebuild genres when only metadata changed, unless `genre` changes. The proposed merge preserves genre.
- Avoid borrow checker tangles by cloning the selected station before mutating library, which `confirm_search` already does.

## Edge cases

- Existing saved station has UUID, incoming has same UUID but different URL. Enrich metadata, preserve saved URL.
- Existing saved station has no UUID, incoming has same normalized URL and a UUID. Fill UUID.
- Existing saved station has a custom name. Preserve it.
- Incoming trust data says `last_check_ok = false`. Refreshing that is useful, but it may surprise users. The Station Details panel can explain reachability.
- Incoming tags are empty. Do not clear saved tags.

## Tests

In `src/radio/station.rs`:

```rust
#[test]
fn enrich_from_fills_missing_metadata_without_replacing_name_or_url() {
    let mut saved = Station::basic("My Label", "http://old", "Radio", "", 0);
    let mut incoming = Station::basic("API Label", "http://new", "Jazz", "Bosnia", 128);
    incoming.station_uuid = Some("uuid-1".to_string());
    incoming.country_code = "BA".to_string();
    incoming.tags = vec!["jazz".to_string()];
    incoming.codec = "MP3".to_string();
    incoming.last_check_ok = Some(true);

    assert!(saved.enrich_from(&incoming));
    assert_eq!(saved.name, "My Label");
    assert_eq!(saved.url, "http://old");
    assert_eq!(saved.station_uuid.as_deref(), Some("uuid-1"));
    assert_eq!(saved.country_code, "BA");
    assert_eq!(saved.tags, vec!["jazz".to_string()]);
    assert_eq!(saved.codec, "MP3");
    assert_eq!(saved.last_check_ok, Some(true));
}
```

In `src/favorites.rs`:

```rust
#[test]
fn library_enriches_matching_station_by_normalized_url() {
    let mut lib = Library::in_memory(vec![Station::basic(
        "Saved", " HTTP://STREAM/ ", "Radio", "", 0,
    )]);
    let mut incoming = Station::basic("API", "http://stream", "Radio", "US", 128);
    incoming.station_uuid = Some("uuid".to_string());

    assert!(lib.enrich_matching_station(&incoming));
    assert_eq!(lib.stations[0].station_uuid.as_deref(), Some("uuid"));
}
```

In `src/app/search.rs`:

```rust
#[test]
fn search_confirm_refreshes_metadata_for_existing_station() {
    let saved = Station::basic("Saved", "http://stream", "Radio", "US", 0);
    let mut app = App::new(Library::in_memory(vec![saved]));
    let mut result = Station::basic("Result", "http://stream", "Radio", "US", 128);
    result.codec = "MP3".to_string();

    app.update(Action::EnterSearch);
    app.search.results = vec![result];
    app.nav.selected = 0;
    app.update(Action::SearchConfirm);

    assert_eq!(app.library.stations[0].codec, "MP3");
}
```

---

# 6. Ranking tune-up

## Goal

Keep local ranking deterministic, but make prefixed searches prefer exact field matches over vague popularity.

## Files

```text
src/radio/rank.rs
src/radio/query.rs
```

## Current ranking

Current `SearchScore`:

```rust
struct SearchScore {
    name_exact: u8,
    name_prefix: u8,
    field_match: u8,
    checked_ok: u8,
    https: u8,
    known_codec: u8,
    known_bitrate: u8,
    click_count: u32,
    votes: u32,
}
```

This is good for general results, but prefixed searches need sharper field relevance.

## Suggested score fields

Replace `field_match: u8` with a more expressive set:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct SearchScore {
    exact_field_match: u8,
    prefix_field_match: u8,
    partial_field_match: u8,
    name_exact: u8,
    name_prefix: u8,
    checked_ok: u8,
    https: u8,
    known_codec: u8,
    known_bitrate: u8,
    click_count: u32,
    votes: u32,
}
```

Then calculate field quality:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FieldMatchQuality {
    None,
    Partial,
    Prefix,
    Exact,
}

fn field_match_quality(query: &StationSearchQuery, station: &Station) -> FieldMatchQuality {
    let value = query.value().trim().to_lowercase();
    if value.is_empty() {
        return FieldMatchQuality::None;
    }

    match query.field() {
        SearchField::Name => text_quality(&station.name, &value),
        SearchField::Tag => station
            .tags
            .iter()
            .map(|tag| text_quality(tag, &value))
            .chain(std::iter::once(text_quality(&station.genre, &value)))
            .max_by_key(|quality| match quality {
                FieldMatchQuality::None => 0,
                FieldMatchQuality::Partial => 1,
                FieldMatchQuality::Prefix => 2,
                FieldMatchQuality::Exact => 3,
            })
            .unwrap_or(FieldMatchQuality::None),
        SearchField::Country => text_quality(&station.country, &value),
        SearchField::CountryCode => {
            if station.country_code.eq_ignore_ascii_case(query.value()) {
                FieldMatchQuality::Exact
            } else {
                FieldMatchQuality::None
            }
        }
        SearchField::Language => text_quality(&station.language, &value),
        SearchField::Codec => {
            if station.codec.eq_ignore_ascii_case(query.value()) {
                FieldMatchQuality::Exact
            } else {
                FieldMatchQuality::None
            }
        }
    }
}

fn text_quality(text: &str, lowered_query: &str) -> FieldMatchQuality {
    let value = text.trim().to_lowercase();
    if value == lowered_query {
        FieldMatchQuality::Exact
    } else if value.starts_with(lowered_query) {
        FieldMatchQuality::Prefix
    } else if value.contains(lowered_query) {
        FieldMatchQuality::Partial
    } else {
        FieldMatchQuality::None
    }
}
```

Then in `station_score`:

```rust
let quality = field_match_quality(query, station);

SearchScore {
    exact_field_match: u8::from(quality == FieldMatchQuality::Exact),
    prefix_field_match: u8::from(quality == FieldMatchQuality::Prefix),
    partial_field_match: u8::from(quality == FieldMatchQuality::Partial),
    name_exact: u8::from(name == value),
    name_prefix: u8::from(name.starts_with(&value)),
    checked_ok: u8::from(station.last_check_ok == Some(true)),
    https: u8::from(station.url.starts_with("https://")),
    known_codec: u8::from(!station.codec.trim().is_empty()),
    known_bitrate: u8::from(station.bitrate > 0),
    click_count: station.click_count.unwrap_or(0),
    votes: station.votes.unwrap_or(0),
}
```

## Exact match examples

```text
tag:jazz
  exact: tag "jazz"
  prefix: tag "jazz fusion"
  partial: tag "smooth jazz"

country:Bosnia
  exact: country "Bosnia"
  prefix: country "Bosnia and Herzegovina"
  partial: country "Federation of Bosnia..." if ever present

country:BA
  exact: country_code "BA"

codec:mp3
  exact: codec "MP3"
```

## Pitfalls

- `SearchScore` field order matters because derived `Ord` compares fields in struct order. Put semantic relevance before popularity.
- Do not let `click_count` beat exact `codec`, `countrycode`, or tag matches.
- Keep deterministic tie-breakers:

```rust
.then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
.then_with(|| a.url.cmp(&b.url))
```

- Do not overfit to one prefix. All search fields should share the same ranking logic where possible.

## Edge cases

- Stations with no tags but matching `genre` should still rank for `tag:`.
- `codec:mp3` should not prefer an AAC station just because it has high clicks.
- `country:BA` should not match unrelated text containing `ba`.
- Empty query values should never rank everything as exact.

## Tests

In `src/radio/rank.rs`:

```rust
#[test]
fn tag_search_prefers_exact_tag_over_partial_popular_tag() {
    let query = StationSearchQuery::parse("tag:jazz");

    let mut exact = station("Exact", "http://exact");
    exact.tags = vec!["jazz".to_string()];
    exact.click_count = Some(1);

    let mut partial = station("Partial", "http://partial");
    partial.tags = vec!["smooth jazz lounge".to_string()];
    partial.click_count = Some(10_000);

    let ranked = rank_search_results(&query, vec![partial, exact]);
    assert_eq!(ranked[0].url, "http://exact");
}

#[test]
fn codec_search_prefers_exact_codec_over_popularity() {
    let query = StationSearchQuery::parse("codec:mp3");

    let mut mp3 = station("MP3", "http://mp3");
    mp3.codec = "MP3".to_string();
    mp3.click_count = Some(1);

    let mut aac = station("AAC", "http://aac");
    aac.codec = "AAC".to_string();
    aac.click_count = Some(9999);

    let ranked = rank_search_results(&query, vec![aac, mp3]);
    assert_eq!(ranked[0].url, "http://mp3");
}

#[test]
fn country_code_search_requires_exact_code_match() {
    let query = StationSearchQuery::parse("country:BA");

    let mut ba = station("Bosnia", "http://ba");
    ba.country_code = "BA".to_string();

    let mut other = station("Contains ba", "http://other");
    other.country = "Barbados".to_string();
    other.country_code = "BB".to_string();
    other.click_count = Some(9999);

    let ranked = rank_search_results(&query, vec![other, ba]);
    assert_eq!(ranked[0].url, "http://ba");
}
```

---

# 7. Better error messages for Radio Browser mirror failures

## Goal

Keep detailed mirror diagnostics available, but show friendly search errors in the TUI.

Current client behavior gathers server errors and joins them with `|`, then `src/radio.rs` may return:

```text
HTTPS search failed: ...; HTTP fallback failed: ...
```

Current `src/ui/search.rs` truncates by splitting at `|`, which is a brittle little gremlin in the wires.

## Files

```text
src/radio/client.rs
src/radio.rs
src/ui/search.rs
src/app/search.rs
```

## Design option A, minimal string cleanup

Keep `anyhow::Result<Vec<Station>>`, but make errors start with a friendly summary and put details after a delimiter.

In `src/radio/client.rs`:

```rust
pub(super) struct SearchServerErrors {
    attempted: usize,
    details: Vec<String>,
}

impl SearchServerErrors {
    fn friendly_message(&self) -> String {
        format!(
            "Search temporarily unavailable. Tried {} Radio Browser mirrors.",
            self.attempted
        )
    }

    fn detailed_message(&self) -> String {
        self.details.join(" | ")
    }
}
```

But if you still return `anyhow`, wrap as:

```rust
anyhow::bail!(
    "{} Details: {}",
    report.friendly_message(),
    report.detailed_message()
)
```

Then UI can display the first sentence:

```rust
fn public_search_error_message(message: &str) -> String {
    message
        .split("Details:")
        .next()
        .unwrap_or(message)
        .trim()
        .to_string()
}
```

## Design option B, typed error

Better long-term but slightly larger:

```rust
#[derive(Debug)]
pub struct SearchError {
    pub public_message: String,
    pub details: String,
}
```

This requires changing `search_stations` result type or converting to `anyhow` at the boundary. For `0.3.1`, option A is likely enough.

## Suggested UI helper

In `src/ui/search.rs`:

```rust
fn public_search_error_message(message: &str) -> String {
    let trimmed = message.trim();
    trimmed
        .split("Details:")
        .next()
        .unwrap_or(trimmed)
        .split('|')
        .next()
        .unwrap_or(trimmed)
        .trim()
        .to_string()
}
```

Then render:

```rust
SearchStatus::Error { message, .. } => {
    Span::styled(format!("  {}", public_search_error_message(message)), theme::error())
}
```

## Preserve detail in tests

The lower-level client tests should still prove server context exists:

```rust
#[test]
fn format_search_errors_keeps_server_context() {
    let errors = vec![
        "https://de1.api.radio-browser.info: timeout".to_string(),
        "https://de2.api.radio-browser.info: tls".to_string(),
    ];

    assert!(format_search_errors(&errors).contains("de1"));
    assert!(format_search_errors(&errors).contains("de2"));
}
```

## Pitfalls

- Do not hide all details from logs/tests. Search outages are hard to diagnose without server names.
- Do not show raw TLS/reqwest errors as the main TUI message.
- Do not accidentally remove HTTP fallback behavior.
- Keep timeout unchanged unless there is a separate reason. `0.3.1` should not tune networking broadly.

## Edge cases

- HTTPS all fail, HTTP succeeds: no error shown.
- HTTPS succeeds with empty results: show empty-result hint, not mirror failure.
- All mirrors fail: show friendly summary.
- One mirror returns non-JSON: continue fallback and include detail.
- DNS outage: attempted count should still be correct.

## Tests

In `src/ui/search.rs`:

```rust
#[test]
fn public_search_error_message_hides_details() {
    assert_eq!(
        public_search_error_message(
            "Search temporarily unavailable. Tried 8 Radio Browser mirrors. Details: de1: timeout | de2: tls"
        ),
        "Search temporarily unavailable. Tried 8 Radio Browser mirrors."
    );
}
```

In `src/radio/client.rs`:

```rust
#[test]
fn friendly_error_mentions_attempted_mirrors() {
    let errors = vec!["server-a: timeout".to_string(), "server-b: tls".to_string()];
    let message = friendly_search_error(errors.len());
    assert!(message.contains("2"));
    assert!(message.contains("Radio Browser mirrors"));
}
```

---

# 8. More compatibility and regression tests

## Goal

Lock down the new `0.3.0` behavior before polishing it. The tests should protect old libraries, new metadata, query aliases, ranking, saved detection, and UI hints.

## Files

```text
src/radio/query.rs
src/radio/map.rs
src/radio/rank.rs
src/radio/station.rs
src/favorites.rs
src/app/search.rs
src/ui/search.rs
src/ui/station_details.rs
```

## Test checklist

### Query parsing

Add or verify tests for:

```text
plain name search
name:lofi
station:lofi
tag:ambient
genre:ambient
country:ba
country:Bosnia
cc:BA
lang:english
language:serbian
codec:mp3
format:aac
unknown prefix fallback
empty prefix values are short
uppercase prefixes
whitespace around prefix and value
```

Potential whitespace trap:

```rust
StationSearchQuery::parse(" tag : ambient ")
```

Current parser uses `split_once(':')`, then trims prefix and value, so it should work.

### API mapping

Add or verify tests for:

```text
empty URL dropped
url_resolved preferred over url
raw url used when resolved missing
name trimmed with fallback
first tag becomes genre
tags trimmed, empties dropped, duplicates removed
country code uppercased
codec normalized
lastcheckok 1 true, 0 false, other None
absurd bitrate clamped
UUID blank becomes None
homepage trimmed
```

### Ranking

Add or verify tests for:

```text
exact name outranks prefix
exact tag outranks partial tag with high clicks
exact country code outranks unrelated country text
exact codec outranks popularity
checked_ok breaks ties
HTTPS breaks ties
click_count applies after relevance
votes applies after click_count
UUID dedupe keeps stronger candidate
URL dedupe trims slash and case
ranking deterministic for equal scores
```

### Library compatibility

Add tests in `src/favorites.rs` for old and new JSON.

Old format without metadata:

```rust
#[test]
fn old_library_station_without_metadata_loads() {
    let json = r#"{
        "version": 1,
        "stations": [{
            "name": "Old FM",
            "url": "http://old",
            "genre": "Radio",
            "country": "US",
            "bitrate": 128
        }],
        "settings": {}
    }"#;

    let (stations, _, warning) = parse_library_file(json).unwrap();
    assert!(warning.is_none());
    assert_eq!(stations.len(), 1);
    assert_eq!(stations[0].station_uuid, None);
    assert!(stations[0].tags.is_empty());
    assert_eq!(stations[0].codec, "");
}
```

New metadata round trip:

```rust
#[test]
fn rich_station_metadata_round_trips_through_saved_station() {
    let mut station = Station::basic("Rich", "http://rich", "Jazz", "Bosnia", 128);
    station.station_uuid = Some("uuid".to_string());
    station.country_code = "BA".to_string();
    station.tags = vec!["jazz".to_string(), "live".to_string()];
    station.language = "Bosnian".to_string();
    station.codec = "MP3".to_string();
    station.homepage = "https://example.com".to_string();
    station.last_check_ok = Some(true);
    station.votes = Some(42);
    station.click_count = Some(1200);

    let saved = SavedStation::from(&station);
    let loaded = Station::from(saved);
    assert_eq!(loaded, station);
}
```

If `SavedStation` is private, this test belongs inside the same module, which it already does.

### App search behavior

Add tests for:

```text
confirming new result saves station
confirming duplicate result enriches metadata
confirming duplicate result preserves saved name/url
search audition does not enrich or save
search response with saved station displays saved indicator through contains_station
```

Audition should not write metadata because audition means “sample, do not commit.”

### UI helpers

Add tests for:

```text
empty plain query hint
empty tag query hint
empty country query hint
empty country code query hint
empty language query hint
empty codec query hint
unknown prefix fallback hint
public search error message truncates details
compact label truncates long values
```

## Pitfalls

- Avoid tests that rely on network access. Search client unit tests should test URL/param/error formatting, not live Radio Browser.
- Keep test stations built through `Station::basic` unless specifically testing metadata.
- If helpers are private, add tests in the same module instead of widening visibility just for tests.

## Commands

Run:

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
```

If clippy is not currently clean for unrelated reasons, document the exact warning and avoid adding new ones.

---

# 9. Documentation polish

## Goal

Document `0.3.1` as a patch polish release, not a feature leap.

## Files

```text
Cargo.toml
Cargo.lock
CHANGELOG.md
README.md
docs/releases/0.3.1.md
docs/release-checklist.md
docs/testing-strategy.md
```

## Version bump

In `Cargo.toml`:

```toml
version = "0.3.1"
```

Then run a cargo command that updates `Cargo.lock`, usually:

```bash
cargo check
```

Verify `Cargo.lock` package version changed from `0.3.0` to `0.3.1` for `pulsedeck`.

## CHANGELOG entry

Add under `[Unreleased]` or replace with dated `0.3.1` entry when ready:

```md
## [0.3.1] - 2026-06-18

### Fixed
*   Improved empty-result guidance for structured search prefixes.
*   Hardened Radio Browser metadata parsing for unusual tags, codecs, bitrates, and last-check values.
*   Kept older `library.json` station entries compatible with the richer 0.3 metadata model.
*   Made Radio Browser mirror failures display a friendly search error while preserving server details internally.

### Improved
*   Added clearer in-app and README examples for `tag:`, `country:`, `lang:`, `codec:`, and `name:` searches.
*   Added forgiving search aliases such as `station:`, `cc:`, and `format:`.
*   Refreshed saved-station metadata when a selected search result matches an existing library entry.
*   Tuned ranking so exact tag, country-code, language, and codec matches beat loose popularity signals.

### Internal
*   Expanded regression coverage for query parsing, metadata mapping, station enrichment, ranking, and legacy library loading.
```

Adjust categories based on actual final implementation. Do not claim work that did not land.

## Release notes file

Create `docs/releases/0.3.1.md`:

```md
# PulseDeck 0.3.1 Release Notes

PulseDeck 0.3.1 polishes the structured search system introduced in 0.3.0.

## Highlights

- Search prefixes are easier to discover from the app and README.
- New aliases make searches more forgiving: `station:`, `cc:`, and `format:`.
- Empty searches now explain what to try next based on the prefix used.
- Radio Browser metadata is normalized more defensively before display or saving.
- Existing saved stations can be refreshed with richer metadata when a matching search result is selected.
- Ranking now favors exact prefix-field matches before popularity signals.
- Search outage messages are friendlier when Radio Browser mirrors fail.

## Examples

```text
tag:jazz
country:BA
cc:BA
lang:english
codec:MP3
format:AAC
station:lofi
```

No new playback modes, recording workflows, or multi-prefix query syntax are included in this patch release.
```

## README updates

Update the search section near “Finding and adding a new station.” Include a compact table:

```md
### Search prefixes

PulseDeck also supports focused search prefixes:

| Prefix | Also accepts | Example | Searches |
| :--- | :--- | :--- | :--- |
| `name:` | `station:` | `name:lofi` | Station names |
| `tag:` | `genre:` | `tag:ambient` | Genres and tags |
| `country:` | `cc:` | `country:BA` | Country name or two-letter code |
| `lang:` | `language:` | `lang:english` | Station language |
| `codec:` | `format:` | `codec:mp3` | Stream codec |
```

Make clear that plain text still searches station names.

## Help overlay updates

Keep help text compact. Users are in a terminal app, not a scroll cathedral.

Suggested copy:

```text
Search prefixes: tag:ambient, country:BA, lang:english, codec:mp3, name:lofi
Aliases: genre:, cc:, language:, format:, station:
```

## Testing strategy docs

In `docs/testing-strategy.md`, add a subsection:

```md
### Structured search regression tests

Patch releases after 0.3.0 should cover:

- prefix parsing and aliases
- old and rich library JSON loading
- Radio Browser metadata normalization
- UUID and normalized URL station identity
- saved-station metadata enrichment
- ranking relevance before popularity
- user-facing search error and empty-result hints
```

## Release checklist

Verify `docs/release-checklist.md` references `0.3.x` commands and not stale `0.1.x` examples. `0.2.4` already cleaned up some stale release checklist wording, but check again.

## Pitfalls

- Do not document multi-prefix search. It is not supported in this release.
- Do not imply saved station names or URLs are automatically replaced. Say “metadata refreshed,” not “station updated.”
- Do not promise Radio Browser availability. Mirror failure handling only improves messaging.

---

# Cross-feature connection map

## Query parsing to UI hints

```text
src/radio/query.rs
    StationSearchQuery::parse
    known_search_prefix
    prefix_examples_inline
        -> src/ui/search.rs empty_search_hint
        -> src/ui/help.rs search help copy
        -> README search prefix table
```

Risk: if UI imports too much from `radio::query`, re-export through `src/radio.rs` to keep module boundaries clean.

## Metadata mapping to persistence

```text
src/radio/map.rs map_api_station
    -> Station fields normalized
    -> src/radio/rank.rs uses normalized codec/country/tags
    -> src/ui/station_details.rs displays safe metadata
    -> src/favorites.rs saves metadata to library.json
```

Risk: changing normalization can alter tests and snapshots. Keep expected output explicit.

## Saved detection to enrichment

```text
src/ui/search.rs
    app.library.contains_station(station) draws saved marker

src/app/search.rs confirm_search
    Library::add handles new station
    Library::enrich_matching_station handles duplicate selected station
    mark_library_dirty persists enrichment
```

Risk: enrichment during mere search browsing creates surprise writes. Enrich only on commit/import unless explicitly chosen otherwise.

## Ranking to empty-result UX

```text
src/radio/rank.rs
    Exact field matches first
    Trust/popularity after relevance

src/ui/search.rs
    Empty result hints help users broaden searches
```

Risk: ranking changes can make popular stations move down. That is intended when exact prefix relevance is stronger.

## Error reporting

```text
src/radio/client.rs
    collect per-server errors
    format friendly summary plus details

src/radio.rs
    combine HTTPS and HTTP fallback errors

src/ui/search.rs
    display friendly summary only
```

Risk: throwing away server details makes debugging upstream outages painful. Keep details in the error string after `Details:` or in a typed structure.

---

# Detailed task checklist

## Query and prefix work

- [ ] Re-export `SearchField` if UI needs it:

```rust
pub use query::{SearchField, StationSearchQuery};
```

- [ ] Add aliases in `StationSearchQuery::parse`:

```text
station -> Name
cc -> Country/CountryCode
format -> Codec
```

- [ ] Add canonical prefix help list or formatting helper.
- [ ] Add unknown-prefix helper if empty hints need it.
- [ ] Add parser tests for aliases, uppercase, whitespace, unknown prefixes, and short values.

## UI guidance work

- [ ] Replace hardcoded empty hint with prefix-aware helper.
- [ ] Add public search error formatter.
- [ ] Update search footer/title if space allows.
- [ ] Update help overlay with compact prefix examples.
- [ ] Add UI helper tests.

## Metadata hardening work

- [ ] Update `clean_tag_values` to dedupe case-insensitively.
- [ ] Add `normalize_codec`.
- [ ] Add `normalize_country_code`.
- [ ] Add `normalize_station_uuid`.
- [ ] Add `sanitize_bitrate`.
- [ ] Add `normalize_last_check_ok` in mapper.
- [ ] Update `SavedStation -> Station` conversion to use the same helpers.
- [ ] Make Station Details safe for long URLs, homepage, tags, and UUIDs.
- [ ] Add tests.

## Enrichment work

- [ ] Add `Station::enrich_from`.
- [ ] Add `Library::enrich_matching_station`.
- [ ] Call enrichment from `App::confirm_search` when `Library::add` returns duplicate.
- [ ] Decide whether CLI import should enrich duplicates.
- [ ] Ensure dirty flag/save happens when enrichment changes data.
- [ ] Add tests for preserving saved name/url and filling metadata.

## Ranking work

- [ ] Replace single `field_match` with field match quality.
- [ ] Ensure exact field match outranks popularity.
- [ ] Add tests for tag, country code, language, and codec exactness.
- [ ] Keep deterministic name/url tie-breakers.

## Error work

- [ ] Add friendly mirror failure summary.
- [ ] Preserve server details in lower-level message.
- [ ] Update UI to show public part only.
- [ ] Add tests for error formatting.

## Docs and release work

- [ ] Bump `Cargo.toml` to `0.3.1`.
- [ ] Update `Cargo.lock`.
- [ ] Add `docs/releases/0.3.1.md`.
- [ ] Update `CHANGELOG.md`.
- [ ] Update README search prefix section.
- [ ] Update help text and testing strategy docs.
- [ ] Run final checks.

---

# Suggested final verification script

Run these manually from the repository root:

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
cargo run --release -- --version
```

Manual smoke checks:

```text
1. Launch PulseDeck.
2. Press / to open search.
3. Type tag:ambient and verify results appear.
4. Type genre:ambient and verify equivalent behavior.
5. Type cc:BA and verify country-code search works.
6. Type format:mp3 and verify codec search works.
7. Type mood:rain and verify unknown prefix is treated as plain search with a helpful empty hint if no results.
8. Select an existing saved station returned by search and press Enter.
9. Verify it does not duplicate, plays, and refreshes metadata if missing.
10. Open Station Details with i and verify metadata does not overflow.
11. Temporarily simulate search failure if possible and verify friendly mirror failure text.
```

Network-dependent smoke tests should not block release if Radio Browser is temporarily down, but they should be retried before publishing crates.io if possible.

---

# Rollback plan

If a change becomes risky late in the release:

1. Keep alias parsing and docs. Low risk.
2. Keep empty-result hints. Low risk.
3. Keep metadata normalization only for tags/codecs/lastcheckok. Medium-low risk.
4. Drop automatic enrichment if dirty/save behavior gets tangled. It can move to `0.3.2`.
5. Drop ranking tune-up if exact ordering becomes controversial. Keep tests for current ranking instead.
6. Keep friendly error text only if details remain available.

The minimum valuable `0.3.1` is:

```text
prefix aliases + better hints + metadata hardening + tests + docs
```

The ideal `0.3.1` includes all nine improvements in this plan.
