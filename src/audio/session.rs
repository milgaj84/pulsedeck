use super::buffer::BufferQueue;
use super::buffer_meter::BufferStatusMeter;
use super::stream_reader::{StreamReader, StreamReaderConfig};
use super::visualizer::VisualizerSource;
use super::{AudioStatus, RecordStateShared};

use rodio::{Decoder, Sink};
use std::collections::VecDeque;
use std::io::Read;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

const STREAM_BUFFER_CAPACITY: usize = 4 * 1024 * 1024;
const MIN_PREBUFFER_SECONDS: usize = 2;
const MIN_PREBUFFER_BYTES: usize = 128 * 1024;
const MAX_PREBUFFER_WAIT: Duration = Duration::from_secs(4);

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

fn prebuffer_stream(queue: &BufferQueue, bytes_per_sec: usize) {
    let target = bytes_per_sec
        .saturating_mul(MIN_PREBUFFER_SECONDS)
        .max(MIN_PREBUFFER_BYTES)
        .min(queue.capacity / 2);

    let _ = queue.wait_until_at_least(target, MAX_PREBUFFER_WAIT);
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

    let buffer_capacity = STREAM_BUFFER_CAPACITY;
    let queue = Arc::new(BufferQueue::new(buffer_capacity));
    let buffer_meter = Arc::new(BufferStatusMeter::new(bytes_per_sec));

    let queue_clone = queue.clone();
    let buffer_meter_clone = buffer_meter.clone();
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
                    buffer_meter_clone.report_fill_level(len, cap, &status_tx_clone);
                }
                Err(_) => {
                    queue_clone.set_disconnected(true);
                    break;
                }
            }
        }
    });

    prebuffer_stream(&queue, bytes_per_sec);

    let reader = StreamReader::new(StreamReaderConfig {
        url: url.to_string(),
        queue,
        buffer_meter,
        status_tx: context.status_tx,
        conn_id: context.conn_id,
        active_conn_id: context.active_conn_id,
        record_state: context.record_state,
        metaint,
    });

    let source = Decoder::new(reader).map_err(|err| format!("Decode error: {err}"))?;
    let wrapped_source = VisualizerSource::new(source, context.sample_buffer);
    let sink = Sink::try_new(handle)
        .map_err(|err| super::hardware_output_error(format!("Sink error: {err}")))?;

    sink.append(wrapped_source);

    Ok(sink)
}

#[cfg(test)]
mod prebuffer_tests {
    use super::*;

    #[test]
    fn prebuffer_target_is_capped_by_half_capacity() {
        let queue = BufferQueue::new(256 * 1024);
        prebuffer_stream(&queue, 512 * 1024);
        assert_eq!(queue.len(), 0);
    }
}
