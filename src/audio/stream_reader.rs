use super::metadata::parse_stream_title;
use super::AudioStatus;

use std::io::{Read, Seek, SeekFrom};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc};

/// StreamReader consumes a live HTTP response and strips ICY metadata boundaries.
///
/// It intentionally does not emulate file seeking. Live radio is an append-only
/// byte stream; pretending otherwise can make decoders skip real audio bytes.
pub(super) struct StreamReader<R> {
    url: String,
    inner: R,
    pos: u64,
    metaint: Option<usize>,
    bytes_until_meta: usize,
    status_tx: mpsc::Sender<AudioStatus>,
    conn_id: u64,
    active_conn_id: Arc<AtomicU64>,
}

pub(super) struct StreamReaderConfig<R> {
    pub(super) url: String,
    pub(super) inner: R,
    pub(super) status_tx: mpsc::Sender<AudioStatus>,
    pub(super) conn_id: u64,
    pub(super) active_conn_id: Arc<AtomicU64>,
    pub(super) metaint: Option<usize>,
}

impl<R: Read> StreamReader<R> {
    pub(super) fn new(config: StreamReaderConfig<R>) -> Self {
        let bytes_until_meta = config.metaint.unwrap_or(0);

        Self {
            url: config.url,
            inner: config.inner,
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
        self.read_exact_from_inner(&mut length_byte)?;
        let length = length_byte[0] as usize * 16;

        if length > 0 {
            let mut meta_buf = vec![0u8; length];
            self.read_exact_from_inner(&mut meta_buf)?;
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

    fn read_exact_from_inner(&mut self, buf: &mut [u8]) -> std::io::Result<()> {
        let mut filled = 0;
        while filled < buf.len() {
            let bytes_read = self.inner.read(&mut buf[filled..])?;
            if bytes_read == 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "stream ended inside ICY metadata block",
                ));
            }
            filled += bytes_read;
        }
        Ok(())
    }
}

impl<R: Read> Read for StreamReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.active_conn_id.load(Ordering::SeqCst) != self.conn_id {
            return Err(std::io::Error::other("Abandoned"));
        }

        let Some(metaint) = self.metaint else {
            let n = self.inner.read(buf)?;
            self.pos += n as u64;
            return Ok(n);
        };

        if self.bytes_until_meta == 0 {
            self.read_metadata_block()?;
            self.bytes_until_meta = metaint;
        }

        let max_to_read = buf.len().min(self.bytes_until_meta);
        let n = self.inner.read(&mut buf[..max_to_read])?;
        self.pos += n as u64;
        self.bytes_until_meta -= n;

        Ok(n)
    }
}

impl<R: Read> Seek for StreamReader<R> {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        match pos {
            SeekFrom::Current(0) => Ok(self.pos),
            SeekFrom::Start(0) if self.pos == 0 => Ok(0),
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
    use std::io::{Cursor, ErrorKind};

    fn test_reader(bytes: &[u8], metaint: Option<usize>) -> StreamReader<Cursor<Vec<u8>>> {
        let (status_tx, _status_rx) = mpsc::channel();
        StreamReader::new(StreamReaderConfig {
            url: "http://stream".to_string(),
            inner: Cursor::new(bytes.to_vec()),
            status_tx,
            conn_id: 1,
            active_conn_id: Arc::new(AtomicU64::new(1)),
            metaint,
        })
    }

    #[test]
    fn icy_metadata_block_is_read_exactly_before_more_audio_is_returned() {
        let mut reader = test_reader(b"A\x01StreamTitle='X';B", Some(1));
        let mut buf = [0u8; 2];

        assert_eq!(reader.read(&mut buf).unwrap(), 1);
        assert_eq!(buf[0], b'A');

        assert_eq!(reader.read(&mut buf).unwrap(), 1);
        assert_eq!(buf[0], b'B');
    }

    #[test]
    fn icy_metadata_eof_returns_unexpected_eof_instead_of_audio_bytes() {
        let mut reader = test_reader(b"A\x01partial", Some(1));

        let mut byte = [0u8; 1];
        assert_eq!(reader.read(&mut byte).unwrap(), 1);
        assert_eq!(byte[0], b'A');

        let err = reader.read(&mut byte).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::UnexpectedEof);
    }

    #[test]
    fn seek_reports_position_but_refuses_to_discard_live_audio() {
        let mut reader = test_reader(b"abcdef", None);

        let mut buf = [0u8; 2];
        assert_eq!(reader.read(&mut buf).unwrap(), 2);
        assert_eq!(reader.stream_position().unwrap(), 2);

        let err = reader.seek(SeekFrom::Current(4)).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Unsupported);

        let mut next = [0u8; 1];
        assert_eq!(reader.read(&mut next).unwrap(), 1);
        assert_eq!(next[0], b'c');
    }
}
