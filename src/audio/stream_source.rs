use super::metadata::parse_stream_title;
use super::types::{EngineEvent, Generation};

use std::io::{self, Read, Seek, SeekFrom};
use std::sync::atomic::{AtomicU64, Ordering::SeqCst};
use std::sync::{mpsc, Arc};

// ---------------------------------------------------------------------------
// IcyDemux
// ---------------------------------------------------------------------------

/// Tracks position within an ICY stream and handles metadata block parsing.
///
/// ICY streams interleave fixed-size audio chunks with variable-length metadata
/// blocks.  The interval (`metaint`) is negotiated in the HTTP response headers.
/// A metadata block is preceded by a single length byte (multiply by 16 to get
/// the block size); if the length byte is 0 the block is absent and no bytes
/// are consumed.
struct IcyDemux {
    /// Number of audio bytes remaining before the next metadata block.
    bytes_until_next_meta: usize,
    /// The ICY metadata interval in bytes.
    metaint: usize,
}

impl IcyDemux {
    fn new(metaint: usize) -> Self {
        Self {
            bytes_until_next_meta: metaint,
            metaint,
        }
    }
}

// ---------------------------------------------------------------------------
// StreamSource
// ---------------------------------------------------------------------------

/// A byte reader that strips ICY metadata blocks and emits `TrackChanged` events.
///
/// This is the hardened successor to `StreamReader`.  It implements `Read` and
/// `Seek`; seek always returns an error because live streams are append-only.
///
/// # ICY demux
///
/// When `icy` is `Some`, every `metaint` audio bytes the reader consumes the
/// next metadata block from the inner reader (length byte + body), parses
/// `StreamTitle`, and sends a `TrackChanged` event if the title changed.
/// Metadata bytes are *never* written into `buf`.
///
/// # Generation guard
///
/// Before any I/O, `read` checks whether `generation == active_generation`.
/// If not, it returns `io::Error::other("Abandoned")` immediately.
pub(super) struct StreamSource<R: Read> {
    inner: R,
    icy: Option<IcyDemux>,
    generation: Generation,
    active_generation: Arc<AtomicU64>,
    event_tx: mpsc::Sender<EngineEvent>,
    /// Most recently seen `StreamTitle` — used to suppress duplicate events.
    last_title: Option<String>,
}

impl<R: Read> StreamSource<R> {
    pub(super) fn new(
        inner: R,
        metaint: Option<usize>,
        generation: Generation,
        active_generation: Arc<AtomicU64>,
        event_tx: mpsc::Sender<EngineEvent>,
    ) -> Self {
        Self {
            inner,
            icy: metaint.map(IcyDemux::new),
            generation,
            active_generation,
            event_tx,
            last_title: None,
        }
    }

    /// Read exactly `buf.len()` bytes from `inner`, never partially filling.
    ///
    /// Returns `UnexpectedEof` if the stream ends before the buffer is full.
    fn read_exact_inner(&mut self, buf: &mut [u8]) -> io::Result<()> {
        let mut filled = 0;
        while filled < buf.len() {
            let n = self.inner.read(&mut buf[filled..])?;
            if n == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "stream ended inside ICY metadata block",
                ));
            }
            filled += n;
        }
        Ok(())
    }

    /// Consume one ICY metadata block from the inner reader.
    ///
    /// Reads the 1-byte length indicator, multiplies by 16 to get the body
    /// size, reads the body, and — if a `StreamTitle` is found — sends a
    /// `TrackChanged` event (deduped against `last_title`).
    ///
    /// Resets `bytes_until_next_meta` to `metaint` on success.
    fn consume_metadata_block(&mut self, demux: &mut IcyDemux) -> io::Result<()> {
        let mut len_byte = [0u8; 1];
        self.read_exact_inner(&mut len_byte)?;
        let body_len = len_byte[0] as usize * 16;

        if body_len > 0 {
            let mut body = vec![0u8; body_len];
            self.read_exact_inner(&mut body)?;

            // Parse title, ignoring non-UTF-8 metadata blocks.
            if let Ok(meta_str) = String::from_utf8(body) {
                if let Some(title) = parse_stream_title(&meta_str) {
                    // Only emit an event when the title actually changes.
                    let changed = self.last_title.as_deref() != Some(title.as_str());
                    if changed {
                        self.last_title = Some(title.clone());
                        let _ = self.event_tx.send(EngineEvent::TrackChanged {
                            generation: self.generation,
                            title,
                        });
                    }
                }
            }
        }

        demux.bytes_until_next_meta = demux.metaint;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// std::io::Read
// ---------------------------------------------------------------------------

impl<R: Read> Read for StreamSource<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        // Generation guard — must be checked before any I/O.
        if self.active_generation.load(SeqCst) != self.generation {
            return Err(io::Error::other("Abandoned"));
        }

        // Fast path: no ICY demux.
        let Some(ref mut demux) = self.icy else {
            return self.inner.read(buf);
        };

        // If we're exactly at a metadata boundary, consume the block first.
        if demux.bytes_until_next_meta == 0 {
            // We need a mutable borrow of both `self` fields simultaneously, so
            // we take the demux out, operate, and put it back.
            let Some(mut demux_owned) = self.icy.take() else {
                return Err(io::Error::other("ICY demux state lost"));
            };
            let result = self.consume_metadata_block(&mut demux_owned);
            self.icy = Some(demux_owned);
            result?;
        }

        // Deliver at most `bytes_until_next_meta` audio bytes so we never
        // straddle a metadata boundary in a single read call.
        let Some(demux) = self.icy.as_mut() else {
            return Err(io::Error::other("ICY demux state lost after restore"));
        };
        let max = buf.len().min(demux.bytes_until_next_meta);
        let n = self.inner.read(&mut buf[..max])?;
        demux.bytes_until_next_meta -= n;
        Ok(n)
    }
}

// ---------------------------------------------------------------------------
// std::io::Seek
// ---------------------------------------------------------------------------

impl<R: Read> Seek for StreamSource<R> {
    fn seek(&mut self, _pos: SeekFrom) -> io::Result<u64> {
        Err(io::Error::other("seek not supported on live stream"))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, ErrorKind, Read};
    use std::sync::atomic::AtomicU64;

    // ---- helpers -----------------------------------------------------------

    /// Build a `StreamSource` backed by an in-memory cursor.
    fn make_source(
        bytes: Vec<u8>,
        metaint: Option<usize>,
    ) -> (StreamSource<Cursor<Vec<u8>>>, mpsc::Receiver<EngineEvent>) {
        let (tx, rx) = mpsc::channel();
        let active = Arc::new(AtomicU64::new(1));
        let src = StreamSource::new(Cursor::new(bytes), metaint, 1, active, tx);
        (src, rx)
    }

    /// Build a `StreamSource` with an explicit active-generation `Arc`.
    fn make_source_with_active(
        bytes: Vec<u8>,
        metaint: Option<usize>,
        active: Arc<AtomicU64>,
    ) -> (StreamSource<Cursor<Vec<u8>>>, mpsc::Receiver<EngineEvent>) {
        let (tx, rx) = mpsc::channel();
        let src = StreamSource::new(Cursor::new(bytes), metaint, 1, active, tx);
        (src, rx)
    }

    /// Build a raw ICY stream: audio bytes interleaved with metadata blocks.
    ///
    /// A metadata block is only inserted when a full `metaint`-byte boundary is
    /// reached.  A final partial audio chunk (stream ends before the next
    /// boundary) has **no** trailing metadata block — exactly as a real ICY
    /// server behaves.
    fn build_icy_stream(metaint: usize, audio: &[u8], titles: &[&str]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut audio_offset = 0;
        let mut title_idx = 0;

        while audio_offset < audio.len() {
            let chunk_end = (audio_offset + metaint).min(audio.len());
            let is_full_chunk = (chunk_end - audio_offset) == metaint;
            out.extend_from_slice(&audio[audio_offset..chunk_end]);
            audio_offset = chunk_end;

            // Only append a metadata block after a *full* metaint chunk.
            // A trailing partial chunk does NOT get a metadata block.
            if is_full_chunk {
                if title_idx < titles.len() {
                    let title = titles[title_idx];
                    if title.is_empty() {
                        out.push(0); // zero-length block
                    } else {
                        let meta_str = format!("StreamTitle='{}';", title);
                        let body_len = meta_str.len().div_ceil(16) * 16;
                        let len_byte = (body_len / 16) as u8;
                        out.push(len_byte);
                        let mut body = meta_str.into_bytes();
                        body.resize(body_len, 0);
                        out.extend_from_slice(&body);
                    }
                    title_idx += 1;
                } else {
                    out.push(0); // zero-length block
                }
            }
        }
        let _ = title_idx; // suppress unused warning
        out
    }

    // ---- passthrough (no ICY) ----------------------------------------------

    #[test]
    fn no_icy_passes_bytes_through_unchanged() {
        let (mut src, _rx) = make_source(b"hello world".to_vec(), None);
        let mut buf = vec![0u8; 11];
        let n = src.read(&mut buf).unwrap();
        assert_eq!(n, 11);
        assert_eq!(&buf[..n], b"hello world");
    }

    // ---- exact-boundary reads ----------------------------------------------

    #[test]
    fn audio_bytes_before_metadata_block_returned_exactly() {
        // metaint = 1: one audio byte, then metadata, then another audio byte.
        let stream = build_icy_stream(1, b"AB", &["X", "Y"]);
        let (mut src, rx) = make_source(stream, Some(1));

        let mut buf = [0u8; 4];

        // First read: one audio byte 'A'.
        let n = src.read(&mut buf).unwrap();
        assert_eq!(n, 1);
        assert_eq!(buf[0], b'A');

        // Second read: crosses the first boundary — consumes meta block "X",
        // then returns one audio byte 'B'.
        let n = src.read(&mut buf).unwrap();
        assert_eq!(n, 1);
        assert_eq!(buf[0], b'B');

        // Third read: crosses the second boundary — consumes meta block "Y".
        // Inner reader is now at EOF so returns Ok(0).
        let n = src.read(&mut buf).unwrap();
        assert_eq!(n, 0);

        // Both TrackChanged events should have been emitted.
        assert!(rx.try_recv().is_ok(), "Expected first TrackChanged event");
        assert!(rx.try_recv().is_ok(), "Expected second TrackChanged event");
    }

    #[test]
    fn exact_boundary_read_does_not_include_metadata_bytes() {
        let metaint = 4;
        let audio = b"ABCDEFGH"; // 8 bytes = 2 chunks
        let stream = build_icy_stream(metaint, audio, &["Track1", "Track2"]);

        let (mut src, _rx) = make_source(stream, Some(metaint));
        let mut collected = Vec::new();
        let mut buf = [0u8; 4];
        loop {
            match src.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => collected.extend_from_slice(&buf[..n]),
                Err(_) => break,
            }
        }
        assert_eq!(collected, b"ABCDEFGH");
    }

    #[test]
    fn read_smaller_than_metaint_advances_position_correctly() {
        let metaint = 8;
        let audio = b"12345678";
        let stream = build_icy_stream(metaint, audio, &[""]);

        let (mut src, _rx) = make_source(stream, Some(metaint));

        let mut buf = [0u8; 3];
        let n1 = src.read(&mut buf).unwrap();
        assert_eq!(n1, 3);
        assert_eq!(&buf[..3], b"123");

        let _n2 = src.read(&mut buf).unwrap();
        assert_eq!(&buf[..3], b"456");

        // This read is limited to the remaining 2 audio bytes before metadata.
        let n3 = src.read(&mut buf).unwrap();
        assert_eq!(n3, 2);
        assert_eq!(&buf[..2], b"78");
    }

    // ---- EOF inside metadata block -----------------------------------------

    #[test]
    fn eof_inside_metadata_block_returns_unexpected_eof() {
        // Audio byte 'A', then a metadata block that is cut short.
        // Length byte says 16 bytes follow, but only "partial" (7 bytes) exist.
        let mut stream = vec![b'A']; // one audio byte
        stream.push(0x01); // length byte: 1 * 16 = 16 bytes
        stream.extend_from_slice(b"partial"); // only 7 bytes — stream cut short

        let (mut src, _rx) = make_source(stream, Some(1));

        let mut buf = [0u8; 1];
        // Read the audio byte.
        assert_eq!(src.read(&mut buf).unwrap(), 1);
        assert_eq!(buf[0], b'A');

        // Next read tries to consume the metadata block and hits EOF.
        let err = src.read(&mut buf).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::UnexpectedEof);
    }

    #[test]
    fn eof_reading_length_byte_propagates_error() {
        // Exactly one audio byte, then stream ends (no length byte at all).
        let stream = vec![b'Z'];
        let (mut src, _rx) = make_source(stream, Some(1));

        let mut buf = [0u8; 1];
        assert_eq!(src.read(&mut buf).unwrap(), 1);

        // Attempting to read past should see EOF from inner, which propagates.
        // (The inner reader returns Ok(0), which read_exact_inner converts to UnexpectedEof.)
        let err = src.read(&mut buf).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::UnexpectedEof);
    }

    // ---- seek refusal ------------------------------------------------------

    #[test]
    fn seek_always_returns_error() {
        let (mut src, _rx) = make_source(b"data".to_vec(), None);

        let err = src.seek(SeekFrom::Start(0)).unwrap_err();
        // Error message contains "seek not supported on live stream"
        assert!(err
            .to_string()
            .contains("seek not supported on live stream"));

        let err2 = src.seek(SeekFrom::Current(4)).unwrap_err();
        assert!(err2
            .to_string()
            .contains("seek not supported on live stream"));

        let err3 = src.seek(SeekFrom::End(0)).unwrap_err();
        assert!(err3
            .to_string()
            .contains("seek not supported on live stream"));
    }

    // ---- abandon on stale generation ---------------------------------------

    #[test]
    fn read_returns_abandoned_when_generation_becomes_stale() {
        let active = Arc::new(AtomicU64::new(1));
        let (mut src, _rx) = make_source_with_active(b"data".to_vec(), None, active.clone());

        // Invalidate the generation.
        active.store(0, SeqCst);

        let mut buf = [0u8; 4];
        let err = src.read(&mut buf).unwrap_err();
        assert_eq!(err.to_string(), "Abandoned");
    }

    #[test]
    fn read_returns_abandoned_when_generation_bumped_to_new_value() {
        let active = Arc::new(AtomicU64::new(1));
        let (mut src, _rx) = make_source_with_active(b"ABCDEF".to_vec(), Some(2), active.clone());

        // Read first audio chunk successfully.
        let mut buf = [0u8; 2];
        let _ = src.read(&mut buf);

        // Now bump to a new generation (e.g. a new Play command arrived).
        active.store(2, SeqCst);

        let err = src.read(&mut buf).unwrap_err();
        assert_eq!(err.to_string(), "Abandoned");
    }

    // ---- track changed events ----------------------------------------------

    #[test]
    fn track_changed_event_emitted_on_new_title() {
        let metaint = 2;
        let audio = b"AABB";
        let stream = build_icy_stream(metaint, audio, &["Song One", "Song Two"]);

        let (mut src, rx) = make_source(stream, Some(metaint));

        let mut buf = [0u8; 8];
        while let Ok(n) = src.read(&mut buf) {
            if n == 0 {
                break;
            }
        }

        let ev1 = rx.recv().unwrap();
        let ev2 = rx.recv().unwrap();

        match ev1 {
            EngineEvent::TrackChanged { title, .. } => assert_eq!(title, "Song One"),
            _ => panic!("Expected TrackChanged"),
        }
        match ev2 {
            EngineEvent::TrackChanged { title, .. } => assert_eq!(title, "Song Two"),
            _ => panic!("Expected TrackChanged"),
        }
    }

    #[test]
    fn duplicate_titles_do_not_emit_duplicate_events() {
        let metaint = 2;
        let audio = b"AABB";
        // Both blocks report the same title.
        let stream = build_icy_stream(metaint, audio, &["Same Song", "Same Song"]);

        let (mut src, rx) = make_source(stream, Some(metaint));

        let mut buf = [0u8; 8];
        while let Ok(n) = src.read(&mut buf) {
            if n == 0 {
                break;
            }
        }

        // Only one event for the first occurrence.
        assert!(rx.try_recv().is_ok());
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn zero_length_metadata_block_emits_no_event() {
        let stream = build_icy_stream(2, b"AB", &[""]);
        let (mut src, rx) = make_source(stream, Some(2));

        let mut buf = [0u8; 4];
        while let Ok(n) = src.read(&mut buf) {
            if n == 0 {
                break;
            }
        }

        assert!(rx.try_recv().is_err());
    }

    // ---- metadata disabled -------------------------------------------------

    #[test]
    fn when_metadata_disabled_all_bytes_pass_through() {
        // If metaint is None the source is constructed without ICY demux.
        // Any raw bytes (including what would be metadata in a live stream)
        // are returned verbatim.
        let raw = b"raw bytes no icy";
        let (mut src, _rx) = make_source(raw.to_vec(), None);

        let mut buf = vec![0u8; raw.len()];
        let n = src.read(&mut buf).unwrap();
        assert_eq!(n, raw.len());
        assert_eq!(&buf[..n], raw);
    }

    // ---------------------------------------------------------------------------
    // Property-based tests
    // ---------------------------------------------------------------------------

    #[cfg(test)]
    mod prop_tests {
        use super::*;
        use proptest::prelude::*;
        use std::io::{Cursor, Read};
        use std::sync::atomic::AtomicU64;

        // ---- Helpers -----------------------------------------------------------

        /// Encode `title` as an ICY metadata body padded to a 16-byte multiple.
        fn encode_meta_block(title: &str) -> Vec<u8> {
            if title.is_empty() {
                return vec![0u8]; // zero-length block
            }
            let body_str = format!("StreamTitle='{}';", title);
            let body_len = body_str.len().div_ceil(16) * 16;
            let mut out = Vec::with_capacity(1 + body_len);
            out.push((body_len / 16) as u8);
            let mut body = body_str.into_bytes();
            body.resize(body_len, 0);
            out.extend_from_slice(&body);
            out
        }

        /// Build a complete ICY byte stream from `audio_bytes`, `metaint`, and
        /// a list of title strings (one per inter-chunk boundary).
        ///
        /// A metadata block is only inserted after a **full** `metaint`-byte audio
        /// chunk.  A final partial chunk (stream ends before the next boundary) has
        /// no trailing metadata block.
        fn build_stream(audio: &[u8], metaint: usize, titles: &[String]) -> Vec<u8> {
            assert!(metaint > 0, "metaint must be > 0");
            let mut out = Vec::new();
            let mut offset = 0;
            let mut ti = 0;

            while offset < audio.len() {
                let end = (offset + metaint).min(audio.len());
                let is_full_chunk = (end - offset) == metaint;
                out.extend_from_slice(&audio[offset..end]);
                offset = end;

                // Only emit a metadata block after a full metaint chunk.
                if is_full_chunk {
                    let title = titles.get(ti).map(String::as_str).unwrap_or("");
                    out.extend_from_slice(&encode_meta_block(title));
                    ti += 1;
                }
            }
            out
        }

        /// Read *all* bytes from a `StreamSource` into a `Vec<u8>`.
        fn drain_source<R: Read>(src: &mut StreamSource<R>) -> Vec<u8> {
            let mut out = Vec::new();
            let mut buf = [0u8; 512];
            loop {
                match src.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => out.extend_from_slice(&buf[..n]),
                    Err(_) => break,
                }
            }
            out
        }

        // ---- Strategies --------------------------------------------------------

        /// Strategy: a valid ICY metaint value (1..=8192).
        fn arb_metaint() -> impl Strategy<Value = usize> {
            1usize..=8192
        }

        /// Strategy: arbitrary audio payload (0..=4096 bytes).
        fn arb_audio() -> impl Strategy<Value = Vec<u8>> {
            prop::collection::vec(any::<u8>(), 0..=4096)
        }

        /// Strategy: a list of at most 64 ICY title strings, each up to 128 chars,
        /// avoiding the `';` terminator sequence so the parser stays clean.
        fn arb_titles() -> impl Strategy<Value = Vec<String>> {
            prop::collection::vec("[^';]{0,128}", 0..=64)
        }

        // ========================================================================
        // Property 7: ICY safety
        //
        // For any (audio_bytes, metaint, title_strings), all bytes returned by
        // StreamSource::read equal audio_bytes with no metadata bytes present.
        //
        // Validates: Requirements 8.1, 8.3
        // ========================================================================

        proptest! {
            #[test]
            fn icy_safety_no_metadata_bytes_in_output(
                audio in arb_audio(),
                metaint in arb_metaint(),
                titles in arb_titles(),
            ) {
                let stream_bytes = build_stream(&audio, metaint, &titles);
                let (tx, _rx) = mpsc::channel();
                let active = Arc::new(AtomicU64::new(1));
                let mut src = StreamSource::new(
                    Cursor::new(stream_bytes),
                    Some(metaint),
                    1,
                    active,
                    tx,
                );

                let output = drain_source(&mut src);

                prop_assert_eq!(
                    output,
                    audio,
                    "StreamSource output must equal raw audio bytes"
                );
            }
        }

        // ========================================================================
        // Property 14: Stale StreamSource read is immediately abandoned
        //
        // For any StreamSource whose generation is made inactive before reading,
        // the first read returns Abandoned without blocking.
        //
        // Validates: Requirements 8.5
        // ========================================================================

        proptest! {
            #[test]
            fn stale_generation_read_returns_abandoned(
                audio in arb_audio(),
                metaint in proptest::option::of(arb_metaint()),
                // Use any nonzero active generation different from 1.
                new_gen in 0u64..=100,
            ) {
                // Only test when new_gen != 1 (i.e., the generation is stale).
                prop_assume!(new_gen != 1);

                let (tx, _rx) = mpsc::channel();
                let active = Arc::new(AtomicU64::new(1));
                let mut src = StreamSource::new(
                    Cursor::new(audio),
                    metaint,
                    1,
                    active.clone(),
                    tx,
                );

                // Make the generation stale.
                active.store(new_gen, SeqCst);

                let mut buf = [0u8; 512];
                let err = src.read(&mut buf).expect_err("read should fail when generation is stale");
                prop_assert_eq!(err.to_string(), "Abandoned");
            }
        }
    }
}
