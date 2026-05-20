# ✦ DriftFM ✦

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/Rust-1.75%2B-orange.svg)](https://www.rust-lang.org/)
[![Warnings](https://img.shields.io/badge/Warnings-0--clean-success.svg)](#)
[![Clippy](https://img.shields.io/badge/Clippy-lint--free-blueviolet.svg)](#)

> A blazing-fast, zero-warning, zero-lint **cyber-botanical retrowave audio deck & smart tape recorder TUI** engineered in Rust. Live-stream retro music directly to your terminal with pixel-perfect animations, circular buffering, and smart folder segmentation.

---

## 📸 Interface Preview

![DriftFM Cyber-Deck TUI Interface](assets/screenshot.png)

---

## ⚡ Key Features

*   **🌀 Bounded Circular Resiliency Buffer**: Decouples connection socket streaming from Symphonia's audio blocks using a thread-safe lock-free `BufferQueue` (`1 MB` ring-buffer). Playback remains immune to temporary network jitter and packet stutters.
*   **🔊 Non-Blocking Playback Crossfading**: Ramps stream volumes exponentially on pause, resume, and station switches (dimming over `150ms` and swelling over `250ms`), emulating the tactile sensation of physical vacuum-tube tuning dials with zero GUI lag.
*   **📼 Smart Segmenting Tape Recorder**: Intercepts StreamTitle changes inside incoming ICY metadata packets, dynamically flushes current track frames, and starts a fresh file with zero boundary latency.
*   **📂 Dynamic Subgenre Folder Sorter**: Heuristically resolves parent genres (e.g. Synthwave, Chiptune, Cyberpunk) and dynamically constructs folder hierarchies to automatically store tracks inside `recordings/<Subgenre>/<Artist> - <Title>.mp3`.
*   **🏷️ ID3v2 Tagging Engine**: Automatically injects parsed artist name, track title, and station album tags into the capture metadata upon finalization.
*   **🗑️ Smart Content Sweep & Snippet Discarder**: Matches parsed metadata against known advertisement strings (`ADVERT`, `COMMERCIAL`, `WEATHER`, `NEWS`, `DJ SPEECH`) and automatically purges recorded tracks under `90 seconds` from disk unless configured otherwise in the settings.
*   **💫 animated Cassette Tape Deck UI**: Dual spinning cassette reels that dynamically resize, real-time buffer progress bar alerts, and a gorgeous composite wave Braille Canvas audio oscilloscope.
*   **🎛️ Neon Config Console popup**: Press `,` to pull down a glowing settings panel. Features toggleable options for persistent startup stations, OS notifications, and snippet purges, saving instantly to `library.json`.

---

## ⌨️ Control HUD Hotkey Bindings

Press **`h`** or **`?`** to summon the Control HUD inside the application at any time.

| Category | Keybinding | Action Description |
| :--- | :--- | :--- |
| **Navigation** | `Up` / `k` | Highlight previous station in current category |
| | `Down` / `j` | Highlight next station in current category |
| | `Enter` | Tune and play highlighted station |
| | `Tab` | Cycle forward through genre categories |
| | `Shift + Tab` | Cycle backward through genre categories |
| **Playback** | `Space` | Toggle pause/resume (with smooth fading) |
| | `s` | Stop active playback stream |
| | `+` / `=` | Increase volume by 5% |
| | `-` | Decrease volume by 5% |
| | `m` | Mute/unmute stream volume |
| | `r` | Toggle smart tape recording |
| **Search** | `/` | Enter interactive catalog search input mode |
| | `f` | Toggle favorite status on highlighted station |
| **Bento Toggles** | `b` | Toggle right-hand bento widgets panel ON/OFF |
| | `p` | Cycle right-hand page views (Cassette Tape ↔ History) |
| | `,` | Open Neon Configuration Settings modal |
| | `Esc` | Dismiss active overlay or exit the application |

---

## 🛠️ System Prerequisites

DriftFM relies on `rodio` and `symphonia` for audio decoding, which interacts directly with your native host soundcard.

### Windows (MSRV 1.75+)
*   No external dependencies required! Compiles natively with MSVC toolchains out of the box.

### Linux (Debian/Ubuntu)
Install development libraries for ALSA:
```bash
sudo apt-get install libasound2-dev
```

### macOS
*   Native CoreAudio bindings are resolved automatically.

---

## 🚀 Installation & Running

1.  Clone the repository:
    ```bash
    git clone https://github.com/yourusername/driftfm.git
    cd driftfm
    ```
2.  Build and run in release mode for maximum rendering performance:
    ```bash
    cargo run --release
    ```

---

## 🧪 Advanced Quality Verification

To run unit tests:
```bash
cargo test
```

To run standard clippy lints to guarantee a warning-free state:
```bash
cargo clippy --all-targets -- -D warnings
```

Refer to the complete [Advanced Testing Playbook](testing_playbook.md) for automated layout integration test examples using Ratatui's virtual `TestBackend`, parallel testing with `cargo-nextest`, and coverage reports using `cargo-tarpaulin`.

---

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
