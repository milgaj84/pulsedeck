# Releasing PulseDeck

PulseDeck publishes to **crates.io** and ships prebuilt binaries via GitHub Releases. This is the checklist for a stable `1.0.0`-style release.

## Prerequisites

- [ ] You have write access to `milgaj84/pulsedeck` and `CARGO_REGISTRY_TOKEN` is configured in the repo secrets.
- [ ] Local `cargo` toolchain is at least the **MSRV (1.75.0)**.
- [ ] `cargo-audit` and `cargo-deny` are installed locally (`cargo install cargo-audit cargo-deny --locked`).

## Pre-release verification

Run these **exactly** as CI does, and fix anything that fails before tagging:

```bash
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
cargo build --locked --release
cargo audit --ignore RUSTSEC-2026-0194 --ignore RUSTSEC-2026-0195
cargo deny check
```

> The two `RUSTSEC-2026-0194/0195` ignores are documented in `SECURITY.md` with justification and mitigation. See "Dependency Audit Policy".

## Manual smoke test

Run the release binary and verify:

```bash
cargo run --release
```

- [ ] App launches, library renders, no blank screen.
- [ ] Search + play a station.
- [ ] Press `q` — terminal restores cleanly (no raw-mode artifacts).
- [ ] Press `Ctrl+C` — terminal restores and state persists.
- [ ] From another shell: `kill <pid>` (SIGTERM) — terminal restores and state persists.

## Bump the version

1. Decide the version (semver). For a breaking change → `0.x.0` / `1.x.0`; a fix → patch bump.
2. Update `version` in `Cargo.toml`.
3. Add a `CHANGELOG.md` entry under a new section for the version (see the existing format; copy the most recent release's style).
4. Commit: `git add Cargo.toml Cargo.lock CHANGELOG.md && git commit -m "Release v<VERSION>"`.

## Tag and push

```bash
git tag v<VERSION>
git push origin master --tags
```

Pushing a `v*` tag triggers `.github/workflows/release.yml`, which:

1. Verifies the tag matches `Cargo.toml`'s `version` (fails the job if they differ).
2. Runs fmt, clippy, test, and release build on the 3-OS matrix.
3. Pushes to crates.io via `cargo publish --locked`.

## Post-release

- [ ] Confirm the GitHub Actions run passed.
- [ ] Confirm the crates.io page shows the new version.
- [ ] Create a GitHub Release (draft) from the tag, attach the release binary if desired.

## Rollback

- [ ] `cargo yank --version <VERSION>` on crates.io if a bad release was pushed.
- [ ] Revert `Cargo.toml`/`CHANGELOG.md` and re-tag with the previous fixed version.
