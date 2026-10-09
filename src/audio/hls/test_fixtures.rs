//! Builders for synthetic HLS segment data (tests only).

use super::ts::PACKET_SIZE;

/// An ADTS frame with a patterned payload.
/// `mpeg2` sets the ID bit (`0xFFF9`), `crc` adds the 2-byte CRC (`protection_absent` = 0).
pub(crate) fn adts_frame(payload_len: usize, mpeg2: bool, crc: bool) -> Vec<u8> {
    let header_len = if crc { 9 } else { 7 };
    let len = header_len + payload_len;
    let mut frame = vec![
        0xFF,
        0xF0 | (u8::from(mpeg2) << 3) | u8::from(!crc),
        0x50,
        0x80 | ((len >> 11) & 3) as u8,
        (len >> 3) as u8,
        (((len & 7) as u8) << 5) | 0x1F,
        0xFC,
    ];
    if crc {
        frame.extend_from_slice(&[0xAB, 0xCD]);
    }
    frame.extend((0..payload_len).map(|i| (i % 200) as u8));
    frame
}

/// An MPEG-1 Layer III frame header followed by a patterned payload.
pub(crate) fn mp3_frame(payload_len: usize) -> Vec<u8> {
    let mut frame = vec![0xFF, 0xFB, 0x90, 0x00];
    frame.extend((0..payload_len).map(|i| (i % 200) as u8));
    frame
}

/// An ID3v2.4 tag with `body_len` zero bytes after the header.
pub(crate) fn id3_tag(body_len: usize) -> Vec<u8> {
    let mut tag = vec![
        b'I',
        b'D',
        b'3',
        4,
        0,
        0,
        ((body_len >> 21) & 0x7F) as u8,
        ((body_len >> 14) & 0x7F) as u8,
        ((body_len >> 7) & 0x7F) as u8,
        (body_len & 0x7F) as u8,
    ];
    tag.extend(std::iter::repeat_n(0u8, body_len));
    tag
}

/// One 188-byte TS packet. Short payloads are padded with adaptation-field stuffing.
pub(crate) fn ts_packet(pid: u16, pusi: bool, cc: u8, payload: &[u8]) -> Vec<u8> {
    assert!(payload.len() <= PACKET_SIZE - 4);
    let mut packet = vec![0u8; PACKET_SIZE];
    packet[0] = 0x47;
    packet[1] = (u8::from(pusi) << 6) | ((pid >> 8) & 0x1F) as u8;
    packet[2] = pid as u8;
    let room = PACKET_SIZE - 4;
    if payload.len() == room {
        packet[3] = 0x10 | (cc & 0x0F);
        packet[4..].copy_from_slice(payload);
    } else {
        packet[3] = 0x30 | (cc & 0x0F);
        let af_len = room - 1 - payload.len();
        packet[4] = af_len as u8;
        if af_len > 0 {
            packet[5] = 0x00;
            packet[6..5 + af_len].fill(0xFF);
        }
        packet[PACKET_SIZE - payload.len()..].copy_from_slice(payload);
    }
    packet
}

pub(crate) fn pat_packet(pmt_pid: u16) -> Vec<u8> {
    let section = [
        0x00,
        0xB0,
        0x0D,
        0x00,
        0x01,
        0xC1,
        0x00,
        0x00,
        0x00,
        0x01,
        0xE0 | ((pmt_pid >> 8) & 0x1F) as u8,
        pmt_pid as u8,
        0,
        0,
        0,
        0,
    ];
    let mut payload = vec![0x00];
    payload.extend_from_slice(&section);
    ts_packet(0, true, 0, &payload)
}

/// A PMT announcing `streams` as `(stream_type, pid)` pairs.
pub(crate) fn pmt_packet(pmt_pid: u16, streams: &[(u8, u16)]) -> Vec<u8> {
    let section_length = 9 + 5 * streams.len() + 4;
    let mut section = vec![
        0x02,
        0xB0 | ((section_length >> 8) & 0x0F) as u8,
        section_length as u8,
        0x00,
        0x01,
        0xC1,
        0x00,
        0x00,
        0xE1,
        0x00,
        0xF0,
        0x00,
    ];
    for (stream_type, pid) in streams {
        section.extend_from_slice(&[
            *stream_type,
            0xE0 | ((pid >> 8) & 0x1F) as u8,
            *pid as u8,
            0xF0,
            0x00,
        ]);
    }
    section.extend_from_slice(&[0, 0, 0, 0]);
    let mut payload = vec![0x00];
    payload.extend_from_slice(&section);
    ts_packet(pmt_pid, true, 0, &payload)
}

/// `es` wrapped in one PES and split into TS packets for `pid`, continuity
/// counters starting at `cc`.
pub(crate) fn pes_packets(pid: u16, mut cc: u8, es: &[u8]) -> Vec<u8> {
    let mut pes = vec![0x00, 0x00, 0x01, 0xC0, 0x00, 0x00, 0x80, 0x00, 0x00];
    pes.extend_from_slice(es);

    let mut out = Vec::new();
    for (index, chunk) in pes.chunks(PACKET_SIZE - 4).enumerate() {
        out.extend(ts_packet(pid, index == 0, cc, chunk));
        cc = (cc + 1) & 0x0F;
    }
    out
}

/// PAT + PMT + the given audio frames, one PES per frame.
pub(crate) fn ts_segment(stream_type: u8, frames: &[Vec<u8>]) -> Vec<u8> {
    const PMT_PID: u16 = 0x100;
    const AUDIO_PID: u16 = 0x101;
    let mut segment = pat_packet(PMT_PID);
    segment.extend(pmt_packet(PMT_PID, &[(stream_type, AUDIO_PID)]));
    let mut cc = 0;
    for frame in frames {
        let packets = pes_packets(AUDIO_PID, cc, frame);
        cc = (cc + (packets.len() / PACKET_SIZE) as u8) & 0x0F;
        segment.extend(packets);
    }
    segment
}

pub(crate) const AUDIO_PID: u16 = 0x101;
pub(crate) const PMT_PID: u16 = 0x100;
