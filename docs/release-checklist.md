# Release checklist

This checklist is for publishing PulseDeck to crates.io and creating the matching GitHub release.

## Preflight

```bash
git switch master
git pull origin master
git fetch --prune
```

Confirm the version:

```bash
grep -n 'version = "0.1.6"' Cargo.toml
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

For the 0.1.6 visual-enchantments release, manually check:

```text
Split layout: cassette stays stable while reels animate
Right-only Bento layout: stable cassette, status strip, and framed signal screen compose cleanly
Visualizer modes: Spectrum, Real Oscilloscope, and Simulated Oscilloscope show framed mode titles
Spectrum analyzer: high-frequency bars do not show an artificial final-bin spike or deep treble valley
Tape History page: title uses cassette/tape language
Theme cycling: Retrowave and all Catppuccin themes keep deck colors consistent
```

Use these in-app keys during the smoke pass:

```text
b   cycle split / library-only / full-deck Bento layout
v   cycle visualizer modes
p   switch Tape Deck / Tape History
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
git tag -a v0.1.6 -m "PulseDeck 0.1.6"
git push origin v0.1.6
```

## GitHub release

Create a GitHub release from tag `v0.1.6` and paste the notes from the `0.1.6` section of `CHANGELOG.md`.

## Post-release sanity check

```bash
cargo install pulsedeck --version 0.1.6
pulsedeck
```

## Important

Published crates.io versions are permanent. A version cannot be overwritten after upload. If something goes wrong after publishing, publish a new patch version.
