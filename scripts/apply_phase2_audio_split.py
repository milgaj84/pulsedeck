#!/usr/bin/env python3
"""Apply Phase 2's first audio-module split.

This is intentionally mechanical and conservative:
- keep src/audio.rs as the public module root
- preserve crate::audio::{AudioCommand, AudioEngine, AudioStatus}
- extract leaf helpers only: buffer, visualizer, ICY metadata, recording helpers
- leave the audio loop and StreamReader in audio.rs for the next smaller step

The script fails loudly if expected anchors are missing.
"""

from pathlib import Path

AUDIO = Path("src/audio.rs")
AUDIO_DIR = Path("src/audio")


def read(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def write(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")


def extract_block(text: str, start_marker: str, end_marker: str) -> tuple[str, str]:
    start = text.find(start_marker)
    if start == -1:
        raise SystemExit(f"Missing start marker: {start_marker!r}")
    end = text.find(end_marker, start)
    if end == -1:
        raise SystemExit(f"Missing end marker after {start_marker!r}: {end_marker!r}")
    block = text[start:end].rstrip() + "\n"
    text = text[:start].rstrip() + "\n\n" + text[end:].lstrip()
    return text, block


def replace_once(text: str, old: str, new: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"Expected exactly one match, found {count}: {old[:120]!r}")
    return text.replace(old, new, 1)


def main() -> None:
    text = read(AUDIO)

    if "mod buffer;" in text:
        print("Phase 2 audio split appears already applied; exiting.")
        return

    text, buffer_block = extract_block(
        text,
        "/// Bounded Producer-Consumer circular byte queue (Resiliency Buffer)",
        "/// Handle to communicate with the audio engine running on a background thread.",
    )

    text, visualizer_block = extract_block(
        text,
        "/// A custom source wrapper that intercepts audio sample frames",
        "/// StreamReader consuming from thread-safe ring-buffer and stripping metadata boundaries",
    )

    text, helper_block = extract_block(
        text,
        "/// Inject ID3 metadata frames into completed local recordings",
        "#[cfg(test)]\nmod tests {",
    )

    # Drop the old combined audio helper test module from audio.rs.
    tests_start = text.find("#[cfg(test)]\nmod tests {")
    if tests_start == -1:
        raise SystemExit("Missing old audio helper test module")
    text = text[:tests_start].rstrip() + "\n"

    old_imports = """use rodio::cpal::Sample as CpalSample;
use rodio::{Decoder, OutputStream, Sample as RodioSample, Sink, Source as RodioSource};
use std::collections::VecDeque;
use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;
"""
    new_imports = """mod buffer;
mod metadata;
mod recording;
mod visualizer;

use buffer::BufferQueue;
use metadata::parse_stream_title;
use recording::{inject_id3_tags, sanitize_filename};
use visualizer::VisualizerSource;

use rodio::{Decoder, OutputStream, Sink};
use std::collections::VecDeque;
use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;
"""
    text = replace_once(text, old_imports, new_imports)

    buffer_block = buffer_block.replace("struct BufferQueue", "pub(super) struct BufferQueue", 1)
    buffer_block = buffer_block.replace("    capacity: usize,", "    pub(super) capacity: usize,", 1)
    for sig in [
        "fn new(",
        "fn push(",
        "fn pop(",
        "fn len(",
        "fn set_disconnected(",
    ]:
        buffer_block = buffer_block.replace(sig, f"pub(super) {sig}")

    buffer_module = """use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex};

""" + buffer_block

    visualizer_block = visualizer_block.replace("pub struct VisualizerSource", "pub(super) struct VisualizerSource", 1)
    visualizer_module = """use rodio::cpal::Sample as CpalSample;
use rodio::{Sample as RodioSample, Source as RodioSource};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

""" + visualizer_block

    # The helper block contains inject_id3_tags, sanitize_filename, and parse_stream_title.
    parse_marker = "/// Parse the `StreamTitle` field from an ICY metadata string."
    parse_start = helper_block.find(parse_marker)
    if parse_start == -1:
        raise SystemExit("Missing parse_stream_title helper")

    recording_helpers = helper_block[:parse_start].rstrip() + "\n"
    metadata_helper = helper_block[parse_start:].rstrip() + "\n"

    recording_helpers = recording_helpers.replace("fn inject_id3_tags", "pub(super) fn inject_id3_tags", 1)
    recording_helpers = recording_helpers.replace("fn sanitize_filename", "pub(super) fn sanitize_filename", 1)
    metadata_helper = metadata_helper.replace("fn parse_stream_title", "pub(super) fn parse_stream_title", 1)

    recording_module = recording_helpers + """
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("normal_file.mp3"), "normal_file.mp3");
        assert_eq!(sanitize_filename("artist/song?.mp3"), "artist-song-.mp3");
        assert_eq!(
            sanitize_filename("windows\\invalid:name*char\".mp3"),
            "windows-invalid-name-char-.mp3"
        );
        assert_eq!(sanitize_filename("<tag> | pipe.mp3"), "-tag- - pipe.mp3");
    }
}
"""

    metadata_module = metadata_helper + """
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_stream_title() {
        assert_eq!(
            parse_stream_title("StreamTitle='Lazerhawk - King of The Streets';StreamUrl='';"),
            Some("Lazerhawk - King of The Streets".to_string())
        );

        assert_eq!(
            parse_stream_title("StreamTitle='  Kavinsky - Nightcall  ';StreamUrl='';"),
            Some("Kavinsky - Nightcall".to_string())
        );

        assert_eq!(parse_stream_title("StreamUrl='';"), None);
        assert_eq!(parse_stream_title("StreamTitle='';"), Some("".to_string()));
    }
}
"""

    write(AUDIO_DIR / "buffer.rs", buffer_module)
    write(AUDIO_DIR / "visualizer.rs", visualizer_module)
    write(AUDIO_DIR / "recording.rs", recording_module)
    write(AUDIO_DIR / "metadata.rs", metadata_module)
    write(AUDIO, text)

    print("Phase 2 audio leaf-module split applied.")


if __name__ == "__main__":
    main()
