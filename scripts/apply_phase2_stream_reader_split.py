#!/usr/bin/env python3
"""Extract StreamReader into src/audio/stream_reader.rs.

This continues Phase 2 conservatively:
- keep AudioCommand, AudioStatus, AudioEngine, audio_loop, and connection retry flow in src/audio.rs
- move only StreamReader and its recording/metadata read implementation
- preserve behavior and visibility with pub(super) boundaries
- remove temporary phase scripts after applying
"""

from pathlib import Path

AUDIO = Path("src/audio.rs")
STREAM_READER = Path("src/audio/stream_reader.rs")
DOC = Path("docs/audio-architecture.md")


def replace_once(text: str, old: str, new: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"Expected exactly one match, found {count}: {old[:120]!r}")
    return text.replace(old, new, 1)


def main() -> None:
    text = AUDIO.read_text(encoding="utf-8")

    if "mod stream_reader;" in text:
        print("StreamReader split already applied; cleaning scripts only.")
    else:
        start_marker = "/// StreamReader consuming from thread-safe ring-buffer and stripping metadata boundaries"
        start = text.find(start_marker)
        if start == -1:
            raise SystemExit("Could not find StreamReader block start")

        stream_block = text[start:].rstrip() + "\n"
        text = text[:start].rstrip() + "\n"

        stream_block = stream_block.replace("struct StreamReader", "pub(super) struct StreamReader", 1)
        stream_block = stream_block.replace("    fn new(", "    pub(super) fn new(", 1)

        stream_module = """use super::buffer::BufferQueue;
use super::metadata::parse_stream_title;
use super::recording::{inject_id3_tags, sanitize_filename};
use super::{AudioStatus, RecordStateShared};

use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc};

""" + stream_block

        text = replace_once(text, "mod recording;\nmod visualizer;", "mod recording;\nmod stream_reader;\nmod visualizer;")
        text = replace_once(text, "use metadata::parse_stream_title;\n", "")
        text = replace_once(text, "use recording::{inject_id3_tags, sanitize_filename};\n", "")
        text = replace_once(text, "use visualizer::VisualizerSource;", "use stream_reader::StreamReader;\nuse visualizer::VisualizerSource;")
        text = replace_once(text, "use std::fs::File;\n", "")
        text = replace_once(text, "use std::io::{Read, Write};", "use std::io::Read;")
        text = replace_once(text, "use std::path::PathBuf;\n", "")

        STREAM_READER.write_text(stream_module, encoding="utf-8")
        AUDIO.write_text(text, encoding="utf-8")

    DOC.parent.mkdir(parents=True, exist_ok=True)
    DOC.write_text("""# Audio architecture

DriftFM keeps audio playback on a dedicated blocking thread so the terminal UI can stay responsive.

## Public boundary

The public audio API remains exposed from `crate::audio`:

- `AudioEngine` is the UI-facing handle.
- `AudioCommand` is the command channel from the app to the audio thread.
- `AudioStatus` is the status channel from the audio thread back to the app.

Other audio modules are implementation details and should stay private or `pub(super)` unless there is a clear cross-module need.

## Current module map

- `src/audio.rs` owns the public API, audio thread loop, playback state, command handling, connection retry flow, and sink creation.
- `src/audio/buffer.rs` owns the bounded producer-consumer byte queue used between the network downloader and decoder.
- `src/audio/stream_reader.rs` owns ICY metadata boundary stripping, recording segment lifecycle, and the `Read`/`Seek` adapter consumed by `rodio::Decoder`.
- `src/audio/metadata.rs` owns ICY metadata parsing helpers.
- `src/audio/recording.rs` owns recording filename sanitization and ID3 tagging helpers.
- `src/audio/visualizer.rs` owns sample interception for the visualizer buffer.

## Refactor rules

- Keep behavior changes out of mechanical extraction PRs.
- Preserve `crate::audio::{AudioCommand, AudioEngine, AudioStatus}` unless an app-level migration is planned.
- Keep networking, decoding, recording, and UI status updates testable through small helpers where possible.
- Prefer one subsystem movement per PR so regressions are easy to bisect.

## Known follow-ups

- Extract connection/retry/session logic after the `StreamReader` split is merged.
- Consider lazy audio device initialization so the app can browse/search even when no output device is available.
- Improve recording filename collision handling and stream format detection in a behavior-change PR.
""", encoding="utf-8")

    Path("scripts/apply_phase2_audio_split.py").unlink(missing_ok=True)
    Path("scripts/apply_phase2_stream_reader_split.py").unlink(missing_ok=True)

    print("Extracted StreamReader and updated audio architecture docs.")


if __name__ == "__main__":
    main()
