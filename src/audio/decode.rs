use super::codec::{detect_codec, CodecDetection, CodecHint};
use super::hls;
use super::panic_guard;
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

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const STREAM_READ_TIMEOUT: Duration = Duration::from_secs(8);
const MAX_ICY_METAINT: usize = 16 * 1024 * 1024;

/// Builds a decoded, visualizer-tapped rodio source from a live byte stream.
///
/// Generic Symphonia probing is the safe default because public radio headers
/// and URL extensions are often inaccurate. The MP3 fast path is used only when
/// MP3 frame or ID3 magic bytes were observed in the prebuffer.
pub(super) struct DecodePipeline;

impl DecodePipeline {
    pub(super) fn build<R: Read + Send + 'static>(
        reader: R,
        sample_buffer: Arc<Mutex<VecDeque<f32>>>,
        detection: CodecDetection,
    ) -> Result<(DecodedSource, StreamFormat), EngineError> {
        // Symphonia has no Opus decoder, so say so plainly instead of failing
        // later with a confusing "probe failed".
        if detection.hint == CodecHint::Opus {
            return Err(EngineError::Decode(
                "Opus audio is not supported".to_string(),
            ));
        }

        let wrapped = ReadWrapper::new(reader);
        let buf_reader = BufReader::new(wrapped);
        let verified_mp3 = detection.verified_mp3();

        // Third-party decoders can panic on input they cannot handle, so the
        // probe runs inside a guard that turns a panic into an error.
        let probed = panic_guard::run_quiet(|| {
            if verified_mp3 {
                Decoder::new_mp3(buf_reader)
            } else {
                Decoder::new(buf_reader)
            }
            .map_err(|error| error.to_string())
        });
        let decoder = match probed {
            Ok(Ok(decoder)) => decoder,
            Ok(Err(message)) if verified_mp3 => {
                return Err(EngineError::Decode(format!(
                    "verified MP3 stream could not be decoded: {message}"
                )))
            }
            Ok(Err(message)) => {
                return Err(EngineError::Decode(format!(
                    "{} probe failed: {message}",
                    detection.hint.label()
                )))
            }
            Err(()) => return Err(decoder_crashed()),
        };

        let format = StreamFormat {
            codec: detection.hint.label().to_string(),
            sample_rate: decoder.sample_rate(),
            channels: decoder.channels(),
        };
        let visualizer = VisualizerSource::new(decoder.convert_samples::<f32>(), sample_buffer);
        Ok((Box::new(visualizer), format))
    }
}

/// The error for a decoder that panicked while probing a stream.
pub(super) fn decoder_crashed() -> EngineError {
    EngineError::Decode(
        "this stream's container crashed the decoder and is not supported".to_string(),
    )
}

/// Adapts a live reader to rodio's `Read + Seek + Send + Sync` requirement.
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

// SAFETY: ReadWrapper exposes mutation only through `&mut self`; it contains no
// interior-mutability primitives and is never read concurrently by PulseDeck.
unsafe impl<R: Read + Send> Sync for ReadWrapper<R> {}

fn guard_active(generation: Generation, active: &Arc<AtomicU64>) -> bool {
    active.load(SeqCst) == generation
}

#[derive(Debug)]
pub(super) enum PrebufferFailure {
    Abandoned,
    Timeout,
    Read(io::Error),
}

pub(super) fn fill_prebuffer<R: Read>(
    reader: &mut R,
    request: &ConnectRequest,
    event_tx: &mpsc::Sender<EngineEvent>,
    active_generation: &Arc<AtomicU64>,
) -> Result<Vec<u8>, PrebufferFailure> {
    let generation = request.generation;
    let mut prebuffer = Vec::with_capacity(request.prebuffer.min_bytes);
    let started = Instant::now();
    let mut chunk = vec![0_u8; 4096];

    loop {
        if prebuffer.len() >= request.prebuffer.min_bytes {
            break;
        }
        if !guard_active(generation, active_generation) {
            return Err(PrebufferFailure::Abandoned);
        }
        if started.elapsed() >= request.prebuffer.fill_timeout {
            return Err(PrebufferFailure::Timeout);
        }

        let remaining = request.prebuffer.max_bytes.saturating_sub(prebuffer.len());
        if remaining == 0 {
            break;
        }

        let read_len = chunk.len().min(remaining);
        match reader.read(&mut chunk[..read_len]) {
            Ok(0) => break,
            Ok(read) => {
                prebuffer.extend_from_slice(&chunk[..read]);

                if !guard_active(generation, active_generation) {
                    return Err(PrebufferFailure::Abandoned);
                }
                if started.elapsed() >= request.prebuffer.fill_timeout
                    && prebuffer.len() < request.prebuffer.min_bytes
                {
                    return Err(PrebufferFailure::Timeout);
                }

                let percent = if request.prebuffer.min_bytes == 0 {
                    99
                } else {
                    prebuffer
                        .len()
                        .checked_mul(100)
                        .and_then(|scaled| scaled.checked_div(request.prebuffer.min_bytes))
                        .map(|pct| pct.min(99) as u8)
                        .unwrap_or(99)
                };
                let _ = event_tx.send(EngineEvent::Buffering {
                    generation,
                    percent,
                });
            }
            Err(error) if is_abandoned_error(&error) => {
                return Err(PrebufferFailure::Abandoned);
            }
            Err(error) if is_timeout_error(&error) => {
                return Err(PrebufferFailure::Timeout);
            }
            Err(error) => return Err(PrebufferFailure::Read(error)),
        }
    }

    if prebuffer.is_empty() {
        return Err(PrebufferFailure::Read(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "stream ended before sending audio data",
        )));
    }

    Ok(prebuffer)
}

fn is_timeout_error(error: &io::Error) -> bool {
    if matches!(
        error.kind(),
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
    ) {
        return true;
    }

    let message = error.to_string().to_ascii_lowercase();
    message.contains("timed out") || message.contains("timeout")
}

fn is_abandoned_error(error: &io::Error) -> bool {
    error.to_string().eq_ignore_ascii_case("abandoned")
}

pub(super) fn send_abandoned(event_tx: &mpsc::Sender<EngineEvent>, generation: Generation) {
    let _ = event_tx.send(EngineEvent::StreamEnded {
        generation,
        reason: EndReason::Abandoned,
    });
}

pub(super) fn send_failure(
    event_tx: &mpsc::Sender<EngineEvent>,
    generation: Generation,
    error: EngineError,
) {
    let _ = event_tx.send(EngineEvent::Failed { generation, error });
}

fn hls_context<'a>(
    client: reqwest::blocking::Client,
    request: &'a ConnectRequest,
    event_tx: &'a mpsc::Sender<EngineEvent>,
    active_generation: &'a Arc<AtomicU64>,
    sample_buffer: Arc<Mutex<VecDeque<f32>>>,
) -> hls::HlsContext<'a> {
    hls::HlsContext {
        client,
        request,
        event_tx,
        active_generation,
        sample_buffer,
    }
}

/// Connect, prebuffer, classify, and construct one decoded stream source.
pub(super) fn run_worker(
    request: ConnectRequest,
    event_tx: mpsc::Sender<EngineEvent>,
    active_generation: Arc<AtomicU64>,
    sample_buffer: Arc<Mutex<VecDeque<f32>>>,
) {
    let generation = request.generation;
    if !guard_active(generation, &active_generation) {
        send_abandoned(&event_tx, generation);
        return;
    }

    // Reqwest blocking responses apply this timeout independently to every body
    // read, so continuous healthy streams are not limited to eight seconds total.
    let client = match reqwest::blocking::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(STREAM_READ_TIMEOUT)
        .user_agent(format!("PulseDeck/{}", env!("CARGO_PKG_VERSION")))
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            send_failure(
                &event_tx,
                generation,
                EngineError::Connect(format!("could not initialize HTTP client: {error}")),
            );
            return;
        }
    };

    let mut http_request = client.get(&request.url);
    if request.options.metadata_enabled {
        http_request = http_request.header("Icy-MetaData", "1");
    }

    let response = match http_request.send() {
        Ok(response) => response,
        Err(error) => {
            let message = if error.is_timeout() {
                format!(
                    "connection or response headers timed out after {} seconds",
                    STREAM_READ_TIMEOUT.as_secs()
                )
            } else {
                format!("could not connect: {error}")
            };
            send_failure(&event_tx, generation, EngineError::Connect(message));
            return;
        }
    };

    if !guard_active(generation, &active_generation) {
        send_abandoned(&event_tx, generation);
        return;
    }

    let status = response.status();
    if !status.is_success() {
        send_failure(&event_tx, generation, EngineError::Http(status.as_u16()));
        return;
    }

    let metaint = if request.options.metadata_enabled {
        response
            .headers()
            .get("icy-metaint")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|value| *value > 0 && *value <= MAX_ICY_METAINT)
    } else {
        None
    };

    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let final_url = response.url().as_str().to_string();

    if hls::looks_like_hls(&content_type, &final_url) {
        let body = match hls::read_capped(response, hls::MAX_PLAYLIST_BODY) {
            Ok(body) => body,
            Err(error) => {
                send_failure(
                    &event_tx,
                    generation,
                    EngineError::Connect(format!("could not read HLS playlist: {}", error.0)),
                );
                return;
            }
        };
        hls::run(
            hls_context(
                client,
                &request,
                &event_tx,
                &active_generation,
                sample_buffer,
            ),
            &final_url,
            body,
        );
        return;
    }

    let mut stream = StreamSource::new(
        response,
        metaint,
        generation,
        Arc::clone(&active_generation),
        event_tx.clone(),
    );

    let prebuffer = match fill_prebuffer(&mut stream, &request, &event_tx, &active_generation) {
        Ok(prebuffer) => prebuffer,
        Err(PrebufferFailure::Abandoned) => {
            send_abandoned(&event_tx, generation);
            return;
        }
        Err(PrebufferFailure::Timeout) => {
            send_failure(
                &event_tx,
                generation,
                EngineError::Connect(format!(
                    "stream read timed out while buffering after {} seconds",
                    request.prebuffer.fill_timeout.as_secs()
                )),
            );
            return;
        }
        Err(PrebufferFailure::Read(error)) => {
            send_failure(
                &event_tx,
                generation,
                EngineError::Connect(format!("stream read failed while buffering: {error}")),
            );
            return;
        }
    };

    if !guard_active(generation, &active_generation) {
        send_abandoned(&event_tx, generation);
        return;
    }

    if hls::looks_like_playlist(&prebuffer) {
        // The server did not say it is HLS, but the body is a playlist.
        let mut body = prebuffer;
        if body.len() < hls::MAX_PLAYLIST_BODY {
            let room = (hls::MAX_PLAYLIST_BODY - body.len()) as u64;
            let _ = stream.by_ref().take(room).read_to_end(&mut body);
        }
        hls::run(
            hls_context(
                client,
                &request,
                &event_tx,
                &active_generation,
                sample_buffer,
            ),
            &final_url,
            body,
        );
        return;
    }

    let detection = detect_codec(&prebuffer, &content_type, &final_url);
    let buffered_stream = BufReader::with_capacity(64 * 1024, stream);
    let chained = Cursor::new(prebuffer).chain(buffered_stream);

    match DecodePipeline::build(chained, sample_buffer, detection) {
        Ok((source, format)) => {
            let _ = event_tx.send(EngineEvent::Connected {
                generation,
                source,
                format,
            });
        }
        Err(error) => send_failure(&event_tx, generation, error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::codec::{CodecHint, CodecSource};
    use crate::audio::types::{PlaybackOptions, PrebufferConfig};
    use proptest::prelude::*;
    use std::io::Seek;

    fn request(fill_timeout: Duration, min_bytes: usize, max_bytes: usize) -> ConnectRequest {
        ConnectRequest::new(
            1,
            "http://test.invalid/stream".to_string(),
            PrebufferConfig {
                min_bytes,
                max_bytes,
                fill_timeout,
            },
            PlaybackOptions {
                metadata_enabled: false,
                ..PlaybackOptions::default()
            },
        )
    }

    fn active_generation() -> Arc<AtomicU64> {
        Arc::new(AtomicU64::new(1))
    }

    fn detection(hint: CodecHint, source: CodecSource) -> CodecDetection {
        CodecDetection { hint, source }
    }

    struct TimeoutReader;

    impl Read for TimeoutReader {
        fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::new(io::ErrorKind::TimedOut, "read timed out"))
        }
    }

    struct PanicReader;

    impl Read for PanicReader {
        fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
            panic!("reader must not be called after cancellation")
        }
    }

    #[test]
    fn prebuffer_timeout_error_is_classified_explicitly() {
        let (tx, _rx) = mpsc::channel();
        let result = fill_prebuffer(
            &mut TimeoutReader,
            &request(Duration::from_secs(5), 1024, 4096),
            &tx,
            &active_generation(),
        );

        assert!(matches!(result, Err(PrebufferFailure::Timeout)));
    }

    #[test]
    fn elapsed_prebuffer_deadline_fires_before_read() {
        let (tx, _rx) = mpsc::channel();
        let result = fill_prebuffer(
            &mut PanicReader,
            &request(Duration::ZERO, 1024, 4096),
            &tx,
            &active_generation(),
        );

        assert!(matches!(result, Err(PrebufferFailure::Timeout)));
    }

    #[test]
    fn cancellation_is_checked_before_read() {
        let (tx, _rx) = mpsc::channel();
        let inactive = Arc::new(AtomicU64::new(2));
        let result = fill_prebuffer(
            &mut PanicReader,
            &request(Duration::from_secs(5), 1024, 4096),
            &tx,
            &inactive,
        );

        assert!(matches!(result, Err(PrebufferFailure::Abandoned)));
    }

    #[test]
    fn prebuffer_emits_progress_and_respects_minimum() {
        let (tx, rx) = mpsc::channel();
        let mut reader = Cursor::new(vec![1_u8; 2048]);
        let prebuffer = fill_prebuffer(
            &mut reader,
            &request(Duration::from_secs(5), 1024, 4096),
            &tx,
            &active_generation(),
        )
        .unwrap();

        assert!(prebuffer.len() >= 1024);
        assert!(prebuffer.len() <= 4096);
        assert!(matches!(rx.try_recv(), Ok(EngineEvent::Buffering { .. })));
    }

    #[test]
    fn empty_stream_returns_read_failure() {
        let (tx, _rx) = mpsc::channel();
        let mut reader = Cursor::new(Vec::<u8>::new());
        let result = fill_prebuffer(
            &mut reader,
            &request(Duration::from_secs(5), 1024, 4096),
            &tx,
            &active_generation(),
        );

        assert!(matches!(result, Err(PrebufferFailure::Read(_))));
    }

    #[test]
    fn timeout_detection_handles_kind_and_message() {
        assert!(is_timeout_error(&io::Error::new(
            io::ErrorKind::TimedOut,
            "slow"
        )));
        assert!(is_timeout_error(&io::Error::other("operation timeout")));
        assert!(!is_timeout_error(&io::Error::other("connection reset")));
    }

    #[test]
    fn read_wrapper_tracks_position_and_rejects_real_seeks() {
        let mut reader = ReadWrapper::new(Cursor::new(b"abcdef".to_vec()));
        let mut buffer = [0_u8; 3];

        assert_eq!(reader.read(&mut buffer).unwrap(), 3);
        assert_eq!(reader.stream_position().unwrap(), 3);
        assert_eq!(
            reader.seek(io::SeekFrom::Start(0)).unwrap_err().kind(),
            io::ErrorKind::Unsupported
        );
    }

    #[test]
    fn short_verified_mp3_attempts_decode_without_panicking() {
        let bytes = vec![0xFF, 0xFB, 0x90, 0x00, 0x00, 0x00, 0x00, 0x00];
        let sample_buffer = Arc::new(Mutex::new(VecDeque::new()));
        let result = DecodePipeline::build(
            Cursor::new(bytes),
            sample_buffer,
            detection(CodecHint::Mp3, CodecSource::MagicBytes),
        );

        assert!(matches!(result, Ok(_) | Err(EngineError::Decode(_))));
    }

    #[test]
    fn unverified_mp3_hint_uses_safe_probe_path() {
        let sample_buffer = Arc::new(Mutex::new(VecDeque::new()));
        let result = DecodePipeline::build(
            Cursor::new(vec![0_u8; 32]),
            sample_buffer,
            detection(CodecHint::Mp3, CodecSource::ContentType),
        );

        match result {
            Err(EngineError::Decode(message)) => assert!(message.contains("MP3 probe failed")),
            Ok(_) => {}
            Err(other) => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn visualizer_lock_contention_does_not_block_pipeline_construction() {
        let sample_buffer = Arc::new(Mutex::new(VecDeque::new()));
        let _guard = sample_buffer.lock().unwrap();
        let sample_buffer_clone = Arc::clone(&sample_buffer);

        let handle = std::thread::spawn(move || {
            DecodePipeline::build(
                Cursor::new(vec![0_u8; 32]),
                sample_buffer_clone,
                detection(CodecHint::Unknown, CodecSource::Unknown),
            )
        });

        assert!(handle.join().is_ok());
    }

    #[test]
    fn configured_timeouts_are_finite_and_nonzero() {
        assert!(CONNECT_TIMEOUT > Duration::ZERO);
        assert!(STREAM_READ_TIMEOUT > Duration::ZERO);
        assert!(STREAM_READ_TIMEOUT <= CONNECT_TIMEOUT);
    }

    proptest! {
        #[test]
        fn prebuffer_never_exceeds_maximum(
            data in prop::collection::vec(any::<u8>(), 1..=8192),
            max_bytes in 1usize..=4096,
        ) {
            let (tx, _rx) = mpsc::channel();
            let mut reader = Cursor::new(data);
            let result = fill_prebuffer(
                &mut reader,
                &request(Duration::from_secs(30), max_bytes, max_bytes),
                &tx,
                &active_generation(),
            );

            if let Ok(prebuffer) = result {
                prop_assert!(prebuffer.len() <= max_bytes);
            }
        }
    }

    mod worker_hls {
        use super::*;
        use crate::audio::test_http::{fixed, Handler, Response, TestServer};

        const TONE_AAC: &[u8] = include_bytes!("hls/testdata/tone.aac");
        const MPEGURL: &str = "application/vnd.apple.mpegurl";
        const PLAYLIST: &str = "#EXTM3U\n#EXT-X-TARGETDURATION:1\n#EXT-X-MEDIA-SEQUENCE:0\n#EXTINF:1,\ns0.aac\n#EXTINF:1,\ns1.aac\n#EXTINF:1,\ns2.aac\n#EXT-X-ENDLIST\n";

        fn route(
            response: impl Fn() -> Response + Send + Sync + 'static,
        ) -> Box<dyn Fn(u32) -> Response + Send + Sync> {
            fixed(response)
        }

        fn segments() -> Vec<(&'static str, Handler)> {
            ["/s0.aac", "/s1.aac", "/s2.aac"]
                .into_iter()
                .map(|path| (path, route(|| Response::ok("audio/aac", TONE_AAC))))
                .collect()
        }

        /// Run the worker and collect its events. The stream is abandoned when
        /// this returns; use `run_worker_alive` to keep it playing.
        fn run_worker_against(url: String) -> Vec<EngineEvent> {
            let (events, active, _rx) = run_worker_alive(url, false);
            active.store(0, SeqCst);
            events
        }

        /// Like `run_worker_against`, but leaves the stream active so the
        /// caller can keep reading it. The caller must set the flag to 0.
        fn run_worker_alive(
            url: String,
            metadata_enabled: bool,
        ) -> (
            Vec<EngineEvent>,
            Arc<AtomicU64>,
            mpsc::Receiver<EngineEvent>,
        ) {
            let (event_tx, event_rx) = mpsc::channel();
            let active = active_generation();
            let mut req = request(Duration::from_secs(8), 4096, 512 * 1024);
            req.url = url;
            req.options.metadata_enabled = metadata_enabled;

            run_worker(
                req,
                event_tx,
                Arc::clone(&active),
                Arc::new(Mutex::new(VecDeque::new())),
            );
            (event_rx.try_iter().collect(), active, event_rx)
        }

        fn terminal(events: &[EngineEvent]) -> &EngineEvent {
            events
                .iter()
                .rfind(|event| !matches!(event, EngineEvent::Buffering { .. }))
                .expect("a terminal event")
        }

        fn assert_hls_connected(events: &[EngineEvent]) {
            match terminal(events) {
                EngineEvent::Connected { format, .. } => assert_eq!(format.codec, "HLS AAC"),
                EngineEvent::Failed { error, .. } => {
                    panic!("expected Connected, got {}", error.to_status_string())
                }
                _ => panic!("expected Connected"),
            }
        }

        #[test]
        fn playlist_content_type_takes_the_hls_path() {
            let mut routes = segments();
            routes.push(("/live", route(|| Response::ok(MPEGURL, PLAYLIST))));
            let server = TestServer::start(routes);

            assert_hls_connected(&run_worker_against(server.url("/live")));
        }

        #[test]
        fn m3u8_extension_takes_the_hls_path_even_with_a_wrong_content_type() {
            let mut routes = segments();
            routes.push(("/live.m3u8", route(|| Response::ok("text/plain", PLAYLIST))));
            let server = TestServer::start(routes);

            assert_hls_connected(&run_worker_against(server.url("/live.m3u8")));
        }

        #[test]
        fn playlist_body_is_recognised_without_any_hint() {
            let mut routes = segments();
            routes.push(("/stream", route(|| Response::ok("text/plain", PLAYLIST))));
            let server = TestServer::start(routes);

            assert_hls_connected(&run_worker_against(server.url("/stream")));
        }

        #[test]
        fn plain_m3u_is_not_treated_as_hls() {
            let plain = "#EXTM3U\n#EXTINF:-1,Some Radio\nhttp://example.invalid/stream\n";
            let server = TestServer::start(vec![(
                "/radio.pls",
                route(move || Response::ok("text/plain", plain)),
            )]);

            let events = run_worker_against(server.url("/radio.pls"));

            match terminal(&events) {
                EngineEvent::Failed { error, .. } => {
                    assert!(!error.to_status_string().contains("HLS"));
                }
                _ => panic!("a plain playlist is not decodable audio"),
            }
        }

        #[test]
        fn plain_audio_streams_are_unaffected() {
            let server = TestServer::start(vec![(
                "/radio",
                route(|| Response::ok("audio/aac", TONE_AAC)),
            )]);

            let events = run_worker_against(server.url("/radio"));

            match terminal(&events) {
                EngineEvent::Connected { format, .. } => assert_eq!(format.codec, "AAC"),
                _ => panic!("expected a plain AAC stream to connect"),
            }
        }

        /// Manual check against a real station; needs network access:
        /// `PULSEDECK_HLS_URL=<playlist url> cargo test manual_real_hls -- --ignored --nocapture`
        #[test]
        #[ignore = "needs network and PULSEDECK_HLS_URL"]
        fn manual_real_hls_stream_connects() {
            let Ok(url) = std::env::var("PULSEDECK_HLS_URL") else {
                return;
            };
            let (events, active, event_rx) = run_worker_alive(url, true);
            let mut titles: Vec<String> = events
                .iter()
                .filter_map(|event| match event {
                    EngineEvent::TrackChanged { title, .. } => Some(title.clone()),
                    _ => None,
                })
                .collect();
            let Some(EngineEvent::Connected { source, format, .. }) = events
                .into_iter()
                .rfind(|event| !matches!(event, EngineEvent::Buffering { .. }))
            else {
                panic!("did not connect");
            };
            println!(
                "CONNECTED codec={} rate={} channels={}",
                format.codec, format.sample_rate, format.channels
            );

            // 15 seconds of audio crosses several segment boundaries.
            let wanted = format.sample_rate as usize * usize::from(format.channels) * 15;
            let samples: Vec<f32> = source.take(wanted).collect();
            active.store(0, SeqCst);
            titles.extend(event_rx.try_iter().filter_map(|event| match event {
                EngineEvent::TrackChanged { title, .. } => Some(title),
                _ => None,
            }));
            println!("TITLES {titles:?}");
            let peak = samples.iter().fold(0.0_f32, |max, s| max.max(s.abs()));
            println!(
                "DECODED {} of {} samples, peak {peak:.3}",
                samples.len(),
                wanted
            );
            assert_eq!(samples.len(), wanted, "the stream ended early");
            assert!(peak > 0.001, "decoded audio is silent");
        }

        #[test]
        fn hls_playlist_http_errors_fail_without_starting_hls() {
            let server = TestServer::start(vec![("/live.m3u8", route(|| Response::status(500)))]);

            let events = run_worker_against(server.url("/live.m3u8"));

            match terminal(&events) {
                EngineEvent::Failed { error, .. } => {
                    assert!(matches!(error, EngineError::Http(500)))
                }
                _ => panic!("expected an HTTP failure"),
            }
        }
    }

    mod real_codecs {
        use super::*;
        use crate::audio::test_http::{fixed, Response, TestServer};

        const OGG_VORBIS: &[u8] = include_bytes!("testdata/tone.ogg");
        const OPUS: &[u8] = include_bytes!("testdata/tone.opus");
        const FLAC: &[u8] = include_bytes!("testdata/tone.flac");
        const FRAGMENTED_MP4: &[u8] = include_bytes!("testdata/tone_frag.mp4");

        fn build(bytes: &[u8]) -> Result<(DecodedSource, StreamFormat), EngineError> {
            DecodePipeline::build(
                Cursor::new(bytes.to_vec()),
                Arc::new(Mutex::new(VecDeque::new())),
                detect_codec(bytes, "", ""),
            )
        }

        fn first_samples(bytes: &[u8], count: usize) -> (StreamFormat, Vec<f32>) {
            let (source, format) =
                build(bytes).unwrap_or_else(|e| panic!("{}", e.to_status_string()));
            (format, source.take(count).collect())
        }

        #[test]
        fn ogg_vorbis_decodes_real_audio() {
            let (format, samples) = first_samples(OGG_VORBIS, 4000);

            assert_eq!(format.codec, "Ogg Vorbis");
            assert_eq!(format.sample_rate, 48_000);
            assert_eq!(samples.len(), 4000);
            assert!(samples.iter().fold(0.0_f32, |m, s| m.max(s.abs())) > 0.01);
        }

        #[test]
        fn flac_still_decodes_real_audio() {
            let (format, samples) = first_samples(FLAC, 4000);

            assert_eq!(format.codec, "FLAC");
            assert_eq!(format.sample_rate, 44_100);
            assert_eq!(samples.len(), 4000);
        }

        #[test]
        fn opus_fails_with_a_clear_not_supported_error() {
            let Err(EngineError::Decode(message)) = build(OPUS) else {
                panic!("Opus must be rejected as a decode error");
            };

            assert_eq!(message, "Opus audio is not supported");
        }

        #[test]
        fn a_fragmented_mp4_stream_is_an_error_not_a_panic() {
            // rodio asserts "unreachable" on this input; the guard must turn it
            // into an ordinary error and leave the panic hook active again.
            let result = build(FRAGMENTED_MP4);

            assert!(matches!(result, Err(EngineError::Decode(_))));
            assert!(!crate::audio::quiet_panics_on_this_thread());
        }

        fn run_worker_collecting(url: String) -> Vec<EngineEvent> {
            let (event_tx, event_rx) = mpsc::channel();
            let active = active_generation();
            let mut req = request(Duration::from_secs(8), 1024, 512 * 1024);
            req.url = url;
            run_worker(
                req,
                event_tx,
                Arc::clone(&active),
                Arc::new(Mutex::new(VecDeque::new())),
            );
            active.store(0, SeqCst);
            event_rx.try_iter().collect()
        }

        fn serve(content_type: &'static str, body: &'static [u8]) -> TestServer {
            TestServer::start(vec![(
                "/stream",
                fixed(move || Response::ok(content_type, body)),
            )])
        }

        #[test]
        fn an_ogg_vorbis_station_connects_through_the_worker() {
            let server = serve("application/ogg", OGG_VORBIS);

            let events = run_worker_collecting(server.url("/stream"));

            match events.last() {
                Some(EngineEvent::Connected { format, .. }) => {
                    assert_eq!(format.codec, "Ogg Vorbis")
                }
                _ => panic!("expected the Ogg stream to connect"),
            }
        }

        #[test]
        fn an_opus_station_fails_with_the_clear_message() {
            let server = serve("audio/ogg", OPUS);

            let events = run_worker_collecting(server.url("/stream"));

            match events.last() {
                Some(EngineEvent::Failed { error, .. }) => {
                    assert!(error
                        .to_status_string()
                        .contains("Opus audio is not supported"))
                }
                _ => panic!("expected an Opus failure"),
            }
        }

        #[test]
        fn a_fragmented_mp4_station_fails_without_taking_down_the_worker() {
            let server = serve("audio/mp4", FRAGMENTED_MP4);

            let events = run_worker_collecting(server.url("/stream"));

            assert!(matches!(
                events.last(),
                Some(EngineEvent::Failed {
                    error: EngineError::Decode(_),
                    ..
                })
            ));
        }
    }
}
