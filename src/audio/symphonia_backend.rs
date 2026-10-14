//! Symphonia-driven decoding of a live byte stream.
//!
//! Rodio's own decoder reports every reader as seekable (so a non-seekable
//! fragmented MP4 panics) and ends playback at an Ogg track change. Driving
//! Symphonia directly avoids both, and lets PulseDeck add codecs of its own.

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{
    CodecParameters, Decoder, DecoderOptions, CODEC_TYPE_NULL, CODEC_TYPE_OPUS,
};
use symphonia::core::errors::Error;
use symphonia::core::formats::{FormatOptions, FormatReader};
use symphonia::core::io::{MediaSource, MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use super::codecs::codec_registry;
use super::pcm_source::{FrameBackend, SignalInfo};

/// Consecutive undecodable packets tolerated before giving up on the stream.
const MAX_DECODE_ERRORS: u32 = 3;

pub(super) struct SymphoniaBackend {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    track_id: u32,
    /// Linear gain applied to every sample (Opus output gain; 1.0 otherwise).
    gain: f32,
    samples: Option<SampleBuffer<f32>>,
}

impl SymphoniaBackend {
    /// Probe `source` and set up a decoder for its first audio track.
    /// `extension` is a format hint (`"mp3"` for a stream verified as MP3).
    pub(super) fn open(
        source: impl MediaSource + 'static,
        extension: Option<&str>,
    ) -> Result<Self, String> {
        let stream = MediaSourceStream::new(Box::new(source), MediaSourceStreamOptions::default());
        let mut hint = Hint::new();
        if let Some(extension) = extension {
            hint.with_extension(extension);
        }
        let probed = symphonia::default::get_probe()
            .format(
                &hint,
                stream,
                &FormatOptions {
                    enable_gapless: true,
                    ..FormatOptions::default()
                },
                &MetadataOptions::default(),
            )
            .map_err(|error| error.to_string())?;
        let format = probed.format;
        let picked = pick_track(format.as_ref())?;
        Ok(Self {
            format,
            decoder: picked.decoder,
            track_id: picked.track_id,
            gain: picked.gain,
            samples: None,
        })
    }

    /// Symphonia asks for the decoder to be rebuilt when the stream's codec
    /// parameters change (a new logical Ogg stream, for example).
    fn rebuild(&mut self) -> Result<(), String> {
        let picked = pick_track(self.format.as_ref())?;
        self.track_id = picked.track_id;
        self.decoder = picked.decoder;
        self.gain = picked.gain;
        Ok(())
    }
}

struct PickedTrack {
    track_id: u32,
    decoder: Box<dyn Decoder>,
    gain: f32,
}

fn pick_track(format: &dyn FormatReader) -> Result<PickedTrack, String> {
    let track = format
        .tracks()
        .iter()
        .find(|track| track.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| "no decodable audio track".to_string())?;
    let params = &track.codec_params;
    let is_opus = params.codec == CODEC_TYPE_OPUS;
    if is_opus {
        // libopus is wrapped for mono and stereo only.
        let channels = params.channels.map_or(0, |channels| channels.count());
        if channels > 2 {
            return Err(format!(
                "Opus audio with {channels} channels is not supported"
            ));
        }
    }
    let decoder = codec_registry()
        .make(params, &DecoderOptions::default())
        .map_err(|error| error.to_string())?;
    Ok(PickedTrack {
        track_id: track.id,
        decoder,
        gain: if is_opus {
            opus_output_gain(params)
        } else {
            1.0
        },
    })
}

/// The linear gain an Opus stream asks for in its `OpusHead` header.
///
/// The header holds a signed Q7.8 value in dB at bytes 16-17. Symphonia does not
/// apply it and neither does the libopus adapter, so players must.
fn opus_output_gain(params: &CodecParameters) -> f32 {
    let Some(head) = params.extra_data.as_deref() else {
        return 1.0;
    };
    let Some(&[low, high]) = head.get(16..18) else {
        return 1.0;
    };
    let q78 = i16::from_le_bytes([low, high]);
    10_f32.powf(f32::from(q78) / (20.0 * 256.0))
}

impl FrameBackend for SymphoniaBackend {
    fn next_frame(&mut self, out: &mut Vec<f32>) -> Result<Option<SignalInfo>, String> {
        let mut errors = 0;
        loop {
            let packet = match self.format.next_packet() {
                Ok(packet) => packet,
                Err(Error::ResetRequired) => {
                    self.rebuild()?;
                    continue;
                }
                // End of stream, or a read error the stream cannot recover from.
                Err(_) => return Ok(None),
            };
            if packet.track_id() != self.track_id {
                continue;
            }
            match self.decoder.decode(&packet) {
                Ok(decoded) => {
                    let spec = *decoded.spec();
                    let wanted = decoded.frames() * spec.channels.count();
                    if wanted == 0 {
                        continue;
                    }
                    let samples = match &mut self.samples {
                        Some(samples) if samples.capacity() >= wanted => samples,
                        slot => slot.insert(SampleBuffer::new(decoded.capacity() as u64, spec)),
                    };
                    samples.copy_interleaved_ref(decoded);
                    let start = out.len();
                    out.extend_from_slice(samples.samples());
                    if self.gain != 1.0 {
                        for sample in &mut out[start..] {
                            *sample *= self.gain;
                        }
                    }
                    return Ok(Some(SignalInfo {
                        channels: spec.channels.count() as u16,
                        rate: spec.rate,
                    }));
                }
                Err(Error::DecodeError(_)) if errors < MAX_DECODE_ERRORS => errors += 1,
                Err(Error::ResetRequired) => self.rebuild()?,
                Err(_) => return Ok(None),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::pcm_source::PcmSource;
    use rodio::Source;
    use std::io::{BufReader, Cursor};
    use symphonia::core::io::ReadOnlySource;

    const OGG_VORBIS: &[u8] = include_bytes!("testdata/tone.ogg");
    const CHAINED_OGG: &[u8] = include_bytes!("testdata/tone_chained.ogg");
    const FLAC: &[u8] = include_bytes!("testdata/tone.flac");
    const OPUS: &[u8] = include_bytes!("testdata/tone.opus");
    const CHAINED_OPUS: &[u8] = include_bytes!("testdata/tone_chained.opus");
    const SIX_CHANNEL_OPUS: &[u8] = include_bytes!("testdata/tone_6ch.opus");
    const AAC: &[u8] = include_bytes!("hls/testdata/tone.aac");
    const MP3: &[u8] = include_bytes!("hls/testdata/tone.mp3");

    fn open(bytes: &[u8], extension: Option<&str>) -> SymphoniaBackend {
        SymphoniaBackend::open(ReadOnlySource::new(Cursor::new(bytes.to_vec())), extension)
            .expect("open")
    }

    /// Our decode of `bytes`: (sample rate, channels, samples).
    fn ours(bytes: &[u8], extension: Option<&str>) -> (u32, u16, Vec<f32>) {
        let source = PcmSource::new(open(bytes, extension)).expect("first frame");
        (source.sample_rate(), source.channels(), source.collect())
    }

    /// Rodio's own decode of the same bytes, as it did before PulseDeck drove
    /// Symphonia itself.
    fn rodios(bytes: &[u8], extension: Option<&str>) -> (u32, u16, Vec<f32>) {
        let reader = BufReader::new(Cursor::new(bytes.to_vec()));
        let decoder = if extension == Some("mp3") {
            rodio::Decoder::new_mp3(reader)
        } else {
            rodio::Decoder::new(reader)
        }
        .expect("rodio decode");
        let (rate, channels) = (decoder.sample_rate(), decoder.channels());
        (rate, channels, decoder.convert_samples::<f32>().collect())
    }

    #[test]
    fn matches_rodio_on_every_format_radio_streams_use() {
        let cases: [(&str, &[u8], Option<&str>); 4] = [
            ("ogg vorbis", OGG_VORBIS, None),
            ("flac", FLAC, None),
            ("adts aac", AAC, None),
            ("mp3", MP3, Some("mp3")),
        ];
        for (name, bytes, extension) in cases {
            let (rate, channels, samples) = ours(bytes, extension);
            let (rodio_rate, rodio_channels, rodio_samples) = rodios(bytes, extension);

            assert_eq!(rate, rodio_rate, "{name}: sample rate");
            assert_eq!(channels, rodio_channels, "{name}: channels");
            assert_eq!(samples.len(), rodio_samples.len(), "{name}: sample count");
            assert!(!samples.is_empty(), "{name}: no audio");
            // Rodio rounds to 16 bits on the way through; we keep full f32.
            let worst = samples
                .iter()
                .zip(&rodio_samples)
                .map(|(a, b)| (a - b).abs())
                .fold(0.0_f32, f32::max);
            assert!(worst <= 2.0 / 32768.0, "{name}: samples differ by {worst}");
            let peak = samples.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
            assert!(peak > 0.01, "{name}: silent");
        }
    }

    #[test]
    fn a_chained_ogg_stream_keeps_playing_across_the_track_change() {
        let mut backend = open(CHAINED_OGG, None);
        let mut frames: Vec<(u32, usize)> = Vec::new();
        let mut buffer = Vec::new();
        loop {
            buffer.clear();
            match backend.next_frame(&mut buffer).expect("decode") {
                Some(info) => frames.push((info.rate, buffer.len())),
                None => break,
            }
        }

        let at = |rate: u32| -> usize {
            frames
                .iter()
                .filter(|(r, _)| *r == rate)
                .map(|(_, n)| n)
                .sum()
        };
        // 0.5 s of mono audio at each rate, give or take codec padding.
        assert!(at(44_100) >= 20_000, "first track: {}", at(44_100));
        assert!(at(48_000) >= 22_000, "second track: {}", at(48_000));
        // The format switch happens once, in order.
        let first_48k = frames.iter().position(|(r, _)| *r == 48_000).unwrap();
        assert!(frames[..first_48k].iter().all(|(r, _)| *r == 44_100));
        assert!(frames[first_48k..].iter().all(|(r, _)| *r == 48_000));
    }

    #[test]
    fn pcm_source_reports_the_new_format_after_a_track_change() {
        let source = PcmSource::new(open(CHAINED_OGG, None)).expect("first frame");
        assert_eq!(source.sample_rate(), 44_100);
        let mut seen = Vec::new();
        let mut source = source;
        while source.next().is_some() {
            if seen.last() != Some(&source.sample_rate()) {
                seen.push(source.sample_rate());
            }
        }
        assert_eq!(seen, vec![44_100, 48_000]);
    }

    #[test]
    fn rodio_resamples_a_chained_stream_across_the_format_change() {
        // The output device mixer converts every source to one rate and channel
        // count; it must pick up the switch at the right sample.
        let source = PcmSource::new(open(CHAINED_OGG, None)).expect("first frame");
        let converted: Vec<f32> =
            rodio::source::UniformSourceIterator::new(source, 2, 48_000).collect();

        // 0.5 s at 44.1 kHz plus 0.5 s at 48 kHz, as stereo at 48 kHz.
        let seconds = converted.len() as f64 / 2.0 / 48_000.0;
        assert!(
            (0.9..1.1).contains(&seconds),
            "expected about one second of audio, got {seconds:.3} s"
        );
        let peak = converted.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
        assert!(peak > 0.01);
    }

    fn peak(samples: &[f32]) -> f32 {
        samples.iter().fold(0.0_f32, |m, s| m.max(s.abs()))
    }

    /// Ogg's page checksum: CRC-32 (poly 0x04C11DB7, no reflection, init 0) over
    /// the page with its checksum field zeroed.
    fn ogg_crc(page: &[u8]) -> u32 {
        let mut crc = 0_u32;
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

    /// `OPUS` with the `OpusHead` output gain replaced (Q7.8 dB). The header is
    /// alone in the first Ogg page, whose checksum is recomputed.
    fn opus_with_gain(q78: i16) -> Vec<u8> {
        let mut bytes = OPUS.to_vec();
        let segments = usize::from(bytes[26]);
        let payload = 27 + segments;
        assert_eq!(&bytes[payload..payload + 8], b"OpusHead");
        bytes[payload + 16..payload + 18].copy_from_slice(&q78.to_le_bytes());
        let page_len = payload
            + bytes[27..27 + segments]
                .iter()
                .map(|&len| usize::from(len))
                .sum::<usize>();
        bytes[22..26].fill(0);
        let crc = ogg_crc(&bytes[..page_len]);
        bytes[22..26].copy_from_slice(&crc.to_le_bytes());
        bytes
    }

    #[test]
    fn opus_decodes_at_48khz() {
        let (rate, channels, samples) = ours(OPUS, None);

        assert_eq!((rate, channels), (48_000, 1));
        // ffmpeg decodes this file to 24000 samples; Symphonia's Ogg reader does
        // not trim the end-of-stream padding, so allow that much extra.
        assert!(
            (24_000..=25_000).contains(&samples.len()),
            "{} samples",
            samples.len()
        );
        assert!(peak(&samples) > 0.01);
    }

    #[test]
    fn opus_output_gain_from_the_header_is_applied() {
        let (_, _, plain) = ours(OPUS, None);
        // +6 dB in Q7.8 is 6 * 256 = 1536, a factor of 10^(6/20).
        let (_, _, louder) = ours(&opus_with_gain(1536), None);
        let (_, _, quieter) = ours(&opus_with_gain(-1536), None);

        let factor = 10_f32.powf(6.0 / 20.0);
        assert_eq!(plain.len(), louder.len());
        assert!((peak(&louder) / peak(&plain) - factor).abs() < 0.01 * factor);
        assert!((peak(&quieter) / peak(&plain) - 1.0 / factor).abs() < 0.01);
    }

    #[test]
    fn a_chained_opus_stream_follows_a_mono_to_stereo_change() {
        let mut backend = open(CHAINED_OPUS, None);
        let mut channels: Vec<u16> = Vec::new();
        let mut samples_per_channel_count = [0_usize; 3];
        let mut buffer = Vec::new();
        loop {
            buffer.clear();
            match backend.next_frame(&mut buffer).expect("decode") {
                Some(info) => {
                    channels.push(info.channels);
                    samples_per_channel_count[usize::from(info.channels)] +=
                        buffer.len() / usize::from(info.channels);
                }
                None => break,
            }
        }

        let first_stereo = channels.iter().position(|&c| c == 2).expect("stereo track");
        assert!(channels[..first_stereo].iter().all(|&c| c == 1));
        assert!(channels[first_stereo..].iter().all(|&c| c == 2));
        // 0.5 s of audio at 48 kHz in each track.
        assert!(samples_per_channel_count[1] >= 23_000);
        assert!(samples_per_channel_count[2] >= 23_000);
    }

    #[test]
    fn opus_with_more_than_two_channels_is_a_clear_error() {
        let Err(message) = SymphoniaBackend::open(
            ReadOnlySource::new(Cursor::new(SIX_CHANNEL_OPUS.to_vec())),
            None,
        ) else {
            panic!("6-channel Opus must be rejected");
        };

        assert!(message.contains("Opus audio with 6 channels is not supported"));
    }

    #[test]
    fn garbage_is_an_error_not_a_panic() {
        let result =
            SymphoniaBackend::open(ReadOnlySource::new(Cursor::new(vec![0x5A; 4096])), None);
        assert!(result.is_err());
    }
}
