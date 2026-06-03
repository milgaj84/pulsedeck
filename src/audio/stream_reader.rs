use super::buffer::BufferQueue;
use super::buffer_meter::BufferStatusMeter;
use super::metadata::parse_stream_title;
use super::AudioStatus;

use std::io::Read;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc};

/// StreamReader consumes the byte queue and strips ICY metadata boundaries.
pub(super) struct StreamReader {
    url: String,
    queue: Arc<BufferQueue>,
    buffer_meter: Arc<BufferStatusMeter>,
    pos: u64,
    metaint: Option<usize>,
    bytes_until_meta: usize,
    status_tx: mpsc::Sender<AudioStatus>,
    conn_id: u64,
    active_conn_id: Arc<AtomicU64>,
}

pub(super) struct StreamReaderConfig {
    pub(super) url: String,
    pub(super) queue: Arc<BufferQueue>,
    pub(super) buffer_meter: Arc<BufferStatusMeter>,
    pub(super) status_tx: mpsc::Sender<AudioStatus>,
    pub(super) conn_id: u64,
    pub(super) active_conn_id: Arc<AtomicU64>,
    pub(super) metaint: Option<usize>,
}

impl StreamReader {
    pub(super) fn new(config: StreamReaderConfig) -> Self {
        let bytes_until_meta = config.metaint.unwrap_or(0);

        Self {
            url: config.url,
            queue: config.queue,
            buffer_meter: config.buffer_meter,
            pos: 0,
            metaint: config.metaint,
            bytes_until_meta,
            status_tx: config.status_tx,
            conn_id: config.conn_id,
            active_conn_id: config.active_conn_id,
        }
    }

    fn read_metadata_block(&mut self) -> std::io::Result<()> {
        let mut length_byte = [0u8; 1];
        let bytes_read = self.queue.pop(&mut length_byte)?;
        self.record_buffer_consumption(bytes_read);
        let length = length_byte[0] as usize * 16;

        if length > 0 {
            let mut meta_buf = vec![0u8; length];
            let bytes_read = self.queue.pop(&mut meta_buf)?;
            self.record_buffer_consumption(bytes_read);
            if let Ok(meta_str) = String::from_utf8(meta_buf) {
                if let Some(title) = parse_stream_title(&meta_str) {
                    let _ = self.status_tx.send(AudioStatus::TrackChanged {
                        url: self.url.clone(),
                        title,
                    });
                }
            }
        }

        Ok(())
    }

    fn record_buffer_consumption(&self, bytes_read: usize) {
        self.buffer_meter.record_consumed(
            bytes_read,
            self.queue.len(),
            self.queue.capacity,
            &self.status_tx,
        );
    }
}

impl std::io::Read for StreamReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.active_conn_id.load(Ordering::SeqCst) != self.conn_id {
            return Err(std::io::Error::other("Abandoned"));
        }

        let Some(metaint) = self.metaint else {
            let n = self.queue.pop(buf)?;
            self.pos += n as u64;
            self.record_buffer_consumption(n);
            return Ok(n);
        };

        if self.bytes_until_meta == 0 {
            self.read_metadata_block()?;
            self.bytes_until_meta = metaint;
        }

        let max_to_read = buf.len().min(self.bytes_until_meta);
        let n = self.queue.pop(&mut buf[..max_to_read])?;
        self.pos += n as u64;
        self.bytes_until_meta -= n;
        self.record_buffer_consumption(n);

        Ok(n)
    }
}

impl std::io::Seek for StreamReader {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        match pos {
            std::io::SeekFrom::Current(0) | std::io::SeekFrom::Start(_) => Ok(self.pos),
            std::io::SeekFrom::Current(offset) if offset > 0 => {
                let mut remaining = offset as u64;
                let mut discard = [0u8; 8192];
                while remaining > 0 {
                    let to_read = remaining.min(discard.len() as u64) as usize;
                    let n = self.read(&mut discard[..to_read])?;
                    if n == 0 {
                        break;
                    }
                    remaining -= n as u64;
                }
                Ok(self.pos)
            }
            _ => Ok(self.pos),
        }
    }
}
