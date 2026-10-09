# Releasing PulseDeck

PulseDeck publishes to **crates.io** and ships prebuilt binaries via GitHub Releases. This is the checklist for a release.

## Prerequisites

- [ ] You have write access to `milgaj84/pulsedeck` and `CARGO_REGISTRY_TOKEN` is configured in the repo secrets (Settings → Secrets and variables → Actions, or `gh secret set CARGO_REGISTRY_TOKEN`). The publish job fails with a clear error if it is missing.
- [ ] Local `cargo` toolchain is at least the **MSRV (1.89.0)**.
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
3. Add a `CHANGELOG.md` entry under a `## [<VERSION>] - <date>` heading (see the existing format). The release workflow uses this section as the GitHub Release notes and fails if it is missing.
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
4. Builds release binaries for Linux x86_64, macOS (Intel + Apple Silicon) and Windows x86_64 with `scripts/package-release.sh`, which writes `pulsedeck-<tag>-<target>.{tar.gz,zip}` plus a `.sha256` file for each.
5. Creates the GitHub Release from the matching `CHANGELOG.md` section and attaches the archives. If the release already exists, the archives are attached to it instead (`--clobber`), so a re-run is safe.

## Dry-running the binary build

The binary build also runs from a manual dispatch, without publishing or creating a release (those jobs only run on `v*` tags). Do this once before a release that changes the workflow or build setup:

```bash
gh workflow run release.yml --ref master
gh run watch
```

Then download the `dist-*` artifacts from the run page and check an archive unpacks and the binary starts (`pulsedeck --version`). To test the packaging step locally: `scripts/package-release.sh v0.0.0-test x86_64-unknown-linux-gnu target/release/pulsedeck /tmp/dist`.

## Post-release

- [ ] Confirm the GitHub Actions run passed.
- [ ] Confirm the crates.io page shows the new version.
- [ ] Confirm the GitHub Release exists, its notes look right and the four archives plus `.sha256` files are attached.
- [ ] If publishing failed, fix the cause and re-run the failed job (`gh run rerun --failed`) instead of re-pushing the tag.

## Rollback

- [ ] `cargo yank --version <VERSION>` on crates.io if a bad release was pushed.
- [ ] Revert `Cargo.toml`/`CHANGELOG.md` and re-tag with the previous fixed version.

## Dependency advisories

`.github/workflows/audit.yml` runs `cargo audit` and `cargo deny check` every Monday, so new advisories show up without a push. Fix them with a lockfile bump where possible (`cargo update -p <crate>`); only add an `--ignore` with a justification in `SECURITY.md`.
