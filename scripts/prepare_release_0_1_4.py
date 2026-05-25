#!/usr/bin/env python3
"""Prepare DriftFM 0.1.4 release files.

This script updates crate metadata and release documents, then removes itself so
only release-ready files remain in the final branch state.
"""

from pathlib import Path

VERSION = "0.1.4"
RELEASE_DATE = "2026-05-25"

CARGO_TOML = Path("Cargo.toml")
CARGO_LOCK = Path("Cargo.lock")
CHANGELOG = Path("CHANGELOG.md")
README = Path("README.md")
RELEASE_CHECKLIST = Path("docs/release-checklist.md")
RELEASE_NOTES = Path("docs/releases/0.1.4.md")
THIS_SCRIPT = Path("scripts/prepare_release_0_1_4.py")


def replace_once(text: str, old: str, new: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"Expected exactly one match, found {count}: {old[:160]!r}")
    return text.replace(old, new, 1)


def write(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text.strip() + "\n", encoding="utf-8")


def update_cargo_files() -> None:
    toml = CARGO_TOML.read_text(encoding="utf-8")
    toml = replace_once(toml, 'version = "0.1.3"', f'version = "{VERSION}"')
    CARGO_TOML.write_text(toml, encoding="utf-8")

    lock = CARGO_LOCK.read_text(encoding="utf-8")
    lock = replace_once(
        lock,
        'name = "driftfm"\nversion = "0.1.3"',
        f'name = "driftfm"\nversion = "{VERSION}"',
    )
    CARGO_LOCK.write_text(lock, encoding="utf-8")


def update_changelog() -> None:
    text = CHANGELOG.read_text(encoding="utf-8")
    if f"## [{VERSION}]" in text:
        return

    start = text.find("## [Unreleased]")
    next_release = text.find("\n---\n\n## [0.1.3]", start)
    if start == -1 or next_release == -1:
        raise SystemExit("Could not locate current [Unreleased] changelog block")

    unreleased_body = text[start + len("## [Unreleased]"):next_release].strip()
    released = (
        "## [Unreleased]\n\n"
        "No unreleased changes yet.\n\n"
        "---\n\n"
        f"## [{VERSION}] - {RELEASE_DATE}\n\n"
        f"{unreleased_body}\n"
    )

    text = text[:start] + released + text[next_release:]
    CHANGELOG.write_text(text, encoding="utf-8")


def update_readme() -> None:
    text = README.read_text(encoding="utf-8")

    text = replace_once(
        text,
        "- 📡 **Search 30,000+ stations** from the global radio-browser.info catalog — by name, tag, or country",
        "- 📡 **Search 30,000+ stations** from the global radio-browser.info catalog — by name, tag, or country, with mirror failover for upstream outages",
    )

    text = replace_once(
        text,
        "The search bar shows clear states while you work: `Type 2+ chars to search`, `searching ...`, result counts, `No results`, `★ Saved to library`, or `Search failed. Check connection.` Older search responses are ignored if you have already typed a newer query.",
        "The search bar shows clear states while you work: `Type 2+ chars to search`, `searching ...`, result counts, `No results`, `★ Saved to library`, or a compact `Search failed: ...` error. Older search responses are ignored if you have already typed a newer query.",
    )

    text = replace_once(
        text,
        "The codebase also keeps UI colors routed through the semantic palette in `theme.rs` and isolates blocking audio work from the TUI event loop.",
        "The codebase also keeps UI colors routed through the semantic palette in `theme.rs`, isolates blocking audio work from the TUI event loop, and keeps app/audio architecture notes in `docs/`.",
    )

    README.write_text(text, encoding="utf-8")


def write_release_docs() -> None:
    write(RELEASE_CHECKLIST, f'''
# Release checklist

This checklist is for publishing DriftFM to crates.io and creating the matching GitHub release.

## Preflight

```bash
git switch master
git pull origin master
git fetch --prune
```

Confirm the version:

```bash
grep -n 'version = "{VERSION}"' Cargo.toml
grep -A2 -n 'name = "driftfm"' Cargo.lock
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
git tag -a v{VERSION} -m "DriftFM {VERSION}"
git push origin v{VERSION}
```

## GitHub release

Create a GitHub release from tag `v{VERSION}` and paste the notes from:

```text
docs/releases/{VERSION}.md
```

## Post-release sanity check

```bash
cargo install driftfm --version {VERSION}
driftfm
```

## Important

Published crates.io versions are permanent. A version cannot be overwritten after upload. If something goes wrong after publishing, publish a new patch version.
''')

    write(RELEASE_NOTES, f'''
# DriftFM {VERSION}

Release date: {RELEASE_DATE}

DriftFM {VERSION} is a quality, architecture, and reliability release. It keeps the same user-facing radio workflow while making the internals safer, easier to maintain, and better covered by deterministic tests.

## Highlights

- Added CI quality gates for formatting, clippy, tests, release build, RustSec audit, and a static TLS safety guard.
- Removed the insecure HTTPS invalid-certificate bypass from stream setup.
- Surfaced library persistence failures in the TUI instead of silently swallowing them.
- Split audio internals into focused modules for buffering, metadata parsing, recording helpers, stream reading, visualizer wrapping, and session connection logic.
- Added lazy audio device initialization so browsing/search can work even before an output device is available.
- Hardened Radio Browser search with HTTPS mirror failover, HTTP fallback for upstream TLS outages, and compact real error messages.
- Split the monolithic app reducer into focused app-state modules while preserving the public `crate::app` API.
- Added deterministic tests for app reducers, Radio Browser helpers, audio buffer status math, keymaps, library behavior, theme behavior, metadata parsing, and recording filename sanitization.
- Added architecture and testing documentation under `docs/`.

## Validation

Validated before release with:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo build --release
cargo run
cargo publish --dry-run
```

## Notes for Linux and WSL users

On Linux, make sure ALSA development/runtime packages are installed. On WSLg, PulseAudio may need to be exposed through ALSA for audio playback. See `docs/testing-strategy.md` for the runtime smoke checklist.
''')


def main() -> None:
    update_cargo_files()
    update_changelog()
    update_readme()
    write_release_docs()
    THIS_SCRIPT.unlink(missing_ok=True)
    print(f"Prepared DriftFM {VERSION} release files.")


if __name__ == "__main__":
    main()
