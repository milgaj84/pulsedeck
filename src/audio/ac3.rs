//! AC-3 and E-AC-3 (Dolby Digital and Digital Plus) decoding.
//!
//! Symphonia has no decoder for either, so the audio comes from the pure-Rust
//! `oxideav-ac3` crate. Its decoder is not a Symphonia `Decoder` (it works on
//! one access unit at a time and cannot be shared between threads), so this
//! module is a [`FrameBackend`] of its own: it frames the byte stream into
//! syncframes, groups E-AC-3 dependent substreams with their independent
//! frame, and decodes each group to interleaved `f32` samples.
//!
//! Surround sound is mixed down to stereo, the same way the rest of the player
//! treats multichannel audio.

use std::io::Read;

use oxideav_core::{CodecId, CodecParameters, Decoder, Frame, Packet, TimeBase};

use super::pcm_source::{FrameBackend, SignalInfo};

/// Syncframe start marker.
const SYNC: [u8; 2] = [0x0B, 0x77];
/// How much to read from the stream at a time.
const READ_CHUNK: usize = 8 * 1024;
/// How much of the start of a stream to look through for syncframes.
const DETECT_WINDOW: usize = 16 * 1024;
/// Give up on a stream that has produced no frame for this many bytes.
const MAX_GARBAGE: usize = 256 * 1024;
/// Consecutive undecodable access units tolerated before the stream is dropped.
const MAX_DECODE_ERRORS: u32 = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Dolby {
    Ac3,
    Eac3,
}

impl Dolby {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Ac3 => "AC-3",
            Self::Eac3 => "E-AC-3",
        }
    }
}

/// What a syncframe's header says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FrameInfo {
    pub(super) kind: Dolby,
    /// Length of the whole syncframe in bytes.
    pub(super) len: usize,
    pub(super) rate: u32,
    /// An AC-3 frame, or an E-AC-3 independent substream frame. False for an
    /// E-AC-3 dependent substream, which extends the independent frame before it.
    pub(super) independent: bool,
    /// E-AC-3 only; 0 for the main program.
    pub(super) substream_id: u8,
}

/// Parse the header at the start of `data`. `None` if it is not a valid
/// syncframe header (or too short to tell).
pub(super) fn frame_info(data: &[u8]) -> Option<FrameInfo> {
    if data.len() < 6 || data[..2] != SYNC {
        return None;
    }
    // Both formats keep `bsid` in the same place.
    let bsid = data[5] >> 3;
    match bsid {
        0..=10 => {
            let info = oxideav_ac3::syncinfo::parse(data).ok()?;
            Some(FrameInfo {
                kind: Dolby::Ac3,
                len: usize::try_from(info.frame_length).ok()?,
                rate: info.sample_rate,
                independent: true,
                substream_id: 0,
            })
        }
        11..=16 => {
            let stream_type = data[2] >> 6;
            let frame_words = (usize::from(data[2] & 7) << 8) | usize::from(data[3]);
            let fscod = data[4] >> 6;
            let rate = match fscod {
                0 => 48_000,
                1 => 44_100,
                2 => 32_000,
                // Half rates: the real rate code is in the next two bits.
                _ => match (data[4] >> 4) & 3 {
                    0 => 24_000,
                    1 => 22_050,
                    2 => 16_000,
                    _ => return None,
                },
            };
            if stream_type == 3 {
                return None; // reserved
            }
            Some(FrameInfo {
                kind: Dolby::Eac3,
                len: (frame_words + 1) * 2,
                rate,
                // Types 0 and 2 stand on their own; 1 depends on the frame before.
                independent: stream_type != 1,
                substream_id: (data[2] >> 3) & 7,
            })
        }
        _ => None,
    }
}

/// Splits a byte stream into validated syncframes, skipping anything between.
///
/// Consumed bytes are skipped with a cursor rather than removed from the front
/// of the buffer, so feeding in megabytes at once stays linear.
#[derive(Debug, Default)]
pub(super) struct Framer {
    buf: Vec<u8>,
    /// Start of what has not been used yet.
    pos: usize,
    /// Bytes thrown away since the last frame was found.
    discarded: usize,
}

impl Framer {
    pub(super) fn push(&mut self, data: &[u8]) {
        // What is left is less than a frame (everything complete was taken).
        self.buf.drain(..self.pos);
        self.pos = 0;
        self.buf.extend_from_slice(data);
    }

    /// Bytes thrown away while looking for the next frame.
    pub(super) fn discarded(&self) -> usize {
        self.discarded
    }

    fn discard(&mut self, count: usize) {
        self.pos += count;
        self.discarded += count;
    }

    /// The next complete syncframe. A candidate is only accepted when another
    /// syncword follows it, so a stray `0B 77` in other data is not mistaken for
    /// a frame; `at_end` waives that for the last frame of a stream. `None`
    /// means more data is needed.
    pub(super) fn next_frame(&mut self, at_end: bool) -> Option<(FrameInfo, Vec<u8>)> {
        loop {
            let data = &self.buf[self.pos..];
            let Some(start) = data.windows(2).position(|pair| pair == SYNC) else {
                // Keep a trailing 0x0B: the 0x77 may be in the next chunk.
                let keep = usize::from(data.last() == Some(&SYNC[0]));
                self.discard(data.len() - keep);
                return None;
            };
            self.discard(start);

            let data = &self.buf[self.pos..];
            if data.len() < 6 {
                return None;
            }
            let Some(info) = frame_info(data) else {
                self.discard(1);
                continue;
            };
            let needed = if at_end { info.len } else { info.len + 2 };
            if data.len() < needed {
                return None;
            }
            if data.len() >= info.len + 2 && data[info.len..info.len + 2] != SYNC {
                // Not a real frame boundary.
                self.discard(1);
                continue;
            }
            let frame = data[..info.len].to_vec();
            self.pos += info.len;
            self.discarded = 0;
            return Some((info, frame));
        }
    }
}

/// Whether `bytes` (the start of a stream) are AC-3 or E-AC-3. A live stream can
/// start in the middle of a frame, so the first frame may be found a little in.
pub(super) fn detect(bytes: &[u8]) -> Option<Dolby> {
    let mut framer = Framer::default();
    framer.push(&bytes[..bytes.len().min(DETECT_WINDOW)]);
    framer.next_frame(false).map(|(info, _)| info.kind)
}

/// One decodable unit: an AC-3 syncframe, or an E-AC-3 independent frame with
/// the dependent frames that follow it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AccessUnit {
    pub(super) kind: Dolby,
    pub(super) rate: u32,
    pub(super) bytes: Vec<u8>,
}

/// Builds [`AccessUnit`]s from syncframes in stream order.
#[derive(Debug, Default)]
pub(super) struct Assembler {
    /// The E-AC-3 unit being collected: it is complete once the next
    /// independent frame arrives.
    open: Option<AccessUnit>,
    /// Inside another program's substream: its dependents are skipped too.
    skipping: bool,
}

impl Assembler {
    pub(super) fn push(&mut self, info: FrameInfo, frame: &[u8]) -> Option<AccessUnit> {
        let unit = |frame: &[u8]| AccessUnit {
            kind: info.kind,
            rate: info.rate,
            bytes: frame.to_vec(),
        };
        match info.kind {
            Dolby::Ac3 => Some(unit(frame)),
            Dolby::Eac3 if info.independent => {
                // Only the main program (substream 0) is played.
                self.skipping = info.substream_id != 0;
                if self.skipping {
                    None
                } else {
                    self.open.replace(unit(frame))
                }
            }
            Dolby::Eac3 => {
                if let (Some(open), false) = (&mut self.open, self.skipping) {
                    open.bytes.extend_from_slice(frame);
                }
                None
            }
        }
    }

    /// The unit still being collected when the stream ends.
    pub(super) fn finish(&mut self) -> Option<AccessUnit> {
        self.open.take()
    }
}

/// Decodes a raw AC-3 / E-AC-3 byte stream.
pub(super) struct Ac3Backend {
    reader: Box<dyn Read + Send>,
    framer: Framer,
    assembler: Assembler,
    ended: bool,
    decoder: Option<(Dolby, Box<dyn Decoder>)>,
    errors: u32,
}

impl Ac3Backend {
    pub(super) fn new(reader: impl Read + Send + 'static) -> Self {
        Self {
            reader: Box::new(reader),
            framer: Framer::default(),
            assembler: Assembler::default(),
            ended: false,
            decoder: None,
            errors: 0,
        }
    }

    /// The next access unit; `Ok(None)` at the end of the stream.
    fn next_unit(&mut self) -> Result<Option<AccessUnit>, String> {
        loop {
            if let Some((info, frame)) = self.framer.next_frame(self.ended) {
                if let Some(unit) = self.assembler.push(info, &frame) {
                    return Ok(Some(unit));
                }
                continue;
            }
            if self.ended {
                return Ok(self.assembler.finish());
            }
            // A stream with no frame for this long, at the start or after it has
            // turned to junk, is not AC-3 (any more).
            if self.framer.discarded() > MAX_GARBAGE {
                return Err("no AC-3 frames found in the stream".to_string());
            }
            let mut chunk = [0u8; READ_CHUNK];
            match self.reader.read(&mut chunk) {
                Ok(0) => self.ended = true,
                Ok(read) => self.framer.push(&chunk[..read]),
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                // A stream that cannot be read any more has ended.
                Err(_) => self.ended = true,
            }
        }
    }

    fn decoder_for(&mut self, kind: Dolby) -> Result<&mut dyn Decoder, String> {
        if self
            .decoder
            .as_ref()
            .is_none_or(|(known, _)| *known != kind)
        {
            let (id, make): (_, fn(&CodecParameters) -> _) = match kind {
                Dolby::Ac3 => ("ac3", oxideav_ac3::decoder::make_decoder),
                Dolby::Eac3 => ("eac3", oxideav_ac3::decoder::make_eac3_decoder),
            };
            // Surround is mixed down to stereo by the decoder.
            let params = CodecParameters::audio(CodecId::new(id)).channels(2);
            let decoder = make(&params).map_err(|error| error.to_string())?;
            self.decoder = Some((kind, decoder));
        }
        match &mut self.decoder {
            Some((_, decoder)) => Ok(decoder.as_mut()),
            None => Err("AC-3 decoder unavailable".to_string()),
        }
    }

    /// Decode one access unit, appending its samples to `out`.
    fn decode(&mut self, unit: &AccessUnit, out: &mut Vec<f32>) -> Result<Option<u16>, String> {
        let decoder = self.decoder_for(unit.kind)?;
        let packet = Packet::new(0, TimeBase::new(1, 48_000), unit.bytes.clone());
        decoder
            .send_packet(&packet)
            .map_err(|error| error.to_string())?;

        let mut channels = None;
        while let Ok(Frame::Audio(frame)) = decoder.receive_frame() {
            let Some(data) = frame.data.first() else {
                continue;
            };
            let per_channel = usize::try_from(frame.samples).unwrap_or(0);
            if per_channel == 0 || data.len() % (2 * per_channel) != 0 {
                continue;
            }
            // 16-bit little-endian, interleaved. The frame does not say how many
            // channels it holds, so work it out from its size.
            channels = u16::try_from(data.len() / 2 / per_channel).ok();
            out.extend(
                data.as_chunks::<2>()
                    .0
                    .iter()
                    .map(|bytes| f32::from(i16::from_le_bytes(*bytes)) / 32_768.0),
            );
        }
        Ok(channels)
    }
}

impl FrameBackend for Ac3Backend {
    fn next_frame(&mut self, out: &mut Vec<f32>) -> Result<Option<SignalInfo>, String> {
        loop {
            let Some(unit) = self.next_unit()? else {
                return Ok(None);
            };
            let start = out.len();
            match self.decode(&unit, out) {
                Ok(Some(channels)) if channels > 0 && out.len() > start => {
                    self.errors = 0;
                    return Ok(Some(SignalInfo {
                        channels,
                        rate: unit.rate,
                    }));
                }
                // No audio came out, or the unit was damaged: skip it.
                _ => {
                    out.truncate(start);
                    self.errors += 1;
                    if self.errors > MAX_DECODE_ERRORS {
                        return Ok(None);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::pcm_source::PcmSource;
    use proptest::prelude::*;
    use rodio::Source;
    use std::io::Cursor;

    const AC3_STEREO: &[u8] = include_bytes!("testdata/dolby/tone.ac3");
    const EAC3_STEREO: &[u8] = include_bytes!("testdata/dolby/tone.eac3");
    const AC3_SURROUND: &[u8] = include_bytes!("testdata/dolby/tone51.ac3");
    const EAC3_SURROUND: &[u8] = include_bytes!("testdata/dolby/tone51.eac3");

    fn all_frames(stream: &[u8]) -> Vec<(FrameInfo, Vec<u8>)> {
        let mut framer = Framer::default();
        framer.push(stream);
        let mut frames = Vec::new();
        while let Some(frame) = framer.next_frame(true) {
            frames.push(frame);
        }
        frames
    }

    /// A fake E-AC-3 syncframe header (not decodable) for assembling tests.
    fn eac3_header(stream_type: u8, substream: u8, words: usize) -> Vec<u8> {
        let frmsiz = words - 1;
        let mut frame = vec![
            0x0B,
            0x77,
            (stream_type << 6) | (substream << 3) | ((frmsiz >> 8) as u8 & 7),
            frmsiz as u8,
            0x3F, // 48 kHz, 6 blocks, 5.1
            16 << 3,
        ];
        frame.resize(words * 2, 0xAA);
        frame
    }

    fn decode_all(stream: &[u8]) -> (u32, u16, Vec<f32>) {
        let source = PcmSource::new(Ac3Backend::new(Cursor::new(stream.to_vec()))).unwrap();
        (source.sample_rate(), source.channels(), source.collect())
    }

    /// Sign changes of the first channel per second of audio, over the middle
    /// half (the ends can hold codec padding and fades).
    fn crossings_per_second(samples: &[f32], channels: usize, rate: u32) -> f64 {
        let quarter = samples.len() / 4;
        let middle = &samples[quarter..samples.len() - quarter];
        let left: Vec<f32> = middle.iter().step_by(channels).copied().collect();
        let crossings = left.windows(2).filter(|w| w[0] * w[1] < 0.0).count();
        crossings as f64 / (left.len() as f64 / f64::from(rate))
    }

    // --- Headers ------------------------------------------------------------

    #[test]
    fn headers_of_real_streams_are_read() {
        let ac3 = frame_info(AC3_STEREO).unwrap();
        assert_eq!((ac3.kind, ac3.rate, ac3.len), (Dolby::Ac3, 48_000, 256));
        assert!(ac3.independent);

        let eac3 = frame_info(EAC3_STEREO).unwrap();
        assert_eq!((eac3.kind, eac3.rate), (Dolby::Eac3, 48_000));
        assert!(eac3.independent);
        assert_eq!(eac3.substream_id, 0);
        assert!(eac3.len > 100 && eac3.len.is_multiple_of(2));
    }

    #[test]
    fn invalid_headers_are_refused() {
        assert_eq!(frame_info(&[]), None);
        assert_eq!(frame_info(&[0x0B, 0x77, 1, 2, 3]), None);
        assert_eq!(frame_info(b"ID3\x04\x00\x00\x00\x00\x00\x00"), None);
        // An unknown bit stream id.
        assert_eq!(frame_info(&[0x0B, 0x77, 0, 0, 0, 20 << 3]), None);
        // The reserved E-AC-3 stream type.
        assert_eq!(frame_info(&[0x0B, 0x77, 0xC0, 0x10, 0, 16 << 3]), None);
    }

    #[test]
    fn eac3_frame_types_and_half_rates_are_understood() {
        let dependent = frame_info(&eac3_header(1, 0, 100)).unwrap();
        assert!(!dependent.independent);
        assert_eq!(dependent.len, 200);

        let converted = frame_info(&eac3_header(2, 0, 100)).unwrap();
        assert!(converted.independent);

        let other_program = frame_info(&eac3_header(0, 3, 100)).unwrap();
        assert_eq!(other_program.substream_id, 3);

        let mut half = eac3_header(0, 0, 100);
        half[4] = 0xC0 | (1 << 4); // fscod 3, fscod2 1: 22.05 kHz
        assert_eq!(frame_info(&half).unwrap().rate, 22_050);
    }

    // --- Framing ------------------------------------------------------------

    #[test]
    fn a_stream_splits_into_whole_frames() {
        for (stream, kind) in [(AC3_STEREO, Dolby::Ac3), (EAC3_STEREO, Dolby::Eac3)] {
            let frames = all_frames(stream);

            assert_eq!(
                frames.iter().map(|(_, f)| f.len()).sum::<usize>(),
                stream.len()
            );
            assert!(frames.len() >= 30);
            assert!(frames.iter().all(|(info, frame)| info.kind == kind
                && info.len == frame.len()
                && frame[..2] == SYNC));
        }
    }

    #[test]
    fn framing_does_not_depend_on_how_the_bytes_arrive() {
        let whole = all_frames(AC3_STEREO);
        for chunk in [1, 3, 7, 100, 1000] {
            let mut framer = Framer::default();
            let mut frames = Vec::new();
            for piece in AC3_STEREO.chunks(chunk) {
                framer.push(piece);
                while let Some(frame) = framer.next_frame(false) {
                    frames.push(frame);
                }
            }
            while let Some(frame) = framer.next_frame(true) {
                frames.push(frame);
            }
            assert_eq!(frames, whole, "chunks of {chunk}");
        }
    }

    #[test]
    fn junk_before_and_inside_the_stream_is_skipped() {
        let clean = all_frames(AC3_STEREO);
        let mut stream = b"ID3\x04\x00\x00\x00\x00\x00\x10 some tag bytes".to_vec();
        stream.extend_from_slice(&AC3_STEREO[..256 * 3]);
        stream.extend_from_slice(&[0xDE, 0xAD, 0x0B, 0x00, 0x0B, 0x77, 0x01]); // junk with a false sync
        stream.extend_from_slice(&AC3_STEREO[256 * 3..]);

        let frames = all_frames(&stream);

        // The frame right before the junk cannot be confirmed (no syncword
        // follows it), so it is dropped; everything else comes through intact.
        assert_eq!(frames.len(), clean.len() - 1);
        assert_eq!(frames[10].1, clean[11].1);
    }

    #[test]
    fn a_stream_that_starts_mid_frame_is_found() {
        let frames = all_frames(&AC3_STEREO[100..]);

        assert_eq!(frames.len(), all_frames(AC3_STEREO).len() - 1);
    }

    #[test]
    fn the_last_frame_needs_the_end_of_the_stream_to_be_confirmed() {
        let mut framer = Framer::default();
        framer.push(&AC3_STEREO[..256]);

        assert!(framer.next_frame(false).is_none());
        assert!(framer.next_frame(true).is_some());
    }

    #[test]
    fn a_truncated_last_frame_is_dropped() {
        let mut framer = Framer::default();
        framer.push(&AC3_STEREO[..256 + 100]);

        assert!(framer.next_frame(true).is_some());
        assert!(framer.next_frame(true).is_none());
    }

    #[test]
    fn detection_finds_both_formats_and_ignores_other_data() {
        assert_eq!(detect(AC3_STEREO), Some(Dolby::Ac3));
        assert_eq!(detect(EAC3_STEREO), Some(Dolby::Eac3));
        assert_eq!(detect(AC3_SURROUND), Some(Dolby::Ac3));
        assert_eq!(detect(&EAC3_SURROUND[777..]), Some(Dolby::Eac3));
        assert_eq!(detect(&[]), None);
        assert_eq!(detect(&[0x0B, 0x77]), None);
        assert_eq!(detect(&[0u8; 4096]), None);
        assert_eq!(detect(include_bytes!("hls/testdata/tone.mp3")), None);
        assert_eq!(detect(include_bytes!("hls/testdata/tone.aac")), None);
        assert_eq!(detect(include_bytes!("testdata/tone.ogg")), None);
    }

    // --- Access units -------------------------------------------------------

    #[test]
    fn dependent_frames_join_the_independent_frame_before_them() {
        let mut assembler = Assembler::default();
        let mut units = Vec::new();
        let frames = [
            eac3_header(0, 0, 50),
            eac3_header(1, 0, 20),
            eac3_header(1, 0, 20),
            eac3_header(0, 0, 50),
            eac3_header(0, 0, 50),
            eac3_header(1, 0, 30),
        ];
        for frame in &frames {
            if let Some(unit) = assembler.push(frame_info(frame).unwrap(), frame) {
                units.push(unit);
            }
        }
        units.extend(assembler.finish());

        let lengths: Vec<usize> = units.iter().map(|u| u.bytes.len()).collect();
        assert_eq!(lengths, vec![100 + 40 + 40, 100, 100 + 60]);
    }

    #[test]
    fn other_programs_and_their_dependents_are_skipped() {
        let mut assembler = Assembler::default();
        let mut units = Vec::new();
        let frames = [
            eac3_header(0, 0, 50),
            eac3_header(0, 1, 40), // another program
            eac3_header(1, 1, 20), // and its dependent
            eac3_header(0, 0, 50),
        ];
        for frame in &frames {
            if let Some(unit) = assembler.push(frame_info(frame).unwrap(), frame) {
                units.push(unit);
            }
        }
        units.extend(assembler.finish());

        // The other program's frames are gone from the main one's unit too.
        assert_eq!(units.len(), 2);
        assert!(units.iter().all(|u| u.bytes.len() == 100));
    }

    #[test]
    fn ac3_frames_are_units_on_their_own() {
        let mut assembler = Assembler::default();
        let frames = all_frames(AC3_STEREO);

        let units: Vec<_> = frames
            .iter()
            .filter_map(|(info, frame)| assembler.push(*info, frame))
            .collect();

        assert_eq!(units.len(), frames.len());
        assert!(assembler.finish().is_none());
    }

    // --- Decoding -----------------------------------------------------------

    #[test]
    fn stereo_streams_decode_to_a_tone() {
        for stream in [AC3_STEREO, EAC3_STEREO] {
            let (rate, channels, samples) = decode_all(stream);

            assert_eq!((rate, channels), (48_000, 2));
            // One second of stereo.
            assert!(samples.len().abs_diff(96_000) < 4_000, "{}", samples.len());
            let peak = samples.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
            assert!(peak > 0.05, "silent: {peak}");
            // A 440 Hz sine crosses zero about 880 times a second.
            let crossings = crossings_per_second(&samples, 2, rate);
            assert!((crossings - 880.0).abs() < 60.0, "{crossings}");
        }
    }

    #[test]
    fn surround_is_mixed_down_to_stereo() {
        for stream in [AC3_SURROUND, EAC3_SURROUND] {
            let (rate, channels, samples) = decode_all(stream);

            assert_eq!((rate, channels), (48_000, 2));
            assert!(samples.len().abs_diff(96_000) < 4_000, "{}", samples.len());
            let crossings = crossings_per_second(&samples, 2, rate);
            assert!((crossings - 880.0).abs() < 60.0, "{crossings}");
        }
    }

    #[test]
    fn decoding_does_not_depend_on_how_the_bytes_arrive() {
        // A reader that hands out a few bytes at a time.
        struct Dribble(Cursor<Vec<u8>>);
        impl Read for Dribble {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                let limit = buf.len().min(37);
                self.0.read(&mut buf[..limit])
            }
        }
        let whole = decode_all(EAC3_STEREO).2;

        let source =
            PcmSource::new(Ac3Backend::new(Dribble(Cursor::new(EAC3_STEREO.to_vec())))).unwrap();

        assert_eq!(source.collect::<Vec<_>>(), whole);
    }

    #[test]
    fn a_stream_with_no_frames_is_an_error() {
        let result = PcmSource::new(Ac3Backend::new(Cursor::new(vec![0x55; 100_000])));
        assert!(result.is_err());

        let enough_garbage = PcmSource::new(Ac3Backend::new(Cursor::new(vec![0x55; 600_000])));
        assert!(enough_garbage.is_err());
    }

    #[test]
    fn a_stream_that_turns_to_junk_ends_instead_of_reading_forever() {
        // Real frames, then an endless supply of junk (capped so that a missing
        // limit shows up as a failed assertion, not a hang).
        struct Junk {
            start: Cursor<Vec<u8>>,
            junk_read: std::sync::Arc<std::sync::atomic::AtomicUsize>,
        }
        impl Read for Junk {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                let read = self.start.read(buf)?;
                if read > 0 {
                    return Ok(read);
                }
                let so_far = self.junk_read.load(std::sync::atomic::Ordering::SeqCst);
                if so_far >= 20 * 1024 * 1024 {
                    return Ok(0);
                }
                buf.fill(0x55);
                self.junk_read
                    .fetch_add(buf.len(), std::sync::atomic::Ordering::SeqCst);
                Ok(buf.len())
            }
        }
        let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let source = PcmSource::new(Ac3Backend::new(Junk {
            start: Cursor::new(AC3_STEREO.to_vec()),
            junk_read: std::sync::Arc::clone(&counter),
        }))
        .unwrap();

        let samples: Vec<f32> = source.collect();

        assert!(samples.len() > 60_000, "the real part still plays");
        let junk = counter.load(std::sync::atomic::Ordering::SeqCst);
        assert!(junk < 1024 * 1024, "kept reading {junk} bytes of junk");
    }

    #[test]
    fn framing_large_inputs_stays_linear() {
        // These took minutes with a framer that removed from the front of its buffer.
        let started = std::time::Instant::now();
        // 4 MiB of false syncwords with an invalid header.
        let junk: Vec<u8> = [0x0B, 0x77, 0x00, 0x00, 0x00, 0xA0]
            .iter()
            .copied()
            .cycle()
            .take(4 * 1024 * 1024)
            .collect();
        let mut framer = Framer::default();
        framer.push(&junk);
        assert!(framer.next_frame(true).is_none());
        // 8 MiB of real frames pushed in one go.
        let mut framer = Framer::default();
        framer.push(&AC3_STEREO[..256].repeat(32 * 1024));
        let mut frames = 0;
        while framer.next_frame(true).is_some() {
            frames += 1;
        }
        assert_eq!(frames, 32 * 1024);
        assert!(
            started.elapsed() < std::time::Duration::from_secs(20),
            "took {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn a_read_error_ends_the_stream_cleanly() {
        struct Breaks(Cursor<Vec<u8>>, bool);
        impl Read for Breaks {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                if self.1 {
                    return Err(std::io::Error::other("connection reset"));
                }
                let limit = buf.len().min(1000);
                let read = self.0.read(&mut buf[..limit])?;
                self.1 = self.0.position() >= 3000;
                Ok(read)
            }
        }

        let source = PcmSource::new(Ac3Backend::new(Breaks(
            Cursor::new(AC3_STEREO.to_vec()),
            false,
        )))
        .unwrap();

        let samples: Vec<f32> = source.collect();
        assert!(samples.len() > 1000 && samples.len() < 96_000);
    }

    #[test]
    fn damaged_frames_are_skipped_not_fatal() {
        let mut stream = AC3_STEREO.to_vec();
        // Scramble the audio data of every fifth frame, keeping the headers.
        for frame in (0..stream.len() / 256).filter(|i| i % 5 == 2) {
            for byte in &mut stream[frame * 256 + 20..frame * 256 + 250] {
                *byte = byte.wrapping_mul(31).wrapping_add(7);
            }
        }

        let (rate, channels, samples) = decode_all(&stream);

        assert_eq!((rate, channels), (48_000, 2));
        assert!(samples.len() > 60_000);
    }

    proptest! {
        // Decoding is slow in debug builds, so keep the case count modest.
        #![proptest_config(ProptestConfig::with_cases(24))]

        #[test]
        fn arbitrary_bytes_never_panic_the_framer(bytes in proptest::collection::vec(any::<u8>(), 0..4096)) {
            let mut framer = Framer::default();
            framer.push(&bytes);
            while framer.next_frame(true).is_some() {}
            let _ = detect(&bytes);
        }

        #[test]
        fn arbitrary_bytes_never_panic_the_decoder(bytes in proptest::collection::vec(any::<u8>(), 0..4096)) {
            // Frame-shaped garbage: real headers over random bodies.
            let mut stream = Vec::new();
            for chunk in bytes.chunks(200) {
                stream.extend_from_slice(&AC3_STEREO[..8]);
                stream.extend_from_slice(chunk);
                stream.resize(stream.len().next_multiple_of(256), 0);
            }
            if let Ok(source) = PcmSource::new(Ac3Backend::new(Cursor::new(stream))) {
                let _ = source.take(20_000).count();
            }
        }

        #[test]
        fn corrupted_real_streams_never_panic(
            edits in proptest::collection::vec((any::<usize>(), any::<u8>()), 1..40),
            surround in any::<bool>(),
        ) {
            let mut stream = if surround { EAC3_SURROUND.to_vec() } else { AC3_STEREO.to_vec() };
            for (at, value) in edits {
                let at = at % stream.len();
                stream[at] = value;
            }
            if let Ok(source) = PcmSource::new(Ac3Backend::new(Cursor::new(stream))) {
                let _ = source.take(100_000).count();
            }
        }
    }
}
