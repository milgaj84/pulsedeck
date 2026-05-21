<div align="center">

# ✦ DriftFM ✦

**A cyber-synthwave internet radio player for your terminal.**

*Stream any radio station on Earth. Record tracks automatically. Never leave the command line.*

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Built with Rust](https://img.shields.io/badge/Built%20with-Rust-orange.svg)](https://www.rust-lang.org/)
[![Platform: Windows | Linux | macOS](https://img.shields.io/badge/Platform-Windows%20%7C%20Linux%20%7C%20macOS-blue.svg)](#-installation)
[![Zero warnings](https://img.shields.io/badge/cargo%20clippy-zero%20warnings-brightgreen.svg)](#)
[![Memory safe](https://img.shields.io/badge/Memory-Safe-critical.svg)](https://www.rust-lang.org/)

</div>

---

![DriftFM — Cyber-Deck TUI Interface](assets/screenshot.png)

---

## What is DriftFM?

DriftFM is a **terminal internet radio player** with a retrowave soul, built in Rust. It works like any radio — tune in, listen, discover — but it lives entirely in your terminal, starts instantly, and uses a few megabytes of RAM.

It ships pre-loaded with handpicked synthwave, chiptune, and cyberpunk stations so it sounds great from the first keypress. But you can search, save, and play **any public internet radio station in the world**.

Think of it as: *VLC for internet radio, but it fits in your terminal and records tracks into named folders automatically.*

---

## What makes it different?

Most TUI radio players just wrap ffplay. DriftFM is purpose-built from scratch in Rust with features you'd otherwise only find in native desktop apps:

- 📡 **Search 30,000+ stations** from the global radio-browser.info catalog — by name, tag, or country
- 📼 **Automatic track recording** — press `r` and it captures tracks as separate files, named `Artist - Title.mp3`, tagged with ID3 metadata, sorted into genre subfolders
- 🧹 **Smart ad filtering** — DJ speech, news breaks, and commercial spots are detected and silently discarded. Only real music is kept.
- 🔊 **Smooth tuning transitions** — switching stations fades out the current stream and fades in the new one, like turning an analog dial
- 💾 **Favorites & history** — your stations are remembered between sessions, and the last-played station can auto-resume on launch
- 🔔 **Desktop notifications** — a silent system notification shows the current track when a new song starts
- 🎛️ **Resilient streaming** — a circular buffer absorbs network hiccups so your audio doesn't cut out when the connection stutters

---

## Installation

**Prerequisites:** [Rust & Cargo](https://rustup.rs/) (1.75+)

> On Linux, also install ALSA dev headers first:
> ```bash
> sudo apt-get install libasound2-dev   # Debian/Ubuntu
> sudo dnf install alsa-lib-devel       # Fedora
> ```

```bash
# Clone and run
git clone https://github.com/milgaj84/driftFM.git
cd driftFM
cargo run --release
```

That's it. No config files to write. No API keys. Stations are pre-loaded and the player starts immediately.

---

## How to use it

DriftFM is keyboard-driven. Press **`h`** at any time to see the full control reference.

The most important keys to get started:

| Key | What it does |
| :--- | :--- |
| `↑` / `↓` | Move between stations |
| `Enter` | Play selected station |
| `Tab` | Switch genre categories |
| `/` | Search for any station worldwide |
| `Space` | Pause / Resume |
| `r` | Start / stop recording |
| `f` | In search: add to library. In library: remove station |
| `,` | Open settings |
| `q` | Quit |

---

## Workflow

**Finding and adding a new station:**

1. Press `/` to open search, type a genre, city, or station name — results come from the [radio-browser.info](https://www.radio-browser.info/) catalog of 30,000+ stations worldwide
2. Use `↑` / `↓` to browse results
3. Press `Enter` to tune in instantly — this also **automatically adds the station to your library** so it's there next time
4. Alternatively, press `f` on a search result to save it to your library without playing it yet

**Managing your library:**

- Your library is the main station list you see on launch
- To remove a station, highlight it in the main list and press `f`
- Switch between genre categories with `Tab` / `Shift+Tab` to browse the pre-loaded catalog

**Coming back tomorrow:**

- DriftFM remembers your library between sessions
- Enable *Auto-resume last station* in settings (`,`) and it starts playing where you left off automatically

---

## Recording

Press `r` while a station is playing. DriftFM will:

1. Wait for the next song boundary (so you never capture a partial intro)
2. Record each track to its own file in the **native stream format** — `recordings/Synthwave/Perturbator - Venger.mp3` or `.aac` depending on what the station broadcasts. No transcoding, no quality loss.
3. Embed the correct ID3 tags (artist, title, station name as album)
4. Discard anything under 90 seconds — DJ speech, ads, station IDs are swept automatically
5. Stop cleanly when you press `r` again

The minimum song duration and whether to keep short clips are configurable in the settings (`,`).

---

## Settings

Press `,` to open the settings panel. Current options:

- **Auto-resume last station on startup** — picks up where you left off
- **Desktop notifications** — show track info when a song changes
- **Keep partial recordings** — whether short clips are kept or silently deleted
- **Keep ad snippets** — whether to keep non-music audio segments

Settings are saved automatically to a JSON file in your config directory.

---

## Platform Support

| Platform | Status |
| :--- | :--- |
| Windows | ✅ Full support (native WASAPI audio) |
| Linux | ✅ Full support (ALSA) |
| macOS | ✅ Full support (CoreAudio) |

---

## Built with

*All native Rust — no ffmpeg, no Python, no Electron. A single self-contained binary.*

- [Ratatui](https://ratatui.rs/) — Terminal UI framework
- [Rodio](https://github.com/RustAudio/rodio) + [Symphonia](https://github.com/pdeljanov/Symphonia) — Audio decoding & playback (native, no ffmpeg dependency)
- [Tokio](https://tokio.rs/) — Async runtime for API search
- [reqwest](https://docs.rs/reqwest) — HTTP streaming with ICY metadata support
- [id3](https://docs.rs/id3) — ID3 tag injection into recorded files

---

## License

MIT — see [LICENSE](LICENSE) for details.
