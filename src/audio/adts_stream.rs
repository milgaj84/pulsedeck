//! Plain AAC streams in the ADTS forms Symphonia cannot read.
//!
//! Symphonia's ADTS reader only accepts the MPEG-4 form of the header with no
//! CRC (sync word `0xFFF1`). Many radio streams (iHeart, talkSPORT, Q-music,
//! Times Radio and about a quarter of the popular AAC stations) use the MPEG-2
//! form (`0xFFF9`) or CRC-protected frames. For those, the probe finds nothing
//! and on a live stream keeps reading, so they never started. HLS already
//! rewrites its frames; [`AdtsStream`] does the same for plain streams.

use std::io::{self, Read};

use super::hls::AdtsNormalizer;

/// How much of the start of a stream to look through for a frame.
const SCAN_WINDOW: usize = 4096;

/// The first ADTS frame in `bytes` that is confirmed by another frame right
/// after it, as (header_len, id_bit_set). A live stream can start mid-frame.
fn first_frame(bytes: &[u8]) -> Option<(usize, bool)> {
    let window = &bytes[..bytes.len().min(SCAN_WINDOW)];
    let is_header =
        |at: usize| matches!(window.get(at..at + 2), Some([0xFF, b1]) if b1 & 0xF6 == 0xF0);
    let frame_len = |at: usize| -> Option<usize> {
        let h = window.get(at..at + 6)?;
        Some((usize::from(h[3] & 3) << 11) | (usize::from(h[4]) << 3) | usize::from(h[5] >> 5))
    };
    (0..window.len().saturating_sub(7)).find_map(|at| {
        if !is_header(at) {
            return None;
        }
        let b1 = window[at + 1];
        let header_len = if b1 & 1 == 1 { 7 } else { 9 };
        let len = frame_len(at)?;
        // Confirmed by the next frame's header; a stray FF Fx is not enough.
        (len >= header_len && is_header(at + len)).then_some((header_len, b1 & 0x08 != 0))
    })
}

/// Whether the ADTS stream starting in `bytes` is in a form Symphonia rejects:
/// the MPEG-2 form of the header, or frames with a CRC.
pub(super) fn needs_rewrite(bytes: &[u8]) -> bool {
    matches!(first_frame(bytes), Some((header_len, mpeg2)) if mpeg2 || header_len == 9)
}

/// Reads an ADTS stream with every frame rewritten into the canonical form.
pub(super) struct AdtsStream<R> {
    inner: R,
    normalizer: AdtsNormalizer,
    ready: Vec<u8>,
    pos: usize,
}

impl<R: Read> AdtsStream<R> {
    pub(super) fn new(inner: R) -> Self {
        Self {
            inner,
            normalizer: AdtsNormalizer::default(),
            ready: Vec::new(),
            pos: 0,
        }
    }
}

impl<R: Read> Read for AdtsStream<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        while self.pos >= self.ready.len() {
            self.ready.clear();
            self.pos = 0;
            let mut chunk = [0u8; 8192];
            let read = self.inner.read(&mut chunk)?;
            if read == 0 {
                // A partial last frame is dropped.
                return Ok(0);
            }
            self.normalizer
                .push(&chunk[..read], &mut self.ready)
                .map_err(|_| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        "multi-frame ADTS audio is not supported",
                    )
                })?;
        }
        let count = buf.len().min(self.ready.len() - self.pos);
        buf[..count].copy_from_slice(&self.ready[self.pos..self.pos + count]);
        self.pos += count;
        Ok(count)
    }
}

#[cfg(test)]
pub(super) mod fixtures {
    //! The same AAC audio in the ADTS forms Symphonia rejects.

    pub(crate) const AAC: &[u8] = include_bytes!("hls/testdata/tone.aac");

    /// The ADTS frames of `AAC` as (header, payload).
    pub(crate) fn frames() -> Vec<(Vec<u8>, Vec<u8>)> {
        let mut frames = Vec::new();
        let mut pos = 0;
        while pos + 7 <= AAC.len() {
            let len = (usize::from(AAC[pos + 3] & 3) << 11)
                | (usize::from(AAC[pos + 4]) << 3)
                | usize::from(AAC[pos + 5] >> 5);
            frames.push((AAC[pos..pos + 7].to_vec(), AAC[pos + 7..pos + len].to_vec()));
            pos += len;
        }
        frames
    }

    /// The same audio with the MPEG-2 ID bit set in every header.
    pub(crate) fn mpeg2(frames: &[(Vec<u8>, Vec<u8>)]) -> Vec<u8> {
        let mut out = Vec::new();
        for (header, payload) in frames {
            let mut header = header.clone();
            header[1] |= 0x08;
            out.extend_from_slice(&header);
            out.extend_from_slice(payload);
        }
        out
    }

    /// The same audio with a (made-up) CRC after every header.
    pub(crate) fn with_crc(frames: &[(Vec<u8>, Vec<u8>)]) -> Vec<u8> {
        let mut out = Vec::new();
        for (header, payload) in frames {
            let mut header = header.clone();
            header[1] &= !1; // protection present
            let len = header.len() + 2 + payload.len();
            header[3] = (header[3] & 0xFC) | ((len >> 11) & 3) as u8;
            header[4] = (len >> 3) as u8;
            header[5] = (((len & 7) as u8) << 5) | (header[5] & 0x1F);
            out.extend_from_slice(&header);
            out.extend_from_slice(&[0xC0, 0xDE]);
            out.extend_from_slice(payload);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::{frames, mpeg2, with_crc, AAC};
    use super::*;
    use std::io::Cursor;

    #[test]
    fn only_the_forms_symphonia_rejects_need_a_rewrite() {
        let frames = frames();
        assert!(frames.len() > 10);

        assert!(!needs_rewrite(AAC));
        assert!(needs_rewrite(&mpeg2(&frames)));
        assert!(needs_rewrite(&with_crc(&frames)));
    }

    #[test]
    fn a_stream_joined_in_the_middle_of_a_frame_is_still_recognised() {
        let stream = mpeg2(&frames());

        assert!(needs_rewrite(&stream[37..]));
        assert!(!needs_rewrite(&AAC[37..]));
    }

    #[test]
    fn other_data_is_left_alone() {
        assert!(!needs_rewrite(&[]));
        assert!(!needs_rewrite(&[0xFF, 0xF9]));
        assert!(!needs_rewrite(&[0u8; 5000]));
        assert!(!needs_rewrite(include_bytes!("hls/testdata/tone.mp3")));
        assert!(!needs_rewrite(include_bytes!("testdata/tone.ogg")));
        // A stray sync-like pair that no second frame confirms.
        let mut stray = vec![0x11u8; 300];
        stray[100] = 0xFF;
        stray[101] = 0xF9;
        assert!(!needs_rewrite(&stray));
    }

    fn read_all(mut reader: impl Read) -> Vec<u8> {
        let mut out = Vec::new();
        reader.read_to_end(&mut out).unwrap();
        out
    }

    #[test]
    fn rewriting_gives_back_the_canonical_stream() {
        let frames = frames();

        for source in [mpeg2(&frames), with_crc(&frames)] {
            let rewritten = read_all(AdtsStream::new(Cursor::new(source)));

            assert_eq!(rewritten, AAC);
        }
    }

    #[test]
    fn the_result_does_not_depend_on_how_the_bytes_arrive() {
        struct Dribble(Cursor<Vec<u8>>, usize);
        impl Read for Dribble {
            fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                let limit = buf.len().min(self.1);
                self.0.read(&mut buf[..limit])
            }
        }
        let source = mpeg2(&frames());

        for step in [1, 5, 100, 8192] {
            let reader = AdtsStream::new(Dribble(Cursor::new(source.clone()), step));
            assert_eq!(read_all(reader), AAC, "step {step}");
        }
    }

    #[test]
    fn reading_in_small_pieces_works() {
        let mut reader = AdtsStream::new(Cursor::new(mpeg2(&frames())));
        let mut out = Vec::new();
        let mut piece = [0u8; 13];
        loop {
            let read = reader.read(&mut piece).unwrap();
            if read == 0 {
                break;
            }
            out.extend_from_slice(&piece[..read]);
        }

        assert_eq!(out, AAC);
    }

    #[test]
    fn junk_before_the_first_frame_and_a_cut_off_last_frame_are_dropped() {
        let mut source = b"some junk before".to_vec();
        source.extend(mpeg2(&frames()));
        source.truncate(source.len() - 10);

        let rewritten = read_all(AdtsStream::new(Cursor::new(source)));

        // Everything but the cut-off last frame.
        assert!(rewritten.len() < AAC.len() && rewritten.len() > AAC.len() - 1000);
        assert_eq!(rewritten[..], AAC[..rewritten.len()]);
    }

    #[test]
    fn multi_frame_adts_is_an_error_not_garbage() {
        let mut source = mpeg2(&frames());
        source[6] |= 1; // two raw data blocks in the first frame
        let mut reader = AdtsStream::new(Cursor::new(source));

        let error = reader.read(&mut [0u8; 64]).unwrap_err();

        assert!(error.to_string().contains("multi-frame"));
    }
}
