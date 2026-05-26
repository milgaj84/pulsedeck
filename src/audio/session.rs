use super::buffer::BufferQueue;
use super::stream_reader::StreamReader;
use super::visualizer::VisualizerSource;
use super::{AudioStatus, RecordStateShared};

use rodio::{Decoder, Sink};
use std::collections::VecDeque;
use std::io::Read;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

fn buffer_level_status(len: usize, capacity: usize, bytes_per_sec: usize) -> (u8, u32) {
    let percent = len.saturating_mul(100).checked_div(capacity).unwrap_or(0) as u8;
    let seconds = len.checked_div(bytes_per_sec).unwrap_or(0) as u32;

    (percent, seconds)
}

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
                    let (percent, seconds) = buffer_level_status(len, cap, bytes_per_sec);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffer_level_status_reports_percent_and_seconds() {
        let (percent, seconds) = buffer_level_status(160_000, 1_000_000, 16_000);

        assert_eq!(percent, 16);
        assert_eq!(seconds, 10);
    }

    #[test]
    fn buffer_level_status_handles_zero_capacity_and_rate() {
        let (percent, seconds) = buffer_level_status(160_000, 0, 0);

        assert_eq!(percent, 0);
        assert_eq!(seconds, 0);
    }
}
