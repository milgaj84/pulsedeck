use rodio::cpal::Sample as CpalSample;
use rodio::{Sample as RodioSample, Source as RodioSource};
use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

const MAX_PCM_SECONDS: usize = 6;
const READY_PCM_MS: usize = 600;
const VIS_BATCH: usize = 512;
const MAX_VIS_SAMPLES: usize = 4096;

struct PcmState<T> {
    samples: VecDeque<T>,
    finished: bool,
}

struct SharedPcm<T> {
    state: Mutex<PcmState<T>>,
    can_write: Condvar,
    can_read: Condvar,
    max_samples: usize,
}

pub(super) struct PcmBufferSource<S>
where
    S: RodioSource + Send + 'static,
    S::Item: RodioSample + CpalSample<Float = f32> + Copy + Send + 'static,
{
    shared: Arc<SharedPcm<S::Item>>,
    sample_buffer: Arc<Mutex<VecDeque<f32>>>,
    local_vis: Vec<f32>,
    channels: u16,
    sample_rate: u32,
    total_duration: Option<Duration>,
}

impl<S> PcmBufferSource<S>
where
    S: RodioSource + Send + 'static,
    S::Item: RodioSample + CpalSample<Float = f32> + Copy + Send + 'static,
{
    pub(super) fn spawn(source: S, sample_buffer: Arc<Mutex<VecDeque<f32>>>) -> Self {
        let channels = source.channels();
        let sample_rate = source.sample_rate();
        let total_duration = source.total_duration();
        let max_samples = sample_count(sample_rate, channels, MAX_PCM_SECONDS).max(VIS_BATCH * 4);
        let shared = Arc::new(SharedPcm {
            state: Mutex::new(PcmState {
                samples: VecDeque::with_capacity(max_samples.min(262_144)),
                finished: false,
            }),
            can_write: Condvar::new(),
            can_read: Condvar::new(),
            max_samples,
        });
        run_pcm_worker(source, shared.clone());
        Self {
            shared,
            sample_buffer,
            local_vis: Vec::with_capacity(VIS_BATCH),
            channels,
            sample_rate,
            total_duration,
        }
    }

    pub(super) fn wait_ready(&self, max_wait: Duration) -> usize {
        let target = sample_count_ms(self.sample_rate, self.channels, READY_PCM_MS).max(VIS_BATCH);
        let deadline = Instant::now() + max_wait;
        let mut state = self.shared.state.lock().unwrap();
        while state.samples.len() < target && !state.finished {
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            let remaining = deadline.saturating_duration_since(now);
            let (next_state, timeout) = self.shared.can_read.wait_timeout(state, remaining).unwrap();
            state = next_state;
            if timeout.timed_out() {
                break;
            }
        }
        state.samples.len()
    }
}
