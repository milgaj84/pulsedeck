use std::io::{Read, Seek, SeekFrom};

const INITIAL_PROBE_BYTES: usize = 4 * 1024 * 1024;

pub struct ProbeReplayReader<R> {
    inner: R,
    replay: Vec<u8>,
    pos: u64,
    read_past_replay: bool,
}

impl<R: Read> ProbeReplayReader<R> {
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            replay: Vec::with_capacity(INITIAL_PROBE_BYTES),
            pos: 0,
            read_past_replay: false,
        }
    }
}

impl<R: Read> Read for ProbeReplayReader<R> {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        let replay_len = self.replay.len() as u64;
        if self.pos < replay_len {
            let available = (replay_len - self.pos) as usize;
            let n = available.min(out.len());
            let start = self.pos as usize;
            out[..n].copy_from_slice(&self.replay[start..start + n]);
            self.pos += n as u64;
            return Ok(n);
        }

        let n = self.inner.read(out)?;
        if n > 0 {
            if self.replay.len() < INITIAL_PROBE_BYTES {
                let remaining = INITIAL_PROBE_BYTES - self.replay.len();
                let captured = n.min(remaining);
                self.replay.extend_from_slice(&out[..captured]);
                if captured < n {
                    self.read_past_replay = true;
                }
            } else {
                self.read_past_replay = true;
            }
        }
        self.pos += n as u64;
        Ok(n)
    }
}

impl<R: Read> Seek for ProbeReplayReader<R> {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let target = match pos {
            SeekFrom::Start(n) => n,
            SeekFrom::Current(0) => return Ok(self.pos),
            SeekFrom::Current(offset) if offset < 0 => self
                .pos
                .checked_sub(offset.unsigned_abs())
                .ok_or_else(unsupported_seek)?,
            SeekFrom::Current(offset) => self.pos.saturating_add(offset as u64),
            SeekFrom::End(_) => return Err(unsupported_seek()),
        };

        if target == self.pos {
            return Ok(self.pos);
        }

        if self.read_past_replay {
            return Err(unsupported_seek());
        }

        if target <= self.replay.len() as u64 {
            self.pos = target;
            Ok(self.pos)
        } else {
            Err(unsupported_seek())
        }
    }
}

fn unsupported_seek() -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "live radio stream can only seek inside the initial probe buffer",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, ErrorKind};

    #[test]
    fn can_replay_initial_bytes_after_seek_to_start() {
        let mut reader = ProbeReplayReader::new(Cursor::new(b"abcdef".to_vec()));
        let mut first = [0; 3];
        reader.read_exact(&mut first).unwrap();
        assert_eq!(&first, b"abc");

        reader.seek(SeekFrom::Start(0)).unwrap();
        let mut second = [0; 3];
        reader.read_exact(&mut second).unwrap();
        assert_eq!(&second, b"abc");
    }

    #[test]
    fn seeking_beyond_replay_is_unsupported() {
        let mut reader = ProbeReplayReader::new(Cursor::new(b"abcdef".to_vec()));
        let mut first = [0; 3];
        reader.read_exact(&mut first).unwrap();

        let err = reader.seek(SeekFrom::Start(4)).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Unsupported);
    }

    #[test]
    fn end_seek_is_unsupported() {
        let mut reader = ProbeReplayReader::new(Cursor::new(b"abcdef".to_vec()));
        let err = reader.seek(SeekFrom::End(0)).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Unsupported);
    }

    #[test]
    fn forward_seek_beyond_replay_does_not_consume_inner_reader() {
        let mut reader = ProbeReplayReader::new(Cursor::new(b"abcdef".to_vec()));
        let mut first = [0; 2];
        reader.read_exact(&mut first).unwrap();

        let err = reader.seek(SeekFrom::Current(1)).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Unsupported);

        let mut next = [0; 2];
        reader.read_exact(&mut next).unwrap();
        assert_eq!(&next, b"cd");
    }

    #[test]
    fn rewind_after_reading_past_replay_window_is_unsupported() {
        let data = vec![b'x'; INITIAL_PROBE_BYTES + 1];
        let mut reader = ProbeReplayReader::new(Cursor::new(data));
        let mut first = vec![0; INITIAL_PROBE_BYTES + 1];
        reader.read_exact(&mut first).unwrap();

        let err = reader.seek(SeekFrom::Start(0)).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Unsupported);
        assert_eq!(reader.stream_position().unwrap(), (INITIAL_PROBE_BYTES + 1) as u64);
    }
}
