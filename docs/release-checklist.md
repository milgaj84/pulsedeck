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
grep -n 'version = "0.1.5"' Cargo.toml
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
git tag -a v0.1.5 -m "PulseDeck 0.1.5"
git push origin v0.1.5
```

## GitHub release

Create a GitHub release from tag `v0.1.5` and paste the notes from:

```text
docs/releases/0.1.5.md
```

## Post-release sanity check

```bash
cargo install pulsedeck --version 0.1.5
pulsedeck
```

## Important

Published crates.io versions are permanent. A version cannot be overwritten after upload. If something goes wrong after publishing, publish a new patch version.
