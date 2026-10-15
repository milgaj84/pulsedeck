use super::ac3::Ac3Backend;
use super::adts_stream::AdtsStream;
use super::codec::{detect_codec, CodecDetection, CodecHint};
use super::hls;
use super::loudness::{LevelControl, Leveller};
use super::panic_guard;
use super::pcm_source::{FailureSink, FrameBackend, PcmSource};
use super::stream_source::StreamSource;
use super::symphonia_backend::SymphoniaBackend;
use super::types::{
    ConnectRequest, DecodedSource, EndReason, EngineError, EngineEvent, Generation, StreamFormat,
};
use super::visualizer::VisualizerSource;

use rodio::Source;
use std::collections::VecDeque;
use std::io::{self, BufReader, Cursor, Read};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::SeqCst};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const STREAM_READ_TIMEOUT: Duration = Duration::from_secs(8);
const MAX_ICY_METAINT: usize = 16 * 1024 * 1024;

/// How much of a stream may be read while its format is being identified,
/// before it is given up as unrecognisable. Symphonia's own limit is 1 MiB,
/// which on a live stream of 16-32 KB/s is a minute or more of "Connecting".
const PROBE_MAX_BYTES: u64 = 512 * 1024;
const PROBE_MAX_TIME: Duration = Duration::from_secs(20);

/// Builds a decoded, visualizer-tapped rodio source from a live byte stream.
///
/// Generic Symphonia probing is the safe default because public radio headers
/// and URL extensions are often inaccurate. The MP3 hint is used only when MP3
/// frame or ID3 magic bytes were observed in the prebuffer.
pub(super) struct DecodePipeline;

impl DecodePipeline {
    pub(super) fn build<R: Read + Send + 'static>(
        reader: R,
        sample_buffer: Arc<Mutex<VecDeque<f32>>>,
        level: Option<Arc<LevelControl>>,
        detection: CodecDetection,
        failures: Option<FailureSink>,
    ) -> Result<(DecodedSource, StreamFormat), EngineError> {
        let verified_mp3 = detection.verified_mp3();
        let extension = verified_mp3.then_some("mp3");
        // Symphonia cannot read MPEG-2 ADTS or CRC-protected AAC as it is.
        let reader: Box<dyn Read + Send> = if detection.adts_rewrite {
            Box::new(AdtsStream::new(reader))
        } else {
            Box::new(reader)
        };

        // Third-party decoders can panic on input they cannot handle, so the
        // probe runs inside a guard that turns a panic into an error.
        // The limit on identifying the stream ends once it has been (or has failed to be).
        let probing = Arc::new(AtomicBool::new(true));
        let limit = ProbeLimit::new(Arc::clone(&probing), PROBE_MAX_BYTES, PROBE_MAX_TIME);
        let opened = panic_guard::run_quiet(|| match detection.hint {
            // Symphonia has no AC-3 decoder; these go to their own backend.
            CodecHint::Ac3 | CodecHint::Eac3 => {
                open_pcm(Ok(Ac3Backend::new(reader)), level, failures)
            }
            _ => open_pcm(
                SymphoniaBackend::open(ReadWrapper::new(reader).with_probe_limit(limit), extension),
                level,
                failures,
            ),
        });
        probing.store(false, SeqCst);
        let (pcm, sample_rate, channels) = match opened {
            Ok(Ok(opened)) => opened,
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
            sample_rate,
            channels,
        };
        let visualizer = VisualizerSource::new(pcm, sample_buffer);
        Ok((Box::new(visualizer), format))
    }
}

/// A decoded stream ready to play, with the format of its first frame.
type Opened = (Box<dyn Source<Item = f32> + Send>, u32, u16);

/// Decode the first frame of a backend so the stream's format is known.
fn open_pcm<B: FrameBackend + 'static>(
    backend: Result<B, String>,
    level: Option<Arc<LevelControl>>,
    failures: Option<FailureSink>,
) -> Result<Opened, String> {
    let mut source = PcmSource::new(backend?)?;
    if let Some(sink) = failures {
        source = source.with_failure_sink(sink);
    }
    if let Some(level) = level {
        source = source.with_processor(Box::new(Leveller::new(level)));
    }
    let (rate, channels) = (source.sample_rate(), source.channels());
    Ok((Box::new(source), rate, channels))
}

/// The error for a decoder that panicked while probing a stream.
pub(super) fn decoder_crashed() -> EngineError {
    EngineError::Decode(
        "this stream's container crashed the decoder and is not supported".to_string(),
    )
}

/// Adapts a live reader to Symphonia's `MediaSource` (`Read + Seek + Send + Sync`).
struct ReadWrapper<R: Read> {
    inner: R,
    pos: u64,
    probe: Option<ProbeLimit>,
}

/// Cuts off the reading of a stream whose format cannot be identified, while
/// `armed` is set.
struct ProbeLimit {
    armed: Arc<AtomicBool>,
    max_bytes: u64,
    max_time: Duration,
    started: Instant,
}

impl ProbeLimit {
    fn new(armed: Arc<AtomicBool>, max_bytes: u64, max_time: Duration) -> Self {
        Self {
            armed,
            max_bytes,
            max_time,
            started: Instant::now(),
        }
    }

    /// Why reading must stop now, if it must.
    fn exceeded(&self, read: u64) -> Option<String> {
        if !self.armed.load(SeqCst) {
            return None;
        }
        if read >= self.max_bytes {
            return Some(format!(
                "no recognisable audio found in the first {} KiB of the stream",
                self.max_bytes / 1024
            ));
        }
        if self.started.elapsed() >= self.max_time {
            return Some(format!(
                "no recognisable audio found in the first {} seconds of the stream",
                self.max_time.as_secs()
            ));
        }
        None
    }
}

impl<R: Read> ReadWrapper<R> {
    fn new(inner: R) -> Self {
        Self {
            inner,
            pos: 0,
            probe: None,
        }
    }

    fn with_probe_limit(mut self, limit: ProbeLimit) -> Self {
        self.probe = Some(limit);
        self
    }
}

impl<R: Read> Read for ReadWrapper<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if let Some(reason) = self
            .probe
            .as_ref()
            .and_then(|limit| limit.exceeded(self.pos))
        {
            return Err(io::Error::new(io::ErrorKind::InvalidData, reason));
        }
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

impl<R: Read + Send> symphonia::core::io::MediaSource for ReadWrapper<R> {
    /// A live stream cannot seek. Saying so lets demuxers read it in order
    /// (a fragmented MP4 needs this) instead of trying to seek and failing.
    fn is_seekable(&self) -> bool {
        false
    }

    fn byte_len(&self) -> Option<u64> {
        None
    }
}

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

    let failures: FailureSink = {
        let event_tx = event_tx.clone();
        Box::new(move |error| send_failure(&event_tx, generation, error))
    };
    let level = Some(Arc::clone(&request.options.level));
    match DecodePipeline::build(chained, sample_buffer, level, detection, Some(failures)) {
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
        CodecDetection {
            hint,
            source,
            adts_rewrite: false,
        }
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
            None,
            detection(CodecHint::Mp3, CodecSource::MagicBytes),
            None,
        );

        assert!(matches!(result, Ok(_) | Err(EngineError::Decode(_))));
    }

    #[test]
    fn unverified_mp3_hint_uses_safe_probe_path() {
        let sample_buffer = Arc::new(Mutex::new(VecDeque::new()));
        let result = DecodePipeline::build(
            Cursor::new(vec![0_u8; 32]),
            sample_buffer,
            None,
            detection(CodecHint::Mp3, CodecSource::ContentType),
            None,
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
                None,
                detection(CodecHint::Unknown, CodecSource::Unknown),
                None,
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
            let (events, active, _rx) =
                run_worker_alive(url, false, Arc::new(LevelControl::default()));
            active.store(0, SeqCst);
            events
        }

        /// Like `run_worker_against`, but leaves the stream active so the
        /// caller can keep reading it. The caller must set the flag to 0.
        fn run_worker_alive(
            url: String,
            metadata_enabled: bool,
            level: Arc<LevelControl>,
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
            req.options.level = level;

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
        /// Also set `PULSEDECK_SECONDS=<n>` (default 15) to listen longer and
        /// `PULSEDECK_LOUDNESS=off` to switch loudness normalisation off; the
        /// output prints the loudness of the last 3 s and the gain applied.
        #[test]
        #[ignore = "needs network and PULSEDECK_HLS_URL"]
        fn manual_real_hls_stream_connects() {
            let Ok(url) = std::env::var("PULSEDECK_HLS_URL") else {
                return;
            };
            let level = Arc::new(LevelControl::new(
                std::env::var("PULSEDECK_LOUDNESS").as_deref() != Ok("off"),
                -16.0,
            ));
            let seconds: usize = std::env::var("PULSEDECK_SECONDS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(15);
            let (events, active, event_rx) = run_worker_alive(url, true, Arc::clone(&level));
            let mut titles: Vec<String> = events
                .iter()
                .filter_map(|event| match event {
                    EngineEvent::TrackChanged { title, .. } => Some(title.clone()),
                    _ => None,
                })
                .collect();
            let terminal = events
                .into_iter()
                .rfind(|event| !matches!(event, EngineEvent::Buffering { .. }));
            let Some(EngineEvent::Connected { source, format, .. }) = terminal else {
                if let Some(EngineEvent::Failed { error, .. }) = &terminal {
                    println!("FAILED {}", error.to_status_string());
                }
                panic!("did not connect");
            };
            println!(
                "CONNECTED codec={} rate={} channels={}",
                format.codec, format.sample_rate, format.channels
            );

            // 15 seconds of audio crosses several segment boundaries.
            let wanted = format.sample_rate as usize * usize::from(format.channels) * seconds;
            let samples: Vec<f32> = source.take(wanted).collect();
            active.store(0, SeqCst);
            titles.extend(event_rx.try_iter().filter_map(|event| match event {
                EngineEvent::TrackChanged { title, .. } => Some(title),
                _ => None,
            }));
            println!("TITLES {titles:?}");
            if let Ok(mut meter) = ebur128::EbuR128::new(
                u32::from(format.channels),
                format.sample_rate,
                ebur128::Mode::S,
            ) {
                if format.channels == 1 {
                    let _ = meter.set_channel_map(&[ebur128::Channel::DualMono]);
                }
                let tail = format.sample_rate as usize * usize::from(format.channels) * 3;
                let _ = meter.add_frames_f32(&samples[samples.len().saturating_sub(tail)..]);
                println!(
                    "LOUDNESS {:.1} LUFS (last 3 s), gain {:+.1} dB",
                    meter.loudness_shortterm().unwrap_or(f64::NAN),
                    level.gain_db()
                );
            }
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
        const OPUS_SIX_CHANNELS: &[u8] = include_bytes!("testdata/tone_6ch.opus");
        const AC3: &[u8] = include_bytes!("testdata/dolby/tone.ac3");
        const EAC3: &[u8] = include_bytes!("testdata/dolby/tone.eac3");
        const AC3_SURROUND: &[u8] = include_bytes!("testdata/dolby/tone51.ac3");
        const FLAC: &[u8] = include_bytes!("testdata/tone.flac");
        const FRAGMENTED_MP4: &[u8] = include_bytes!("testdata/tone_frag.mp4");

        fn build(bytes: &[u8]) -> Result<(DecodedSource, StreamFormat), EngineError> {
            DecodePipeline::build(
                Cursor::new(bytes.to_vec()),
                Arc::new(Mutex::new(VecDeque::new())),
                None,
                detect_codec(bytes, "", ""),
                None,
            )
        }

        /// A mono 16-bit WAV of a loud 997 Hz sine.
        fn loud_wav(secs: usize) -> Vec<u8> {
            let rate = 16_000_u32;
            let count = rate as usize * secs;
            let mut wav = Vec::with_capacity(44 + count * 2);
            wav.extend_from_slice(b"RIFF");
            wav.extend_from_slice(&(36 + count as u32 * 2).to_le_bytes());
            wav.extend_from_slice(b"WAVEfmt ");
            wav.extend_from_slice(&16_u32.to_le_bytes());
            wav.extend_from_slice(&1_u16.to_le_bytes());
            wav.extend_from_slice(&1_u16.to_le_bytes());
            wav.extend_from_slice(&rate.to_le_bytes());
            wav.extend_from_slice(&(rate * 2).to_le_bytes());
            wav.extend_from_slice(&2_u16.to_le_bytes());
            wav.extend_from_slice(&16_u16.to_le_bytes());
            wav.extend_from_slice(b"data");
            wav.extend_from_slice(&(count as u32 * 2).to_le_bytes());
            for index in 0..count {
                let phase = 2.0 * std::f32::consts::PI * 997.0 * index as f32 / rate as f32;
                wav.extend_from_slice(&((phase.sin() * 20_000.0) as i16).to_le_bytes());
            }
            wav
        }

        fn rms_of_last_second(level: Option<Arc<LevelControl>>) -> f32 {
            let bytes = loud_wav(8);
            let (source, _) = DecodePipeline::build(
                Cursor::new(bytes.clone()),
                Arc::new(Mutex::new(VecDeque::new())),
                level,
                detect_codec(&bytes, "", ""),
                None,
            )
            .unwrap_or_else(|e| panic!("{}", e.to_status_string()));
            let samples: Vec<f32> = source.collect();
            let tail = &samples[samples.len() - 16_000..];
            (tail.iter().map(|s| s * s).sum::<f32>() / tail.len() as f32).sqrt()
        }

        #[test]
        fn loudness_normalisation_is_applied_to_the_decoded_stream() {
            let plain = rms_of_last_second(None);
            let leveled = rms_of_last_second(Some(Arc::new(LevelControl::new(true, -23.0))));
            let off = rms_of_last_second(Some(Arc::new(LevelControl::new(false, -23.0))));

            assert!(leveled < plain * 0.5, "{leveled} vs {plain}");
            assert_eq!(off, plain, "switched off it must not touch the audio");
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
        fn opus_decodes_real_audio() {
            let (format, samples) = first_samples(OPUS, 4000);

            assert_eq!(format.codec, "Opus");
            assert_eq!(format.sample_rate, 48_000);
            assert_eq!(samples.len(), 4000);
            assert!(samples.iter().fold(0.0_f32, |m, s| m.max(s.abs())) > 0.01);
        }

        /// A stream of zeros that counts how much of it was read (stopping at
        /// 8 MiB, so that a missing limit shows up as a failed test, not a hang).
        struct Endless(Arc<std::sync::atomic::AtomicU64>);
        impl Read for Endless {
            fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                if self.0.load(SeqCst) >= 8 * 1024 * 1024 {
                    return Ok(0);
                }
                buf.fill(0);
                self.0.fetch_add(buf.len() as u64, SeqCst);
                Ok(buf.len())
            }
        }

        #[test]
        fn an_unrecognisable_stream_is_given_up_quickly_not_after_a_megabyte() {
            let read = Arc::new(std::sync::atomic::AtomicU64::new(0));
            let result = DecodePipeline::build(
                Endless(Arc::clone(&read)),
                Arc::new(Mutex::new(VecDeque::new())),
                None,
                detect_codec(&[0u8; 64], "audio/aac", ""),
                None,
            );

            let Err(EngineError::Decode(message)) = result else {
                panic!("an unrecognisable stream must fail");
            };
            assert!(message.contains("no recognisable audio"), "{message}");
            // Symphonia alone would read 1 MiB or more before giving up.
            let read = read.load(SeqCst);
            assert!(read < 800 * 1024, "read {read} bytes");
        }

        #[test]
        fn a_large_id3_tag_before_the_audio_is_still_fine() {
            // 300 KB of tag (cover art), well inside the limit.
            let mp3 = include_bytes!("hls/testdata/tone.mp3");
            let mut stream = b"ID3\x04\x00\x00".to_vec();
            let size = 300_000usize;
            stream.extend_from_slice(&[
                ((size >> 21) & 0x7F) as u8,
                ((size >> 14) & 0x7F) as u8,
                ((size >> 7) & 0x7F) as u8,
                (size & 0x7F) as u8,
            ]);
            stream.extend(std::iter::repeat_n(0u8, size));
            stream.extend_from_slice(mp3);

            let (format, samples) = first_samples(&stream, 4000);

            assert_eq!(format.codec, "MP3");
            assert_eq!(samples.len(), 4000);
        }

        #[test]
        fn the_probe_limit_stops_a_slow_stream_by_time_and_lifts_once_disarmed() {
            struct Slow;
            impl Read for Slow {
                fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                    std::thread::sleep(Duration::from_millis(20));
                    let n = buf.len().min(100);
                    buf[..n].fill(0);
                    Ok(n)
                }
            }
            let armed = Arc::new(AtomicBool::new(true));
            let limit = ProbeLimit::new(Arc::clone(&armed), u64::MAX, Duration::from_millis(150));
            let mut reader = ReadWrapper::new(Slow).with_probe_limit(limit);
            let mut buf = [0u8; 100];
            let started = Instant::now();

            let error = loop {
                if let Err(error) = reader.read(&mut buf) {
                    break error;
                }
                assert!(started.elapsed() < Duration::from_secs(5), "never stopped");
            };

            assert!(
                error.to_string().contains("seconds of the stream"),
                "{error}"
            );
            // Once the stream is identified, reading is unlimited again.
            armed.store(false, SeqCst);
            assert!(reader.read(&mut buf).is_ok());
        }

        #[test]
        fn ac3_and_eac3_decode_real_audio() {
            for (bytes, label) in [(AC3, "AC-3"), (EAC3, "E-AC-3"), (AC3_SURROUND, "AC-3")] {
                let (format, samples) = first_samples(bytes, 4000);

                assert_eq!(format.codec, label);
                assert_eq!((format.sample_rate, format.channels), (48_000, 2));
                assert_eq!(samples.len(), 4000);
                assert!(samples.iter().fold(0.0_f32, |m, s| m.max(s.abs())) > 0.01);
            }
        }

        #[test]
        fn mpeg2_and_crc_protected_adts_aac_decode_like_the_plain_form() {
            use crate::audio::adts_stream::fixtures::{frames, mpeg2, with_crc, AAC};
            let (plain_format, plain) = first_samples(AAC, 4000);
            let frames = frames();

            for stream in [mpeg2(&frames), with_crc(&frames)] {
                let (format, samples) = first_samples(&stream, 4000);

                assert_eq!(format.codec, "AAC");
                assert_eq!(format.sample_rate, plain_format.sample_rate);
                // The same audio, sample for sample, once the headers are rewritten.
                assert_eq!(samples, plain);
            }
        }

        #[test]
        fn six_channel_opus_fails_with_a_clear_not_supported_error() {
            let Err(EngineError::Decode(message)) = build(OPUS_SIX_CHANNELS) else {
                panic!("6-channel Opus must be rejected as a decode error");
            };

            assert!(message.contains("Opus audio with 6 channels is not supported"));
        }

        #[test]
        fn a_fragmented_mp4_stream_decodes_without_seeking() {
            // rodio reported this reader as seekable and asserted "unreachable".
            // The stream is now read in order, like a live one.
            let (_format, samples) = first_samples(FRAGMENTED_MP4, 4000);

            assert_eq!(samples.len(), 4000);
            assert!(samples.iter().fold(0.0_f32, |m, s| m.max(s.abs())) > 0.01);
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
        fn an_opus_station_connects_through_the_worker() {
            let server = serve("audio/ogg", OPUS);

            let events = run_worker_collecting(server.url("/stream"));

            assert!(
                events
                    .iter()
                    .any(|event| matches!(event, EngineEvent::Connected { .. })),
                "expected a connection"
            );
        }

        #[test]
        fn an_mpeg2_adts_aac_station_connects_through_the_worker() {
            use crate::audio::adts_stream::fixtures::{frames, mpeg2, with_crc};
            let frames = frames();
            let mpeg2 = mpeg2(&frames);
            // Whatever the content type, and joined in the middle of a frame.
            let cases = [
                (mpeg2.clone(), "audio/aac"),
                (mpeg2.clone(), "audio/aacp"),
                (mpeg2[23..].to_vec(), "audio/aac"),
                (with_crc(&frames), "audio/aac"),
            ];
            for (bytes, content_type) in cases {
                // `serve` wants the body for as long as the server runs.
                let server = serve(content_type, Box::leak(bytes.into_boxed_slice()));

                let events = run_worker_collecting(server.url("/stream"));

                match events.iter().find_map(|event| match event {
                    EngineEvent::Connected { format, .. } => Some(format.clone()),
                    _ => None,
                }) {
                    Some(format) => assert_eq!(format.codec, "AAC"),
                    None => panic!("no connection for {content_type}"),
                }
            }
        }

        #[test]
        fn a_stream_that_breaks_after_playing_reports_why() {
            // Real AC-3 frames, then more junk than the decoder will look through.
            let mut stream = AC3.to_vec();
            stream.extend(std::iter::repeat_n(0x55, 400 * 1024));
            let (tx, rx) = mpsc::channel();
            let sink: FailureSink = Box::new(move |error| tx.send(error).unwrap());
            let (source, _) = DecodePipeline::build(
                Cursor::new(stream),
                Arc::new(Mutex::new(VecDeque::new())),
                None,
                detect_codec(AC3, "", ""),
                Some(sink),
            )
            .unwrap_or_else(|e| panic!("{}", e.to_status_string()));

            let samples = source.count();

            assert!(samples > 60_000, "the good part plays: {samples}");
            let error = rx.try_recv().expect("a failure report");
            assert!(
                error.to_status_string().contains("no AC-3 frames"),
                "{}",
                error.to_status_string()
            );
            assert!(rx.try_recv().is_err());
        }

        #[test]
        fn an_ac3_station_connects_through_the_worker_whatever_its_content_type() {
            // Dolby streams are recognised by their frames, so a wrong type is fine.
            for content_type in ["audio/ac3", "audio/mpeg", "application/octet-stream"] {
                let server = serve(content_type, AC3);

                let events = run_worker_collecting(server.url("/stream"));

                match events.iter().find_map(|event| match event {
                    EngineEvent::Connected { format, .. } => Some(format.clone()),
                    _ => None,
                }) {
                    Some(format) => assert_eq!(format.codec, "AC-3", "{content_type}"),
                    None => panic!("no connection for {content_type}"),
                }
            }
        }

        #[test]
        fn a_fragmented_mp4_station_connects_through_the_worker() {
            let server = serve("audio/mp4", FRAGMENTED_MP4);

            let events = run_worker_collecting(server.url("/stream"));

            assert!(
                events
                    .iter()
                    .any(|event| matches!(event, EngineEvent::Connected { .. })),
                "expected a connection"
            );
        }
    }
}
