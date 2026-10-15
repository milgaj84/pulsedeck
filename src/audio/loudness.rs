//! Loudness normalisation across stations.
//!
//! Radio stations differ by 10 LU or more in how loud they are mastered. The
//! [`Leveller`] measures the short-term loudness (EBU R128, 3 s window) of the
//! stream it is playing and moves a gain toward the user's target, slowly enough
//! that nobody hears it work: cuts take a few seconds, boosts longer and never
//! faster than 1 dB per second. The gain is bounded, frozen while the stream is
//! silent, and a peak limiter keeps a boosted signal from clipping.
//!
//! It runs inside [`PcmSource`](super::pcm_source::PcmSource), on each decoded
//! frame and before the visualizer and the volume control, so it levels the
//! stream and not what the listener's volume knob does to it.
//!
//! The settings live in a [`LevelControl`] shared with the engine, so switching
//! the feature on and off, or changing the target, takes effect on the stream
//! that is already playing.

use std::sync::atomic::{AtomicBool, AtomicI32, Ordering::Relaxed};

use ebur128::{Channel, EbuR128, Mode};

use super::pcm_source::{FrameProcessor, SignalInfo};

/// The target most listeners want (streaming services use -14 to -16).
const DEFAULT_TARGET_LUFS: f32 = -16.0;

/// Gain limits in dB: a stream is never made more than this much quieter...
const MIN_GAIN_DB: f32 = -12.0;
/// ...or louder.
const MAX_GAIN_DB: f32 = 9.0;
/// Below this the stream counts as silent and the gain is left alone.
const SILENCE_LUFS: f64 = -45.0;

/// How quickly the gain follows when it has to come down...
const CUT_TIME_CONSTANT: f32 = 3.0;
/// ...and the most it may fall per second.
const CUT_SLEW_DB_PER_SEC: f32 = 6.0;
/// The same for going up, which is slower: a station coming back from a quiet
/// passage should not be pumped up.
const BOOST_TIME_CONSTANT: f32 = 10.0;
const BOOST_SLEW_DB_PER_SEC: f32 = 1.0;

/// The short-term window is only full after this long; until then the
/// momentary loudness is used and the gain may only come down.
const WARM_UP_SECS: f32 = 3.0;
/// The momentary loudness needs this much audio.
const MOMENTARY_SECS: f32 = 0.4;
/// After the stream's format changes the gain is held while the meter refills.
const FORMAT_CHANGE_HOLD_SECS: f32 = 3.0;

/// Peaks are limited to this level (-1 dBFS).
const LIMITER_CEILING: f32 = 0.891_25;
const LIMITER_ATTACK_SECS: f32 = 0.001;
const LIMITER_RELEASE_SECS: f32 = 0.15;

/// The loudness settings, shared between the engine and the stream being played.
#[derive(Debug)]
pub struct LevelControl {
    enabled: AtomicBool,
    /// The target in tenths of a LUFS.
    target_tenths: AtomicI32,
    /// The gain being applied right now, in hundredths of a dB.
    gain_centi_db: AtomicI32,
}

impl Default for LevelControl {
    fn default() -> Self {
        Self::new(true, DEFAULT_TARGET_LUFS)
    }
}

impl LevelControl {
    pub fn new(enabled: bool, target_lufs: f32) -> Self {
        let control = Self {
            enabled: AtomicBool::new(enabled),
            target_tenths: AtomicI32::new(0),
            gain_centi_db: AtomicI32::new(0),
        };
        control.set(enabled, target_lufs);
        control
    }

    pub fn set(&self, enabled: bool, target_lufs: f32) {
        let target = if target_lufs.is_finite() {
            target_lufs.clamp(-40.0, -5.0)
        } else {
            DEFAULT_TARGET_LUFS
        };
        self.target_tenths
            .store((target * 10.0).round() as i32, Relaxed);
        self.enabled.store(enabled, Relaxed);
    }

    pub fn enabled(&self) -> bool {
        self.enabled.load(Relaxed)
    }

    pub fn target_lufs(&self) -> f32 {
        self.target_tenths.load(Relaxed) as f32 / 10.0
    }

    /// The gain being applied, in dB (0 when switched off).
    pub fn gain_db(&self) -> f32 {
        self.gain_centi_db.load(Relaxed) as f32 / 100.0
    }

    fn publish_gain(&self, gain_db: f32) {
        self.gain_centi_db
            .store((gain_db * 100.0).round() as i32, Relaxed);
    }
}

/// What the meter was built for.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Format {
    channels: u16,
    rate: u32,
}

struct Meter {
    format: Format,
    ebu: EbuR128,
    /// Seconds of audio the meter has seen.
    fed_secs: f32,
    limiter_attack: f32,
    limiter_release: f32,
}

impl Meter {
    fn new(format: Format) -> Option<Self> {
        let mut ebu = EbuR128::new(u32::from(format.channels), format.rate, Mode::S).ok()?;
        if format.channels == 1 {
            // A mono stream is heard on both speakers, so it counts twice.
            ebu.set_channel_map(&[Channel::DualMono]).ok()?;
        }
        let coefficient = |secs: f32| (-1.0 / (secs * format.rate as f32)).exp();
        Some(Self {
            format,
            ebu,
            fed_secs: 0.0,
            limiter_attack: coefficient(LIMITER_ATTACK_SECS),
            limiter_release: coefficient(LIMITER_RELEASE_SECS),
        })
    }
}

pub(super) struct Leveller {
    control: std::sync::Arc<LevelControl>,
    meter: Option<Meter>,
    /// The gain the stream is being given, in dB.
    gain_db: f32,
    /// The linear gain at the end of the previous frame, to ramp from.
    applied: f32,
    /// The limiter's current gain reduction (1.0 = none).
    limit: f32,
    /// Seconds left of the hold after a format change.
    hold_secs: f32,
}

impl Leveller {
    pub(super) fn new(control: std::sync::Arc<LevelControl>) -> Self {
        Self {
            control,
            meter: None,
            gain_db: 0.0,
            applied: 1.0,
            limit: 1.0,
            hold_secs: 0.0,
        }
    }

    /// Back to the state of a stream that has just started.
    fn reset(&mut self) {
        self.meter = None;
        self.gain_db = 0.0;
        self.applied = 1.0;
        self.limit = 1.0;
        self.hold_secs = 0.0;
        self.control.publish_gain(0.0);
    }

    /// The gain the loudness measured so far asks for, if it can be trusted.
    fn desired_gain(&self, target: f32) -> Option<f32> {
        let meter = self.meter.as_ref()?;
        if meter.fed_secs < MOMENTARY_SECS {
            return None;
        }
        // The last 400 ms say whether the stream is silent right now; the
        // 3 s window would take that long to notice.
        let momentary = meter.ebu.loudness_momentary().ok()?;
        if momentary.is_nan() || momentary <= SILENCE_LUFS {
            return None;
        }
        let measured = if meter.fed_secs >= WARM_UP_SECS {
            meter.ebu.loudness_shortterm().ok()?
        } else {
            momentary
        };
        (measured.is_finite() && measured > SILENCE_LUFS)
            .then(|| (target - measured as f32).clamp(MIN_GAIN_DB, MAX_GAIN_DB))
    }

    fn move_gain(&mut self, desired: f32, secs: f32) {
        if desired < self.gain_db {
            let step = (desired - self.gain_db) * (1.0 - (-secs / CUT_TIME_CONSTANT).exp());
            self.gain_db += step.max(-CUT_SLEW_DB_PER_SEC * secs);
        } else if self
            .meter
            .as_ref()
            .is_some_and(|m| m.fed_secs >= WARM_UP_SECS)
        {
            let step = (desired - self.gain_db) * (1.0 - (-secs / BOOST_TIME_CONSTANT).exp());
            self.gain_db += step.min(BOOST_SLEW_DB_PER_SEC * secs);
        }
        self.gain_db = self.gain_db.clamp(MIN_GAIN_DB, MAX_GAIN_DB);
    }
}

impl FrameProcessor for Leveller {
    fn process(&mut self, frame: &mut [f32], info: SignalInfo) {
        let channels = usize::from(info.channels);
        let usable = (1..=2).contains(&channels)
            && info.rate >= 8_000
            && !frame.is_empty()
            && frame.len().is_multiple_of(channels);
        if !self.control.enabled() || !usable {
            if self.meter.is_some() || self.gain_db != 0.0 {
                self.reset();
            }
            return;
        }

        let format = Format {
            channels: info.channels,
            rate: info.rate,
        };
        if self.meter.as_ref().map(|m| m.format) != Some(format) {
            // The first stream starts at once; a changed format holds the
            // gain while the new meter fills.
            if self.meter.is_some() {
                self.hold_secs = FORMAT_CHANGE_HOLD_SECS;
            }
            self.meter = Meter::new(format);
        }
        let secs = (frame.len() / channels) as f32 / info.rate as f32;
        let target = self.control.target_lufs();

        if let Some(meter) = self.meter.as_mut() {
            if meter.ebu.add_frames_f32(frame).is_ok() {
                meter.fed_secs += secs;
            }
        }
        self.hold_secs = (self.hold_secs - secs).max(0.0);
        if self.hold_secs == 0.0 {
            if let Some(desired) = self.desired_gain(target) {
                self.move_gain(desired, secs);
            }
        }
        self.control.publish_gain(self.gain_db);

        let Some(meter) = self.meter.as_ref() else {
            return;
        };
        let (attack, release) = (meter.limiter_attack, meter.limiter_release);
        let from = self.applied;
        let to = 10f32.powf(self.gain_db / 20.0);
        let count = frame.len() / channels;
        for (index, samples) in frame.chunks_exact_mut(channels).enumerate() {
            let gain = from + (to - from) * (index + 1) as f32 / count as f32;
            let mut peak = 0.0_f32;
            for sample in samples.iter_mut() {
                *sample = if sample.is_finite() {
                    *sample * gain
                } else {
                    0.0
                };
                peak = peak.max(sample.abs());
            }
            let wanted = if peak > LIMITER_CEILING {
                LIMITER_CEILING / peak
            } else {
                1.0
            };
            let coefficient = if wanted < self.limit { attack } else { release };
            self.limit = wanted + (self.limit - wanted) * coefficient;
            for sample in samples.iter_mut() {
                *sample = (*sample * self.limit).clamp(-1.0, 1.0);
            }
        }
        self.applied = to;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use std::sync::Arc;

    const RATE: u32 = 16_000;
    /// A typical decoded frame.
    const FRAME: usize = 1_024;

    fn leveller(enabled: bool, target: f32) -> (Leveller, Arc<LevelControl>) {
        let control = Arc::new(LevelControl::new(enabled, target));
        (Leveller::new(Arc::clone(&control)), control)
    }

    /// A 997 Hz sine of amplitude `amp`, `secs` long, as interleaved frames.
    fn sine(amp: f32, channels: u16, rate: u32, secs: f32, start: &mut usize) -> Vec<f32> {
        let count = (secs * rate as f32) as usize;
        let mut out = Vec::with_capacity(count * usize::from(channels));
        for _ in 0..count {
            let value =
                amp * (2.0 * std::f32::consts::PI * 997.0 * *start as f32 / rate as f32).sin();
            *start += 1;
            for _ in 0..channels {
                out.push(value);
            }
        }
        out
    }

    /// Run `signal` through `leveller` a frame at a time.
    fn run(leveller: &mut Leveller, signal: &[f32], channels: u16, rate: u32) -> Vec<f32> {
        let mut out = Vec::with_capacity(signal.len());
        for chunk in signal.chunks(FRAME * usize::from(channels)) {
            let mut frame = chunk.to_vec();
            leveller.process(&mut frame, SignalInfo { channels, rate });
            out.extend(frame);
        }
        out
    }

    /// Short-term loudness of the last 3 s of `signal`.
    fn loudness(signal: &[f32], channels: u16, rate: u32) -> f64 {
        let mut meter = EbuR128::new(u32::from(channels), rate, Mode::S).unwrap();
        if channels == 1 {
            meter.set_channel_map(&[Channel::DualMono]).unwrap();
        }
        meter.add_frames_f32(signal).unwrap();
        meter.loudness_shortterm().unwrap()
    }

    fn amplitude_for(lufs: f32) -> f32 {
        // A stereo sine measures about its own level in dBFS.
        10f32.powf(lufs / 20.0)
    }

    fn steady(lufs: f32, secs: f32) -> Vec<f32> {
        sine(amplitude_for(lufs), 2, RATE, secs, &mut 0)
    }

    #[test]
    fn a_quiet_stream_is_raised_to_the_target() {
        let (mut leveller, control) = leveller(true, -16.0);
        let input = steady(-24.0, 90.0);
        assert!((loudness(&input, 2, RATE) + 24.0).abs() < 0.5);

        let output = run(&mut leveller, &input, 2, RATE);

        let tail = &output[output.len() - 3 * RATE as usize * 2..];
        let level = loudness(tail, 2, RATE);
        assert!((level + 16.0).abs() < 0.7, "settled at {level} LUFS");
        assert!(
            (control.gain_db() - 8.0).abs() < 0.7,
            "{}",
            control.gain_db()
        );
    }

    #[test]
    fn a_loud_stream_is_brought_down() {
        let (mut leveller, _) = leveller(true, -16.0);
        let input = steady(-8.0, 40.0);

        let output = run(&mut leveller, &input, 2, RATE);

        let tail = &output[output.len() - 3 * RATE as usize * 2..];
        let level = loudness(tail, 2, RATE);
        assert!((level + 16.0).abs() < 0.7, "settled at {level} LUFS");
    }

    #[test]
    fn the_boost_is_capped() {
        let (mut leveller, control) = leveller(true, -16.0);
        // 24 dB too quiet, but the leveller will not add more than 9.
        let input = steady(-40.0, 120.0);

        run(&mut leveller, &input, 2, RATE);

        assert!(
            (control.gain_db() - MAX_GAIN_DB).abs() < 0.1,
            "{}",
            control.gain_db()
        );
    }

    #[test]
    fn the_cut_is_capped() {
        let (mut leveller, control) = leveller(true, -16.0);
        // 20 dB too loud, but the leveller will not take off more than 12.
        let input = steady(-2.5, 60.0);

        run(&mut leveller, &input, 2, RATE);

        assert!(
            (control.gain_db() - MIN_GAIN_DB).abs() < 0.2,
            "{}",
            control.gain_db()
        );
    }

    #[test]
    fn nothing_is_boosted_during_warm_up() {
        let (mut leveller, control) = leveller(true, -16.0);

        run(&mut leveller, &steady(-26.0, 2.9), 2, RATE);

        assert!(control.gain_db() <= 0.0, "{}", control.gain_db());
    }

    #[test]
    fn silence_freezes_the_gain_and_stays_finite() {
        let (mut leveller, control) = leveller(true, -16.0);
        run(&mut leveller, &steady(-24.0, 60.0), 2, RATE);
        let settled = control.gain_db();
        assert!(settled > 5.0);

        let silence = vec![0.0_f32; 30 * RATE as usize * 2];
        let output = run(&mut leveller, &silence, 2, RATE);

        // Only the last 400 ms of music can still move it.
        assert!((control.gain_db() - settled).abs() < 0.1);
        assert!(output.iter().all(|s| *s == 0.0));
    }

    #[test]
    fn a_stream_that_starts_silent_is_not_a_problem() {
        let (mut leveller, control) = leveller(true, -16.0);
        let mut input = vec![0.0_f32; 10 * RATE as usize * 2];
        input.extend(steady(-30.0, 20.0));

        let output = run(&mut leveller, &input, 2, RATE);

        assert!(output.iter().all(|s| s.is_finite()));
        assert!(control.gain_db() >= 0.0);
    }

    #[test]
    fn peaks_never_pass_full_scale_even_when_boosted() {
        let (mut leveller, control) = leveller(true, -16.0);
        let mut input = steady(-25.0, 40.0);
        // A click every 100 ms that is 12 dB above the music.
        for index in (0..input.len()).step_by(RATE as usize / 10 * 2) {
            input[index] = 0.7;
            input[index + 1] = -0.7;
        }

        let output = run(&mut leveller, &input, 2, RATE);

        assert!(control.gain_db() > 3.0, "the test needs a real boost");
        let peak = output.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
        assert!(peak <= 1.0, "peak {peak}");
        assert!(peak > 0.8, "the limiter should not mute the clicks: {peak}");
    }

    #[test]
    fn mono_and_stereo_end_up_at_the_same_gain() {
        let (mut stereo, stereo_control) = leveller(true, -16.0);
        let (mut mono, mono_control) = leveller(true, -16.0);
        let amp = amplitude_for(-24.0);

        run(&mut stereo, &sine(amp, 2, RATE, 90.0, &mut 0), 2, RATE);
        run(&mut mono, &sine(amp, 1, RATE, 90.0, &mut 0), 1, RATE);

        let difference = (stereo_control.gain_db() - mono_control.gain_db()).abs();
        assert!(difference < 0.3, "{difference}");
    }

    #[test]
    fn a_format_change_holds_the_gain_and_keeps_going() {
        let (mut leveller, control) = leveller(true, -16.0);
        run(&mut leveller, &steady(-24.0, 60.0), 2, RATE);
        let before = control.gain_db();
        assert!(before > 5.0);

        // Stereo 16 kHz becomes mono 24 kHz, at a very different level.
        let mono = sine(amplitude_for(-8.0), 1, 24_000, 2.5, &mut 0);
        let output = run(&mut leveller, &mono, 1, 24_000);

        assert_eq!(control.gain_db(), before, "held while the meter refills");
        assert!(output.iter().all(|s| s.is_finite() && s.abs() <= 1.0));

        let later = sine(amplitude_for(-8.0), 1, 24_000, 30.0, &mut 0);
        run(&mut leveller, &later, 1, 24_000);
        assert!(control.gain_db() < before, "then it follows the new stream");
    }

    #[test]
    fn switched_off_it_changes_nothing() {
        let (mut leveller, control) = leveller(false, -16.0);
        let input = steady(-30.0, 10.0);

        let output = run(&mut leveller, &input, 2, RATE);

        assert_eq!(output, input);
        assert_eq!(control.gain_db(), 0.0);
    }

    #[test]
    fn it_can_be_switched_on_and_off_while_playing() {
        let (mut leveller, control) = leveller(true, -16.0);
        let input = steady(-24.0, 60.0);
        run(&mut leveller, &input, 2, RATE);
        assert!(control.gain_db() > 5.0);

        control.set(false, -16.0);
        let output = run(&mut leveller, &input[..FRAME * 2], 2, RATE);
        assert_eq!(output, &input[..FRAME * 2]);
        assert_eq!(control.gain_db(), 0.0);

        control.set(true, -16.0);
        run(&mut leveller, &input, 2, RATE);
        assert!(control.gain_db() > 0.0, "it starts over");
    }

    #[test]
    fn a_new_target_is_followed() {
        let (mut leveller, control) = leveller(true, -16.0);
        let input = steady(-24.0, 120.0);
        run(&mut leveller, &input[..input.len() / 2], 2, RATE);
        let first = control.gain_db();

        control.set(true, -23.0);
        run(&mut leveller, &input[input.len() / 2..], 2, RATE);

        assert!(
            control.gain_db() < first - 4.0,
            "{first} -> {}",
            control.gain_db()
        );
    }

    #[test]
    fn the_gain_changes_gently() {
        let (mut leveller, control) = leveller(true, -16.0);
        // Quiet, then suddenly loud, then quiet again.
        let mut input = steady(-26.0, 30.0);
        input.extend(steady(-6.0, 20.0));
        input.extend(steady(-26.0, 30.0));

        let mut previous = 0.0_f32;
        for chunk in input.chunks(FRAME * 2) {
            let mut frame = chunk.to_vec();
            leveller.process(
                &mut frame,
                SignalInfo {
                    channels: 2,
                    rate: RATE,
                },
            );
            let step = control.gain_db() - previous;
            let seconds = (chunk.len() / 2) as f32 / RATE as f32;
            assert!(step <= BOOST_SLEW_DB_PER_SEC * seconds + 0.011, "up {step}");
            assert!(
                step >= -CUT_SLEW_DB_PER_SEC * seconds - 0.011,
                "down {step}"
            );
            previous = control.gain_db();
        }
    }

    #[test]
    fn unusable_formats_pass_through() {
        let (mut leveller, control) = leveller(true, -16.0);
        let mut surround = vec![0.5_f32; 600];
        leveller.process(
            &mut surround,
            SignalInfo {
                channels: 6,
                rate: 48_000,
            },
        );
        assert!(surround.iter().all(|s| *s == 0.5));
        let mut odd = vec![0.5_f32; 7];
        leveller.process(
            &mut odd,
            SignalInfo {
                channels: 2,
                rate: 48_000,
            },
        );
        assert!(odd.iter().all(|s| *s == 0.5));
        assert_eq!(control.gain_db(), 0.0);
    }

    #[test]
    fn the_control_clamps_and_remembers() {
        let control = LevelControl::default();
        assert!(control.enabled());
        assert_eq!(control.target_lufs(), -16.0);
        control.set(false, -23.0);
        assert!(!control.enabled());
        assert_eq!(control.target_lufs(), -23.0);
        control.set(true, f32::NAN);
        assert_eq!(control.target_lufs(), DEFAULT_TARGET_LUFS);
        control.set(true, -500.0);
        assert_eq!(control.target_lufs(), -40.0);
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(24))]

        #[test]
        fn any_audio_comes_out_finite_and_in_range(
            frames in proptest::collection::vec(
                (
                    1u16..=2,
                    prop_oneof![Just(8_000u32), Just(16_000), Just(44_100)],
                    proptest::collection::vec(-4.0f32..4.0, 0..3_000),
                    0u8..8,
                ),
                1..12,
            ),
        ) {
            let (mut leveller, _) = leveller(true, -16.0);
            for (channels, rate, mut samples, kind) in frames {
                if kind == 0 {
                    if let Some(first) = samples.first_mut() { *first = f32::NAN; }
                }
                let usable = samples.len() - samples.len() % usize::from(channels);
                samples.truncate(usable);
                leveller.process(&mut samples, SignalInfo { channels, rate });
                prop_assert!(samples.iter().all(|s| s.is_finite() && s.abs() <= 4.0));
            }
        }

        #[test]
        fn leveled_audio_never_passes_full_scale(
            amp in 0.0f32..1.0,
            seconds in 1.0f32..8.0,
        ) {
            let (mut leveller, _) = leveller(true, -14.0);
            let input = sine(amp, 2, RATE, seconds, &mut 0);
            let output = run(&mut leveller, &input, 2, RATE);
            prop_assert!(output.iter().all(|s| s.abs() <= 1.0));
        }
    }
}
