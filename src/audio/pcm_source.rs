//! Turns a frame-by-frame decoder into a rodio `Source`.
//!
//! A [`FrameBackend`] produces one decoded frame (interleaved `f32`) at a time.
//! [`PcmSource`] hands those samples to rodio and keeps the reported channel
//! count and sample rate in step with the frame being played, so a stream whose
//! format changes mid-way (a chained Ogg stream, say) is played correctly.
//!
//! Every backend call runs inside [`panic_guard::run_quiet`]: a decoder that
//! panics on input it cannot handle ends the stream instead of taking the audio
//! thread down.

use std::time::Duration;

use rodio::Source;

use super::decode::decoder_crashed;
use super::panic_guard;
use super::types::EngineError;

/// Channel count and sample rate of a decoded frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SignalInfo {
    pub(super) channels: u16,
    pub(super) rate: u32,
}

/// Told, once, why a stream stopped when it did not simply end.
pub(super) type FailureSink = Box<dyn FnOnce(EngineError) + Send>;

/// A decoder that yields one frame of interleaved `f32` samples at a time.
pub(super) trait FrameBackend: Send {
    /// Append the next frame's samples to `out` and describe them.
    /// `Ok(None)` is the end of the stream.
    fn next_frame(&mut self, out: &mut Vec<f32>) -> Result<Option<SignalInfo>, String>;
}

/// Changes each decoded frame in place before it is played (loudness
/// levelling, say). Runs once per frame, so it sees format changes as they happen.
pub(super) trait FrameProcessor: Send {
    fn process(&mut self, frame: &mut [f32], info: SignalInfo);
}

pub(super) struct PcmSource<B: FrameBackend> {
    backend: B,
    frame: Vec<f32>,
    pos: usize,
    info: SignalInfo,
    failure: Option<FailureSink>,
    processor: Option<Box<dyn FrameProcessor>>,
}

impl<B: FrameBackend> PcmSource<B> {
    /// Decode the first frame so the stream's format is known. Fails if the
    /// stream ends or errors before producing any audio.
    pub(super) fn new(backend: B) -> Result<Self, String> {
        let mut source = Self {
            backend,
            frame: Vec::new(),
            pos: 0,
            info: SignalInfo {
                channels: 2,
                rate: 44_100,
            },
            failure: None,
            processor: None,
        };
        match source.refill() {
            Ok(true) => Ok(source),
            Ok(false) => Err("stream ended before any audio could be decoded".to_string()),
            Err(message) => Err(message),
        }
    }

    /// Report to `sink`, once, a stream that stops because the decoder failed or
    /// crashed after it had started playing. Without this the stream would just
    /// end, and the player could not tell the listener why.
    pub(super) fn with_failure_sink(mut self, sink: FailureSink) -> Self {
        self.failure = Some(sink);
        self
    }

    /// Run every frame through `processor`, starting with the one already loaded.
    /// A processor that panics is dropped and the audio plays on unchanged.
    pub(super) fn with_processor(mut self, processor: Box<dyn FrameProcessor>) -> Self {
        self.processor = Some(processor);
        self.process_frame();
        self
    }

    fn process_frame(&mut self) {
        let Some(processor) = self.processor.as_mut() else {
            return;
        };
        let frame = &mut self.frame;
        let info = self.info;
        if panic_guard::run_quiet(|| processor.process(frame, info)).is_err() {
            self.processor = None;
        }
    }

    fn report(&mut self, error: EngineError) {
        if let Some(sink) = self.failure.take() {
            sink(error);
        }
    }

    /// Load the next non-empty frame. `Ok(false)` means the stream is over (the
    /// frame buffer is then empty); a backend error or panic ends it too.
    fn refill(&mut self) -> Result<bool, String> {
        loop {
            self.frame.clear();
            self.pos = 0;
            let backend = &mut self.backend;
            let frame = &mut self.frame;
            match panic_guard::run_quiet(|| backend.next_frame(frame)) {
                Ok(Ok(Some(info))) if !self.frame.is_empty() => {
                    self.info = info;
                    self.process_frame();
                    return Ok(true);
                }
                Ok(Ok(Some(_))) => continue,
                Ok(Ok(None)) => {
                    self.frame.clear();
                    return Ok(false);
                }
                Ok(Err(message)) => {
                    self.frame.clear();
                    self.report(EngineError::Decode(message.clone()));
                    return Err(message);
                }
                Err(()) => {
                    self.frame.clear();
                    self.report(decoder_crashed());
                    return Err("decoder panicked".to_string());
                }
            }
        }
    }
}

impl<B: FrameBackend> Iterator for PcmSource<B> {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        let sample = *self.frame.get(self.pos)?;
        self.pos += 1;
        // Load the next frame as soon as this one is used up, so that rodio sees
        // its channel count and sample rate when it asks at the frame boundary.
        if self.pos == self.frame.len() {
            let _ = self.refill();
        }
        Some(sample)
    }
}

impl<B: FrameBackend> Source for PcmSource<B> {
    fn current_frame_len(&self) -> Option<usize> {
        Some(self.frame.len() - self.pos)
    }

    fn channels(&self) -> u16 {
        self.info.channels
    }

    fn sample_rate(&self) -> u32 {
        self.info.rate
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Plays a scripted list of frames, then ends (or panics / errors).
    struct Scripted {
        frames: std::vec::IntoIter<(SignalInfo, Vec<f32>)>,
        then: Then,
    }

    enum Then {
        End,
        Panic,
        Error,
    }

    impl FrameBackend for Scripted {
        fn next_frame(&mut self, out: &mut Vec<f32>) -> Result<Option<SignalInfo>, String> {
            match self.frames.next() {
                Some((info, samples)) => {
                    out.extend(samples);
                    Ok(Some(info))
                }
                None => match self.then {
                    Then::End => Ok(None),
                    Then::Panic => panic!("decoder blew up"),
                    Then::Error => Err("bad packet".to_string()),
                },
            }
        }
    }

    fn info(channels: u16, rate: u32) -> SignalInfo {
        SignalInfo { channels, rate }
    }

    fn scripted(frames: Vec<(SignalInfo, Vec<f32>)>, then: Then) -> Scripted {
        Scripted {
            frames: frames.into_iter(),
            then,
        }
    }

    #[test]
    fn plays_every_sample_in_order() {
        let backend = scripted(
            vec![
                (info(2, 44_100), vec![0.1, 0.2, 0.3, 0.4]),
                (info(2, 44_100), vec![0.5, 0.6]),
            ],
            Then::End,
        );
        let source = PcmSource::new(backend).unwrap();
        let all: Vec<f32> = source.collect();
        assert_eq!(all, vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6]);
    }

    #[test]
    fn format_switches_exactly_at_the_frame_boundary() {
        let backend = scripted(
            vec![
                (info(2, 44_100), vec![0.1, 0.2]),
                (info(1, 48_000), vec![0.3, 0.4, 0.5]),
            ],
            Then::End,
        );
        let mut source = PcmSource::new(backend).unwrap();
        assert_eq!((source.channels(), source.sample_rate()), (2, 44_100));
        assert_eq!(source.current_frame_len(), Some(2));
        source.next();
        assert_eq!(source.current_frame_len(), Some(1));
        assert_eq!((source.channels(), source.sample_rate()), (2, 44_100));
        source.next();
        // The first frame is used up: rodio now sees the next frame's format.
        assert_eq!((source.channels(), source.sample_rate()), (1, 48_000));
        assert_eq!(source.current_frame_len(), Some(3));
    }

    #[test]
    fn empty_frames_are_skipped() {
        let backend = scripted(
            vec![
                (info(2, 44_100), vec![]),
                (info(2, 44_100), vec![0.7, 0.8]),
                (info(2, 44_100), vec![]),
            ],
            Then::End,
        );
        let source = PcmSource::new(backend).unwrap();
        assert_eq!(source.collect::<Vec<_>>(), vec![0.7, 0.8]);
    }

    #[test]
    fn a_stream_with_no_audio_is_an_error() {
        let result = PcmSource::new(scripted(vec![], Then::End));
        assert!(result.is_err());
    }

    #[test]
    fn a_decoder_panic_ends_the_stream_instead_of_unwinding() {
        let backend = scripted(vec![(info(2, 44_100), vec![0.1, 0.2])], Then::Panic);
        let source = PcmSource::new(backend).unwrap();
        assert_eq!(source.collect::<Vec<_>>(), vec![0.1, 0.2]);
        // The panic hook is no longer silenced on this thread afterwards.
        assert!(!panic_guard::quiet_panics_on_this_thread());
    }

    #[test]
    fn a_panic_on_the_first_frame_is_a_construction_error() {
        let result = PcmSource::new(scripted(vec![], Then::Panic));
        assert!(result.is_err());
    }

    fn sink() -> (FailureSink, std::sync::mpsc::Receiver<EngineError>) {
        let (tx, rx) = std::sync::mpsc::channel();
        (Box::new(move |error| tx.send(error).unwrap()), rx)
    }

    #[test]
    fn a_decoder_panic_after_playback_started_is_reported_once() {
        let (sink, reported) = sink();
        let backend = scripted(vec![(info(2, 44_100), vec![0.1, 0.2])], Then::Panic);
        let source = PcmSource::new(backend).unwrap().with_failure_sink(sink);

        assert_eq!(source.collect::<Vec<_>>(), vec![0.1, 0.2]);

        let error = reported.try_recv().expect("a failure report");
        assert!(error.to_status_string().contains("crashed the decoder"));
        assert!(reported.try_recv().is_err(), "reported twice");
    }

    #[test]
    fn a_backend_error_after_playback_started_is_reported_with_its_message() {
        let (sink, reported) = sink();
        let backend = scripted(vec![(info(2, 44_100), vec![0.1])], Then::Error);
        let source = PcmSource::new(backend).unwrap().with_failure_sink(sink);

        let _ = source.collect::<Vec<_>>();

        let error = reported.try_recv().expect("a failure report");
        assert!(error.to_status_string().contains("bad packet"));
    }

    #[test]
    fn a_normal_end_is_not_a_failure() {
        let (sink, reported) = sink();
        let backend = scripted(vec![(info(2, 44_100), vec![0.1, 0.2])], Then::End);
        let source = PcmSource::new(backend).unwrap().with_failure_sink(sink);

        assert_eq!(source.collect::<Vec<_>>(), vec![0.1, 0.2]);

        assert!(reported.try_recv().is_err());
    }

    #[test]
    fn a_backend_error_ends_the_stream() {
        let backend = scripted(vec![(info(2, 44_100), vec![0.1])], Then::Error);
        let source = PcmSource::new(backend).unwrap();
        assert_eq!(source.collect::<Vec<_>>(), vec![0.1]);
    }

    /// Doubles every sample and records the formats it was given.
    struct Doubler(std::sync::mpsc::Sender<SignalInfo>);

    impl FrameProcessor for Doubler {
        fn process(&mut self, frame: &mut [f32], info: SignalInfo) {
            self.0.send(info).unwrap();
            frame.iter_mut().for_each(|sample| *sample *= 2.0);
        }
    }

    #[test]
    fn a_processor_sees_every_frame_including_the_first() {
        let (tx, seen) = std::sync::mpsc::channel();
        let backend = scripted(
            vec![
                (info(2, 44_100), vec![0.1, 0.2]),
                (info(1, 48_000), vec![0.3, 0.4, 0.5]),
            ],
            Then::End,
        );
        let source = PcmSource::new(backend)
            .unwrap()
            .with_processor(Box::new(Doubler(tx)));

        assert_eq!(source.collect::<Vec<_>>(), vec![0.2, 0.4, 0.6, 0.8, 1.0]);
        assert_eq!(
            seen.try_iter().collect::<Vec<_>>(),
            vec![info(2, 44_100), info(1, 48_000)]
        );
    }

    #[test]
    fn a_processor_that_panics_is_dropped_and_the_audio_plays_on() {
        struct Bomb;
        impl FrameProcessor for Bomb {
            fn process(&mut self, _: &mut [f32], _: SignalInfo) {
                panic!("processor blew up");
            }
        }
        let backend = scripted(
            vec![
                (info(2, 44_100), vec![0.1, 0.2]),
                (info(2, 44_100), vec![0.3]),
            ],
            Then::End,
        );
        let source = PcmSource::new(backend)
            .unwrap()
            .with_processor(Box::new(Bomb));

        assert_eq!(source.collect::<Vec<_>>(), vec![0.1, 0.2, 0.3]);
        assert!(!panic_guard::quiet_panics_on_this_thread());
    }
}
