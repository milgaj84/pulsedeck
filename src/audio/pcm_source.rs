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

use super::panic_guard;

/// Channel count and sample rate of a decoded frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SignalInfo {
    pub(super) channels: u16,
    pub(super) rate: u32,
}

/// A decoder that yields one frame of interleaved `f32` samples at a time.
pub(super) trait FrameBackend: Send {
    /// Append the next frame's samples to `out` and describe them.
    /// `Ok(None)` is the end of the stream.
    fn next_frame(&mut self, out: &mut Vec<f32>) -> Result<Option<SignalInfo>, String>;
}

pub(super) struct PcmSource<B: FrameBackend> {
    backend: B,
    frame: Vec<f32>,
    pos: usize,
    info: SignalInfo,
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
        };
        match source.refill() {
            Ok(true) => Ok(source),
            Ok(false) => Err("stream ended before any audio could be decoded".to_string()),
            Err(message) => Err(message),
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
                    return Ok(true);
                }
                Ok(Ok(Some(_))) => continue,
                Ok(Ok(None)) => {
                    self.frame.clear();
                    return Ok(false);
                }
                Ok(Err(message)) => {
                    self.frame.clear();
                    return Err(message);
                }
                Err(()) => {
                    self.frame.clear();
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

    #[test]
    fn a_backend_error_ends_the_stream() {
        let backend = scripted(vec![(info(2, 44_100), vec![0.1])], Then::Error);
        let source = PcmSource::new(backend).unwrap();
        assert_eq!(source.collect::<Vec<_>>(), vec![0.1]);
    }
}
