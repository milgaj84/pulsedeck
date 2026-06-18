# Release checklist

This checklist is for publishing PulseDeck to crates.io and creating the matching GitHub release.

## Preflight

```bash
git switch master
git pull origin master
git fetch --prune
```

Set the release version once for the commands below:

```bash
VERSION=0.3.0
```

Confirm the version:

```bash
grep -n "version = \"$VERSION\"" Cargo.toml
grep -A2 -n 'name = "pulsedeck"' Cargo.lock
```

Run the full local gate:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo build --release
cargo run
```

## Visual release smoke check

For a visual smoke pass, manually check:

```text
Split layout: cassette stays stable while reels animate
Library Focus and Signal Focus layouts compose cleanly at common terminal sizes
Visualizer modes: RTA Spectrum, Real Oscilloscope, and Simulated Oscilloscope show framed mode titles
Spectrum analyzer: high-frequency bars do not show an artificial final-bin spike or deep treble valley
Footer status row: playback, volume, layout, visualizer, and notice labels stay readable at common terminal widths
Shortcut row: normal mode shows the most relevant actions for the current state
Help overlay: shortcut sections match the current keymap
Settings overlay: rows, descriptions, and saved-automatically guidance are readable
Recent Tracks / Listening History overlay: title and footer match the history persistence setting
Library rows: selected/playing markers, country, and bitrate remain readable without long-name overflow
Search rows: saved-result stars and genre/country/bitrate metadata display cleanly
Theme cycling: Retrowave, Catppuccin themes, and Terminal theme keep deck, footer, help, library/search rows, and visualizer colors consistent
```

Use these in-app keys during the smoke pass:

```text
b   cycle Split View / Library Focus / Signal Focus
v   cycle visualizer modes
/   check search row metadata and saved-result stars
g   check Recent Tracks / Listening History copy
h   check help overlay wording
,   switch themes in settings
```

## Package checks

Inspect which files Cargo will package:

```bash
cargo package --list
```

Run the publish dry run:

```bash
cargo publish --dry-run
```

Cargo's dry run packages the crate and verifies it without uploading. Do not skip this step.

## Publish

Make sure you are logged in to crates.io:

```bash
cargo login
```

Then publish:

```bash
cargo publish
```

## Tag

After crates.io accepts the package, tag the exact commit that was published:

```bash
git tag -a "v$VERSION" -m "PulseDeck $VERSION"
git push origin "v$VERSION"
```

## GitHub release

Create a GitHub release from tag `v$VERSION` and paste the notes from the matching section of `CHANGELOG.md`.

## Post-release sanity check

```bash
cargo install pulsedeck --version "$VERSION"
pulsedeck
```

## Important

Published crates.io versions are permanent. A version cannot be overwritten after upload. If something goes wrong after publishing, publish a new patch version.
