use super::AudioStatus;
use std::sync::{mpsc, Mutex};
use std::time::Instant;

const EWMA_ALPHA: f32 = 0.15;
const DEFAULT_FALLBACK_BYTES_PER_SEC: f32 = 16_000.0;
const MIN_MEASURED_SECONDS: f32 = 0.001;

#[derive(Debug)]
pub(super) struct BufferStatusMeter {
    fallback_velocity: f32,
    state: Mutex<BufferStatusState>,
}

#[derive(Debug)]
struct BufferStatusState {
    historical_velocity: f32,
    last_consumed_at: Option<Instant>,
    last_sent_percent: Option<u8>,
    last_sent_seconds: Option<u32>,
}

impl BufferStatusMeter {
    pub(super) fn new(fallback_bytes_per_sec: usize) -> Self {
        let fallback_velocity = (fallback_bytes_per_sec as f32).max(DEFAULT_FALLBACK_BYTES_PER_SEC);

        Self {
            fallback_velocity,
            state: Mutex::new(BufferStatusState {
                historical_velocity: fallback_velocity,
                last_consumed_at: None,
                last_sent_percent: None,
                last_sent_seconds: None,
            }),
        }
    }

    pub(super) fn report_fill_level(
        &self,
        len: usize,
        capacity: usize,
        status_tx: &mpsc::Sender<AudioStatus>,
    ) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        let (percent, seconds) = buffer_status_from_velocity(
            len,
            capacity,
            state.historical_velocity,
            self.fallback_velocity,
        );

        send_buffer_status_if_changed(status_tx, &mut state, percent, seconds);
    }

    pub(super) fn note_consumed(
        &self,
        bytes_read: usize,
        len: usize,
        capacity: usize,
        status_tx: &mpsc::Sender<AudioStatus>,
    ) {
        if bytes_read == 0 {
            return;
        }

        let Ok(mut state) = self.state.lock() else {
            return;
        };
        let now = Instant::now();
        let delta_t = state
            .last_consumed_at
            .replace(now)
            .map(|last| now.saturating_duration_since(last).as_secs_f32())
            .unwrap_or(0.0);

        let (percent, seconds) = if delta_t >= MIN_MEASURED_SECONDS {
            buffer_level_status_adaptive_with_fallback(
                len,
                capacity,
                &mut state.historical_velocity,
                bytes_read,
                delta_t,
                self.fallback_velocity,
            )
        } else {
            buffer_status_from_velocity(
                len,
                capacity,
                state.historical_velocity,
                self.fallback_velocity,
            )
        };

        send_buffer_status_if_changed(status_tx, &mut state, percent, seconds);
    }
}

fn send_buffer_status_if_changed(
    status_tx: &mpsc::Sender<AudioStatus>,
    state: &mut BufferStatusState,
    percent: u8,
    seconds: u32,
) {
    let changed =
        state.last_sent_percent != Some(percent) || state.last_sent_seconds != Some(seconds);

    if changed {
        state.last_sent_percent = Some(percent);
        state.last_sent_seconds = Some(seconds);
        let _ = status_tx.send(AudioStatus::BufferLevel { percent, seconds });
    }
}

fn buffer_level_status_adaptive_with_fallback(
    len: usize,
    capacity: usize,
    historical_velocity: &mut f32,
    instant_bytes_read: usize,
    delta_t: f32,
    fallback_velocity: f32,
) -> (u8, u32) {
    if instant_bytes_read > 0 && delta_t >= MIN_MEASURED_SECONDS {
        let current_velocity = instant_bytes_read as f32 / delta_t;
        *historical_velocity = ewma_velocity(*historical_velocity, current_velocity);
    }

    buffer_status_from_velocity(len, capacity, *historical_velocity, fallback_velocity)
}

fn ewma_velocity(previous: f32, current: f32) -> f32 {
    EWMA_ALPHA.mul_add(current, (1.0 - EWMA_ALPHA) * previous)
}

fn buffer_status_from_velocity(
    len: usize,
    capacity: usize,
    historical_velocity: f32,
    fallback_velocity: f32,
) -> (u8, u32) {
    if capacity == 0 {
        return (0, 0);
    }

    let percent = len.saturating_mul(100).checked_div(capacity).unwrap_or(0);
    let percent = percent.min(100) as u8;

    let safe_velocity = if historical_velocity > 1.0 {
        historical_velocity
    } else {
        fallback_velocity.max(DEFAULT_FALLBACK_BYTES_PER_SEC)
    };
    let seconds = (len as f32 / safe_velocity).round() as u32;

    (percent, seconds)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adaptive_status_smooths_consumption_velocity_spikes() {
        let mut velocity = 16_000.0;
        let (_, seconds) = buffer_level_status_adaptive_with_fallback(
            160_000,
            1_000_000,
            &mut velocity,
            64_000,
            1.0,
            16_000.0,
        );

        assert!((velocity - 23_200.0).abs() < 0.1);
        assert_eq!(seconds, 7);
    }

    #[test]
    fn adaptive_status_uses_fallback_when_history_is_not_ready() {
        let mut velocity = 0.0;
        let (percent, seconds) = buffer_level_status_adaptive_with_fallback(
            160_000,
            1_000_000,
            &mut velocity,
            0,
            0.0,
            16_000.0,
        );

        assert_eq!(percent, 16);
        assert_eq!(seconds, 10);
    }

    #[test]
    fn adaptive_status_clamps_percent_to_capacity() {
        let mut velocity = 16_000.0;
        let (percent, seconds) = buffer_level_status_adaptive_with_fallback(
            2_000_000,
            1_000_000,
            &mut velocity,
            16_000,
            1.0,
            16_000.0,
        );

        assert_eq!(percent, 100);
        assert_eq!(seconds, 125);
    }

    #[test]
    fn adaptive_status_handles_zero_capacity() {
        let mut velocity = 16_000.0;
        let (percent, seconds) = buffer_level_status_adaptive_with_fallback(
            160_000,
            0,
            &mut velocity,
            16_000,
            1.0,
            16_000.0,
        );

        assert_eq!(percent, 0);
        assert_eq!(seconds, 0);
    }

    #[test]
    fn ewma_velocity_damps_burst_variability() {
        let first = ewma_velocity(16_000.0, 64_000.0);
        let second = ewma_velocity(first, 8_000.0);

        assert!(first < 64_000.0);
        assert!(second > 8_000.0);
        assert!(second < first);
    }

    #[test]
    fn buffer_status_sends_first_measurement() {
        let meter = BufferStatusMeter::new(16_000);
        let (tx, rx) = std::sync::mpsc::channel();

        meter.report_fill_level(160_000, 1_000_000, &tx);

        assert!(matches!(
            rx.try_recv(),
            Ok(AudioStatus::BufferLevel {
                percent: 16,
                seconds: 10
            })
        ));
    }

    #[test]
    fn buffer_status_suppresses_identical_measurements() {
        let meter = BufferStatusMeter::new(16_000);
        let (tx, rx) = std::sync::mpsc::channel();

        meter.report_fill_level(160_000, 1_000_000, &tx);
        meter.report_fill_level(160_000, 1_000_000, &tx);

        assert!(rx.try_recv().is_ok());
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn buffer_status_sends_when_percent_changes() {
        let meter = BufferStatusMeter::new(16_000);
        let (tx, rx) = std::sync::mpsc::channel();

        meter.report_fill_level(160_000, 1_000_000, &tx);
        meter.report_fill_level(170_000, 1_000_000, &tx);

        assert!(rx.try_recv().is_ok());
        assert!(rx.try_recv().is_ok());
    }

    #[test]
    fn buffer_status_sends_when_seconds_changes() {
        let meter = BufferStatusMeter::new(16_000);
        let (tx, rx) = std::sync::mpsc::channel();

        meter.report_fill_level(160_000, 1_000_000, &tx);
        meter.report_fill_level(168_000, 1_000_000, &tx);

        assert!(rx.try_recv().is_ok());
        assert!(rx.try_recv().is_ok());
    }
}
