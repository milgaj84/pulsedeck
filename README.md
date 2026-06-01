<div align="center">

# ✦ PulseDeck ✦

**A cyber-synthwave internet radio player for your terminal.**

*Stream any radio station on Earth. Record tracks automatically. Never leave the command line.*

[![Crates.io](https://img.shields.io/crates/v/pulsedeck.svg)](https://crates.io/crates/pulsedeck)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Built with Rust](https://img.shields.io/badge/Built%20with-Rust-orange.svg)](https://www.rust-lang.org/)
[![Platform: Windows | Linux | macOS](https://img.shields.io/badge/Platform-Windows%20%7C%20Linux%20%7C%20macOS-blue.svg)](#-installation)
[![CI](https://github.com/milgaj84/pulsedeck/actions/workflows/ci.yml/badge.svg)](https://github.com/milgaj84/pulsedeck/actions/workflows/ci.yml)

</div>

---

![PulseDeck — Cyber-Deck TUI Interface](assets/screenshot.png)

---

## What is PulseDeck?

PulseDeck is a **terminal internet radio player** with a retrowave soul, built in Rust. It works like any radio — tune in, listen, discover — but it lives entirely in your terminal, starts instantly, and uses a few megabytes of RAM.

It ships pre-loaded with handpicked synthwave, chiptune, and cyberpunk stations so it sounds great from the first keypress. But you can search, save, and play **any public internet radio station in the world**.

Think of it as: *VLC for internet radio, but it fits in your terminal and records tracks into named folders automatically.*

> PulseDeck was formerly named DriftFM. The project was renamed to avoid confusion with existing and historical radio-related uses of the old name. Existing DriftFM config is copied into the new PulseDeck config directory on first launch.

---

## What makes it different?

Most TUI radio players just wrap ffplay. PulseDeck is purpose-built from scratch in Rust with features you'd otherwise only find in native desktop apps:

- 📡 **Search 30,000+ stations** from the global radio-browser.info catalog — by name, tag, or country, with mirror failover for upstream outages
- 📼 **Automatic track recording** — press `r` and it captures tracks as separate files, named `Artist - Title.mp3`, tagged with ID3 metadata, sorted into genre subfolders
- 🧹 **Smart ad filtering** — DJ speech, news breaks, and commercial spots are detected and silently discarded. Only real music is kept.
- 🔊 **Smooth tuning transitions** — switching stations fades out the current stream and fades in the new one, like turning an analog dial
- 🎨 **5 built-in themes** — Retrowave (default), plus all 4 [Catppuccin](https://catppuccin.com/) flavors (Mocha, Macchiato, Frappé, Latte), verified pixel-perfect against the official spec. Every UI element — spectrum gradients, recording indicators, search bar, favorite stars, footer chips, list metadata, and help overlay — routes through the 13-role semantic palette. Switch live in settings.
- 🎛️ **Three-Way Bento Dashboard Layout** — press `b` to cycle between standard split panels, closed Bento (maximizing station list), and full-screen ambient cassette deck. Full-deck mode keeps the stable cassette design and adds a framed signal screen with a themed status strip.
- 📊 **Deck visualizers** — press `v` to cycle between a calibrated RTA Spectrum, Real Oscilloscope, and Simulated Oscilloscope. The RTA is tuned to avoid artificial final-treble spikes, preserve crisp treble texture with soft-knee dynamics, keep bars readable, and show a subtle tuning pulse while streams connect.
- 💾 **Favorites & history** — your stations are remembered between sessions, the station list shows compact country/bitrate metadata, and the last-played station can auto-resume on launch
- 🔔 **Desktop notifications** — a silent system notification shows the current track when a new song starts
- 🎛️ **Resilient streaming** — a circular buffer absorbs network hiccups, with adaptive EWMA buffer timing that stays calmer on VBR streams and bursty networks
- 🖥️ **Compact-screen protection** — terminal windows below 80x24 show a clean diagnostic instead of letting deck art and list borders collapse into visual static
- 🔁 **Audio output recovery** — default-device playback retries once after hardware-style sink failures, helping PulseDeck recover from transient headset or Bluetooth dropouts

---

## Installation

**Prerequisites:** [Rust & Cargo](https://rustup.rs/) (1.75+)

> On Linux, also install ALSA dev headers first:
> ```bash
> sudo apt-get install libasound2-dev   # Debian/Ubuntu
> sudo dnf install alsa-lib-devel       # Fedora
> ```

### From crates.io (recommended)

```bash
cargo install pulsedeck
```

### From source

```bash
git clone https://github.com/milgaj84/pulsedeck.git
cd pulsedeck
cargo run --release
```

That's it. No config files to write. No API keys. Stations are pre-loaded and the player starts immediately.

---

## How to use it

PulseDeck is keyboard-driven. Press **`h`** at any time to see the full control reference.

### Core shortcuts

| Key | Where | What it does |
| :--- | :--- | :--- |
| `↑` / `↓` or `j` / `k` | Library or search | Move through the visible list |
| `Enter` | Library | Play the highlighted saved station |
| `Enter` | Search | Save the highlighted result to your **Library**, then play it |
| `Space` | Search | Audition the highlighted result without saving it |
| `Ctrl+Enter` | Search | Audition too, when your terminal reports the key combo |
| `/` | Anywhere in normal mode | Open worldwide station search |
| `Esc` | Search or overlay | Leave search / close overlay |
| `f` | Library only | Remove the highlighted station from your **Library** |
| `u` | Library only | Undo the most recent station removal |
| `Tab` / `Shift+Tab` | Library | Switch genre categories |
| `Space` | Playback | Pause / resume |
| `s` | Playback | Stop playback |
| `+` / `-` | Playback | Volume up / down with fine low-volume and faster high-volume steps |
| `m` | Playback | Mute / unmute |
| `Ctrl+-` / `Alt+-` | Search | Volume down without leaving search |
| `Ctrl+=` / `Ctrl++` / `Alt+=` / `Alt++` | Search | Volume up without leaving search |
| `Ctrl+m` / `Alt+m` | Search | Mute / unmute without leaving search |
| `r` | Playback | Start / stop recording; PulseDeck shows footer status when recording is armed, active, stopped, or unavailable |
| `b` | View | Cycle Split / Library / Deck layout |
| `p` | View | Switch Tape Deck / Tape History |
| `v` | View | Cycle RTA Spectrum / Real Osc / Sim Osc |
| `,` | App | Open settings |
| `h` / `?` | App | Show / hide help |
| `q` | App | Quit |

`Enter` is the search commit action: it adds the highlighted search result to your saved Library and starts playback. `Space` auditions the highlighted result without saving it, so you can sample stations before committing them to `library.json`.

While in search, plain printable characters continue to edit the query. Use the Ctrl/Alt audio shortcuts for volume and mute if the current stream needs adjustment without abandoning the active search.

---

## Workflow

**Finding and adding a new station:**

1. Press `/` to open search, then type a genre, city, country, or station name. Search starts after **2+ characters** and waits briefly while you type, so quick typing does not send a request for every letter.
2. Use `↑` / `↓` to highlight a result.
3. Press `Space` to audition the highlighted station without saving it. You stay in search mode and can keep browsing.
4. Press `Enter` to save that result to your **Library** and start playing it immediately. It will be available next time you launch PulseDeck.
5. Press `Esc` instead to leave search without adding anything.

Search results show saved stations with a star and include compact genre/country/bitrate metadata. Long station names are truncated around the active search term when possible, so matching text stays visible even in narrow result rows. The search bar shows clear states while you work: `Type 2+ chars to search`, an animated query-initializing indicator, `searching ...`, result counts, stale-response discard notices, `No results`, `★ Saved to library`, or a compact `Search failed: ...` error. Older search responses are ignored if you have already typed a newer query, and the discarded query is surfaced in the search bar.

**Managing your library:**

- Your Library is the saved station list shown on launch.
- Rows show the selected station, currently playing station, country, and bitrate without overflowing long names.
- To remove a saved station, highlight it in the Library and press `f`.
- After removal, press `u` to restore removed stations in reverse order. PulseDeck keeps a bounded history of the 10 most recent removals.
- Switch between genre categories with `Tab` / `Shift+Tab`; PulseDeck remembers your last cursor position per category, falling back to the playing station when there is no saved position.

**Using the cassette deck:**

- Press `b` to cycle between split view, library-only view, and full-deck Bento mode.
- Press `v` to cycle the deck signal display between RTA Spectrum, Real Oscilloscope, and Simulated Oscilloscope.
- In RTA Spectrum mode, the signal screen shows a subtle tuning pulse during connection handshakes, so slow streams look active instead of blank.
- During stop or station changes, the deck stays visually active while the audio fade-out completes.
- Critical stream errors are mirrored inside help and settings overlays, so connection failures remain visible even when a modal is open.
- Press `p` to switch between the live Tape Deck and the Local Tape Library page.
- Watch the footer chips for playback state, volume, layout, scope mode, and recording state.

**Coming back tomorrow:**

- PulseDeck remembers your library between sessions.
- PulseDeck also remembers your volume, mute state, layout mode, and visualizer mode in a separate `ui-state.json` file.
- Settings such as auto-resume, audio output, recording folder, recording filters, and theme are saved automatically.
- Enable *Auto-resume last station* in settings (`,`) and it starts playing where you left off automatically.

---

## Recording

Press `r` while a station is playing. PulseDeck will:

1. Show `Recording will start at next track boundary` in the footer
2. Wait for the next song boundary (so you never capture a partial intro)
3. Record each track to its own file in the **native stream format** — `recordings/Synthwave/Perturbator - Venger.mp3` or `.aac` depending on what the station broadcasts. No transcoding, no quality loss.
4. Embed the correct ID3 tags (artist, title, station name as album)
5. Discard anything under 90 seconds — DJ speech, ads, station IDs are swept automatically
6. Stop cleanly when you press `r` again, with `Recording stopped` shown in the footer

If you press `r` before playback starts, PulseDeck shows `Start playback before recording` instead of failing silently.

The minimum song duration and whether to keep short clips are configurable in the settings (`,`).

---

## Local Tape Library

Press `p` to open the **Local Tape Library**, a disk-backed browser for recordings captured by PulseDeck.

- Recordings are scanned from the configured tape capture folder.
- Genre folders become expandable archive groups.
- Use `↑` / `↓` or `j` / `k` to move through folders and tracks.
- Use `Enter` on `[All Recordings]` to switch into a newest-first flat view across every folder.
- Use `Space` to expand or collapse folders.
- Use `Enter` on a track to play the selected local recording inside PulseDeck.
- Duration appears as `03:48` when the file metadata exposes it; otherwise PulseDeck still shows format and size.
- Use `/` or `t` on the tape page to filter recordings by title, folder, artist, extension, or filename.
- Use `Esc` while filtering to clear the local tape filter and return to the folder view.
- Use `Ctrl+r` to rescan the recording folder without restarting.
- Use `o` on a selected track to open its containing folder in the host file manager.
- Use `f` or `Delete` on a selected track to request removal; confirmation moves it to the OS trash, then `y` to confirm or `n` / `Esc` to cancel.
- When a local tape finishes, PulseDeck automatically hands off to the next recording in the same folder.

Local tape playback uses the same audio output and volume controls as live streams. Recording remains limited to live streams, so pressing `r` while a local tape is active shows a friendly notice instead of trying to re-record a file.

---

## Settings

Press `,` to open the settings panel. Current options:

- **Desktop notifications** — show track info when a song changes. On WSL, PulseDeck falls back to a Windows notification balloon if the normal Linux notification path is unavailable.
- **Auto-resume last station on startup** — picks up where you left off.
- **Audio Output** — choose `Default` or a detected output device such as `pulse`, `pipewire`, speakers, or Bluetooth headphones exposed by the audio backend. In `Default` mode, PulseDeck retries once after hardware-style sink failures so transient output changes can recover without a restart. If only `pulse` or `pipewire` appears, select that in PulseDeck and route it to your headphones in PipeWire/PulseAudio with `wpctl`, `pavucontrol`, or your desktop sound settings.
- **Tape capture folder** — cycle between preset recording directories.
- **Keep partial snippets & ads** — whether short clips and non-music segments are kept or silently deleted.
- **Min song duration** — threshold for auto-discarding short recordings. `Space` cycles common presets including 45s and 300s; Left/Right or h/l/a/d nudge by 5 seconds within the 15s–600s range. Only active when **Keep partial snippets & ads** is OFF.
- **Theme** — cycle between Retrowave, Catppuccin Mocha, Macchiato, Frappé, and Latte.

Use `↑` / `↓` or `j` / `k` to move between settings. Use `Space`, `Right`, `l`, or `d` to step values forward; use `Left`, `h`, or `a` to step values backward. Native ALSA/JACK probe diagnostics are suppressed during audio device discovery so backend chatter does not overwrite the TUI. Settings are saved automatically to a JSON file in your config directory.

---

## Migration from DriftFM

PulseDeck automatically copies existing DriftFM config files into the new config directory on first launch:

| Old path | New path |
| :--- | :--- |
| `~/.config/driftfm/library.json` | `~/.config/pulsedeck/library.json` |
| `~/.config/driftfm/ui-state.json` | `~/.config/pulsedeck/ui-state.json` |

The old `~/.config/driftfm` directory is left untouched as a backup. Future saves go to `~/.config/pulsedeck`.

---

## Platform Support

| Platform | Status |
| :--- | :--- |
| Windows | ✅ Full support (native WASAPI audio) |
| Linux | ✅ Full support (ALSA/PulseAudio/PipeWire via CPAL/Rodio, with selectable outputs) |
| macOS | ✅ Full support (CoreAudio) |
| WSL | ✅ Supported with Windows notification fallback |

---

## Code Quality

PulseDeck's CI checks:

- Rust formatting with `cargo fmt --check`
- Clippy across all targets and features with warnings treated as errors
- Tests across all targets and features
- Release build verification
- RustSec dependency audit with `cargo audit`

The codebase also keeps UI colors routed through the semantic palette in `theme.rs`, isolates blocking audio work from the TUI event loop, and keeps app/audio architecture notes in `docs/`.

---

## Built with

*All native Rust — no ffmpeg, no Python, no Electron. A single self-contained binary.*

- [Ratatui](https://ratatui.rs/) — Terminal UI framework
- [Rodio](https://github.com/RustAudio/rodio) + [CPAL](https://github.com/RustAudio/cpal) + [Symphonia](https://github.com/pdeljanov/Symphonia) — Audio output selection, decoding, and playback (native, no ffmpeg dependency)
- [Tokio](https://tokio.rs/) — Async runtime for API search
- [reqwest](https://docs.rs/reqwest) — HTTP streaming with ICY metadata support
- [id3](https://docs.rs/id3) — ID3 tag injection into recorded files

---

## License

MIT — see [LICENSE](LICENSE) for details.
