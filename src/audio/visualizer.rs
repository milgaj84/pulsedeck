use rodio::cpal::Sample as CpalSample;
use rodio::{Sample as RodioSample, Source as RodioSource};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// A custom source wrapper that intercepts audio sample frames, buffers them,
/// and writes them to a thread-safe circular sample buffer for rendering.
pub(super) struct VisualizerSource<S>
where
    S: RodioSource,
    S::Item: RodioSample + CpalSample<Float = f32>,
{
    inner: S,
    sample_buffer: Arc<Mutex<VecDeque<f32>>>,
    local_buf: Vec<f32>,
}

impl<S> VisualizerSource<S>
where
    S: RodioSource,
    S::Item: RodioSample + CpalSample<Float = f32>,
{
    pub fn new(inner: S, sample_buffer: Arc<Mutex<VecDeque<f32>>>) -> Self {
        Self {
            inner,
            sample_buffer,
            local_buf: Vec::with_capacity(128),
        }
    }
}

impl<S> Iterator for VisualizerSource<S>
where
    S: RodioSource,
    S::Item: RodioSample + CpalSample<Float = f32>,
{
    type Item = S::Item;

    fn next(&mut self) -> Option<Self::Item> {
        let sample = self.inner.next();
        if let Some(s) = sample {
            let float_sample = s.to_float_sample();
            self.local_buf.push(float_sample);
            if self.local_buf.len() >= 128 {
                if let Ok(mut buffer) = self.sample_buffer.lock() {
                    buffer.extend(self.local_buf.drain(..));
                    while buffer.len() > 4096 {
                        buffer.pop_front();
                    }
                } else {
                    self.local_buf.clear();
                }
            }
        }
        sample
    }
}

impl<S> RodioSource for VisualizerSource<S>
where
    S: RodioSource,
    S::Item: RodioSample + CpalSample<Float = f32>,
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
