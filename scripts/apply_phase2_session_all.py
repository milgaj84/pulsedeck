#!/usr/bin/env python3
"""Apply the next Phase 2 audio architecture slice in one pass.

This script intentionally performs the whole requested slice together:
- extract stream connection/retry/sink creation into src/audio/session.rs
- make audio device initialization lazy, so DriftFM starts without opening a soundcard
- update docs/audio-architecture.md
- update CHANGELOG.md
- remove this temporary script from the final branch state

Public API is preserved: crate::audio::{AudioCommand, AudioEngine, AudioStatus}.
"""

from pathlib import Path

AUDIO = Path("src/audio.rs")
SESSION = Path("src/audio/session.rs")
DOC = Path("docs/audio-architecture.md")
CHANGELOG = Path("CHANGELOG.md")
THIS_SCRIPT = Path("scripts/apply_phase2_session_all.py")


def replace_once(text: str, old: str, new: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"Expected exactly one match, found {count}: {old[:160]!r}")
    return text.replace(old, new, 1)


def replace_all(text: str, old: str, new: str) -> str:
    count = text.count(old)
    if count == 0:
        raise SystemExit(f"Expected at least one match, found 0: {old[:160]!r}")
    return text.replace(old, new)


def update_audio_rs() -> None:
    text = AUDIO.read_text(encoding="utf-8")

    if "mod session;" in text:
        print("audio.rs already references session module; skipping audio.rs transform")
        return

    session_start = text.find("/// Connect to a stream URL and create a playable Sink")
    if session_start == -1:
        raise SystemExit("Could not find connection/session block in src/audio.rs")
    text = text[:session_start].rstrip() + "\n"

    text = replace_once(
        text,
        "mod recording;\nmod stream_reader;\nmod visualizer;",
        "mod recording;\nmod session;\nmod stream_reader;\nmod visualizer;",
    )
    text = replace_once(text, "use buffer::BufferQueue;\n", "")
    text = replace_once(text, "use stream_reader::StreamReader;\n", "")
    text = replace_once(text, "use visualizer::VisualizerSource;\n", "")
    text = replace_once(
        text,
        "use rodio::{Decoder, OutputStream, Sink};",
        "use rodio::{OutputStream, Sink};",
    )
    text = replace_once(text, "use std::io::Read;\n", "")
    text = replace_once(
        text,
        "use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};",
        "use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};\n\nuse session::{connect_and_decode, ConnectionContext};",
    )

    eager_device = """    // Keep OutputStream alive for the lifetime of this thread.
    let (_stream, handle) = match OutputStream::try_default() {
        Ok(s) => s,
        Err(e) => {
            let _ = status_tx.send(AudioStatus::Error(format!("Soundcard error: {}", e)));
            return;
        }
    };

"""
    lazy_device = """    // Lazily opened on first playback. This keeps browsing/search usable on
    // systems without an immediately available output device.
    let mut output_stream: Option<OutputStream> = None;
    let mut output_handle: Option<rodio::OutputStreamHandle> = None;

"""
    text = replace_once(text, eager_device, lazy_device)

    closure_start = text.find("        // Helper closure to spawn a connection thread")
    closure_end = text.find("        // Non-blocking check for commands", closure_start)
    if closure_start == -1 or closure_end == -1:
        raise SystemExit("Could not find spawn_connection closure block")
    text = text[:closure_start] + text[closure_end:]

    old_call = """spawn_connection(
                                url,
                                &mut current_conn_id,
                                &active_conn_id,
                                &mut connect_thread,
                            );"""
    new_call = """spawn_connection(
                                url,
                                &mut current_conn_id,
                                &active_conn_id,
                                &mut connect_thread,
                                &mut output_stream,
                                &mut output_handle,
                                &status_tx,
                                &record_state,
                                &sample_buffer,
                            );"""
    text = replace_all(text, old_call, new_call)

    helpers = r'''
fn ensure_output_handle(
    output_stream: &mut Option<OutputStream>,
    output_handle: &mut Option<rodio::OutputStreamHandle>,
    status_tx: &mpsc::Sender<AudioStatus>,
) -> Option<rodio::OutputStreamHandle> {
    if output_handle.is_none() {
        match OutputStream::try_default() {
            Ok((stream, handle)) => {
                *output_stream = Some(stream);
                *output_handle = Some(handle);
            }
            Err(err) => {
                let _ = status_tx.send(AudioStatus::Error(format!("Soundcard error: {err}")));
                return None;
            }
        }
    }

    output_handle.clone()
}

fn spawn_connection(
    url: String,
    conn_id_ref: &mut u64,
    active_ref: &Arc<AtomicU64>,
    connect_ref: &mut Option<std::thread::JoinHandle<Result<Sink, String>>>,
    output_stream: &mut Option<OutputStream>,
    output_handle: &mut Option<rodio::OutputStreamHandle>,
    status_tx: &mpsc::Sender<AudioStatus>,
    record_state: &Arc<RecordStateShared>,
    sample_buffer: &Arc<Mutex<VecDeque<f32>>>,
) {
    let Some(handle) = ensure_output_handle(output_stream, output_handle, status_tx) else {
        return;
    };

    *conn_id_ref += 1;
    active_ref.store(*conn_id_ref, Ordering::SeqCst);
    let _ = status_tx.send(AudioStatus::Connecting);

    let conn_id = *conn_id_ref;
    let context = ConnectionContext {
        status_tx: status_tx.clone(),
        conn_id,
        active_conn_id: active_ref.clone(),
        record_state: record_state.clone(),
        sample_buffer: sample_buffer.clone(),
    };

    drop(connect_ref.take());
    *connect_ref = Some(std::thread::spawn(move || {
        connect_and_decode(url, handle, context)
    }));
}
'''

    text = text.rstrip() + "\n" + helpers
    AUDIO.write_text(text, encoding="utf-8")


def write_session_rs() -> None:
    SESSION.write_text(r'''use super::buffer::BufferQueue;
use super::stream_reader::StreamReader;
use super::visualizer::VisualizerSource;
use super::{AudioStatus, RecordStateShared};

use rodio::{Decoder, Sink};
use std::collections::VecDeque;
use std::io::Read;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

#[derive(Clone)]
pub(super) struct ConnectionContext {
    pub(super) status_tx: mpsc::Sender<AudioStatus>,
    pub(super) conn_id: u64,
    pub(super) active_conn_id: Arc<AtomicU64>,
    pub(super) record_state: Arc<RecordStateShared>,
    pub(super) sample_buffer: Arc<Mutex<VecDeque<f32>>>,
}

/// Connect to a stream URL and create a playable Sink, with automatic backoff retries.
pub(super) fn connect_and_decode(
    url: String,
    handle: rodio::OutputStreamHandle,
    context: ConnectionContext,
) -> Result<Sink, String> {
    let mut retries = 0;
    let max_retries = 5;
    let mut backoff = Duration::from_secs(1);

    loop {
        if context.active_conn_id.load(Ordering::SeqCst) != context.conn_id {
            return Err("Abandoned".into());
        }

        match try_connect_and_decode_once(&url, &handle, context.clone()) {
            Ok(sink) => return Ok(sink),
            Err(err) => {
                if err == "Abandoned" {
                    return Err("Abandoned".into());
                }

                retries += 1;
                if retries >= max_retries {
                    return Err(format!("Failed after {max_retries} retries: {err}"));
                }

                let _ = context.status_tx.send(AudioStatus::Connecting);

                let sleep_step = Duration::from_millis(100);
                let steps = (backoff.as_millis() / sleep_step.as_millis()) as usize;
                for _ in 0..steps {
                    if context.active_conn_id.load(Ordering::SeqCst) != context.conn_id {
                        return Err("Abandoned".into());
                    }
                    std::thread::sleep(sleep_step);
                }

                backoff = (backoff * 2).min(Duration::from_secs(8));
            }
        }
    }
}

fn try_connect_and_decode_once(
    url: &str,
    handle: &rodio::OutputStreamHandle,
    context: ConnectionContext,
) -> Result<Sink, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(None)
        .connect_timeout(Duration::from_secs(5))
        .user_agent(format!("DriftFM/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|err| format!("HTTP client error: {err}"))?;

    if context.active_conn_id.load(Ordering::SeqCst) != context.conn_id {
        return Err("Abandoned".into());
    }

    let response = client
        .get(url)
        .header("Icy-MetaData", "1")
        .send()
        .map_err(|err| format!("Connection failed: {err}"))?;

    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }

    if context.active_conn_id.load(Ordering::SeqCst) != context.conn_id {
        return Err("Abandoned".into());
    }

    let metaint = response
        .headers()
        .get("icy-metaint")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<usize>().ok());

    let bitrate_kbps = response
        .headers()
        .get("icy-br")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(128);
    let bytes_per_sec = (bitrate_kbps * 1000 / 8).max(1) as usize;

    let buffer_capacity = 1024 * 1024;
    let queue = Arc::new(BufferQueue::new(buffer_capacity));

    let queue_clone = queue.clone();
    let active_conn_id_clone = context.active_conn_id.clone();
    let conn_id = context.conn_id;
    let status_tx_clone = context.status_tx.clone();
    let mut response_reader = response;

    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            if active_conn_id_clone.load(Ordering::SeqCst) != conn_id {
                queue_clone.set_disconnected(true);
                break;
            }
            match response_reader.read(&mut buf) {
                Ok(0) => {
                    queue_clone.set_disconnected(true);
                    break;
                }
                Ok(n) => {
                    queue_clone.push(&buf[..n]);

                    let len = queue_clone.len();
                    let cap = queue_clone.capacity;
                    let percent = ((len * 100) / cap) as u8;
                    let seconds = (len / bytes_per_sec) as u32;
                    let _ = status_tx_clone.send(AudioStatus::BufferLevel { percent, seconds });
                }
                Err(_) => {
                    queue_clone.set_disconnected(true);
                    break;
                }
            }
        }
    });

    let reader = StreamReader::new(
        url.to_string(),
        queue,
        context.status_tx,
        context.conn_id,
        context.active_conn_id,
        context.record_state,
        metaint,
    );

    let source = Decoder::new(reader).map_err(|err| format!("Decode error: {err}"))?;
    let wrapped_source = VisualizerSource::new(source, context.sample_buffer);
    let sink = Sink::try_new(handle).map_err(|err| format!("Sink error: {err}"))?;

    sink.append(wrapped_source);

    Ok(sink)
}
''', encoding="utf-8")


def update_docs() -> None:
    text = DOC.read_text(encoding="utf-8")
    text = replace_once(
        text,
        "- `src/audio.rs` owns the public API, audio thread loop, playback state, command handling, connection retry flow, and sink creation.\n",
        "- `src/audio.rs` owns the public API, audio thread loop, playback state, command handling, and lazy output-device initialization.\n",
    )
    text = replace_once(
        text,
        "- `src/audio/stream_reader.rs` owns ICY metadata boundary stripping, recording segment lifecycle, and the `Read`/`Seek` adapter consumed by `rodio::Decoder`.\n",
        "- `src/audio/session.rs` owns stream connection, retry/backoff, downloader setup, decoder setup, and sink creation.\n- `src/audio/stream_reader.rs` owns ICY metadata boundary stripping, recording segment lifecycle, and the `Read`/`Seek` adapter consumed by `rodio::Decoder`.\n",
    )
    text = replace_once(
        text,
        "- Extract connection/retry/session logic after the `StreamReader` split is merged.\n- Consider lazy audio device initialization so the app can browse/search even when no output device is available.\n- Improve recording filename collision handling and stream format detection in a behavior-change PR.\n",
        "- Split the audio thread loop into a small playback state object if command handling grows further.\n- Improve recording filename collision handling and stream format detection in a behavior-change PR.\n",
    )
    DOC.write_text(text, encoding="utf-8")


def update_changelog() -> None:
    text = CHANGELOG.read_text(encoding="utf-8")
    if "Audio Session Extraction" not in text:
        text = replace_once(
            text,
            "### Improved\n",
            "### Improved\n*   **Audio Session Extraction**: Moved stream connection, retry/backoff, decoder setup, downloader setup, and sink creation into a dedicated audio session module while preserving playback behavior.\n*   **Lazy Audio Device Initialization**: DriftFM now opens the system output device on first playback instead of app startup, so browsing and search remain usable when no soundcard is immediately available.\n",
        )
    CHANGELOG.write_text(text, encoding="utf-8")


def main() -> None:
    update_audio_rs()
    write_session_rs()
    update_docs()
    update_changelog()
    THIS_SCRIPT.unlink(missing_ok=True)
    print("Applied one-pass audio session extraction, lazy device initialization, docs, and changelog updates.")


if __name__ == "__main__":
    main()
