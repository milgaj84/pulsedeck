use super::codec::{detect_codec, CodecDetection};
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
        let wrapped = ReadWrapper::new(reader);
        let buf_reader = BufReader::new(wrapped);

        if detection.verified_mp3() {
            return match Decoder::new_mp3(buf_reader) {
                Ok(decoder) => {
                    let format = StreamFormat {
                        codec: detection.hint.label().to_string(),
                        sample_rate: decoder.sample_rate(),
                        channels: decoder.channels(),
                    };
                    let visualizer =
                        VisualizerSource::new(decoder.convert_samples::<f32>(), sample_buffer);
                    Ok((Box::new(visualizer), format))
                }
                Err(error) => Err(EngineError::Decode(format!(
                    "verified MP3 stream could not be decoded: {error}"
                ))),
            };
        }

        match Decoder::new(buf_reader) {
            Ok(decoder) => {
                let format = StreamFormat {
                    codec: detection.hint.label().to_string(),
                    sample_rate: decoder.sample_rate(),
                    channels: decoder.channels(),
                };
                let visualizer =
                    VisualizerSource::new(decoder.convert_samples::<f32>(), sample_buffer);
                Ok((Box::new(visualizer), format))
            }
            Err(error) => Err(EngineError::Decode(format!(
                "{} probe failed: {error}",
                detection.hint.label()
            ))),
        }
    }
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
enum PrebufferFailure {
    Abandoned,
    Timeout,
    Read(io::Error),
}

fn fill_prebuffer<R: Read>(
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

fn send_abandoned(event_tx: &mpsc::Sender<EngineEvent>, generation: Generation) {
    let _ = event_tx.send(EngineEvent::StreamEnded {
        generation,
        reason: EndReason::Abandoned,
    });
}

fn send_failure(event_tx: &mpsc::Sender<EngineEvent>, generation: Generation, error: EngineError) {
    let _ = event_tx.send(EngineEvent::Failed { generation, error });
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
}
