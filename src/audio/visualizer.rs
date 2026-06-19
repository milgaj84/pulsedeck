use rodio::cpal::Sample as CpalSample;
use rodio::{Sample as RodioSample, Source as RodioSource};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const SAMPLE_BATCH_SIZE: usize = 512;
const MAX_VISUALIZER_SAMPLES: usize = 4096;

/// A passive source tap for the visualizer.
///
/// This wrapper deliberately does not create another decoded-PCM pipeline. Rodio
/// pulls from the decoder exactly once, and the visualizer only copies a small
/// batch of float samples when the UI buffer is available.
pub(super) struct VisualizerSource<S>
where
    S: RodioSource,
    S::Item: RodioSample + CpalSample<Float = f32> + Copy,
{
    inner: S,
    sample_buffer: Arc<Mutex<VecDeque<f32>>>,
    local_buf: Vec<f32>,
}

impl<S> VisualizerSource<S>
where
    S: RodioSource,
    S::Item: RodioSample + CpalSample<Float = f32> + Copy,
{
    pub fn new(inner: S, sample_buffer: Arc<Mutex<VecDeque<f32>>>) -> Self {
        Self {
            inner,
            sample_buffer,
            local_buf: Vec::with_capacity(SAMPLE_BATCH_SIZE),
        }
    }

    fn capture_visualizer_sample(&mut self, sample: S::Item) {
        self.local_buf.push(sample.to_float_sample());
        if self.local_buf.len() < SAMPLE_BATCH_SIZE {
            return;
        }

        if let Ok(mut buffer) = self.sample_buffer.try_lock() {
            buffer.extend(self.local_buf.drain(..));
            while buffer.len() > MAX_VISUALIZER_SAMPLES {
                buffer.pop_front();
            }
        } else {
            self.local_buf.clear();
        }
    }
}

impl<S> Iterator for VisualizerSource<S>
where
    S: RodioSource,
    S::Item: RodioSample + CpalSample<Float = f32> + Copy,
{
    type Item = S::Item;

    fn next(&mut self) -> Option<Self::Item> {
        let sample = self.inner.next()?;
        self.capture_visualizer_sample(sample);
        Some(sample)
    }
}

impl<S> RodioSource for VisualizerSource<S>
where
    S: RodioSource,
    S::Item: RodioSample + CpalSample<Float = f32> + Copy,
{
    fn current_frame_len(&self) -> Option<usize> {
        self.inner.current_frame_len()
    }

    fn channels(&self) -> u16 {
        self.inner.channels()
    }

    fn sample_rate(&self) -> u32 {
        self.inner.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visualizer_constants_keep_small_ui_buffer() {
        assert_eq!(SAMPLE_BATCH_SIZE, 512);
        assert_eq!(MAX_VISUALIZER_SAMPLES, 4096);
    }
}
