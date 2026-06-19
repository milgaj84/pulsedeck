use super::buffer::BufferQueue;
use super::buffer_meter::BufferStatusMeter;
use super::metadata::parse_stream_title;
use super::AudioStatus;

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
        self.read_exact_from_queue(&mut length_byte)?;
        let length = length_byte[0] as usize * 16;

        if length > 0 {
            let mut meta_buf = vec![0u8; length];
            self.read_exact_from_queue(&mut meta_buf)?;
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

    fn read_exact_from_queue(&mut self, buf: &mut [u8]) -> std::io::Result<()> {
        let mut filled = 0;
        while filled < buf.len() {
            let bytes_read = self.queue.pop(&mut buf[filled..])?;
            if bytes_read == 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "stream ended inside ICY metadata block",
                ));
            }
            filled += bytes_read;
            self.note_buffer_consumption(bytes_read);
        }
        Ok(())
    }

    fn note_buffer_consumption(&self, bytes_read: usize) {
        self.buffer_meter.note_consumed(
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
            self.note_buffer_consumption(n);
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
        self.note_buffer_consumption(n);

        Ok(n)
    }
}

impl std::io::Seek for StreamReader {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        match pos {
            std::io::SeekFrom::Current(0) => Ok(self.pos),
            std::io::SeekFrom::Start(0) if self.pos == 0 => Ok(0),
            _ => Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "live radio streams cannot seek",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Seek};
    use std::time::Duration;

    fn test_reader(queue: Arc<BufferQueue>, metaint: Option<usize>) -> StreamReader {
        let (status_tx, _status_rx) = mpsc::channel();
        StreamReader::new(StreamReaderConfig {
            url: "http://stream".to_string(),
            queue,
            buffer_meter: Arc::new(BufferStatusMeter::new(16_000)),
            status_tx,
            conn_id: 1,
            active_conn_id: Arc::new(AtomicU64::new(1)),
            metaint,
        })
    }

    #[test]
    fn icy_metadata_block_is_read_exactly_before_more_audio_is_returned() {
        let queue = Arc::new(BufferQueue::new(1024));
        let mut reader = test_reader(queue.clone(), Some(1));
        queue.push(b"A\x01Stre");

        let producer_queue = queue.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(10));
            producer_queue.push(b"amTitle='X';B");
        });

        let mut byte = [0u8; 1];
        assert_eq!(reader.read(&mut byte).unwrap(), 1);
        assert_eq!(byte[0], b'A');

        assert_eq!(reader.read(&mut byte).unwrap(), 1);
        assert_eq!(byte[0], b'B');
    }

    #[test]
    fn icy_metadata_eof_returns_unexpected_eof_instead_of_audio_bytes() {
        let queue = Arc::new(BufferQueue::new(1024));
        let mut reader = test_reader(queue.clone(), Some(1));
        queue.push(b"A\x01partial");

        let mut byte = [0u8; 1];
        assert_eq!(reader.read(&mut byte).unwrap(), 1);
        assert_eq!(byte[0], b'A');
        queue.set_disconnected(true);

        let err = reader.read(&mut byte).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::UnexpectedEof);
    }

    #[test]
    fn seek_reports_position_but_refuses_to_discard_live_audio() {
        let queue = Arc::new(BufferQueue::new(1024));
        let mut reader = test_reader(queue.clone(), None);
        queue.push(b"abcdef");

        let mut buf = [0u8; 2];
        assert_eq!(reader.read(&mut buf).unwrap(), 2);
        assert_eq!(reader.stream_position().unwrap(), 2);

        let err = reader.seek(std::io::SeekFrom::Current(4)).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::Unsupported);

        let mut next = [0u8; 1];
        assert_eq!(reader.read(&mut next).unwrap(), 1);
        assert_eq!(next[0], b'c');
    }
}
