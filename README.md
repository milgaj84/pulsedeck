<div align="center">

# ✦ PulseDeck ✦

**Internet radio in your terminal. Search, tune in, listen.**

[![Crates.io](https://img.shields.io/crates/v/pulsedeck.svg)](https://crates.io/crates/pulsedeck)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Built with Rust](https://img.shields.io/badge/Built%20with-Rust-orange.svg)](https://www.rust-lang.org/)
[![Platform: Windows | Linux | macOS](https://img.shields.io/badge/Platform-Windows%20%7C%20Linux%20%7C%20macOS-blue.svg)](#installation)
[![CI](https://github.com/milgaj84/pulsedeck/actions/workflows/ci.yml/badge.svg)](https://github.com/milgaj84/pulsedeck/actions/workflows/ci.yml)

</div>

![PulseDeck - Cyber-Deck TUI Interface](assets/screenshot.png)

---

## You want to listen to radio. You open a terminal.

You don't want to open a browser tab that eats 300MB of RAM. You don't want an Electron app. You don't want to wrestle with `ffplay` and stream URLs. You just want music playing while you work — something you can launch, tune, and forget about.

**PulseDeck is that.**

One command to install. One keypress to search 30,000+ stations worldwide. One more to start listening. It remembers what you like, switches stations with smooth fades, and reconnects automatically when streams drop. When you close it tomorrow, it picks up where you left off.

```bash
cargo install pulsedeck
```

That's it. No config files. No API keys. Launch it and you're listening in seconds.

---

## Your first 30 seconds

```
$ pulsedeck
```

PulseDeck starts with handpicked stations ready to go. But you want something specific:

1. Press `/` — search opens
2. Type `tag:ambient` — results appear as you type
3. Press `Space` on one to preview it (no commitment)
4. Like it? Press `Enter` — saved to your library forever
5. Press `Esc` to close search. You're listening.

That's the loop. **Search → Preview → Save → Listen.** Everything else is optional.

---

## What you get

<table>
<tr><td>

**🎵 Listening**
- 30,000+ stations via radio-browser.info
- Smooth fade transitions between stations
- Auto-reconnect on stream dropout
- Live audio output device switching
- Sleep timer (fade out and stop)
- Desktop notifications for track changes

</td><td>

**📚 Your Library**
- Stations persist between sessions
- Favorites float to the top (★)
- Genre tabs, fuzzy filter, sort modes
- Station presets (like TV channel buttons)
- Import/export as M3U or JSON

</td></tr>
<tr><td>

**🎨 The Vibe**
- 6 themes (Retrowave, Catppuccin ×4, Terminal)
- 3 visualizers (Spectrum, Oscilloscope ×2)
- 3 dashboard layouts
- Mini mode for tmux panes
- Station health dots (green/yellow/red)

</td><td>

**⚙️ Your Way**
- Customizable keybindings (JSON)
- Unified TOML config with hot-reload
- Crash-resistant atomic file persistence
- Command palette (`:` or `Ctrl+p`)
- CLI mode for import/export/backup
- Works on Windows, Linux, macOS, WSL

</td></tr>
</table>

---

## Installation

**You need:** [Rust & Cargo](https://rustup.rs/) (1.75+)

```bash
cargo install pulsedeck
```

> **Linux users** — install ALSA headers first:
> ```bash
> sudo apt-get install libasound2-dev   # Debian/Ubuntu
> sudo dnf install alsa-lib-devel       # Fedora
> ```

Or build from source:
```bash
git clone https://github.com/milgaj84/pulsedeck.git
cd pulsedeck
cargo run --release
```

---

## Controls

PulseDeck is keyboard-driven. Press `h` anytime for the full reference.

**The essentials:**

| Key | What happens |
| :--- | :--- |
| `/` | Open search |
| `Enter` | Play (or save + play from search) |
| `Space` | Pause/resume (or preview in search) |
| `j`/`k` or `↑`/`↓` | Navigate |
| `+`/`-` | Volume |
| `m` | Mute |
| `b` | Cycle layout |
| `v` | Cycle visualizer |
| `q` | Quit |

**Search prefixes** for focused queries:

| Type | Example |
| :--- | :--- |
| `tag:ambient` | Search by genre/tag |
| `country:JP` | Search by country |
| `lang:english` | Search by language |
| `codec:mp3` | Search by stream format |

**Power user shortcuts:**

| Key | What |
| :--- | :--- |
| `:` / `Ctrl+p` | Command palette |
| `*` | Toggle favorite |
| `Ctrl+l` | Filter library |
| `Ctrl+1`–`5` | Assign preset slot |
| `Alt+1`–`5` | Play from preset |
| `t` | Sleep timer |
| `F6` | Mini mode |
| `i` | Station details |
| `d` | Playback doctor |

---

## Making it yours

Press `,` to open settings, or edit `~/.config/pulsedeck/pulsedeck.toml`:

```toml
[audio]
output_device = "Built-in Speakers"
default_volume = 80

[ui]
theme = "Retrowave"        # Retrowave | Catppuccin Mocha | Macchiato | Frappé | Latte | Terminal
notifications_enabled = true

[playback]
autoplay_last = true       # pick up where you left off
save_history = true        # remember every song title
reconnect_max_attempts = 3
reconnect_backoff_seconds = [3, 6, 12]

[discover]
genre_weight = 3
exclude_tags = ["news", "talk"]
```

Config hot-reloads — edit the file and PulseDeck picks up changes without restart.

Drop a `keybindings.json` next to it to remap anything:

```json
[
  {"key": "char(k)", "modifiers": ["ctrl"], "action": "prev_station", "mode": "Normal"},
  {"key": "f5", "modifiers": [], "action": "toggle_mute"}
]
```

---

## CLI

PulseDeck also works headless for library management:

```bash
pulsedeck export ~/backup.m3u        # export library
pulsedeck import ~/stations.m3u      # merge new stations
pulsedeck import file.json --preview # dry run, show what would change
pulsedeck config init                # generate default pulsedeck.toml
pulsedeck keybindings validate       # check your keybindings file
```

---

## Coming back tomorrow

PulseDeck remembers everything: your library, volume, theme, layout, visualizer mode, mute state. Enable **Auto-resume** in settings and it starts playing your last station on launch. No setup, no ceremony. Open terminal → music plays.

---

## Platform support

| Platform | Status |
| :--- | :--- |
| Linux | ✅ ALSA / PulseAudio / PipeWire |
| macOS | ✅ CoreAudio |
| Windows | ✅ WASAPI |
| WSL | ✅ with native Windows toast notifications |

---

## How it works (for the curious)

<details>
<summary>Audio engine architecture</summary>

PulseDeck treats internet radio as a live stream, not a seekable file. The audio engine runs on a dedicated OS thread with a single-owner state machine. Generation-guarded worker threads ensure rapid station switching discards stale connections instantly. A bounded prebuffer with timeout guarantees the engine never hangs in `Connecting`.

Codec support: MP3 (fast-path), AAC, OGG/Vorbis, Opus, FLAC, WAV via Symphonia probing with reliable stream classification. ICY metadata is stripped by a dedicated reader that provably never leaks metadata bytes into the decoder. The visualizer is a passive tap that never blocks audio. Live output device switching is transactional and preserves active playback on failure.

If something goes wrong, press `d` for the Playback Doctor — it shows diagnostics, context-aware recovery hints, and numbered one-click fixes you can execute directly.

</details>

<details>
<summary>Code quality</summary>

- Zero clippy warnings (`cargo clippy -- -D warnings`)
- 1446 tests: unit, integration, state-transition, and property-based (proptest)
- Strict architecture: business logic has zero UI dependencies
- Trait-abstracted I/O (audio, network, notifications) for testability
- CI: fmt, clippy, test, release build, dependency audit

</details>

<details>
<summary>Built with</summary>

All native Rust — no ffmpeg, no Python, no Electron. A single self-contained binary.

- [Ratatui](https://ratatui.rs/) — Terminal UI
- [Rodio](https://github.com/RustAudio/rodio) + [CPAL](https://github.com/RustAudio/cpal) + [Symphonia](https://github.com/pdeljanov/Symphonia) — Audio
- [Tokio](https://tokio.rs/) — Async runtime
- [reqwest](https://docs.rs/reqwest) — HTTP + ICY streaming

</details>

---

<div align="center">

*A neon radio console for the terminal. Quick to launch, easy to tune, built for listening.*

MIT License — [see LICENSE](LICENSE)

</div>
