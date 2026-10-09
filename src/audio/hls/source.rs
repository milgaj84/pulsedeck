//! `Read` adapter over the fetcher thread's output.
//!
//! The decoder reads on the audio thread, so downloading happens on a separate
//! thread and this type only hands over bytes that are already in memory.

use std::io::{self, Read};
use std::sync::atomic::{AtomicU64, Ordering::SeqCst};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How often a blocked read re-checks whether playback was abandoned.
const POLL: Duration = Duration::from_millis(250);

/// What the fetcher thread sends to the source.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum HlsChunk {
    Bytes(Vec<u8>),
    /// The stream ended cleanly (VOD with `#EXT-X-ENDLIST`).
    End,
    /// The stream failed; the message becomes the read error.
    Fail(String),
}

pub(crate) struct HlsSource {
    rx: Receiver<HlsChunk>,
    generation: u64,
    active_generation: Arc<AtomicU64>,
    idle_timeout: Duration,
    pending: Vec<u8>,
    pos: usize,
    ended: bool,
}

impl HlsSource {
    pub(crate) fn new(
        rx: Receiver<HlsChunk>,
        generation: u64,
        active_generation: Arc<AtomicU64>,
        idle_timeout: Duration,
    ) -> Self {
        Self {
            rx,
            generation,
            active_generation,
            idle_timeout,
            pending: Vec::new(),
            pos: 0,
            ended: false,
        }
    }

    fn abandoned(&self) -> bool {
        self.active_generation.load(SeqCst) != self.generation
    }
}

impl Read for HlsSource {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let started = Instant::now();

        loop {
            if self.abandoned() {
                return Err(io::Error::other("Abandoned"));
            }

            if self.pos < self.pending.len() {
                let n = buf.len().min(self.pending.len() - self.pos);
                buf[..n].copy_from_slice(&self.pending[self.pos..self.pos + n]);
                self.pos += n;
                return Ok(n);
            }
            if self.ended {
                return Ok(0);
            }

            let remaining = self.idle_timeout.saturating_sub(started.elapsed());
            if remaining.is_zero() {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "HLS: timed out waiting for the next segment",
                ));
            }

            match self.rx.recv_timeout(remaining.min(POLL)) {
                Ok(HlsChunk::Bytes(bytes)) => {
                    self.pending = bytes;
                    self.pos = 0;
                }
                Ok(HlsChunk::End) | Err(RecvTimeoutError::Disconnected) => {
                    self.ended = true;
                }
                Ok(HlsChunk::Fail(message)) => {
                    self.ended = true;
                    return Err(io::Error::other(message));
                }
                Err(RecvTimeoutError::Timeout) => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn source(idle: Duration) -> (mpsc::Sender<HlsChunk>, HlsSource, Arc<AtomicU64>) {
        let (tx, rx) = mpsc::channel();
        let active = Arc::new(AtomicU64::new(1));
        (tx, HlsSource::new(rx, 1, Arc::clone(&active), idle), active)
    }

    fn read_all(source: &mut HlsSource) -> io::Result<Vec<u8>> {
        let mut out = Vec::new();
        source.read_to_end(&mut out)?;
        Ok(out)
    }

    #[test]
    fn chunks_are_concatenated_until_end() {
        let (tx, mut source, _) = source(Duration::from_secs(5));
        tx.send(HlsChunk::Bytes(vec![1, 2, 3])).unwrap();
        tx.send(HlsChunk::Bytes(vec![4, 5])).unwrap();
        tx.send(HlsChunk::End).unwrap();

        assert_eq!(read_all(&mut source).unwrap(), [1, 2, 3, 4, 5]);
    }

    #[test]
    fn small_buffers_split_a_chunk_without_losing_bytes() {
        let (tx, mut source, _) = source(Duration::from_secs(5));
        tx.send(HlsChunk::Bytes((0..10).collect())).unwrap();
        tx.send(HlsChunk::End).unwrap();
        let mut out = Vec::new();
        let mut buf = [0u8; 3];

        loop {
            match source.read(&mut buf).unwrap() {
                0 => break,
                n => out.extend_from_slice(&buf[..n]),
            }
        }

        assert_eq!(out, (0..10).collect::<Vec<u8>>());
    }

    #[test]
    fn end_stays_ended() {
        let (tx, mut source, _) = source(Duration::from_secs(5));
        tx.send(HlsChunk::End).unwrap();

        assert_eq!(source.read(&mut [0u8; 4]).unwrap(), 0);
        assert_eq!(source.read(&mut [0u8; 4]).unwrap(), 0);
    }

    #[test]
    fn a_dropped_fetcher_ends_the_stream_after_buffered_data() {
        let (tx, mut source, _) = source(Duration::from_secs(5));
        tx.send(HlsChunk::Bytes(vec![9, 9])).unwrap();
        drop(tx);

        assert_eq!(read_all(&mut source).unwrap(), [9, 9]);
    }

    #[test]
    fn failure_is_delivered_after_the_data_before_it() {
        let (tx, mut source, _) = source(Duration::from_secs(5));
        tx.send(HlsChunk::Bytes(vec![1, 2])).unwrap();
        tx.send(HlsChunk::Fail("HLS: boom".to_string())).unwrap();
        let mut buf = [0u8; 8];

        assert_eq!(source.read(&mut buf).unwrap(), 2);
        let err = source.read(&mut buf).unwrap_err();
        assert_eq!(err.to_string(), "HLS: boom");
        assert_eq!(source.read(&mut buf).unwrap(), 0);
    }

    #[test]
    fn a_stale_generation_is_abandoned_even_with_data_waiting() {
        let (tx, mut source, active) = source(Duration::from_secs(5));
        tx.send(HlsChunk::Bytes(vec![1])).unwrap();
        active.store(2, SeqCst);

        let err = source.read(&mut [0u8; 4]).unwrap_err();

        assert_eq!(err.to_string(), "Abandoned");
    }

    #[test]
    fn abandoning_wakes_a_blocked_read_quickly() {
        let (tx, mut source, active) = source(Duration::from_secs(30));
        let flipper = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            active.store(0, SeqCst);
        });
        let started = Instant::now();

        let err = source.read(&mut [0u8; 4]).unwrap_err();
        flipper.join().unwrap();

        assert_eq!(err.to_string(), "Abandoned");
        assert!(started.elapsed() < Duration::from_secs(5));
        drop(tx);
    }

    #[test]
    fn silence_past_the_idle_timeout_is_a_timeout_error() {
        let (_tx, mut source, _) = source(Duration::from_millis(300));

        let err = source.read(&mut [0u8; 4]).unwrap_err();

        assert_eq!(err.kind(), io::ErrorKind::TimedOut);
        assert!(err.to_string().contains("timed out"));
    }

    #[test]
    fn empty_read_buffer_returns_zero_without_blocking() {
        let (_tx, mut source, _) = source(Duration::from_secs(30));

        assert_eq!(source.read(&mut []).unwrap(), 0);
    }
}
