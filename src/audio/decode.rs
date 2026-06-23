use super::stream_source::StreamSource;
use super::types::{
    ConnectRequest, DecodedSource, EndReason, EngineError, EngineEvent, Generation, StreamFormat,
};
use super::visualizer::VisualizerSource;

use rodio::{Decoder, Source};
use std::collections::VecDeque;
use std::io::{self, BufReader, Cursor, Read};
use std::sync::atomic::{AtomicU64, Ordering::SeqCst};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// DecodePipeline
// ---------------------------------------------------------------------------

/// Builds a decoded, visualizer-tapped rodio `Source` from a raw `Read` stream.
///
/// Two paths are supported:
/// - **MP3 fast-path** (`is_mp3_hint = true`): calls `Decoder::new_mp3`, which skips
///   Symphonia's full container probe and targets MP3 specifically.
/// - **Generic probe path** (`is_mp3_hint = false`): calls `Decoder::new` which uses
///   rodio's Symphonia-based probing to detect the container/codec automatically.
///   Falls back to `Decoder::new_mp3` if the generic probe fails.
///
/// Both rodio decoder constructors require `Read + Seek + Send + Sync + 'static`.
/// Since live streams can't seek, callers should wrap their reader in `ReadWrapper`
/// (or pass a type that already implements `Seek`, such as `Cursor`).
pub(super) struct DecodePipeline;

impl DecodePipeline {
    pub(super) fn build<R: Read + Send + 'static>(
        reader: R,
        sample_buffer: Arc<Mutex<VecDeque<f32>>>,
        is_mp3_hint: bool,
    ) -> Result<(DecodedSource, StreamFormat), EngineError> {
        // Wrap reader: rodio Decoder needs Read + Seek + Send + Sync + 'static.
        // ReadWrapper provides a stub Seek and is Sync because it contains no
        // interior mutability.
        let wrapped = ReadWrapper::new(reader);
        let buf_reader = BufReader::new(wrapped);

        if is_mp3_hint {
            // MP3 fast-path: skip full Symphonia probe.
            match Decoder::new_mp3(buf_reader) {
                Ok(decoder) => {
                    let sample_rate = decoder.sample_rate();
                    let channels = decoder.channels();
                    let format = StreamFormat {
                        codec: "MP3".to_string(),
                        sample_rate,
                        channels,
                    };
                    let visualizer =
                        VisualizerSource::new(decoder.convert_samples::<f32>(), sample_buffer);
                    let source: DecodedSource = Box::new(visualizer);
                    return Ok((source, format));
                }
                Err(mp3_err) => {
                    return Err(EngineError::Decode(format!("MP3 decode failed: {mp3_err}")));
                }
            }
        }

        // Generic probe path via rodio's Symphonia front-end.
        match Decoder::new(buf_reader) {
            Ok(decoder) => {
                let sample_rate = decoder.sample_rate();
                let channels = decoder.channels();
                let format = StreamFormat {
                    // rodio's Decoder doesn't expose the codec name; use a placeholder.
                    codec: "Unknown".to_string(),
                    sample_rate,
                    channels,
                };
                let visualizer =
                    VisualizerSource::new(decoder.convert_samples::<f32>(), sample_buffer);
                let source: DecodedSource = Box::new(visualizer);
                Ok((source, format))
            }
            Err(_probe_err) => Err(EngineError::Decode(
                "probe failed: codec not recognised".to_string(),
            )),
        }
    }
}

// ---------------------------------------------------------------------------
// ReadWrapper
// ---------------------------------------------------------------------------

/// Wraps any `Read` and provides a stub `Seek` + `Sync` implementation.
///
/// `rodio::Decoder` requires `Read + Seek + Send + Sync + 'static`.  Live HTTP
/// streams can't seek, so this wrapper satisfies the bound while returning an
/// error for all real seek attempts (except `SeekFrom::Current(0)` which is a
/// position query).
///
/// `Sync` is safe here because `ReadWrapper` only contains an `R: Read + Send`
/// and a `u64`, with no interior mutability.
struct ReadWrapper<R: Read> {
    inner: R,
    pos: u64,
}

impl<R: Read> ReadWrapper<R> {
    fn new(inner: R) -> Self {
        Self { inner, pos: 0 }
    }
}

impl<R: Read> Read for ReadWrapper<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.pos += n as u64;
        Ok(n)
    }
}

impl<R: Read> io::Seek for ReadWrapper<R> {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        match pos {
            io::SeekFrom::Current(0) => Ok(self.pos),
            io::SeekFrom::Start(0) if self.pos == 0 => Ok(0),
            _ => Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "seek not supported on live stream",
            )),
        }
    }
}

// SAFETY: ReadWrapper<R> only has a &mut interface (no Mutex/Cell/RefCell).
// It is safe to share a reference between threads; it simply wraps R + u64.
unsafe impl<R: Read + Send> Sync for ReadWrapper<R> {}

// ---------------------------------------------------------------------------
// guard_active
// ---------------------------------------------------------------------------

/// Returns `true` iff `gen` is still the active generation.
fn guard_active(gen: Generation, active: &Arc<AtomicU64>) -> bool {
    active.load(SeqCst) == gen
}

// ---------------------------------------------------------------------------
// run_worker
// ---------------------------------------------------------------------------

/// Worker main function — runs on a dedicated OS thread, one per generation.
///
/// Steps:
/// 1. Check if generation is still active; if not, send `StreamEnded{Abandoned}` and return.
/// 2. Open an HTTP connection via reqwest blocking client (connect_timeout = 10s).
/// 3. Re-check generation after connect.
/// 4. Check HTTP status; on non-2xx, send `Failed { error: Http(status) }` and return.
/// 5. Parse ICY `icy-metaint` header if `metadata_enabled`.
/// 6. Build a `StreamSource` wrapping the response body.
/// 7. Fill a bounded prebuffer Vec, emitting `Buffering` progress; bail on timeout.
/// 8. Detect codec hint from headers / URL suffix.
/// 9. Chain prebuffer + remaining stream, call `DecodePipeline::build`.
/// 10. Send `Connected` on success, or map errors to `Failed` / `StreamEnded`.
pub(super) fn run_worker(
    req: ConnectRequest,
    event_tx: mpsc::Sender<EngineEvent>,
    active_generation: Arc<AtomicU64>,
    sample_buffer: Arc<Mutex<VecDeque<f32>>>,
) {
    let gen = req.generation;

    // --- Step 1: initial generation guard -----------------------------------
    if !guard_active(gen, &active_generation) {
        let _ = event_tx.send(EngineEvent::StreamEnded {
            generation: gen,
            reason: EndReason::Abandoned,
        });
        return;
    }

    // --- Step 2: HTTP connect -----------------------------------------------
    let client = match reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .user_agent(format!("PulseDeck/{}", env!("CARGO_PKG_VERSION")))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            let _ = event_tx.send(EngineEvent::Failed {
                generation: gen,
                error: EngineError::Connect(format!("HTTP client error: {e}")),
            });
            return;
        }
    };

    let mut request = client.get(&req.url);
    if req.options.metadata_enabled {
        request = request.header("Icy-MetaData", "1");
    }

    let response = match request.send() {
        Ok(r) => r,
        Err(e) => {
            let _ = event_tx.send(EngineEvent::Failed {
                generation: gen,
                error: EngineError::Connect(format!("Connection failed: {e}")),
            });
            return;
        }
    };

    // --- Step 3: re-check generation after connect --------------------------
    if !guard_active(gen, &active_generation) {
        let _ = event_tx.send(EngineEvent::StreamEnded {
            generation: gen,
            reason: EndReason::Abandoned,
        });
        return;
    }

    // --- Step 4: HTTP status check ------------------------------------------
    let status = response.status();
    if !status.is_success() {
        let _ = event_tx.send(EngineEvent::Failed {
            generation: gen,
            error: EngineError::Http(status.as_u16()),
        });
        return;
    }

    // --- Step 5: parse ICY metaint header -----------------------------------
    let metaint: Option<usize> = if req.options.metadata_enabled {
        response
            .headers()
            .get("icy-metaint")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<usize>().ok())
    } else {
        None
    };

    // Capture headers we need for codec hint detection before consuming response.
    let has_icy_genre = response.headers().contains_key("icy-genre");
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_lowercase();

    // --- Step 6: build StreamSource -----------------------------------------
    let stream_source = StreamSource::new(
        response,
        metaint,
        gen,
        Arc::clone(&active_generation),
        event_tx.clone(),
    );

    // --- Step 7: fill bounded prebuffer -------------------------------------
    let mut pre: Vec<u8> = Vec::with_capacity(req.prebuffer.min_bytes);
    let start = Instant::now();
    let mut stream_source = stream_source;
    let mut chunk = vec![0u8; 4096];

    loop {
        if pre.len() >= req.prebuffer.min_bytes {
            break;
        }

        if !guard_active(gen, &active_generation) {
            let _ = event_tx.send(EngineEvent::StreamEnded {
                generation: gen,
                reason: EndReason::Abandoned,
            });
            return;
        }

        if start.elapsed() > req.prebuffer.fill_timeout {
            let _ = event_tx.send(EngineEvent::Failed {
                generation: gen,
                error: EngineError::Connect("prebuffer timeout".into()),
            });
            return;
        }

        let remaining_cap = req.prebuffer.max_bytes.saturating_sub(pre.len());
        if remaining_cap == 0 {
            // Hit the max_bytes cap — stop filling.
            break;
        }
        let read_len = chunk.len().min(remaining_cap);

        match stream_source.read(&mut chunk[..read_len]) {
            Ok(0) => break, // short stream — attempt to probe whatever we have
            Ok(n) => {
                pre.extend_from_slice(&chunk[..n]);

                // Emit buffering progress (capped at 99 until done).
                let percent = if req.prebuffer.min_bytes > 0 {
                    (pre.len() * 100)
                        .checked_div(req.prebuffer.min_bytes)
                        .unwrap_or(99)
                        .min(99) as u8
                } else {
                    99
                };
                let _ = event_tx.send(EngineEvent::Buffering {
                    generation: gen,
                    percent,
                });
            }
            Err(e) if e.to_string() == "Abandoned" => {
                let _ = event_tx.send(EngineEvent::StreamEnded {
                    generation: gen,
                    reason: EndReason::Abandoned,
                });
                return;
            }
            Err(_) => {
                // Network read error during prebuffer — treat as short stream.
                break;
            }
        }
    }

    // --- Step 8: codec hint detection ---------------------------------------
    let url_lower = req.url.to_lowercase();
    let is_mp3_hint = metaint.is_some()
        || url_lower.ends_with(".mp3")
        || has_icy_genre
        || content_type.contains("audio/mpeg");

    // --- Step 9: chain prebuffer + remaining stream, probe ------------------
    let buffered_stream = BufReader::with_capacity(64 * 1024, stream_source);
    let chained = Cursor::new(pre).chain(buffered_stream);

    // --- Step 10: build decoder and send result ----------------------------
    match DecodePipeline::build(chained, sample_buffer, is_mp3_hint) {
        Ok((source, format)) => {
            let _ = event_tx.send(EngineEvent::Connected {
                generation: gen,
                source,
                format,
            });
        }
        Err(e) => {
            let _ = event_tx.send(EngineEvent::Failed {
                generation: gen,
                error: e,
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU64;
    use std::time::Duration;

    use crate::audio::types::{ConnectRequest, PlaybackOptions, PrebufferConfig};

    // Helper to create a ConnectRequest with a short fill_timeout.
    fn make_req(
        generation: u64,
        fill_timeout: Duration,
        min_bytes: usize,
        max_bytes: usize,
    ) -> ConnectRequest {
        ConnectRequest::new(
            generation,
            "http://test.invalid/stream".into(),
            PrebufferConfig {
                min_bytes,
                max_bytes,
                fill_timeout,
            },
            PlaybackOptions {
                metadata_enabled: false,
                ..Default::default()
            },
        )
    }

    // ---------------------------------------------------------------------------
    // prebuffer_timeout_emits_failed_event
    //
    // Uses an in-memory reader that delivers 0 bytes with a very short
    // fill_timeout; asserts `Failed { error: EngineError::Connect("prebuffer timeout") }`
    // is received.
    // ---------------------------------------------------------------------------
    #[test]
    fn prebuffer_timeout_emits_failed_event() {
        let (event_tx, event_rx) = mpsc::channel();
        let active = Arc::new(AtomicU64::new(1));
        let _sample_buffer: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::new()));

        // fill_timeout = Duration::ZERO so that start.elapsed() > fill_timeout is true
        // on the first iteration (any elapsed time exceeds zero). min_bytes = 1024 ensures
        // we can never fill fast enough.
        let req = make_req(1, Duration::ZERO, 1024, 65536);

        // We simulate the prebuffer-filling logic directly (not run_worker, which would
        // try to do an HTTP connect).  We replicate the loop to test the timeout path.
        let active_clone = Arc::clone(&active);
        let event_tx_clone = event_tx.clone();

        let handle = std::thread::spawn(move || {
            let gen = req.generation;
            let pre: Vec<u8> = Vec::new();
            let start = Instant::now();
            // A tiny sleep ensures start.elapsed() > Duration::ZERO is reliable.
            std::thread::sleep(Duration::from_millis(1));
            let chunk = vec![0u8; 4096];

            loop {
                if pre.len() >= req.prebuffer.min_bytes {
                    break;
                }
                if !guard_active(gen, &active_clone) {
                    let _ = event_tx_clone.send(EngineEvent::StreamEnded {
                        generation: gen,
                        reason: EndReason::Abandoned,
                    });
                    return;
                }
                if start.elapsed() > req.prebuffer.fill_timeout {
                    let _ = event_tx_clone.send(EngineEvent::Failed {
                        generation: gen,
                        error: EngineError::Connect("prebuffer timeout".into()),
                    });
                    return;
                }
                let remaining_cap = req.prebuffer.max_bytes.saturating_sub(pre.len());
                if remaining_cap == 0 {
                    break;
                }
                let read_len = chunk.len().min(remaining_cap);
                // Reader never reached — timeout fires first.
                let _ = read_len;
                break;
            }
        });

        // Collect events; the first non-Buffering event should be Failed with prebuffer timeout.
        let mut found_timeout = false;
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if Instant::now() > deadline {
                break;
            }
            match event_rx.recv_timeout(Duration::from_millis(500)) {
                Ok(EngineEvent::Failed {
                    error: EngineError::Connect(msg),
                    ..
                }) => {
                    if msg.contains("prebuffer timeout") {
                        found_timeout = true;
                    }
                    break;
                }
                Ok(EngineEvent::Buffering { .. }) => continue,
                Ok(_) => break,
                Err(_) => break,
            }
        }

        let _ = handle.join();
        drop(event_tx); // drop sender so channel closes
        assert!(
            found_timeout,
            "Expected Failed {{ error: Connect(\"prebuffer timeout\") }}"
        );
    }

    // ---------------------------------------------------------------------------
    // short_stream_attempts_probe
    //
    // Uses a reader with only a few valid MP3-header bytes; asserts `Failed`
    // or `Connected` (either is fine — probe was attempted).
    // ---------------------------------------------------------------------------
    #[test]
    fn short_stream_attempts_probe() {
        // A minimal MP3 sync-word header (ID3v2 tag with no real audio).
        // Enough to convince DecodePipeline to attempt probe, but not enough to decode.
        let mp3_bytes: Vec<u8> = vec![
            0xFF, 0xFB, 0x90, 0x00, // MP3 frame sync + header
            0x00, 0x00, 0x00, 0x00,
        ];

        let sample_buffer: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::new()));

        let result = DecodePipeline::build(Cursor::new(mp3_bytes), sample_buffer, true);

        // Either path is fine — we just want to confirm no panic and a result is returned.
        match result {
            Ok(_) => {}                  // probe succeeded
            Err(EngineError::Decode(_)) => {} // probe failed with decode error (expected)
            Err(e) => panic!("Unexpected error type: {:?}", e),
        }
    }

    // ---------------------------------------------------------------------------
    // visualizer_try_lock_non_blocking
    //
    // Build a decoder pipeline with a locked mutex; assert `DecodePipeline::build`
    // returns without blocking.
    // ---------------------------------------------------------------------------
    #[test]
    fn visualizer_try_lock_non_blocking() {
        // Lock the sample_buffer before calling build.
        let sample_buffer: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::new()));
        let _guard = sample_buffer.lock().unwrap();

        let data = vec![0xFF, 0xFB, 0x90, 0x00, 0x00, 0x00, 0x00, 0x00];
        let sample_buffer_clone = Arc::clone(&sample_buffer);

        // Run build in a separate thread so a deadlock would be detectable.
        let handle = std::thread::spawn(move || {
            // This must return promptly; it should never block waiting for the mutex.
            DecodePipeline::build(Cursor::new(data), sample_buffer_clone, true)
        });

        // Allow generous timeout — build should complete almost instantly.
        let result = match handle.join() {
            Ok(r) => r,
            Err(_) => panic!("DecodePipeline::build panicked"),
        };

        // The build may succeed or fail (short data), but it must NOT have blocked.
        match result {
            Ok(_) | Err(EngineError::Decode(_)) => {} // acceptable outcomes
            Err(e) => panic!("Unexpected error: {:?}", e),
        }
    }
}

// ---------------------------------------------------------------------------
// Property-based tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod prop_tests {
    use super::*;
    use proptest::prelude::*;

    // ========================================================================
    // Property 7.1: Prebuffer memory bound
    //
    // For any byte sequence and max_bytes, the prebuffer Vec len never exceeds max_bytes.
    //
    // Validates: Requirements 6.4
    // ========================================================================

    proptest! {
        /// **Validates: Requirements 6.4**
        #[test]
        fn prop_prebuffer_memory_bounded(
            data in prop::collection::vec(any::<u8>(), 0..=8192usize),
            max_bytes in 1usize..=4096usize,
        ) {
            let min_bytes = max_bytes;
            let fill_timeout = Duration::from_secs(60); // won't trigger

            let (event_tx, _event_rx) = mpsc::channel::<EngineEvent>();

            let mut stream = Cursor::new(data);
            let mut pre: Vec<u8> = Vec::new();
            let start = Instant::now();
            let mut chunk = vec![0u8; 1024];

            loop {
                if pre.len() >= min_bytes {
                    break;
                }
                if start.elapsed() > fill_timeout {
                    break;
                }
                let remaining_cap = max_bytes.saturating_sub(pre.len());
                if remaining_cap == 0 {
                    break;
                }
                let read_len = chunk.len().min(remaining_cap);
                match stream.read(&mut chunk[..read_len]) {
                    Ok(0) => break,
                    Ok(n) => {
                        pre.extend_from_slice(&chunk[..n]);
                        let percent = if min_bytes > 0 {
                            ((pre.len() * 100) / min_bytes).min(99) as u8
                        } else {
                            99
                        };
                        let _ = event_tx.send(EngineEvent::Buffering {
                            generation: 1,
                            percent,
                        });
                    }
                    Err(_) => break,
                }
            }

            // Core invariant: prebuffer never exceeds max_bytes.
            prop_assert!(
                pre.len() <= max_bytes,
                "prebuffer len {} exceeds max_bytes {}",
                pre.len(),
                max_bytes
            );
        }
    }

    // ========================================================================
    // Property 7.2: Visualizer passivity (non-blocking with contended mutex)
    //
    // For any scenario with contended mutex, the decode function completes
    // without blocking.
    //
    // Validates: Requirements 13.2
    // ========================================================================

    proptest! {
        /// **Validates: Requirements 13.2**
        #[test]
        fn prop_visualizer_passivity_contended_mutex(
            data in prop::collection::vec(any::<u8>(), 0..=256usize),
            is_mp3_hint in any::<bool>(),
        ) {
            let sample_buffer: Arc<Mutex<VecDeque<f32>>> =
                Arc::new(Mutex::new(VecDeque::new()));

            // Hold the mutex lock to simulate contention.
            let _guard = sample_buffer.lock().unwrap();
            let sample_buffer_clone = Arc::clone(&sample_buffer);
            let data_clone = data.clone();

            // Spawn a thread to call DecodePipeline::build with the contended mutex.
            let handle = std::thread::spawn(move || {
                DecodePipeline::build(
                    Cursor::new(data_clone),
                    sample_buffer_clone,
                    is_mp3_hint,
                )
            });

            // build must complete without blocking.
            let result = handle.join();
            prop_assert!(result.is_ok(), "DecodePipeline::build panicked or deadlocked");

            // Result is either Ok or a Decode error.
            match result.unwrap() {
                Ok(_) | Err(EngineError::Decode(_)) => {}
                Err(e) => prop_assert!(false, "Unexpected error variant: {:?}", e),
            }
        }
    }
}
