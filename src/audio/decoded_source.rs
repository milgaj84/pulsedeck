use rodio::cpal::Sample as CpalSample;
use rodio::{Sample as RodioSample, Source as RodioSource};
use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

const MAX_DECODED_SECONDS: usize = 6;
const INITIAL_DECODED_BUFFER_MS: usize = 600;
const SAMPLE_BATCH_SIZE: usize = 512;
const MAX_VISUALIZER_SAMPLES: usize = 4096;

struct DecodedBuffer<T> {
    samples: VecDeque<T>,
    finished: bool,
}

struct SharedDecodedBuffer<T> {
    state: Mutex<DecodedBuffer<T>>,
    can_write: Condvar,
    can_read: Condvar,
    max_samples: usize,
}

pub(super) struct DecodedSource<S>
where
    S: RodioSource + Send + 'static,
    S::Item: RodioSample + CpalSample<Float = f32> + Copy + Send + 'static,
{
    shared: Arc<SharedDecodedBuffer<S::Item>>,
    sample_buffer: Arc<Mutex<VecDeque<f32>>>,
    local_buf: Vec<f32>,
    channels: u16,
    sample_rate: u32,
    total_duration: Option<Duration>,
}

impl<S> DecodedSource<S>
where
    S: RodioSource + Send + 'static,
    S::Item: RodioSample + CpalSample<Float = f32> + Copy + Send + 'static,
{
    pub(super) fn spawn(source: S, sample_buffer: Arc<Mutex<VecDeque<f32>>>) -> Self {
        let channels = source.channels();
        let sample_rate = source.sample_rate();
        let total_duration = source.total_duration();
        let max_samples = decoded_sample_count(sample_rate, channels, MAX_DECODED_SECONDS)
            .max(SAMPLE_BATCH_SIZE * 4);

        let shared = Arc::new(SharedDecodedBuffer {
            state: Mutex::new(DecodedBuffer {
                samples: VecDeque::with_capacity(max_samples.min(262_144)),
                finished: false,
            }),
            can_write: Condvar::new(),
            can_read: Condvar::new(),
            max_samples,
        });

        spawn_decoder_thread(source, shared.clone());

        Self {
            shared,
            sample_buffer,
            local_buf: Vec::with_capacity(SAMPLE_BATCH_SIZE),
            channels,
            sample_rate,
            total_duration,
        }
    }
}
