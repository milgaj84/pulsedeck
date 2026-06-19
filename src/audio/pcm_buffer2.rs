use rodio::cpal::Sample as CpalSample;
use rodio::{Sample as RodioSample, Source as RodioSource};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub(super) struct PcmBufferSource<S>
where
    S: RodioSource,
    S::Item: RodioSample + CpalSample<Float = f32> + Copy,
{
    inner: S,
    sample_buffer: Arc<Mutex<VecDeque<f32>>>,
    local_vis: Vec<f32>,
}

impl<S> PcmBufferSource<S>
where
    S: RodioSource,
    S::Item: RodioSample + CpalSample<Float = f32> + Copy,
{
    pub(super) fn new(inner: S, sample_buffer: Arc<Mutex<VecDeque<f32>>>) -> Self {
        Self { inner, sample_buffer, local_vis: Vec::with_capacity(512) }
    }
}

impl<S> Iterator for PcmBufferSource<S>
where
    S: RodioSource,
    S::Item: RodioSample + CpalSample<Float = f32> + Copy,
{
    type Item = S::Item;
    fn next(&mut self) -> Option<Self::Item> {
        let item = self.inner.next()?;
        self.local_vis.push(item.to_float_sample());
        if self.local_vis.len() >= 512 {
            if let Ok(mut buffer) = self.sample_buffer.try_lock() {
                buffer.extend(self.local_vis.drain(..));
                while buffer.len() > 4096 { buffer.pop_front(); }
            } else {
                self.local_vis.clear();
            }
        }
        Some(item)
    }
}

impl<S> RodioSource for PcmBufferSource<S>
where
    S: RodioSource,
    S::Item: RodioSample + CpalSample<Float = f32> + Copy,
{
    fn current_frame_len(&self) -> Option<usize> { self.inner.current_frame_len() }
    fn channels(&self) -> u16 { self.inner.channels() }
    fn sample_rate(&self) -> u32 { self.inner.sample_rate() }
    fn total_duration(&self) -> Option<Duration> { self.inner.total_duration() }
}
