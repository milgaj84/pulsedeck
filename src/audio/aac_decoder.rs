//! AAC decoding with SBR and Parametric Stereo (HE-AAC v1 and v2).
//!
//! Symphonia's AAC decoder only decodes the AAC-LC *core* of an HE-AAC stream:
//! the core carries the lower half of the spectrum at half the output rate, and
//! the SBR layer that rebuilds the upper half is ignored. Such a stream plays at
//! half the sample rate, without the highs, and a large share of radio stations
//! (the ones listed as "AAC+") use it.
//!
//! This is a Symphonia [`Decoder`] for `CODEC_TYPE_AAC` backed by the pure-Rust
//! `rusty_aac` crate, which decodes SBR and PS (detected from the stream itself,
//! as radio ADTS streams signal it implicitly). It replaces Symphonia's decoder in
//! the codec registry. If `rusty_aac` cannot start, or rejects the very first
//! packet as unsupported syntax, Symphonia's own AAC-LC decoder takes over, so a
//! stream never plays worse than before.

use rusty_aac::{AacDecoder, AudioSpecificConfig, DecodedAudio, Error as AacError};
use symphonia::core::audio::{
    AsAudioBufferRef, AudioBuffer, AudioBufferRef, Channels, Layout, Signal, SignalSpec,
};
use symphonia::core::codecs::{
    CodecDescriptor, CodecParameters, Decoder, DecoderOptions, FinalizeResult, CODEC_TYPE_AAC,
};
use symphonia::core::errors::{decode_error, unsupported_error, Result};
use symphonia::core::formats::Packet;
use symphonia::core::support_codec;

/// How `rusty_aac` is told about the stream.
#[derive(Clone)]
enum Setup {
    /// The out-of-band `AudioSpecificConfig` of an MP4 track (may signal SBR).
    Asc(Vec<u8>),
    /// An ADTS stream: raw frames of AAC-LC, SBR (if any) found in the frames.
    Adts(AudioSpecificConfig),
}

impl Setup {
    fn build(&self) -> Option<AacDecoder> {
        match self {
            Self::Asc(bytes) => AacDecoder::with_config_bytes(bytes).ok().or_else(|| {
                // `rusty_aac` looks past the end of a bare explicit-SBR config for
                // an extension; zero padding says there is none.
                let mut padded = bytes.clone();
                padded.extend_from_slice(&[0, 0]);
                AacDecoder::with_config_bytes(&padded).ok()
            }),
            Self::Adts(config) => Some(AacDecoder::with_config(*config)),
        }
    }
}

enum Engine {
    /// `rusty_aac`, with how to rebuild it on `reset`.
    Sbr {
        decoder: Box<AacDecoder>,
        setup: Setup,
    },
    /// Symphonia's AAC-LC decoder (fallback).
    Lc(Box<dyn Decoder>),
}

pub(super) struct HeAacDecoder {
    params: CodecParameters,
    options: DecoderOptions,
    engine: Engine,
    /// Whether `rusty_aac` has decoded a packet yet.
    decoded_any: bool,
    buf: AudioBuffer<f32>,
}

impl HeAacDecoder {
    fn lc_decoder(params: &CodecParameters, options: &DecoderOptions) -> Result<Engine> {
        Ok(Engine::Lc(Box::new(
            symphonia_codec_aac::AacDecoder::try_new(params, options)?,
        )))
    }

    /// Copy decoded audio into the Symphonia buffer. The buffer takes the *output*
    /// rate and channel count, which for HE-AAC differ from the core's.
    fn publish(&mut self, audio: &DecodedAudio) -> Result<()> {
        let channels = usize::from(audio.channels);
        let layout = match channels {
            1 => Layout::Mono,
            2 => Layout::Stereo,
            _ => return decode_error("aac: unsupported channel count"),
        };
        let frames = audio.frames();
        if audio.sample_rate == 0 || audio.samples.len() != frames * channels {
            return decode_error("aac: inconsistent decoder output");
        }

        let spec = SignalSpec::new(audio.sample_rate, layout.into_channels());
        if self.buf.spec() != &spec || self.buf.capacity() < frames {
            self.buf = AudioBuffer::new(frames as u64, spec);
        }
        self.buf.clear();
        self.buf.render_reserved(Some(frames));
        for channel in 0..channels {
            for (out, sample) in self
                .buf
                .chan_mut(channel)
                .iter_mut()
                .zip(audio.samples.iter().skip(channel).step_by(channels))
            {
                *out = *sample;
            }
        }
        Ok(())
    }
}

fn channel_count(params: &CodecParameters) -> Option<usize> {
    params.channels.map(Channels::count).or_else(|| {
        params
            .channel_layout
            .map(|layout| layout.into_channels().count())
    })
}

impl Decoder for HeAacDecoder {
    fn try_new(params: &CodecParameters, options: &DecoderOptions) -> Result<Self> {
        if params.codec != CODEC_TYPE_AAC {
            return unsupported_error("aac: invalid codec type");
        }
        // Like Symphonia's own decoder, play at most stereo.
        let setup = match &params.extra_data {
            Some(asc) if asc.len() >= 2 => Setup::Asc(asc.to_vec()),
            _ => {
                let (Some(rate), Some(channels)) = (params.sample_rate, channel_count(params))
                else {
                    return unsupported_error("aac: sample rate and channels are required");
                };
                Setup::Adts(AudioSpecificConfig {
                    object_type: 2,
                    sample_rate: rate,
                    channels: u16::try_from(channels).unwrap_or(u16::MAX),
                })
            }
        };
        if matches!(&setup, Setup::Adts(config) if config.channels == 0 || config.channels > 2) {
            return unsupported_error("aac: more than two channels are not supported");
        }

        let engine = match setup.build() {
            Some(decoder) => Engine::Sbr {
                decoder: Box::new(decoder),
                setup,
            },
            None => Self::lc_decoder(params, options)?,
        };
        Ok(Self {
            params: params.clone(),
            options: *options,
            engine,
            decoded_any: false,
            buf: AudioBuffer::new(0, SignalSpec::new(44_100, Layout::Stereo.into_channels())),
        })
    }

    fn supported_codecs() -> &'static [CodecDescriptor] {
        &[support_codec!(
            CODEC_TYPE_AAC,
            "aac",
            "Advanced Audio Coding (with SBR and PS)"
        )]
    }

    fn reset(&mut self) {
        match &mut self.engine {
            Engine::Sbr { decoder, setup } => {
                if let Some(fresh) = setup.build() {
                    **decoder = fresh;
                }
                self.decoded_any = false;
            }
            Engine::Lc(decoder) => decoder.reset(),
        }
    }

    fn codec_params(&self) -> &CodecParameters {
        &self.params
    }

    fn decode(&mut self, packet: &Packet) -> Result<AudioBufferRef<'_>> {
        if let Engine::Sbr { decoder, .. } = &mut self.engine {
            match decoder.decode(&packet.data, None) {
                Ok(audio) => {
                    self.decoded_any = true;
                    self.publish(&audio)?;
                    return Ok(self.buf.as_audio_buffer_ref());
                }
                // Syntax this decoder does not implement, on the very first packet:
                // let Symphonia's LC decoder try the stream instead.
                Err(AacError::Unsupported(_) | AacError::Unimplemented(_)) if !self.decoded_any => {
                    self.engine = Self::lc_decoder(&self.params, &self.options)?;
                }
                Err(AacError::Again | AacError::Eof) => return decode_error("aac: empty packet"),
                Err(AacError::InvalidData(_)) => return decode_error("aac: invalid data"),
                Err(_) => return unsupported_error("aac: unsupported syntax"),
            }
        }
        match &mut self.engine {
            Engine::Lc(decoder) => decoder.decode(packet),
            Engine::Sbr { .. } => decode_error("aac: decoder unavailable"),
        }
    }

    fn finalize(&mut self) -> FinalizeResult {
        FinalizeResult::default()
    }

    fn last_decoded(&self) -> AudioBufferRef<'_> {
        match &self.engine {
            Engine::Sbr { .. } => self.buf.as_audio_buffer_ref(),
            Engine::Lc(decoder) => decoder.last_decoded(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::pcm_source::PcmSource;
    use crate::audio::symphonia_backend::SymphoniaBackend;
    use proptest::prelude::*;
    use rodio::Source;
    use std::io::Cursor;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::{MediaSourceStream, ReadOnlySource};
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    /// Synthetic HE-AAC streams (see the README next to them): a 1 kHz tone plus
    /// noise above 12 kHz, which only SBR can put back.
    const HE_V1: &[u8] = include_bytes!("testdata/heaac/he_v1.aac");
    const HE_V2: &[u8] = include_bytes!("testdata/heaac/he_v2.aac");
    const LC: &[u8] = include_bytes!("hls/testdata/tone.aac");

    /// (rate, channels, samples) as the player decodes `bytes`.
    fn decode(bytes: &[u8]) -> (u32, u16, Vec<f32>) {
        let backend =
            SymphoniaBackend::open(ReadOnlySource::new(Cursor::new(bytes.to_vec())), None)
                .expect("open");
        let source = PcmSource::new(backend).expect("first frame");
        (source.sample_rate(), source.channels(), source.collect())
    }

    /// Power of `samples` at `freq` (Goertzel), per sample.
    fn power_at(samples: &[f32], rate: u32, freq: f32) -> f32 {
        let omega = 2.0 * std::f32::consts::PI * freq / rate as f32;
        let coefficient = 2.0 * omega.cos();
        let (mut s1, mut s2) = (0.0_f32, 0.0_f32);
        for &sample in samples {
            let s0 = sample + coefficient * s1 - s2;
            s2 = s1;
            s1 = s0;
        }
        (s1 * s1 + s2 * s2 - coefficient * s1 * s2) / (samples.len() as f32).powi(2)
    }

    /// Left-channel samples from the middle of the stream.
    fn middle(samples: &[f32], channels: usize) -> Vec<f32> {
        let left: Vec<f32> = samples.iter().step_by(channels).copied().collect();
        let start = left.len() / 2;
        left[start..start + 8192].to_vec()
    }

    #[test]
    fn he_aac_v1_decodes_at_the_full_output_rate() {
        let (rate, channels, samples) = decode(HE_V1);

        // The core is 22.05 kHz; Symphonia alone would stop there.
        assert_eq!((rate, channels), (44_100, 2));
        // 2 s of audio plus the encoder's delay; FFmpeg decodes exactly 188416.
        assert!(samples.len().abs_diff(188_416) < 2_048, "{}", samples.len());
        assert!(samples.iter().fold(0.0_f32, |m, s| m.max(s.abs())) > 0.2);
    }

    #[test]
    fn he_aac_v2_turns_a_mono_core_into_stereo() {
        let (rate, channels, samples) = decode(HE_V2);

        assert_eq!((rate, channels), (44_100, 2));
        // FFmpeg decodes exactly 192512 samples.
        assert!(samples.len().abs_diff(192_512) < 2_048, "{}", samples.len());
        // The two channels are not just a copy of each other (parametric stereo).
        let left = samples.iter().step_by(2);
        let right = samples.iter().skip(1).step_by(2);
        assert!(left.zip(right).any(|(l, r)| (l - r).abs() > 1e-3));
    }

    #[test]
    fn the_sbr_layer_is_really_decoded() {
        for stream in [HE_V1, HE_V2] {
            let (rate, channels, samples) = decode(stream);
            let window = middle(&samples, usize::from(channels));

            // The tone from the core is there...
            let tone = power_at(&window, rate, 1_000.0);
            // ...and so is energy above the core's 11.025 kHz limit.
            let high: f32 = [13_000.0, 14_500.0, 16_000.0, 17_500.0]
                .iter()
                .map(|&freq| power_at(&window, rate, freq))
                .sum();
            assert!(tone > 1e-4, "tone {tone}");
            assert!(high > tone * 1e-4, "no high band: tone {tone}, high {high}");
        }
    }

    #[test]
    fn plain_aac_lc_is_unchanged() {
        let (rate, channels, samples) = decode(LC);

        assert_eq!((rate, channels), (44_100, 1));
        assert!(samples.iter().fold(0.0_f32, |m, s| m.max(s.abs())) > 0.01);
    }

    /// The raw packets and parameters Symphonia's ADTS reader hands a decoder.
    fn packets(bytes: &[u8]) -> (CodecParameters, Vec<Packet>) {
        let stream = MediaSourceStream::new(
            Box::new(ReadOnlySource::new(Cursor::new(bytes.to_vec()))),
            Default::default(),
        );
        let probed = symphonia::default::get_probe()
            .format(
                &Hint::new(),
                stream,
                &FormatOptions::default(),
                &MetadataOptions::default(),
            )
            .expect("probe");
        let mut format = probed.format;
        let params = format.tracks()[0].codec_params.clone();
        let mut packets = Vec::new();
        while let Ok(packet) = format.next_packet() {
            packets.push(packet);
        }
        (params, packets)
    }

    fn decode_packet(decoder: &mut HeAacDecoder, packet: &Packet) -> Vec<f32> {
        let decoded = decoder.decode(packet).expect("decode");
        let mut out = Vec::new();
        let spec = *decoded.spec();
        let mut buffer =
            symphonia::core::audio::SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
        buffer.copy_interleaved_ref(decoded);
        out.extend_from_slice(buffer.samples());
        out
    }

    #[test]
    fn the_decoder_reports_the_output_format_not_the_core_format() {
        let (params, packets) = packets(HE_V1);
        let mut decoder = HeAacDecoder::try_new(&params, &DecoderOptions::default()).unwrap();

        // The ADTS header says 22.05 kHz; the audio is 44.1 kHz.
        assert_eq!(params.sample_rate, Some(22_050));
        let decoded = decoder.decode(&packets[0]).unwrap();
        assert_eq!(decoded.spec().rate, 44_100);
        assert_eq!(decoded.spec().channels.count(), 2);
    }

    #[test]
    fn reset_starts_the_stream_over() {
        let (params, packets) = packets(HE_V1);
        let mut decoder = HeAacDecoder::try_new(&params, &DecoderOptions::default()).unwrap();
        let first: Vec<Vec<f32>> = packets[..6]
            .iter()
            .map(|p| decode_packet(&mut decoder, p))
            .collect();

        decoder.reset();
        let again: Vec<Vec<f32>> = packets[..6]
            .iter()
            .map(|p| decode_packet(&mut decoder, p))
            .collect();

        assert_eq!(first, again);
    }

    #[test]
    fn unusable_parameters_are_refused_not_panicked_on() {
        let options = DecoderOptions::default();
        let mut params = CodecParameters::new();
        params.for_codec(CODEC_TYPE_AAC);
        // No rate or channels, and no config.
        assert!(HeAacDecoder::try_new(&params, &options).is_err());
        // More than two channels (Symphonia's decoder refuses these too).
        params
            .with_sample_rate(44_100)
            .with_channels(Layout::FivePointOne.into_channels());
        assert!(HeAacDecoder::try_new(&params, &options).is_err());
        // A damaged config.
        let mut params = CodecParameters::new();
        params
            .for_codec(CODEC_TYPE_AAC)
            .with_extra_data(vec![0xFF, 0xFF, 0xFF].into());
        assert!(HeAacDecoder::try_new(&params, &options).is_err());
        // Another codec.
        let mut params = CodecParameters::new();
        params
            .for_codec(symphonia::core::codecs::CODEC_TYPE_MP3)
            .with_sample_rate(44_100)
            .with_channels(Layout::Stereo.into_channels());
        assert!(HeAacDecoder::try_new(&params, &options).is_err());
    }

    #[test]
    fn an_mp4_config_with_explicit_sbr_is_accepted() {
        // HE-AAC (5), core 24 kHz, stereo, SBR rate 48 kHz, core AAC-LC: as 3
        // bare bytes, and with the trailing byte encoders usually add.
        for asc in [vec![0x2B, 0x11, 0x88], vec![0x2B, 0x11, 0x88, 0x00]] {
            let mut params = CodecParameters::new();
            params.for_codec(CODEC_TYPE_AAC).with_extra_data(asc.into());

            let decoder = HeAacDecoder::try_new(&params, &DecoderOptions::default()).unwrap();

            assert!(matches!(decoder.engine, Engine::Sbr { .. }));
        }
    }

    #[test]
    fn a_config_rusty_aac_cannot_read_falls_back_to_symphonias_decoder() {
        // A valid AAC-LC config that rusty_aac is not given: force the fallback
        // by building the engine the way a failed start does.
        let (params, packets) = packets(LC);
        let options = DecoderOptions::default();
        let mut decoder = HeAacDecoder::try_new(&params, &options).unwrap();
        decoder.engine = HeAacDecoder::lc_decoder(&params, &options).unwrap();

        let (rate, frames) = {
            let decoded = decoder.decode(&packets[0]).unwrap();
            (decoded.spec().rate, decoded.frames())
        };

        assert_eq!(rate, 44_100);
        assert!(frames > 0);
        assert_eq!(decoder.last_decoded().frames(), frames);
    }

    #[test]
    fn the_core_alone_has_no_high_band() {
        // Contrast for the SBR test: Symphonia's own decoder, which ignores SBR,
        // leaves the band above the core's limit empty.
        let (params, packets) = packets(HE_V1);
        let options = DecoderOptions::default();
        let mut decoder = HeAacDecoder::try_new(&params, &options).unwrap();
        decoder.engine = HeAacDecoder::lc_decoder(&params, &options).unwrap();
        let mut samples = Vec::new();
        for packet in &packets {
            samples.extend(decode_packet(&mut decoder, packet));
        }
        let window = middle(&samples, 2);

        let tone = power_at(&window, 22_050, 1_000.0);
        let high = power_at(&window, 22_050, 9_000.0);
        assert!(tone > 1e-4, "tone {tone}");
        assert!(high < tone * 1e-4, "tone {tone}, high {high}");
    }

    proptest! {
        // Decoding is slow in debug builds, so keep the case count modest.
        #![proptest_config(ProptestConfig::with_cases(24))]

        #[test]
        fn corrupted_he_aac_never_panics(
            edits in proptest::collection::vec((any::<usize>(), any::<u8>()), 1..60),
            v2 in any::<bool>(),
        ) {
            let mut stream = if v2 { HE_V2.to_vec() } else { HE_V1.to_vec() };
            for (at, value) in edits {
                let at = at % stream.len();
                stream[at] = value;
            }
            if let Ok(backend) =
                SymphoniaBackend::open(ReadOnlySource::new(Cursor::new(stream)), None)
            {
                if let Ok(source) = PcmSource::new(backend) {
                    let _ = source.take(300_000).count();
                }
            }
        }
    }
}
