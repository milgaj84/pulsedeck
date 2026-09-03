# Contributing to PulseDeck

Thanks for your interest in helping make PulseDeck better! This document explains how to contribute.

## Getting started

1. **Fork** and clone the repo.
2. Install the [Rust toolchain](https://rustup.rs/) (1.75+).
3. On Linux, install ALSA headers (see the README).
4. Build and run:
   ```bash
   cargo run
   ```

## Development workflow

- **Format** — `cargo fmt --all -- --check`
- **Lint** — `cargo clippy --locked --all-targets --all-features -- -D warnings`
- **Test** — `cargo test --locked --all-targets --all-features`
- **Build** — `cargo build --locked --release`

CI runs all of these on every push/PR across Ubuntu, macOS, and Windows, plus a `cargo audit` job.

## Test strategy

PulseDeck uses a three-level test pyramid:

1. **Unit tests** — pure domain logic, inline `#[cfg(test)]` modules.
2. **State-transition / property tests** — `proptest` for invariants and edge cases.
3. **Integration tests** — `src/app/integration_tests.rs` for multi-step user journeys.

**New features must include integration tests.** The exhaustive action smoke test (`test_all_action_variants_dispatch_without_panic`) will fail to compile if a new `Action` variant is added without being exercised, so keep it updated.

## Architecture

- **Domain layer** (`src/app/`, `src/radio/`, `src/audio/`, `src/recommend.rs`, etc.) has **zero UI dependencies**. Keep it that way.
- **UI layer** (`src/ui/`) renders from a read-only `UiModel` snapshot (`src/ui/model.rs`).
- **Trait-abstracted I/O** — network (`RadioApi`), notifications (`Notifier`), and audio (`AudioSink`) are injected so unit tests never touch real hardware or the network.

Follow SRP: one focused module per responsibility. Keep functions small and single-purpose. Use meaningful names — verbs for functions, predicates for booleans, plurals for collections.

## Committing

- Small, focused commits. One logical change per commit.
- Write clear commit messages describing *why*.
- Run `cargo fmt`, `cargo clippy`, and `cargo test` before pushing.

## Reporting issues

- Use the issue tracker. Include the version, platform, and steps to reproduce.
- For **security** issues, see [SECURITY.md](SECURITY.md) and report privately.

## License

By contributing, you agree that your contributions are licensed under the [MIT License](LICENSE).
