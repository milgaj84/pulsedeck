//! Audio extraction from HLS segments. Pure: bytes in, bytes out.
//!
//! Supported segment formats:
//! - MPEG-TS carrying ADTS AAC (stream type 0x0F) or MPEG audio (0x03/0x04)
//! - "packed audio": raw ADTS AAC or raw MP3, each segment prefixed by an ID3 tag
//!
//! Symphonia's ADTS reader only accepts the exact sync word `0xFFF1`, assumes
//! a 7-byte header and rejects multi-frame ADTS, so every ADTS frame is
//! rewritten into that canonical form (MPEG-2 id bit cleared, CRC removed).

use super::playlist::HlsError;

pub(crate) const PACKET_SIZE: usize = 188;
const PACKET: usize = PACKET_SIZE;
const SYNC: u8 = 0x47;

/// The elementary stream carried by a stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EsKind {
    Adts,
    MpegAudio,
}

impl EsKind {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Adts => "AAC",
            Self::MpegAudio => "MP3",
        }
    }
}

/// Skip any ID3v2 tags at the start of `buf` (packed-audio segments carry one
/// per segment, which would corrupt the joined stream). A truncated tag yields
/// an empty slice.
pub(crate) fn strip_id3(mut buf: &[u8]) -> &[u8] {
    while buf.len() >= 10 && &buf[..3] == b"ID3" {
        let size_bytes = &buf[6..10];
        if size_bytes.iter().any(|b| b & 0x80 != 0) {
            break;
        }
        let size = size_bytes
            .iter()
            .fold(0usize, |acc, b| (acc << 7) | usize::from(*b));
        let footer = if buf[5] & 0x10 != 0 { 10 } else { 0 };
        let total = 10 + size + footer;
        if total > buf.len() {
            return &[];
        }
        buf = &buf[total..];
    }
    buf
}

/// What a downloaded segment contains.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SegmentFormat {
    Ts,
    Adts,
    MpegAudio,
    Unknown,
}

pub(crate) fn classify_segment(bytes: &[u8]) -> SegmentFormat {
    if bytes.first() == Some(&SYNC) && bytes.get(PACKET).is_none_or(|b| *b == SYNC) {
        return SegmentFormat::Ts;
    }
    let audio = strip_id3(bytes);
    match audio {
        [0xFF, b1, ..] if b1 & 0xF0 == 0xF0 && b1 & 0x06 == 0 => SegmentFormat::Adts,
        [0xFF, b1, ..] if b1 & 0xE0 == 0xE0 && b1 & 0x06 != 0 => SegmentFormat::MpegAudio,
        _ => SegmentFormat::Unknown,
    }
}

/// Rewrites ADTS frames into the canonical form Symphonia accepts.
#[derive(Debug, Default)]
pub(crate) struct AdtsNormalizer {
    carry: Vec<u8>,
}

impl AdtsNormalizer {
    pub(crate) fn push(&mut self, data: &[u8], out: &mut Vec<u8>) -> Result<(), HlsError> {
        let mut buf = std::mem::take(&mut self.carry);
        buf.extend_from_slice(data);
        let mut pos = 0;

        while pos + 7 <= buf.len() {
            let header = &buf[pos..];
            if header[0] != 0xFF || header[1] & 0xF6 != 0xF0 {
                pos += 1;
                continue;
            }
            let protection_absent = header[1] & 1 == 1;
            let header_len = if protection_absent { 7 } else { 9 };
            let frame_len = (usize::from(header[3] & 3) << 11)
                | (usize::from(header[4]) << 3)
                | usize::from(header[5] >> 5);
            if frame_len < header_len {
                pos += 1;
                continue;
            }
            if header[6] & 3 != 0 {
                return Err(HlsError::UnsupportedCodec("multi-frame ADTS".to_string()));
            }
            if pos + frame_len > buf.len() {
                break;
            }

            let new_len = frame_len - (header_len - 7);
            out.extend_from_slice(&[
                0xFF,
                0xF1,
                header[2],
                (header[3] & 0xFC) | ((new_len >> 11) & 3) as u8,
                (new_len >> 3) as u8,
                (((new_len & 7) as u8) << 5) | (header[5] & 0x1F),
                header[6],
            ]);
            out.extend_from_slice(&buf[pos + header_len..pos + frame_len]);
            pos += frame_len;
        }

        // Keep a possible partial frame; if no frame start was seen, only the
        // last few bytes can still be the start of one.
        self.carry = buf[pos..].to_vec();
        if self.carry.len() > 8192 {
            let keep = self.carry.len() - 6;
            self.carry.drain(..keep);
        }
        Ok(())
    }

    pub(crate) fn reset(&mut self) {
        self.carry.clear();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Resolution {
    Waiting,
    Audio(EsKind),
}

/// MPEG transport stream demuxer that outputs the first audio stream.
#[derive(Debug)]
pub(crate) struct TsDemuxer {
    carry: Vec<u8>,
    pmt_pid: Option<u16>,
    audio_pid: Option<u16>,
    resolution: Resolution,
    pes: Vec<u8>,
    pes_valid: bool,
    last_cc: Option<u8>,
    adts: AdtsNormalizer,
}

impl Default for TsDemuxer {
    fn default() -> Self {
        Self {
            carry: Vec::new(),
            pmt_pid: None,
            audio_pid: None,
            resolution: Resolution::Waiting,
            pes: Vec::new(),
            pes_valid: false,
            last_cc: None,
            adts: AdtsNormalizer::default(),
        }
    }
}

impl TsDemuxer {
    pub(crate) fn kind(&self) -> Option<EsKind> {
        match self.resolution {
            Resolution::Audio(kind) => Some(kind),
            Resolution::Waiting => None,
        }
    }

    /// Forget stream state (after a discontinuity); keep nothing from before.
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }

    /// Feed bytes (not necessarily aligned to packets) and append extracted
    /// elementary-stream bytes to `out`.
    pub(crate) fn push(&mut self, data: &[u8], out: &mut Vec<u8>) -> Result<(), HlsError> {
        let mut buf = std::mem::take(&mut self.carry);
        buf.extend_from_slice(data);
        let mut pos = 0;

        while pos + PACKET <= buf.len() {
            let aligned =
                buf[pos] == SYNC && (pos + PACKET == buf.len() || buf[pos + PACKET] == SYNC);
            if !aligned {
                pos += 1;
                continue;
            }
            self.packet(&buf[pos..pos + PACKET], out)?;
            pos += PACKET;
        }

        self.carry = buf[pos..].to_vec();
        Ok(())
    }

    /// Emit the PES still being assembled. Call at the end of each segment.
    pub(crate) fn flush(&mut self, out: &mut Vec<u8>) -> Result<(), HlsError> {
        self.flush_pes(out)
    }

    fn packet(&mut self, packet: &[u8], out: &mut Vec<u8>) -> Result<(), HlsError> {
        let pusi = packet[1] & 0x40 != 0;
        let pid = (u16::from(packet[1] & 0x1F) << 8) | u16::from(packet[2]);
        let adaptation = (packet[3] >> 4) & 3;
        let cc = packet[3] & 0x0F;

        if packet[1] & 0x80 != 0 || adaptation & 1 == 0 {
            return Ok(());
        }
        let mut offset = 4;
        if adaptation & 2 != 0 {
            offset += 1 + usize::from(packet[4]);
        }
        if offset >= PACKET {
            return Ok(());
        }
        let payload = &packet[offset..];

        if pid == 0 {
            if self.pmt_pid.is_none() {
                self.pmt_pid = parse_pat(payload, pusi);
            }
        } else if Some(pid) == self.pmt_pid {
            self.parse_pmt(payload, pusi)?;
        } else if Some(pid) == self.audio_pid {
            self.audio_packet(payload, pusi, cc, out)?;
        }
        Ok(())
    }

    fn audio_packet(
        &mut self,
        payload: &[u8],
        pusi: bool,
        cc: u8,
        out: &mut Vec<u8>,
    ) -> Result<(), HlsError> {
        let gap = self
            .last_cc
            .is_some_and(|last| cc != last && cc != (last + 1) & 0x0F);
        self.last_cc = Some(cc);

        if pusi {
            self.flush_pes(out)?;
            self.pes_valid = true;
        } else if gap {
            // Lost packets: this PES is incomplete; drop it, resume at the next one.
            self.pes.clear();
            self.pes_valid = false;
        }
        if self.pes_valid {
            self.pes.extend_from_slice(payload);
        }
        Ok(())
    }

    fn flush_pes(&mut self, out: &mut Vec<u8>) -> Result<(), HlsError> {
        let pes = std::mem::take(&mut self.pes);
        if pes.len() < 9 || pes[..3] != [0, 0, 1] {
            return Ok(());
        }
        let start = 9 + usize::from(pes[8]);
        let Some(es) = pes.get(start..) else {
            return Ok(());
        };
        match self.resolution {
            Resolution::Audio(EsKind::Adts) => self.adts.push(es, out),
            Resolution::Audio(EsKind::MpegAudio) => {
                out.extend_from_slice(es);
                Ok(())
            }
            Resolution::Waiting => Ok(()),
        }
    }

    fn parse_pmt(&mut self, payload: &[u8], pusi: bool) -> Result<(), HlsError> {
        let Some(section) = psi_section(payload, pusi) else {
            return Ok(());
        };
        if section.len() < 12 || section[0] != 0x02 {
            return Ok(());
        }
        let section_length = (usize::from(section[1] & 0x0F) << 8) | usize::from(section[2]);
        let Some(section) = section.get(..3 + section_length) else {
            return Ok(());
        };
        if section.len() < 16 {
            return Ok(());
        }
        let program_info = (usize::from(section[10] & 0x0F) << 8) | usize::from(section[11]);
        let mut pos = 12 + program_info;
        let end = section.len().saturating_sub(4);

        let mut video = false;
        let mut other_audio: Option<&'static str> = None;
        let mut found: Option<(u16, EsKind)> = None;

        while pos + 5 <= end {
            let stream_type = section[pos];
            let pid = (u16::from(section[pos + 1] & 0x1F) << 8) | u16::from(section[pos + 2]);
            let es_info =
                (usize::from(section[pos + 3] & 0x0F) << 8) | usize::from(section[pos + 4]);
            pos += 5 + es_info;

            match stream_type {
                0x0F if found.is_none() => found = Some((pid, EsKind::Adts)),
                0x03 | 0x04 if found.is_none() => found = Some((pid, EsKind::MpegAudio)),
                0x01 | 0x02 | 0x10 | 0x1B | 0x24 => video = true,
                0x11 => other_audio = Some("LATM AAC"),
                0x81 => other_audio = Some("AC-3"),
                0x87 => other_audio = Some("E-AC-3"),
                _ => {}
            }
        }

        if let Some((pid, kind)) = found {
            return match self.resolution {
                Resolution::Waiting => {
                    self.audio_pid = Some(pid);
                    self.resolution = Resolution::Audio(kind);
                    Ok(())
                }
                Resolution::Audio(current) if current != kind => Err(mixed_codecs(current, kind)),
                Resolution::Audio(_) => {
                    if self.audio_pid != Some(pid) {
                        self.audio_pid = Some(pid);
                        self.pes.clear();
                        self.pes_valid = false;
                    }
                    Ok(())
                }
            };
        }
        if self.resolution != Resolution::Waiting {
            // A later PMT without usable audio is ignored mid-stream.
            return Ok(());
        }
        // The PMT is complete and holds no audio we can decode.
        self.pmt_pid = None;
        Err(match (other_audio, video) {
            (Some(codec), _) => HlsError::UnsupportedCodec(codec.to_string()),
            (None, true) => HlsError::VideoOnly,
            (None, false) => HlsError::NoAudioVariant,
        })
    }
}

fn mixed_codecs(from: EsKind, to: EsKind) -> HlsError {
    HlsError::UnsupportedCodec(format!("mixed {}/{}", from.label(), to.label()))
}

/// The PSI section carried in a packet payload (after the pointer field).
fn psi_section(payload: &[u8], pusi: bool) -> Option<&[u8]> {
    if !pusi {
        return None;
    }
    let pointer = usize::from(*payload.first()?);
    payload.get(1 + pointer..)
}

fn parse_pat(payload: &[u8], pusi: bool) -> Option<u16> {
    let section = psi_section(payload, pusi)?;
    if section.len() < 12 || section[0] != 0x00 {
        return None;
    }
    let section_length = (usize::from(section[1] & 0x0F) << 8) | usize::from(section[2]);
    let end = (3 + section_length).min(section.len()).checked_sub(4)?;
    let mut pos = 8;
    while pos + 4 <= end {
        let program = (u16::from(section[pos]) << 8) | u16::from(section[pos + 1]);
        let pid = (u16::from(section[pos + 2] & 0x1F) << 8) | u16::from(section[pos + 3]);
        if program != 0 {
            return Some(pid);
        }
        pos += 4;
    }
    None
}

/// Extracts the audio elementary stream from consecutive segments, whatever
/// their format, and refuses to mix AAC with MP3.
#[derive(Debug, Default)]
pub(crate) struct AudioExtractor {
    ts: TsDemuxer,
    adts: AdtsNormalizer,
    kind: Option<EsKind>,
}

impl AudioExtractor {
    #[cfg(test)]
    pub(crate) fn kind(&self) -> Option<EsKind> {
        self.kind
    }

    /// Call on `#EXT-X-DISCONTINUITY`. The codec may legitimately be
    /// re-detected afterwards but is still not allowed to change.
    pub(crate) fn reset(&mut self) {
        self.ts.reset();
        self.adts.reset();
    }

    pub(crate) fn push_segment(
        &mut self,
        segment: &[u8],
        out: &mut Vec<u8>,
    ) -> Result<(), HlsError> {
        let kind = match classify_segment(segment) {
            SegmentFormat::Ts => {
                self.ts.push(segment, out)?;
                self.ts.flush(out)?;
                self.ts.kind()
            }
            SegmentFormat::Adts => {
                self.adts.push(strip_id3(segment), out)?;
                Some(EsKind::Adts)
            }
            SegmentFormat::MpegAudio => {
                out.extend_from_slice(strip_id3(segment));
                Some(EsKind::MpegAudio)
            }
            SegmentFormat::Unknown => return Err(HlsError::Malformed("unrecognised segment data")),
        };

        match (self.kind, kind) {
            (Some(known), Some(now)) if known != now => Err(mixed_codecs(known, now)),
            (None, Some(now)) => {
                self.kind = Some(now);
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_fixtures::*;
    use super::*;

    fn frames(count: usize, payload: usize) -> Vec<Vec<u8>> {
        (0..count)
            .map(|i| adts_frame(payload + i, false, false))
            .collect()
    }

    fn demux(segment: &[u8]) -> Result<Vec<u8>, HlsError> {
        let mut demuxer = TsDemuxer::default();
        let mut out = Vec::new();
        demuxer.push(segment, &mut out)?;
        demuxer.flush(&mut out)?;
        Ok(out)
    }

    /// Length field of the first ADTS header in `bytes`.
    fn adts_len(bytes: &[u8]) -> usize {
        (usize::from(bytes[3] & 3) << 11)
            | (usize::from(bytes[4]) << 3)
            | usize::from(bytes[5] >> 5)
    }

    #[test]
    fn strip_id3_handles_none_single_repeated_and_footer() {
        assert_eq!(strip_id3(b"abc"), b"abc");
        assert_eq!(strip_id3(&[]), &[] as &[u8]);

        let mut data = id3_tag(20);
        data.extend_from_slice(b"AUDIO");
        assert_eq!(strip_id3(&data), b"AUDIO");

        let mut data = id3_tag(5);
        data.extend(id3_tag(300));
        data.extend_from_slice(b"AUDIO");
        assert_eq!(strip_id3(&data), b"AUDIO");

        let mut footer = id3_tag(4);
        footer[5] |= 0x10;
        footer.extend_from_slice(&[0u8; 10]);
        footer.extend_from_slice(b"AUDIO");
        assert_eq!(strip_id3(&footer), b"AUDIO");
    }

    #[test]
    fn strip_id3_truncated_tag_yields_nothing_and_bad_size_is_left_alone() {
        let truncated = &id3_tag(50)[..30];
        assert!(strip_id3(truncated).is_empty());

        let mut bad = id3_tag(0);
        bad[6] = 0x80;
        assert_eq!(strip_id3(&bad), &bad[..]);
    }

    #[test]
    fn classifies_segment_formats() {
        let adts = adts_frame(30, false, false);
        let mut tagged = id3_tag(10);
        tagged.extend_from_slice(&adts);
        let mp3 = mp3_frame(30);

        assert_eq!(
            classify_segment(&ts_segment(0x0F, std::slice::from_ref(&adts))),
            SegmentFormat::Ts
        );
        assert_eq!(classify_segment(&adts), SegmentFormat::Adts);
        assert_eq!(classify_segment(&tagged), SegmentFormat::Adts);
        assert_eq!(classify_segment(&mp3), SegmentFormat::MpegAudio);
        assert_eq!(classify_segment(b"<html>"), SegmentFormat::Unknown);
        assert_eq!(classify_segment(&[]), SegmentFormat::Unknown);
    }

    #[test]
    fn adts_canonical_frames_pass_through_unchanged() {
        let input: Vec<u8> = frames(3, 40).concat();
        let mut out = Vec::new();

        AdtsNormalizer::default().push(&input, &mut out).unwrap();

        assert_eq!(out, input);
    }

    #[test]
    fn adts_mpeg2_header_is_rewritten_to_0xfff1() {
        let frame = adts_frame(60, true, false);
        assert_eq!(frame[1], 0xF9);
        let mut out = Vec::new();

        AdtsNormalizer::default().push(&frame, &mut out).unwrap();

        assert_eq!(out[1], 0xF1);
        assert_eq!(out.len(), frame.len());
        assert_eq!(&out[7..], &frame[7..]);
    }

    #[test]
    fn adts_crc_is_stripped_and_length_updated() {
        let frame = adts_frame(60, false, true);
        assert_eq!(frame[1], 0xF0);
        assert_eq!(adts_len(&frame), 69);
        let mut out = Vec::new();

        AdtsNormalizer::default().push(&frame, &mut out).unwrap();

        assert_eq!(out[1], 0xF1);
        assert_eq!(out.len(), 67);
        assert_eq!(adts_len(&out), 67);
        assert_eq!(&out[7..], &frame[9..]);
    }

    #[test]
    fn adts_multi_frame_is_rejected() {
        let mut frame = adts_frame(20, false, false);
        frame[6] = 0xFD;

        let err = AdtsNormalizer::default()
            .push(&frame, &mut Vec::new())
            .unwrap_err();

        assert!(matches!(err, HlsError::UnsupportedCodec(_)));
    }

    #[test]
    fn adts_skips_leading_garbage_and_keeps_partial_frames_between_pushes() {
        let all: Vec<u8> = frames(4, 30).concat();
        let mut input = vec![0x12, 0x34, 0x56];
        input.extend_from_slice(&all);

        for split in 0..input.len() {
            let mut normalizer = AdtsNormalizer::default();
            let mut out = Vec::new();
            normalizer.push(&input[..split], &mut out).unwrap();
            normalizer.push(&input[split..], &mut out).unwrap();
            assert_eq!(out, all, "split at {split}");
        }
    }

    #[test]
    fn ts_adts_audio_is_extracted_byte_exact() {
        let input = frames(5, 100);

        let out = demux(&ts_segment(0x0F, &input)).unwrap();

        assert_eq!(out, input.concat());
    }

    #[test]
    fn ts_frames_spanning_many_packets_are_reassembled() {
        let input = vec![adts_frame(1500, false, false), adts_frame(7, false, false)];

        let out = demux(&ts_segment(0x0F, &input)).unwrap();

        assert_eq!(out, input.concat());
    }

    #[test]
    fn ts_mpeg_audio_passes_through() {
        let input: Vec<Vec<u8>> = (0..3).map(|i| mp3_frame(80 + i)).collect();

        for stream_type in [0x03, 0x04] {
            let mut demuxer = TsDemuxer::default();
            let mut out = Vec::new();
            demuxer
                .push(&ts_segment(stream_type, &input), &mut out)
                .unwrap();
            demuxer.flush(&mut out).unwrap();

            assert_eq!(out, input.concat());
            assert_eq!(demuxer.kind(), Some(EsKind::MpegAudio));
        }
    }

    #[test]
    fn ts_reports_the_demuxed_kind() {
        let mut demuxer = TsDemuxer::default();
        assert_eq!(demuxer.kind(), None);

        demuxer
            .push(&ts_segment(0x0F, &frames(1, 10)), &mut Vec::new())
            .unwrap();

        assert_eq!(demuxer.kind(), Some(EsKind::Adts));
    }

    #[test]
    fn ts_video_only_stream_is_rejected() {
        let mut segment = pat_packet(PMT_PID);
        segment.extend(pmt_packet(PMT_PID, &[(0x1B, 0x200)]));

        assert_eq!(demux(&segment), Err(HlsError::VideoOnly));
    }

    #[test]
    fn ts_picks_the_audio_stream_next_to_video() {
        let mut segment = pat_packet(PMT_PID);
        segment.extend(pmt_packet(PMT_PID, &[(0x1B, 0x200), (0x0F, AUDIO_PID)]));
        let frame = adts_frame(50, false, false);
        segment.extend(pes_packets(AUDIO_PID, 0, &frame));

        assert_eq!(demux(&segment).unwrap(), frame);
    }

    #[test]
    fn ts_undecodable_audio_names_the_codec() {
        for (stream_type, name) in [(0x11, "LATM AAC"), (0x81, "AC-3"), (0x87, "E-AC-3")] {
            let mut segment = pat_packet(PMT_PID);
            segment.extend(pmt_packet(PMT_PID, &[(stream_type, AUDIO_PID)]));

            assert_eq!(
                demux(&segment),
                Err(HlsError::UnsupportedCodec(name.to_string()))
            );
        }
    }

    #[test]
    fn ts_pmt_without_streams_has_no_audio() {
        let mut segment = pat_packet(PMT_PID);
        segment.extend(pmt_packet(PMT_PID, &[]));

        assert_eq!(demux(&segment), Err(HlsError::NoAudioVariant));
    }

    #[test]
    fn ts_output_is_the_same_wherever_the_input_is_split() {
        let segment = ts_segment(0x0F, &frames(4, 300));
        let expected = demux(&segment).unwrap();

        for split in (0..segment.len()).step_by(7) {
            let mut demuxer = TsDemuxer::default();
            let mut out = Vec::new();
            demuxer.push(&segment[..split], &mut out).unwrap();
            demuxer.push(&segment[split..], &mut out).unwrap();
            demuxer.flush(&mut out).unwrap();
            assert_eq!(out, expected, "split at {split}");
        }
    }

    #[test]
    fn ts_resyncs_after_leading_garbage() {
        let segment = ts_segment(0x0F, &frames(3, 80));
        let expected = demux(&segment).unwrap();
        let mut input = vec![0x00, 0x47, 0x13, 0x99, 0x01];
        input.extend_from_slice(&segment);

        assert_eq!(demux(&input).unwrap(), expected);
    }

    #[test]
    fn ts_continuity_gap_drops_only_the_damaged_frame() {
        let (a, b, c) = (
            adts_frame(100, false, false),
            adts_frame(400, false, false),
            adts_frame(100, false, false),
        );
        let mut segment = pat_packet(PMT_PID);
        segment.extend(pmt_packet(PMT_PID, &[(0x0F, AUDIO_PID)]));
        let a_packets = pes_packets(AUDIO_PID, 0, &a);
        let next_cc = (a_packets.len() / PACKET_SIZE) as u8;
        let mut b_packets = pes_packets(AUDIO_PID, next_cc, &b);
        let c_cc = (next_cc + (b_packets.len() / PACKET_SIZE) as u8) & 0x0F;
        let c_packets = pes_packets(AUDIO_PID, c_cc, &c);
        assert!(b_packets.len() >= 3 * PACKET_SIZE);
        b_packets.drain(PACKET_SIZE..2 * PACKET_SIZE);
        segment.extend(a_packets);
        segment.extend(b_packets);
        segment.extend(c_packets);

        let out = demux(&segment).unwrap();

        assert_eq!(out, [a, c].concat());
    }

    #[test]
    fn ts_corrupt_pmt_section_lengths_do_not_panic() {
        for section_length in 0..16u8 {
            let mut pmt = pmt_packet(PMT_PID, &[(0x0F, AUDIO_PID)]);
            // table_id 0x02 followed by the section-length high byte.
            let at = pmt.windows(2).position(|w| w == [0x02, 0xB0]).unwrap();
            pmt[at + 1] = 0xB0;
            pmt[at + 2] = section_length;
            let mut segment = pat_packet(PMT_PID);
            segment.extend(pmt);

            let _ = demux(&segment);
        }
    }

    #[test]
    fn ts_codec_change_in_a_later_pmt_is_an_error_not_silence() {
        let mut demuxer = TsDemuxer::default();
        let mut out = Vec::new();
        demuxer
            .push(&ts_segment(0x0F, &frames(1, 40)), &mut out)
            .unwrap();

        let err = demuxer
            .push(&ts_segment(0x03, &[mp3_frame(40)]), &mut out)
            .unwrap_err();

        assert_eq!(err, mixed_codecs(EsKind::Adts, EsKind::MpegAudio));
        assert!(err.to_string().contains("not supported"));
    }

    #[test]
    fn ts_repeated_identical_tables_are_harmless() {
        let first = ts_segment(0x0F, &frames(2, 50));
        let second = ts_segment(0x0F, &frames(2, 50));
        let mut demuxer = TsDemuxer::default();
        let mut out = Vec::new();

        demuxer.push(&first, &mut out).unwrap();
        demuxer.flush(&mut out).unwrap();
        demuxer.push(&second, &mut out).unwrap();
        demuxer.flush(&mut out).unwrap();

        assert_eq!(
            out,
            [frames(2, 50).concat(), frames(2, 50).concat()].concat()
        );
    }

    #[test]
    fn ts_reset_forgets_the_program_tables() {
        let mut demuxer = TsDemuxer::default();
        let mut out = Vec::new();
        demuxer
            .push(&ts_segment(0x0F, &frames(1, 10)), &mut out)
            .unwrap();
        assert!(demuxer.kind().is_some());

        demuxer.reset();

        assert_eq!(demuxer.kind(), None);
        let audio_only = pes_packets(AUDIO_PID, 0, &adts_frame(40, false, false));
        let mut after = Vec::new();
        demuxer.push(&audio_only, &mut after).unwrap();
        demuxer.flush(&mut after).unwrap();
        assert!(after.is_empty());
    }

    #[test]
    fn ts_ignores_packets_flagged_with_transport_errors() {
        let mut segment = ts_segment(0x0F, &frames(2, 60));
        let audio_start = 2 * PACKET_SIZE;
        segment[audio_start + 1] |= 0x80;

        let out = demux(&segment).unwrap();

        assert_eq!(out, frames(2, 60)[1].clone());
    }

    #[test]
    fn extractor_handles_ts_segments_and_records_the_kind() {
        let input = frames(3, 70);
        let mut extractor = AudioExtractor::default();
        let mut out = Vec::new();

        extractor
            .push_segment(&ts_segment(0x0F, &input), &mut out)
            .unwrap();

        assert_eq!(out, input.concat());
        assert_eq!(extractor.kind(), Some(EsKind::Adts));
    }

    #[test]
    fn extractor_strips_id3_from_packed_audio_segments() {
        let input = frames(3, 70);
        let mut extractor = AudioExtractor::default();
        let mut out = Vec::new();

        for _ in 0..2 {
            let mut segment = id3_tag(33);
            segment.extend(input.concat());
            extractor.push_segment(&segment, &mut out).unwrap();
        }

        assert_eq!(out, [input.concat(), input.concat()].concat());
    }

    #[test]
    fn extractor_passes_raw_mp3_through() {
        let mp3 = [mp3_frame(100), mp3_frame(100)].concat();
        let mut segment = id3_tag(8);
        segment.extend_from_slice(&mp3);
        let mut extractor = AudioExtractor::default();
        let mut out = Vec::new();

        extractor.push_segment(&segment, &mut out).unwrap();

        assert_eq!(out, mp3);
        assert_eq!(extractor.kind(), Some(EsKind::MpegAudio));
    }

    #[test]
    fn extractor_rejects_a_codec_change() {
        let mut extractor = AudioExtractor::default();
        let mut out = Vec::new();
        extractor
            .push_segment(&frames(1, 50).concat(), &mut out)
            .unwrap();

        let err = extractor
            .push_segment(&mp3_frame(50), &mut out)
            .unwrap_err();

        assert!(matches!(err, HlsError::UnsupportedCodec(_)));
        assert!(err.to_string().contains("not supported"));
    }

    #[test]
    fn extractor_rejects_unrecognised_segments() {
        let err = AudioExtractor::default()
            .push_segment(b"<html>404</html>", &mut Vec::new())
            .unwrap_err();

        assert_eq!(err, HlsError::Malformed("unrecognised segment data"));
    }

    mod property_tests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn demuxers_never_panic_on_arbitrary_bytes(
                data in proptest::collection::vec(any::<u8>(), 0..2000)
            ) {
                let mut out = Vec::new();
                let _ = TsDemuxer::default().push(&data, &mut out);
                let _ = AdtsNormalizer::default().push(&data, &mut out);
                let _ = AudioExtractor::default().push_segment(&data, &mut out);
                let _ = strip_id3(&data);
            }

            #[test]
            fn demuxers_never_panic_on_corrupted_valid_streams(
                payloads in proptest::collection::vec(1usize..400, 1..6),
                flips in proptest::collection::vec((0usize..4000, any::<u8>()), 0..12),
            ) {
                let input: Vec<Vec<u8>> =
                    payloads.iter().map(|len| adts_frame(*len, false, false)).collect();
                let mut segment = ts_segment(0x0F, &input);
                for (index, byte) in flips {
                    let at = index % segment.len();
                    segment[at] = byte;
                }
                let mut out = Vec::new();
                let _ = AudioExtractor::default().push_segment(&segment, &mut out);
            }

            #[test]
            fn output_is_independent_of_how_the_input_is_chunked(
                payloads in proptest::collection::vec(1usize..600, 1..6),
                cuts in proptest::collection::vec(0usize..4000, 0..6),
            ) {
                let input: Vec<Vec<u8>> =
                    payloads.iter().map(|len| adts_frame(*len, false, false)).collect();
                let segment = ts_segment(0x0F, &input);

                let mut cuts: Vec<usize> =
                    cuts.into_iter().map(|c| c % (segment.len() + 1)).collect();
                cuts.sort_unstable();

                let mut demuxer = TsDemuxer::default();
                let mut out = Vec::new();
                let mut start = 0;
                for cut in cuts {
                    demuxer.push(&segment[start..cut], &mut out).unwrap();
                    start = cut;
                }
                demuxer.push(&segment[start..], &mut out).unwrap();
                demuxer.flush(&mut out).unwrap();

                prop_assert_eq!(out, input.concat());
            }

            #[test]
            fn adts_normalizer_output_is_always_canonical_frames(
                payloads in proptest::collection::vec(0usize..500, 1..8),
                mpeg2 in any::<bool>(),
                crc in any::<bool>(),
            ) {
                let input: Vec<u8> = payloads
                    .iter()
                    .flat_map(|len| adts_frame(*len, mpeg2, crc))
                    .collect();
                let mut out = Vec::new();
                AdtsNormalizer::default().push(&input, &mut out).unwrap();

                let mut pos = 0;
                for len in &payloads {
                    prop_assert_eq!(out[pos], 0xFF);
                    prop_assert_eq!(out[pos + 1], 0xF1);
                    prop_assert_eq!(adts_len(&out[pos..]), 7 + len);
                    pos += 7 + len;
                }
                prop_assert_eq!(pos, out.len());
            }
        }
    }
}
