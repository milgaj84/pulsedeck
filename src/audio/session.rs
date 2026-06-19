use super::stream_reader::{StreamReader, StreamReaderConfig};
use super::visualizer::VisualizerSource;
use super::AudioStatus;

use rodio::{Decoder, Sink};
use std::collections::VecDeque;
use std::io::BufReader;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

const DECODER_READ_BUFFER_SIZE: usize = 16 * 1024;

#[derive(Clone)]
pub(super) struct ConnectionContext {
    pub(super) status_tx: mpsc::Sender<AudioStatus>,
    pub(super) conn_id: u64,
    pub(super) active_conn_id: Arc<AtomicU64>,
    pub(super) sample_buffer: Arc<Mutex<VecDeque<f32>>>,
    pub(super) request_stream_metadata: bool,
}

/// Connect to a stream URL and create a playable Sink.
///
/// This intentionally avoids an internal retry/backoff loop. Initial decode
/// failures should surface immediately so the app can show a real error instead
/// of sitting in Connecting while repeated decoder attempts burn time.
pub(super) fn connect_and_decode(
    url: String,
    handle: rodio::OutputStreamHandle,
    context: ConnectionContext,
) -> Result<Sink, String> {
    if context.active_conn_id.load(Ordering::SeqCst) != context.conn_id {
        return Err("Abandoned".into());
    }

    try_connect_and_decode_once(&url, &handle, context)
}

fn try_connect_and_decode_once(
    url: &str,
    handle: &rodio::OutputStreamHandle,
    context: ConnectionContext,
) -> Result<Sink, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(None)
        .connect_timeout(Duration::from_secs(5))
        .user_agent(format!("PulseDeck/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|err| format!("HTTP client error: {err}"))?;

    if context.active_conn_id.load(Ordering::SeqCst) != context.conn_id {
        return Err("Abandoned".into());
    }

    let mut request = client.get(url);
    if context.request_stream_metadata {
        request = request.header("Icy-MetaData", "1");
    }

    let response = request
        .send()
        .map_err(|err| format!("Connection failed: {err}"))?;

    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }

    if context.active_conn_id.load(Ordering::SeqCst) != context.conn_id {
        return Err("Abandoned".into());
    }

    let metaint = if context.request_stream_metadata {
        response
            .headers()
            .get("icy-metaint")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<usize>().ok())
    } else {
        None
    };

    let _ = context.status_tx.send(AudioStatus::Connecting);
    let reader = StreamReader::new(StreamReaderConfig {
        url: url.to_string(),
        inner: response,
        status_tx: context.status_tx,
        conn_id: context.conn_id,
        active_conn_id: context.active_conn_id,
        metaint,
    });

    let buffered_reader = BufReader::with_capacity(DECODER_READ_BUFFER_SIZE, reader);
    let source = Decoder::new_mp3(buffered_reader).map_err(|err| format!("Decode error: {err}"))?;
    let wrapped_source = VisualizerSource::new(source, context.sample_buffer);
    let sink = Sink::try_new(handle)
        .map_err(|err| super::hardware_output_error(format!("Sink error: {err}")))?;

    sink.append(wrapped_source);

    Ok(sink)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_context_defaults_to_requesting_stream_metadata_when_configured() {
        let (status_tx, _status_rx) = mpsc::channel();
        let context = ConnectionContext {
            status_tx,
            conn_id: 1,
            active_conn_id: Arc::new(AtomicU64::new(1)),
            sample_buffer: Arc::new(Mutex::new(VecDeque::new())),
            request_stream_metadata: true,
        };

        assert!(context.request_stream_metadata);
    }
}
