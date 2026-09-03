# Security Policy

## Supported Versions

PulseDeck tracks the latest release. Security updates are backported to the most recent stable release only.

| Version | Supported |
| ------- | --------- |
| 1.0.x   | ✅ |
| < 1.0   | ❌ |

## Reporting a Vulnerability

Please report security issues privately by opening a GitHub Security Advisory at:

<https://github.com/milgaj84/pulsedeck/security/advisories/new>

or by emailing the maintainer (see the repository owner on GitHub).

Please include:
- A description of the issue and its impact.
- Steps to reproduce, or a minimal proof-of-concept.
- The affected version(s) and platform(s).
- Any suggested fix if you have one.

We aim to respond within 5 business days. Please do **not** open a public issue for a suspected security vulnerability.

## Dependency Audit Policy

PulseDeck runs `cargo audit` in CI on every push/PR. When an advisory affects a transitive dependency, we make an explicit, documented decision:

1. **Fix** — bump the dependency if an updated version resolves the advisory.
2. **Accept** — if no patched version exists upstream and PulseDeck does not exercise the vulnerable code path, we record the exception here with a justification and ignore it in `.cargo/audit.toml`.

### Current Accepted Exceptions

The following advisories are intentionally ignored. Each is documented with the reason and the mitigation already in place.

| Advisory | Package | Impact | Why it is accepted | Mitigation in PulseDeck |
| -------- | ------- | ------ | ------------------ | ----------------------- |
| RUSTSEC-2026-0194 | `quick-xml` (transitive via `tauri-winrt-notification` → `notify-rust`) | XML parsing vulnerability in an upstream dependency | `PulseDeck` only sends **plain-text** Windows toast notifications and does not parse untrusted XML. The affected code path is not exercised. | Notifications are constructed as static, escaped, plain-text toasts; no user-controlled XML is parsed. |
| RUSTSEC-2026-0195 | `quick-xml` (transitive via `tauri-winrt-notification` → `notify-rust`) | XML parsing vulnerability in an upstream dependency | Same as above — `PulseDeck` never parses XML from untrusted input. | Windows toast payloads are emitted via PowerShell with content XML-escaped (`src/app/notifier.rs`). |

These exceptions are re-evaluated on every release. If upstream `tauri-winrt-notification` / `notify-rust` publishes a fixed version, the ignore entries will be removed.

## Dependency Policy

- `Cargo.lock` is committed. CI uses `--locked` for reproducible builds.
- `cargo-deny` is used (where installed) to enforce license and dependency rules; see `deny.toml`.
