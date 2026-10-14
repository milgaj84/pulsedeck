//! Fragmented MP4 (fMP4/CMAF) audio segments. Pure: bytes in, bytes out.
//!
//! The init segment (`#EXT-X-MAP`) says what the audio is; each media segment
//! holds `moof`/`mdat` fragments with the samples. The samples are re-emitted in
//! the same elementary-stream form the MPEG-TS path produces, so nothing
//! downstream changes:
//!
//! - AAC: canonical ADTS frames (the header is rebuilt from the
//!   AudioSpecificConfig in the init segment)
//! - MP3: the samples as they are
//! - Opus: an Ogg Opus stream (one packet per page)
//!
//! Every length and offset in the data is untrusted, so all reads are bounds
//! checked and malformed input is an error, never a panic.

use super::playlist::HlsError;
use super::ts::EsKind;

const TRUNCATED: HlsError = HlsError::Malformed("truncated MP4 data");
const BAD_BOX: HlsError = HlsError::Malformed("invalid MP4 box");

/// More samples than this in one `trun` is not a real audio fragment.
const MAX_SAMPLES_PER_RUN: u32 = 100_000;
/// The largest frame an ADTS header can describe (13-bit length field).
const MAX_ADTS_FRAME: usize = 0x1FFF;
/// Opus always counts time in 48 kHz units in Ogg.
const OPUS_RATE: u64 = 48_000;
/// An Opus frame is 20 ms unless the fragment says otherwise.
const DEFAULT_OPUS_SAMPLES: u64 = 960;

// ---------------------------------------------------------------------------
// Boxes
// ---------------------------------------------------------------------------

struct Mp4Box<'a> {
    kind: [u8; 4],
    /// Offset of the box header from the start of the data it was found in.
    start: usize,
    payload: &'a [u8],
}

/// Split `data` into its sibling boxes.
fn parse_boxes(data: &[u8]) -> Result<Vec<Mp4Box<'_>>, HlsError> {
    let mut boxes = Vec::new();
    let mut pos = 0;
    while pos < data.len() {
        let rest = &data[pos..];
        let size32 = u32::from_be_bytes(
            rest.get(..4)
                .ok_or(TRUNCATED)?
                .try_into()
                .map_err(|_| TRUNCATED)?,
        );
        let kind: [u8; 4] = rest
            .get(4..8)
            .ok_or(TRUNCATED)?
            .try_into()
            .map_err(|_| TRUNCATED)?;
        let (header, size) = match size32 {
            // Runs to the end of the data.
            0 => (8, rest.len() as u64),
            1 => {
                let large: [u8; 8] = rest
                    .get(8..16)
                    .ok_or(TRUNCATED)?
                    .try_into()
                    .map_err(|_| TRUNCATED)?;
                (16, u64::from_be_bytes(large))
            }
            n => (8, u64::from(n)),
        };
        if size < header as u64 || size > rest.len() as u64 {
            return Err(BAD_BOX);
        }
        let size = size as usize;
        boxes.push(Mp4Box {
            kind,
            start: pos,
            payload: &rest[header..size],
        });
        pos += size;
    }
    Ok(boxes)
}

fn find<'a, 'b>(boxes: &'b [Mp4Box<'a>], kind: &[u8; 4]) -> Option<&'b Mp4Box<'a>> {
    boxes.iter().find(|b| &b.kind == kind)
}

/// Big-endian reads that fail with `None` instead of panicking.
struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    fn take(&mut self, len: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(len)?;
        let bytes = self.buf.get(self.pos..end)?;
        self.pos = end;
        Some(bytes)
    }

    fn skip(&mut self, len: usize) -> Option<()> {
        self.take(len).map(|_| ())
    }

    fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|b| b[0])
    }

    fn u16(&mut self) -> Option<u16> {
        self.take(2).map(|b| u16::from_be_bytes([b[0], b[1]]))
    }

    fn u32(&mut self) -> Option<u32> {
        self.take(4)
            .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn u64(&mut self) -> Option<u64> {
        let bytes: [u8; 8] = self.take(8)?.try_into().ok()?;
        Some(u64::from_be_bytes(bytes))
    }

    fn rest(&self) -> &'a [u8] {
        self.buf.get(self.pos..).unwrap_or(&[])
    }
}

/// `version` (1 byte) and 24 bits of flags that start most boxes.
fn version_flags(reader: &mut Reader<'_>) -> Option<(u8, u32)> {
    let word = reader.u32()?;
    Some(((word >> 24) as u8, word & 0x00FF_FFFF))
}

// ---------------------------------------------------------------------------
// Init segment
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
enum Codec {
    Aac {
        /// ADTS `profile`: the audio object type minus one.
        profile: u8,
        frequency_index: u8,
        channel_config: u8,
    },
    Mp3,
    /// A complete `OpusHead` packet.
    Opus {
        head: Vec<u8>,
    },
}

/// Defaults from the `trex` box that fragments fall back to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct TrackDefaults {
    duration: Option<u32>,
    size: Option<u32>,
}

/// What an init segment says about its audio track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Fmp4Init {
    track_id: u32,
    timescale: u32,
    codec: Codec,
    defaults: TrackDefaults,
}

impl Fmp4Init {
    /// The elementary stream the samples are turned into.
    pub(crate) fn kind(&self) -> EsKind {
        match self.codec {
            Codec::Aac { .. } => EsKind::Adts,
            Codec::Mp3 => EsKind::MpegAudio,
            Codec::Opus { .. } => EsKind::OggOpus,
        }
    }

    pub(crate) fn parse(init: &[u8]) -> Result<Self, HlsError> {
        let top = parse_boxes(init)?;
        let moov = find(&top, b"moov").ok_or(HlsError::Malformed("init segment without moov"))?;
        let moov_boxes = parse_boxes(moov.payload)?;

        let mut audio = None;
        for trak in moov_boxes.iter().filter(|b| &b.kind == b"trak") {
            let trak_boxes = parse_boxes(trak.payload)?;
            let mdia =
                find(&trak_boxes, b"mdia").ok_or(HlsError::Malformed("trak without mdia"))?;
            let mdia_boxes = parse_boxes(mdia.payload)?;
            if handler_type(&mdia_boxes) == Some(*b"soun") {
                audio = Some((trak_boxes, mdia_boxes));
                break;
            }
        }
        let (trak_boxes, mdia_boxes) = audio.ok_or(HlsError::VideoOnly)?;

        let track_id = find(&trak_boxes, b"tkhd")
            .and_then(|tkhd| track_id(tkhd.payload))
            .ok_or(HlsError::Malformed("track without an id"))?;
        let timescale = find(&mdia_boxes, b"mdhd")
            .and_then(|mdhd| timescale(mdhd.payload))
            .filter(|&scale| scale > 0)
            .ok_or(HlsError::Malformed("track without a timescale"))?;

        let minf = find(&mdia_boxes, b"minf").ok_or(HlsError::Malformed("track without minf"))?;
        let minf_boxes = parse_boxes(minf.payload)?;
        let stbl = find(&minf_boxes, b"stbl").ok_or(HlsError::Malformed("track without stbl"))?;
        let stbl_boxes = parse_boxes(stbl.payload)?;
        let stsd = find(&stbl_boxes, b"stsd").ok_or(HlsError::Malformed("track without stsd"))?;

        let mut reader = Reader::new(stsd.payload);
        version_flags(&mut reader).ok_or(TRUNCATED)?;
        reader.u32().ok_or(TRUNCATED)?; // entry count
        let entries = parse_boxes(reader.rest())?;
        let entry = entries
            .first()
            .ok_or(HlsError::Malformed("track without a sample entry"))?;

        Ok(Self {
            track_id,
            timescale,
            codec: parse_sample_entry(entry)?,
            defaults: track_defaults(&moov_boxes, track_id),
        })
    }
}

fn handler_type(mdia: &[Mp4Box<'_>]) -> Option<[u8; 4]> {
    let hdlr = find(mdia, b"hdlr")?;
    let mut reader = Reader::new(hdlr.payload);
    reader.skip(8)?; // version/flags, pre_defined
    reader.take(4)?.try_into().ok()
}

fn track_id(tkhd: &[u8]) -> Option<u32> {
    let mut reader = Reader::new(tkhd);
    let (version, _) = version_flags(&mut reader)?;
    // creation and modification times are 4 bytes each in version 0, 8 in version 1.
    reader.skip(if version == 1 { 16 } else { 8 })?;
    reader.u32()
}

fn timescale(mdhd: &[u8]) -> Option<u32> {
    let mut reader = Reader::new(mdhd);
    let (version, _) = version_flags(&mut reader)?;
    reader.skip(if version == 1 { 16 } else { 8 })?;
    reader.u32()
}

fn track_defaults(moov: &[Mp4Box<'_>], track_id: u32) -> TrackDefaults {
    let Some(mvex) = find(moov, b"mvex") else {
        return TrackDefaults::default();
    };
    let Ok(mvex_boxes) = parse_boxes(mvex.payload) else {
        return TrackDefaults::default();
    };
    for trex in mvex_boxes.iter().filter(|b| &b.kind == b"trex") {
        let mut reader = Reader::new(trex.payload);
        let fields = (|| {
            version_flags(&mut reader)?;
            let id = reader.u32()?;
            reader.u32()?; // default sample description index
            Some((id, reader.u32()?, reader.u32()?))
        })();
        if let Some((id, duration, size)) = fields {
            if id == track_id {
                return TrackDefaults {
                    duration: (duration != 0).then_some(duration),
                    size: (size != 0).then_some(size),
                };
            }
        }
    }
    TrackDefaults::default()
}

fn parse_sample_entry(entry: &Mp4Box<'_>) -> Result<Codec, HlsError> {
    // Audio sample entries: 6 reserved bytes, a data reference index, then
    // version, channel count, sample size and rate: 28 bytes before the
    // child boxes.
    let children = || {
        entry
            .payload
            .get(28..)
            .ok_or(TRUNCATED)
            .and_then(parse_boxes)
    };
    match &entry.kind {
        b"mp4a" => {
            let children = children()?;
            let esds =
                find(&children, b"esds").ok_or(HlsError::Malformed("mp4a entry without esds"))?;
            let (object_type, config) = parse_esds(esds.payload)?;
            match object_type {
                0x40 => parse_aac_config(&config),
                0x69 | 0x6B => Ok(Codec::Mp3),
                other => Err(HlsError::UnsupportedCodec(format!(
                    "MPEG-4 audio (object type 0x{other:02X})"
                ))),
            }
        }
        b"Opus" => {
            let children = children()?;
            let dops =
                find(&children, b"dOps").ok_or(HlsError::Malformed("Opus entry without dOps"))?;
            Ok(Codec::Opus {
                head: opus_head(dops.payload)?,
            })
        }
        other => Err(HlsError::UnsupportedCodec(
            String::from_utf8_lossy(other).trim().to_string(),
        )),
    }
}

/// One MPEG-4 descriptor: its tag, body, and what follows it.
fn read_descriptor(buf: &[u8]) -> Option<(u8, &[u8], &[u8])> {
    let mut reader = Reader::new(buf);
    let tag = reader.u8()?;
    let mut len = 0usize;
    for _ in 0..4 {
        let byte = reader.u8()?;
        len = (len << 7) | usize::from(byte & 0x7F);
        if byte & 0x80 == 0 {
            break;
        }
    }
    let body = reader.take(len)?;
    Some((tag, body, reader.rest()))
}

/// The object type indication and decoder-specific info of an `esds` box.
fn parse_esds(esds: &[u8]) -> Result<(u8, Vec<u8>), HlsError> {
    const BAD: HlsError = HlsError::Malformed("invalid esds");
    let mut reader = Reader::new(esds);
    version_flags(&mut reader).ok_or(BAD)?;
    let (tag, es, _) = read_descriptor(reader.rest()).ok_or(BAD)?;
    if tag != 0x03 {
        return Err(BAD);
    }

    let mut es = Reader::new(es);
    es.skip(2).ok_or(BAD)?; // ES_ID
    let flags = es.u8().ok_or(BAD)?;
    if flags & 0x80 != 0 {
        es.skip(2).ok_or(BAD)?; // dependsOn_ES_ID
    }
    if flags & 0x40 != 0 {
        let url_len = es.u8().ok_or(BAD)?;
        es.skip(usize::from(url_len)).ok_or(BAD)?;
    }
    if flags & 0x20 != 0 {
        es.skip(2).ok_or(BAD)?; // OCR_ES_ID
    }

    let (tag, decoder_config, _) = read_descriptor(es.rest()).ok_or(BAD)?;
    if tag != 0x04 {
        return Err(BAD);
    }
    let mut config = Reader::new(decoder_config);
    let object_type = config.u8().ok_or(BAD)?;
    config.skip(12).ok_or(BAD)?; // stream type, buffer size, max and average bitrate
    let specific = match read_descriptor(config.rest()) {
        Some((0x05, body, _)) => body.to_vec(),
        _ => Vec::new(),
    };
    Ok((object_type, specific))
}

/// Reads bits most-significant first.
struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Bits<'_> {
    fn read(&mut self, count: usize) -> Option<u32> {
        let mut value = 0u32;
        for _ in 0..count {
            let byte = *self.data.get(self.pos / 8)?;
            value = (value << 1) | u32::from((byte >> (7 - self.pos % 8)) & 1);
            self.pos += 1;
        }
        Some(value)
    }

    fn audio_object_type(&mut self) -> Option<u32> {
        let object_type = self.read(5)?;
        if object_type == 31 {
            Some(32 + self.read(6)?)
        } else {
            Some(object_type)
        }
    }

    fn frequency_index(&mut self) -> Option<u32> {
        let index = self.read(4)?;
        if index == 15 {
            self.read(24)?; // explicit frequency: ADTS cannot say it
        }
        Some(index)
    }
}

/// The parts of an AudioSpecificConfig an ADTS header can express.
fn parse_aac_config(config: &[u8]) -> Result<Codec, HlsError> {
    const BAD: HlsError = HlsError::Malformed("invalid AudioSpecificConfig");
    let mut bits = Bits {
        data: config,
        pos: 0,
    };
    let mut object_type = bits.audio_object_type().ok_or(BAD)?;
    let frequency_index = bits.frequency_index().ok_or(BAD)?;
    let channel_config = bits.read(4).ok_or(BAD)?;
    if object_type == 5 || object_type == 29 {
        // Explicit SBR/PS: the AAC-LC core follows. Symphonia decodes only the
        // core, exactly as for HE-AAC in MPEG-TS.
        bits.frequency_index().ok_or(BAD)?;
        object_type = bits.audio_object_type().ok_or(BAD)?;
    }

    if !(1..=4).contains(&object_type) {
        return Err(HlsError::UnsupportedCodec(format!(
            "AAC (audio object type {object_type})"
        )));
    }
    if frequency_index >= 13 {
        return Err(HlsError::UnsupportedCodec(
            "AAC with an unusual sampling frequency".to_string(),
        ));
    }
    if channel_config == 0 {
        return Err(HlsError::UnsupportedCodec(
            "AAC with a program config element".to_string(),
        ));
    }
    Ok(Codec::Aac {
        profile: (object_type - 1) as u8,
        frequency_index: frequency_index as u8,
        channel_config: channel_config as u8,
    })
}

/// Rebuild the `OpusHead` packet from an ISO `dOps` box (which differs only in
/// byte order and in having no magic).
fn opus_head(dops: &[u8]) -> Result<Vec<u8>, HlsError> {
    const BAD: HlsError = HlsError::Malformed("invalid dOps");
    let mut reader = Reader::new(dops);
    reader.u8().ok_or(BAD)?; // version
    let channels = reader.u8().ok_or(BAD)?;
    let pre_skip = reader.u16().ok_or(BAD)?;
    let rate = reader.u32().ok_or(BAD)?;
    let gain = reader.u16().ok_or(BAD)?;
    let family = reader.u8().ok_or(BAD)?;

    let mut head = b"OpusHead".to_vec();
    head.push(1);
    head.push(channels);
    head.extend_from_slice(&pre_skip.to_le_bytes());
    head.extend_from_slice(&rate.to_le_bytes());
    head.extend_from_slice(&gain.to_le_bytes());
    head.push(family);
    if family != 0 {
        // Stream count, coupled count and the channel mapping, copied as they are.
        head.extend_from_slice(reader.rest());
    }
    Ok(head)
}

// ---------------------------------------------------------------------------
// Media segments
// ---------------------------------------------------------------------------

struct Sample<'a> {
    data: &'a [u8],
    /// In track timescale units, when the fragment says.
    duration: Option<u32>,
}

#[derive(Default)]
struct TrackFragmentHeader {
    flags: u32,
    track_id: u32,
    base_data_offset: Option<u64>,
    default_duration: Option<u32>,
    default_size: Option<u32>,
}

fn parse_tfhd(payload: &[u8]) -> Option<TrackFragmentHeader> {
    let mut reader = Reader::new(payload);
    let (_, flags) = version_flags(&mut reader)?;
    let mut header = TrackFragmentHeader {
        flags,
        track_id: reader.u32()?,
        ..Default::default()
    };
    if flags & 0x01 != 0 {
        header.base_data_offset = Some(reader.u64()?);
    }
    if flags & 0x02 != 0 {
        reader.u32()?; // sample description index
    }
    if flags & 0x08 != 0 {
        header.default_duration = Some(reader.u32()?);
    }
    if flags & 0x10 != 0 {
        header.default_size = Some(reader.u32()?);
    }
    Some(header)
}

/// All samples of our track in `segment`, in order.
fn collect_samples<'a>(init: &Fmp4Init, segment: &'a [u8]) -> Result<Vec<Sample<'a>>, HlsError> {
    let mut samples = Vec::new();
    for moof in parse_boxes(segment)?.iter().filter(|b| &b.kind == b"moof") {
        for traf in parse_boxes(moof.payload)?
            .iter()
            .filter(|b| &b.kind == b"traf")
        {
            let traf_boxes = parse_boxes(traf.payload)?;
            let tfhd = find(&traf_boxes, b"tfhd")
                .and_then(|tfhd| parse_tfhd(tfhd.payload))
                .ok_or(HlsError::Malformed("fragment without tfhd"))?;
            if tfhd.track_id != init.track_id {
                continue;
            }
            // Offsets count from the start of the moof, unless the header
            // names an absolute base (not used by CMAF).
            let base = match tfhd.base_data_offset {
                Some(offset) if tfhd.flags & 0x02_0000 == 0 => {
                    usize::try_from(offset).map_err(|_| BAD_BOX)?
                }
                _ => moof.start,
            };
            let mut cursor = base;
            for trun in traf_boxes.iter().filter(|b| &b.kind == b"trun") {
                cursor = read_run(
                    init,
                    &tfhd,
                    trun.payload,
                    segment,
                    base,
                    cursor,
                    &mut samples,
                )?;
            }
        }
    }
    Ok(samples)
}

/// Read one `trun`; returns where its sample data ends.
fn read_run<'a>(
    init: &Fmp4Init,
    tfhd: &TrackFragmentHeader,
    payload: &[u8],
    segment: &'a [u8],
    base: usize,
    cursor: usize,
    samples: &mut Vec<Sample<'a>>,
) -> Result<usize, HlsError> {
    let mut reader = Reader::new(payload);
    let (_, flags) = version_flags(&mut reader).ok_or(TRUNCATED)?;
    let count = reader.u32().ok_or(TRUNCATED)?;
    if count > MAX_SAMPLES_PER_RUN {
        return Err(HlsError::Malformed("implausible sample count"));
    }
    let data_offset = if flags & 0x01 != 0 {
        Some(reader.u32().ok_or(TRUNCATED)? as i32)
    } else {
        None
    };
    if flags & 0x04 != 0 {
        reader.u32().ok_or(TRUNCATED)?; // first sample flags
    }

    let mut pos = match data_offset {
        Some(offset) => i64::try_from(base)
            .ok()
            .and_then(|base| base.checked_add(i64::from(offset)))
            .and_then(|start| usize::try_from(start).ok())
            .ok_or(BAD_BOX)?,
        None => cursor,
    };

    for _ in 0..count {
        let duration = if flags & 0x100 != 0 {
            Some(reader.u32().ok_or(TRUNCATED)?)
        } else {
            tfhd.default_duration.or(init.defaults.duration)
        };
        let size = if flags & 0x200 != 0 {
            reader.u32().ok_or(TRUNCATED)?
        } else {
            tfhd.default_size
                .or(init.defaults.size)
                .ok_or(HlsError::Malformed("fragment sample without a size"))?
        };
        if flags & 0x400 != 0 {
            reader.u32().ok_or(TRUNCATED)?; // sample flags
        }
        if flags & 0x800 != 0 {
            reader.u32().ok_or(TRUNCATED)?; // composition time offset
        }

        let end = usize::try_from(size)
            .ok()
            .and_then(|size| pos.checked_add(size))
            .ok_or(BAD_BOX)?;
        let data = segment.get(pos..end).ok_or(TRUNCATED)?;
        samples.push(Sample { data, duration });
        pos = end;
    }
    Ok(pos)
}

// ---------------------------------------------------------------------------
// Output
// ---------------------------------------------------------------------------

/// A 7-byte ADTS header (MPEG-4, no CRC) in the canonical form Symphonia reads.
fn adts_header(profile: u8, frequency_index: u8, channel_config: u8, frame_len: usize) -> [u8; 7] {
    [
        0xFF,
        0xF1,
        (profile << 6) | (frequency_index << 2) | (channel_config >> 2),
        ((channel_config & 3) << 6) | ((frame_len >> 11) & 3) as u8,
        (frame_len >> 3) as u8,
        (((frame_len & 7) as u8) << 5) | 0x1F,
        0xFC,
    ]
}

/// Writes raw Opus packets as an Ogg Opus stream, one packet per page.
#[derive(Debug, Default)]
struct OggOpusMux {
    started: bool,
    serial: u32,
    sequence: u32,
    granule: u64,
    head: Vec<u8>,
}

impl OggOpusMux {
    fn reset(&mut self) {
        self.started = false;
    }

    fn start(&mut self, head: &[u8], out: &mut Vec<u8>) {
        self.started = true;
        self.serial = self.serial.wrapping_add(1);
        self.sequence = 0;
        self.granule = 0;
        self.head = head.to_vec();
        self.page(0x02, 0, head, out);
        let mut tags = b"OpusTags".to_vec();
        tags.extend_from_slice(&0u32.to_le_bytes()); // vendor string length
        tags.extend_from_slice(&0u32.to_le_bytes()); // comment count
        self.page(0x00, 0, &tags, out);
    }

    fn packet(&mut self, data: &[u8], samples_48k: u64, out: &mut Vec<u8>) {
        self.granule += samples_48k;
        self.page(0x00, self.granule, data, out);
    }

    fn page(&mut self, header_type: u8, granule: u64, packet: &[u8], out: &mut Vec<u8>) {
        // Lacing: runs of 255 bytes, then a final value below 255 (zero if the
        // packet is an exact multiple).
        let mut lacing = vec![255u8; packet.len() / 255];
        lacing.push((packet.len() % 255) as u8);

        let start = out.len();
        out.extend_from_slice(b"OggS");
        out.push(0); // version
        out.push(header_type);
        out.extend_from_slice(&granule.to_le_bytes());
        out.extend_from_slice(&self.serial.to_le_bytes());
        out.extend_from_slice(&self.sequence.to_le_bytes());
        out.extend_from_slice(&[0; 4]); // checksum, filled in below
        out.push(lacing.len() as u8);
        out.extend_from_slice(&lacing);
        out.extend_from_slice(packet);
        let crc = ogg_crc(&out[start..]);
        out[start + 22..start + 26].copy_from_slice(&crc.to_le_bytes());
        self.sequence += 1;
    }
}

/// Ogg's page checksum: CRC-32 (polynomial 0x04C11DB7, no reflection, initial
/// value 0) over the page with its checksum field zeroed.
fn ogg_crc(page: &[u8]) -> u32 {
    let mut crc = 0u32;
    for &byte in page {
        crc ^= u32::from(byte) << 24;
        for _ in 0..8 {
            crc = if crc & 0x8000_0000 != 0 {
                (crc << 1) ^ 0x04C1_1DB7
            } else {
                crc << 1
            };
        }
    }
    crc
}

/// Turns fMP4 media segments into an elementary stream.
#[derive(Debug, Default)]
pub(crate) struct Fmp4Demuxer {
    opus: OggOpusMux,
}

impl Fmp4Demuxer {
    /// Call on a discontinuity or a gap in the segments.
    pub(crate) fn reset(&mut self) {
        self.opus.reset();
    }

    pub(crate) fn push(
        &mut self,
        init: &Fmp4Init,
        segment: &[u8],
        out: &mut Vec<u8>,
    ) -> Result<(), HlsError> {
        let samples = collect_samples(init, segment)?;
        match &init.codec {
            Codec::Aac {
                profile,
                frequency_index,
                channel_config,
            } => {
                for sample in samples {
                    let frame_len = sample.data.len() + 7;
                    if frame_len > MAX_ADTS_FRAME {
                        return Err(HlsError::Malformed("AAC frame too large"));
                    }
                    out.extend_from_slice(&adts_header(
                        *profile,
                        *frequency_index,
                        *channel_config,
                        frame_len,
                    ));
                    out.extend_from_slice(sample.data);
                }
            }
            Codec::Mp3 => {
                for sample in samples {
                    out.extend_from_slice(sample.data);
                }
            }
            Codec::Opus { head } => {
                // A new logical stream when the parameters change or after a gap.
                if !self.opus.started || self.opus.head != *head {
                    self.opus.start(head, out);
                }
                for sample in samples {
                    let samples_48k = sample.duration.map_or(DEFAULT_OPUS_SAMPLES, |duration| {
                        u64::from(duration) * OPUS_RATE / u64::from(init.timescale)
                    });
                    self.opus.packet(sample.data, samples_48k, out);
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::ts::AudioExtractor;
    use super::*;
    use crate::audio::pcm_source::FrameBackend;
    use crate::audio::symphonia_backend::SymphoniaBackend;
    use proptest::prelude::*;
    use std::io::Cursor;
    use symphonia::core::io::ReadOnlySource;

    const AAC_INIT: &[u8] = include_bytes!("testdata/fmp4/aac/init.mp4");
    const AAC_SEGMENTS: [&[u8]; 5] = [
        include_bytes!("testdata/fmp4/aac/seg0.m4s"),
        include_bytes!("testdata/fmp4/aac/seg1.m4s"),
        include_bytes!("testdata/fmp4/aac/seg2.m4s"),
        include_bytes!("testdata/fmp4/aac/seg3.m4s"),
        include_bytes!("testdata/fmp4/aac/seg4.m4s"),
    ];
    const TS_SEGMENTS: [&[u8]; 5] = [
        include_bytes!("testdata/fmp4/ts/seg0.ts"),
        include_bytes!("testdata/fmp4/ts/seg1.ts"),
        include_bytes!("testdata/fmp4/ts/seg2.ts"),
        include_bytes!("testdata/fmp4/ts/seg3.ts"),
        include_bytes!("testdata/fmp4/ts/seg4.ts"),
    ];
    const OPUS_INIT: &[u8] = include_bytes!("testdata/fmp4/opus/init.mp4");
    const OPUS_SEGMENTS: [&[u8]; 4] = [
        include_bytes!("testdata/fmp4/opus/seg0.m4s"),
        include_bytes!("testdata/fmp4/opus/seg1.m4s"),
        include_bytes!("testdata/fmp4/opus/seg2.m4s"),
        include_bytes!("testdata/fmp4/opus/seg3.m4s"),
    ];

    // --- A small MP4 writer for hand-made init and media segments. ----------

    fn mp4_box(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut bytes = ((payload.len() + 8) as u32).to_be_bytes().to_vec();
        bytes.extend_from_slice(kind);
        bytes.extend_from_slice(payload);
        bytes
    }

    fn full_box(kind: &[u8; 4], version: u8, flags: u32, payload: &[u8]) -> Vec<u8> {
        let mut body = vec![version];
        body.extend_from_slice(&flags.to_be_bytes()[1..]);
        body.extend_from_slice(payload);
        mp4_box(kind, &body)
    }

    fn descriptor(tag: u8, body: &[u8]) -> Vec<u8> {
        assert!(body.len() < 128);
        let mut bytes = vec![tag, body.len() as u8];
        bytes.extend_from_slice(body);
        bytes
    }

    fn esds(object_type: u8, config: &[u8]) -> Vec<u8> {
        let mut decoder = vec![object_type, 0x15, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        decoder.extend(descriptor(0x05, config));
        let mut es = vec![0, 0, 0]; // ES_ID and flags
        es.extend(descriptor(0x04, &decoder));
        full_box(b"esds", 0, 0, &descriptor(0x03, &es))
    }

    /// An audio sample entry: 28 bytes of fixed fields, then child boxes.
    fn audio_entry(kind: &[u8; 4], children: &[u8]) -> Vec<u8> {
        let mut payload = vec![0; 6];
        payload.extend_from_slice(&1u16.to_be_bytes()); // data reference index
        payload.extend_from_slice(&[0; 8]);
        payload.extend_from_slice(&2u16.to_be_bytes()); // channels
        payload.extend_from_slice(&16u16.to_be_bytes()); // sample size
        payload.extend_from_slice(&[0; 4]);
        payload.extend_from_slice(&(48_000u32 << 16).to_be_bytes());
        payload.extend_from_slice(children);
        mp4_box(kind, &payload)
    }

    fn init_segment(handler: &[u8; 4], entry: &[u8], trex: Option<(u32, u32)>) -> Vec<u8> {
        let mut tkhd = vec![0; 8];
        tkhd.extend_from_slice(&1u32.to_be_bytes()); // track id
        tkhd.extend_from_slice(&[0; 68]);
        let mut mdhd = vec![0; 8];
        mdhd.extend_from_slice(&48_000u32.to_be_bytes());
        mdhd.extend_from_slice(&[0; 8]);
        let mut hdlr = vec![0; 4];
        hdlr.extend_from_slice(handler);
        hdlr.extend_from_slice(&[0; 13]);
        let mut stsd = 1u32.to_be_bytes().to_vec();
        stsd.extend_from_slice(entry);

        let stbl = mp4_box(b"stbl", &full_box(b"stsd", 0, 0, &stsd));
        let minf = mp4_box(b"minf", &stbl);
        let mut mdia = full_box(b"mdhd", 0, 0, &mdhd);
        mdia.extend(full_box(b"hdlr", 0, 0, &hdlr));
        mdia.extend(minf);
        let mut trak = full_box(b"tkhd", 0, 3, &tkhd);
        trak.extend(mp4_box(b"mdia", &mdia));
        let mut moov = mp4_box(b"trak", &trak);
        if let Some((duration, size)) = trex {
            let mut payload = 1u32.to_be_bytes().to_vec();
            payload.extend_from_slice(&1u32.to_be_bytes());
            payload.extend_from_slice(&duration.to_be_bytes());
            payload.extend_from_slice(&size.to_be_bytes());
            payload.extend_from_slice(&[0; 4]);
            moov.extend(mp4_box(b"mvex", &full_box(b"trex", 0, 0, &payload)));
        }
        let mut init = mp4_box(b"ftyp", b"isom\0\0\0\0isom");
        init.extend(mp4_box(b"moov", &moov));
        init
    }

    fn init_for(codec: Codec) -> Fmp4Init {
        Fmp4Init {
            track_id: 1,
            timescale: 48_000,
            codec,
            defaults: TrackDefaults::default(),
        }
    }

    /// A `trun`. `fields` per sample: (duration, size).
    fn trun(data_offset: Option<i32>, samples: &[(Option<u32>, Option<u32>)]) -> Vec<u8> {
        let has_duration = samples.iter().any(|(d, _)| d.is_some());
        let has_size = samples.iter().any(|(_, s)| s.is_some());
        let mut flags = 0u32;
        if data_offset.is_some() {
            flags |= 0x1;
        }
        if has_duration {
            flags |= 0x100;
        }
        if has_size {
            flags |= 0x200;
        }
        let mut payload = (samples.len() as u32).to_be_bytes().to_vec();
        if let Some(offset) = data_offset {
            payload.extend_from_slice(&offset.to_be_bytes());
        }
        for (duration, size) in samples {
            if has_duration {
                payload.extend_from_slice(&duration.unwrap_or(0).to_be_bytes());
            }
            if has_size {
                payload.extend_from_slice(&size.unwrap_or(0).to_be_bytes());
            }
        }
        full_box(b"trun", 0, flags, &payload)
    }

    fn tfhd(track_id: u32, default_size: Option<u32>) -> Vec<u8> {
        let mut payload = track_id.to_be_bytes().to_vec();
        let mut flags = 0x02_0000; // default base is moof
        if let Some(size) = default_size {
            flags |= 0x10;
            payload.extend_from_slice(&size.to_be_bytes());
        }
        full_box(b"tfhd", 0, flags, &payload)
    }

    /// A `moof` + `mdat`. `data_offset` is `Some` to write it into the run.
    fn fragment(track_id: u32, default_size: Option<u32>, run: &[u8], data: &[u8]) -> Vec<u8> {
        let mut traf = tfhd(track_id, default_size);
        traf.extend_from_slice(run);
        let moof = mp4_box(b"moof", &mp4_box(b"traf", &traf));
        let mut fragment = moof;
        fragment.extend(mp4_box(b"mdat", data));
        fragment
    }

    /// The size of the `moof` that `fragment` would write, for data offsets.
    fn data_offset_of(track_id: u32, default_size: Option<u32>, run: &[u8]) -> i32 {
        let mut traf = tfhd(track_id, default_size);
        traf.extend_from_slice(run);
        (mp4_box(b"moof", &mp4_box(b"traf", &traf)).len() + 8) as i32
    }

    fn demux(init: &Fmp4Init, segment: &[u8]) -> Result<Vec<u8>, HlsError> {
        let mut out = Vec::new();
        Fmp4Demuxer::default().push(init, segment, &mut out)?;
        Ok(out)
    }

    // --- Init segments ------------------------------------------------------

    #[test]
    fn aac_init_from_ffmpeg_is_read() {
        let init = Fmp4Init::parse(AAC_INIT).unwrap();

        assert_eq!(
            init.codec,
            Codec::Aac {
                profile: 1, // AAC-LC
                frequency_index: 4,
                channel_config: 1
            }
        );
        assert_eq!(init.kind(), EsKind::Adts);
        assert_eq!(init.timescale, 44_100);
    }

    #[test]
    fn opus_init_from_ffmpeg_is_read() {
        let init = Fmp4Init::parse(OPUS_INIT).unwrap();

        let Codec::Opus { head } = &init.codec else {
            panic!("expected Opus, got {:?}", init.codec);
        };
        assert_eq!(&head[..8], b"OpusHead");
        assert_eq!(head[8], 1); // version
        assert_eq!(head[9], 2); // channels
        assert_eq!(
            u32::from_le_bytes([head[12], head[13], head[14], head[15]]),
            48_000
        );
        assert_eq!(init.kind(), EsKind::OggOpus);
    }

    #[test]
    fn aac_object_type_and_channel_layout_are_read_from_the_config() {
        // AAC-LC (2), 48 kHz (index 3), 2 channels: 00010 0011 0010 000.
        let entry = audio_entry(b"mp4a", &esds(0x40, &[0x11, 0x90]));
        let init = Fmp4Init::parse(&init_segment(b"soun", &entry, None)).unwrap();

        assert_eq!(
            init.codec,
            Codec::Aac {
                profile: 1,
                frequency_index: 3,
                channel_config: 2
            }
        );
    }

    #[test]
    fn explicit_sbr_uses_the_aac_lc_core() {
        // HE-AAC (5), core 24 kHz (index 6), stereo, SBR rate 48 kHz (3), core AAC-LC (2):
        // 00101 0110 0010 0011 00010 000.
        let entry = audio_entry(b"mp4a", &esds(0x40, &[0x2B, 0x11, 0x88]));
        let init = Fmp4Init::parse(&init_segment(b"soun", &entry, None)).unwrap();

        assert_eq!(
            init.codec,
            Codec::Aac {
                profile: 1,
                frequency_index: 6,
                channel_config: 2
            }
        );
    }

    #[test]
    fn unsupported_aac_variants_are_named() {
        // Channel configuration 0 (a program config element follows): 00010 0011 0000 000.
        let pce = audio_entry(b"mp4a", &esds(0x40, &[0x11, 0x80]));
        let error = Fmp4Init::parse(&init_segment(b"soun", &pce, None)).unwrap_err();
        assert!(matches!(error, HlsError::UnsupportedCodec(ref c) if c.contains("program config")));

        // Audio object type 23 (ER AAC LD): 10111 0011 0010 000.
        let er = audio_entry(b"mp4a", &esds(0x40, &[0xB9, 0x90]));
        let error = Fmp4Init::parse(&init_segment(b"soun", &er, None)).unwrap_err();
        assert!(matches!(error, HlsError::UnsupportedCodec(ref c) if c.contains("23")));
    }

    #[test]
    fn mp3_in_mp4_is_recognised() {
        for object_type in [0x69, 0x6B] {
            let entry = audio_entry(b"mp4a", &esds(object_type, &[]));
            let init = Fmp4Init::parse(&init_segment(b"soun", &entry, None)).unwrap();
            assert_eq!(init.codec, Codec::Mp3);
            assert_eq!(init.kind(), EsKind::MpegAudio);
        }
    }

    #[test]
    fn other_codecs_and_video_are_reported_by_name() {
        let ac3 = audio_entry(b"ac-3", &[]);
        let error = Fmp4Init::parse(&init_segment(b"soun", &ac3, None)).unwrap_err();
        assert_eq!(error, HlsError::UnsupportedCodec("ac-3".to_string()));
        assert!(error.to_string().contains("not supported"));

        let error = Fmp4Init::parse(&init_segment(b"vide", &ac3, None)).unwrap_err();
        assert_eq!(error, HlsError::VideoOnly);
    }

    #[test]
    fn opus_head_is_rebuilt_from_dops() {
        // version 0, 2 channels, pre-skip 312, 48000 Hz, gain +1.0 dB (256), family 0.
        let mut dops = vec![0, 2];
        dops.extend_from_slice(&312u16.to_be_bytes());
        dops.extend_from_slice(&48_000u32.to_be_bytes());
        dops.extend_from_slice(&256u16.to_be_bytes());
        dops.push(0);
        let entry = audio_entry(b"Opus", &mp4_box(b"dOps", &dops));

        let init = Fmp4Init::parse(&init_segment(b"soun", &entry, None)).unwrap();

        let mut expected = b"OpusHead".to_vec();
        expected.extend_from_slice(&[1, 2]);
        expected.extend_from_slice(&312u16.to_le_bytes());
        expected.extend_from_slice(&48_000u32.to_le_bytes());
        expected.extend_from_slice(&256u16.to_le_bytes());
        expected.push(0);
        assert_eq!(init.codec, Codec::Opus { head: expected });
    }

    #[test]
    fn init_without_moov_or_with_garbage_is_malformed() {
        assert!(Fmp4Init::parse(&[]).is_err());
        assert!(Fmp4Init::parse(b"not an mp4 at all").is_err());
        assert!(Fmp4Init::parse(&mp4_box(b"ftyp", b"isom")).is_err());
    }

    // --- Media segments -----------------------------------------------------

    #[test]
    fn aac_from_fmp4_is_byte_identical_to_the_mpeg_ts_path() {
        let init = Fmp4Init::parse(AAC_INIT).unwrap();
        let mut demuxer = Fmp4Demuxer::default();
        let mut from_fmp4 = Vec::new();
        for segment in AAC_SEGMENTS {
            demuxer.push(&init, segment, &mut from_fmp4).unwrap();
        }
        let mut extractor = AudioExtractor::default();
        let mut from_ts = Vec::new();
        for segment in TS_SEGMENTS {
            extractor.push_segment(segment, &mut from_ts).unwrap();
        }

        assert!(from_fmp4.len() > 10_000);
        assert_eq!(from_fmp4, from_ts);
    }

    #[test]
    fn extractor_reports_the_kind_and_demands_an_init() {
        let init = Fmp4Init::parse(AAC_INIT).unwrap();
        let mut extractor = AudioExtractor::default();
        let mut out = Vec::new();

        assert_eq!(
            extractor.push_segment(AAC_SEGMENTS[0], &mut out),
            Err(HlsError::Malformed("fMP4 segment without #EXT-X-MAP"))
        );
        extractor
            .push_segment_with_init(AAC_SEGMENTS[0], Some(&init), &mut out)
            .unwrap();
        assert_eq!(extractor.kind(), Some(EsKind::Adts));
        assert!(!out.is_empty());
    }

    #[test]
    fn adts_header_has_the_canonical_form() {
        // AAC-LC, 44.1 kHz, mono, 100-byte frame.
        assert_eq!(
            adts_header(1, 4, 1, 100),
            [0xFF, 0xF1, 0x50, 0x40, 0x0C, 0x9F, 0xFC]
        );
        // The 13-bit length spreads over three bytes.
        let header = adts_header(1, 4, 1, 0x1ABC);
        let length = 0x1ABC_usize;
        assert_eq!(usize::from(header[3] & 3), length >> 11);
        assert_eq!(usize::from(header[4]), (length >> 3) & 0xFF);
        assert_eq!(usize::from(header[5] >> 5), length & 7);
    }

    #[test]
    fn samples_use_explicit_sizes_and_the_data_offset() {
        let run_for = |offset| trun(Some(offset), &[(None, Some(3)), (None, Some(2))]);
        let offset = data_offset_of(1, None, &run_for(0));
        let segment = fragment(1, None, &run_for(offset), b"abcde");

        let out = demux(&init_for(Codec::Mp3), &segment).unwrap();

        assert_eq!(out, b"abcde");
    }

    #[test]
    fn sample_size_falls_back_to_tfhd_then_trex() {
        let run = |offset| trun(Some(offset), &[(None, None), (None, None)]);
        let segment = |default_size: Option<u32>, data: &[u8]| {
            let offset = data_offset_of(1, default_size, &run(0));
            fragment(1, default_size, &run(offset), data)
        };

        let with_tfhd = segment(Some(2), b"aabb");
        assert_eq!(demux(&init_for(Codec::Mp3), &with_tfhd).unwrap(), b"aabb");

        let mut init = init_for(Codec::Mp3);
        init.defaults.size = Some(2);
        let with_trex = segment(None, b"ccdd");
        assert_eq!(demux(&init, &with_trex).unwrap(), b"ccdd");

        let without = segment(None, b"eeff");
        assert_eq!(
            demux(&init_for(Codec::Mp3), &without),
            Err(HlsError::Malformed("fragment sample without a size"))
        );
    }

    #[test]
    fn runs_without_a_data_offset_continue_after_the_previous_run() {
        // The header names an absolute base (the start of the mdat payload) and
        // neither run has a data offset, so the second run follows the first.
        let build = |base: u64| {
            let mut payload = 1u32.to_be_bytes().to_vec();
            payload.extend_from_slice(&base.to_be_bytes());
            let mut traf = full_box(b"tfhd", 0, 0x1, &payload);
            traf.extend(trun(None, &[(None, Some(2))]));
            traf.extend(trun(None, &[(None, Some(3))]));
            mp4_box(b"moof", &mp4_box(b"traf", &traf))
        };
        let moof_len = build(0).len() as u64;
        let mut segment = build(moof_len + 8);
        segment.extend(mp4_box(b"mdat", b"AABBB"));

        assert_eq!(demux(&init_for(Codec::Mp3), &segment).unwrap(), b"AABBB");
    }

    #[test]
    fn several_fragments_in_one_segment_are_all_read() {
        let run = |offset| trun(Some(offset), &[(None, Some(2))]);
        let offset = data_offset_of(1, None, &run(0));
        let mut segment = fragment(1, None, &run(offset), b"11");
        segment.extend(fragment(1, None, &run(offset), b"22"));
        // Unrelated boxes between fragments are skipped.
        segment.extend(mp4_box(b"free", b"padding"));
        segment.extend(fragment(1, None, &run(offset), b"33"));

        assert_eq!(demux(&init_for(Codec::Mp3), &segment).unwrap(), b"112233");
    }

    #[test]
    fn other_tracks_are_ignored() {
        let run = |offset| trun(Some(offset), &[(None, Some(2))]);
        let offset = data_offset_of(2, None, &run(0));
        let segment = fragment(2, None, &run(offset), b"xx");

        assert!(demux(&init_for(Codec::Mp3), &segment).unwrap().is_empty());
    }

    #[test]
    fn large_and_open_ended_box_sizes_are_understood() {
        // mdat written with a 64-bit size.
        let run = |offset| trun(Some(offset), &[(None, Some(4))]);
        let offset = data_offset_of(1, None, &run(0)) + 8;
        let mut traf = tfhd(1, None);
        traf.extend(run(offset));
        let mut segment = mp4_box(b"moof", &mp4_box(b"traf", &traf));
        segment.extend_from_slice(&1u32.to_be_bytes());
        segment.extend_from_slice(b"mdat");
        segment.extend_from_slice(&(16u64 + 4).to_be_bytes());
        segment.extend_from_slice(b"wxyz");
        assert_eq!(demux(&init_for(Codec::Mp3), &segment).unwrap(), b"wxyz");

        // mdat written with size 0: it runs to the end of the segment.
        let offset = data_offset_of(1, None, &run(0));
        let mut traf = tfhd(1, None);
        traf.extend(run(offset));
        let mut segment = mp4_box(b"moof", &mp4_box(b"traf", &traf));
        segment.extend_from_slice(&0u32.to_be_bytes());
        segment.extend_from_slice(b"mdat");
        segment.extend_from_slice(b"1234");
        assert_eq!(demux(&init_for(Codec::Mp3), &segment).unwrap(), b"1234");
    }

    #[test]
    fn damaged_segments_are_errors_not_panics() {
        let init = init_for(Codec::Mp3);
        let run = |offset| trun(Some(offset), &[(None, Some(8))]);
        let offset = data_offset_of(1, None, &run(0));
        let good = fragment(1, None, &run(offset), b"12345678");

        // Cut off in the middle of the sample data.
        assert!(demux(&init, &good[..good.len() - 3]).is_err());
        // A box that claims to be bigger than the data.
        let mut bad = good.clone();
        bad[3] = 0xFF;
        assert!(demux(&init, &bad).is_err());
        // A data offset pointing far outside the segment.
        let outside = fragment(1, None, &run(i32::MAX), b"12345678");
        assert!(demux(&init, &outside).is_err());
        let before = fragment(1, None, &run(i32::MIN), b"12345678");
        assert!(demux(&init, &before).is_err());
        // An absurd sample count.
        let mut payload = 1_000_000u32.to_be_bytes().to_vec();
        payload.extend_from_slice(&[0; 8]);
        let mut traf = tfhd(1, Some(1));
        traf.extend(full_box(b"trun", 0, 0, &payload));
        let huge = mp4_box(b"moof", &mp4_box(b"traf", &traf));
        assert!(demux(&init, &huge).is_err());
    }

    // --- Opus in Ogg --------------------------------------------------------

    /// The pages in `ogg`, each as (header_type, granule, serial, sequence, packet).
    fn pages(ogg: &[u8]) -> Vec<(u8, u64, u32, u32, Vec<u8>)> {
        let mut pages = Vec::new();
        let mut pos = 0;
        while pos < ogg.len() {
            assert_eq!(&ogg[pos..pos + 4], b"OggS", "page at {pos}");
            let segments = usize::from(ogg[pos + 26]);
            let lacing = &ogg[pos + 27..pos + 27 + segments];
            let length: usize = lacing.iter().map(|&l| usize::from(l)).sum();
            let end = pos + 27 + segments + length;

            let mut page = ogg[pos..end].to_vec();
            let stored = u32::from_le_bytes([page[22], page[23], page[24], page[25]]);
            page[22..26].fill(0);
            assert_eq!(ogg_crc(&page), stored, "bad checksum on the page at {pos}");

            pages.push((
                ogg[pos + 5],
                u64::from_le_bytes(ogg[pos + 6..pos + 14].try_into().unwrap()),
                u32::from_le_bytes(ogg[pos + 14..pos + 18].try_into().unwrap()),
                u32::from_le_bytes(ogg[pos + 18..pos + 22].try_into().unwrap()),
                ogg[pos + 27 + segments..end].to_vec(),
            ));
            pos = end;
        }
        pages
    }

    #[test]
    fn ogg_pages_have_valid_checksums_lacing_and_sequence() {
        let mut mux = OggOpusMux::default();
        let mut out = Vec::new();
        mux.start(b"OpusHead-bytes", &mut out);
        let small = vec![7u8; 100];
        let multiple_of_255 = vec![8u8; 510];
        let large = vec![9u8; 700];
        mux.packet(&small, 960, &mut out);
        mux.packet(&multiple_of_255, 960, &mut out);
        mux.packet(&large, 1920, &mut out);

        let pages = pages(&out);

        assert_eq!(pages.len(), 5); // head, tags, three packets
        assert_eq!(pages[0].0, 0x02); // beginning of stream
        assert_eq!(pages[0].4, b"OpusHead-bytes");
        assert!(pages[1].4.starts_with(b"OpusTags"));
        assert_eq!(pages[2].4, small);
        assert_eq!(pages[3].4, multiple_of_255);
        assert_eq!(pages[4].4, large);
        // One logical stream with consecutive page numbers and a running granule.
        assert!(pages.iter().all(|p| p.2 == pages[0].2));
        let sequence: Vec<u32> = pages.iter().map(|p| p.3).collect();
        assert_eq!(sequence, vec![0, 1, 2, 3, 4]);
        let granules: Vec<u64> = pages.iter().map(|p| p.1).collect();
        assert_eq!(granules, vec![0, 0, 960, 1920, 3840]);
        // A 510-byte packet is 255 + 255 + a terminating zero.
        let lacing_of = |index: usize| {
            let start = out
                .windows(4)
                .enumerate()
                .filter(|(_, w)| *w == b"OggS")
                .map(|(i, _)| i)
                .nth(index)
                .unwrap();
            let segments = usize::from(out[start + 26]);
            out[start + 27..start + 27 + segments].to_vec()
        };
        assert_eq!(lacing_of(3), vec![255, 255, 0]);
        assert_eq!(lacing_of(4), vec![255, 255, 190]);
    }

    #[test]
    fn a_reset_or_new_parameters_start_a_new_logical_stream() {
        let init = Fmp4Init::parse(OPUS_INIT).unwrap();
        let mut demuxer = Fmp4Demuxer::default();
        let mut first = Vec::new();
        demuxer.push(&init, OPUS_SEGMENTS[0], &mut first).unwrap();
        let mut same_stream = Vec::new();
        demuxer
            .push(&init, OPUS_SEGMENTS[1], &mut same_stream)
            .unwrap();
        demuxer.reset();
        let mut after_reset = Vec::new();
        demuxer
            .push(&init, OPUS_SEGMENTS[2], &mut after_reset)
            .unwrap();

        let first = pages(&first);
        let same_stream = pages(&same_stream);
        let after_reset = pages(&after_reset);
        assert_eq!(first[0].0, 0x02);
        // The next segment continues the stream: no header pages, next page number.
        assert_eq!(same_stream[0].0, 0x00);
        assert_eq!(same_stream[0].2, first[0].2);
        assert_eq!(same_stream[0].3, first.last().unwrap().3 + 1);
        // After a reset the stream begins again with a new serial number.
        assert_eq!(after_reset[0].0, 0x02);
        assert_ne!(after_reset[0].2, first[0].2);
    }

    #[test]
    fn opus_from_fmp4_decodes_to_audio() {
        let init = Fmp4Init::parse(OPUS_INIT).unwrap();
        let mut demuxer = Fmp4Demuxer::default();
        let mut ogg = Vec::new();
        for segment in OPUS_SEGMENTS {
            demuxer.push(&init, segment, &mut ogg).unwrap();
        }

        let mut backend =
            SymphoniaBackend::open(ReadOnlySource::new(Cursor::new(ogg)), None).unwrap();
        let (mut samples, mut buffer) = (0usize, Vec::new());
        let mut peak = 0.0_f32;
        let mut format = None;
        while let Some(info) = backend.next_frame(&mut buffer).unwrap() {
            format = Some(info);
            samples += buffer.len();
            peak = buffer.iter().fold(peak, |m, s| m.max(s.abs()));
            buffer.clear();
        }

        let info = format.expect("some audio");
        assert_eq!((info.rate, info.channels), (48_000, 2));
        // 3 seconds of stereo at 48 kHz.
        let expected = 3 * 48_000 * 2;
        assert!(
            samples.abs_diff(expected) < expected / 20,
            "{samples} samples, expected about {expected}"
        );
        assert!(peak > 0.05, "decoded audio is too quiet: {peak}");
    }

    // --- Arbitrary input never panics --------------------------------------

    proptest! {
        #[test]
        fn init_parsing_never_panics_on_arbitrary_bytes(bytes in proptest::collection::vec(any::<u8>(), 0..2048)) {
            let _ = Fmp4Init::parse(&bytes);
        }

        #[test]
        fn demuxing_never_panics_on_arbitrary_bytes(bytes in proptest::collection::vec(any::<u8>(), 0..2048)) {
            let mut out = Vec::new();
            let _ = Fmp4Demuxer::default().push(&init_for(Codec::Mp3), &bytes, &mut out);
        }

        #[test]
        fn corrupted_real_segments_never_panic(
            which in 0usize..5,
            edits in proptest::collection::vec((any::<usize>(), any::<u8>()), 1..24),
            cut in any::<usize>(),
        ) {
            let init = Fmp4Init::parse(AAC_INIT).unwrap();
            let mut segment = AAC_SEGMENTS[which].to_vec();
            for (at, value) in edits {
                let at = at % segment.len();
                segment[at] = value;
            }
            segment.truncate(cut % (segment.len() + 1));
            let mut out = Vec::new();
            let _ = Fmp4Demuxer::default().push(&init, &segment, &mut out);
        }

        #[test]
        fn corrupted_real_inits_never_panic(
            edits in proptest::collection::vec((any::<usize>(), any::<u8>()), 1..16),
        ) {
            for source in [AAC_INIT, OPUS_INIT] {
                let mut init = source.to_vec();
                for (at, value) in &edits {
                    let at = at % init.len();
                    init[at] = *value;
                }
                let _ = Fmp4Init::parse(&init);
            }
        }
    }
}
