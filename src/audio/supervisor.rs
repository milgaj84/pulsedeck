use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering::SeqCst};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use super::types::{ConnectRequest, EngineError, EngineEvent, Generation};

/// Hard ceiling for active plus retired connection workers.
///
/// A healthy retired worker exits after the next bounded network read. Refusing
/// to spawn beyond this ceiling is safer than allowing a failing station or
/// network stack to create an unbounded number of detached threads.
const MAX_OUTSTANDING_WORKERS: usize = 8;

/// Manages generation IDs and the lifecycle of connection/decode workers.
///
/// Every `Play` command allocates a new monotonically increasing `Generation`.
/// Workers carry their generation and check the shared `AtomicU64` before and
/// after blocking stages. Stale workers are retired, retained until completion,
/// and joined only after `JoinHandle::is_finished()` reports that joining cannot
/// block the control loop.
pub(super) struct ConnectionSupervisor {
    active_generation: Arc<AtomicU64>,
    current: Generation,
    worker: Option<JoinHandle<()>>,
    retired_workers: VecDeque<JoinHandle<()>>,
}

impl ConnectionSupervisor {
    pub(super) fn new() -> Self {
        Self {
            active_generation: Arc::new(AtomicU64::new(0)),
            current: 0,
            worker: None,
            retired_workers: VecDeque::new(),
        }
    }

    /// Allocate the next non-zero generation.
    pub(super) fn next_generation(&mut self) -> Generation {
        self.current = self.current.wrapping_add(1);
        if self.current == 0 {
            self.current = 1;
        }
        self.active_generation.store(self.current, SeqCst);
        self.current
    }

    /// Spawn a worker while retaining and bounding stale worker handles.
    pub(super) fn spawn(
        &mut self,
        req: ConnectRequest,
        event_tx: mpsc::Sender<EngineEvent>,
        sample_buffer: Arc<Mutex<VecDeque<f32>>>,
    ) {
        self.retire_active();
        self.reap_finished();

        if self.outstanding_worker_count() >= MAX_OUTSTANDING_WORKERS {
            let _ = event_tx.send(EngineEvent::Failed {
                generation: req.generation,
                error: EngineError::Connect(format!(
                    "too many stalled connection workers ({MAX_OUTSTANDING_WORKERS}); wait for previous attempts to finish"
                )),
            });
            return;
        }

        let active_gen_arc = Arc::clone(&self.active_generation);
        let handle = std::thread::spawn(move || {
            super::decode::run_worker(req, event_tx, active_gen_arc, sample_buffer);
        });
        self.worker = Some(handle);
    }

    /// Mark the current worker stale without blocking the control thread.
    pub(super) fn abandon(&mut self) {
        self.active_generation.store(0, SeqCst);
        self.retire_active();
        self.reap_finished();
    }

    /// Join retired workers that have already finished.
    pub(super) fn reap_finished(&mut self) {
        let mut pending = VecDeque::with_capacity(self.retired_workers.len());
        while let Some(handle) = self.retired_workers.pop_front() {
            if handle.is_finished() {
                let _ = handle.join();
            } else {
                pending.push_back(handle);
            }
        }
        self.retired_workers = pending;
    }

    pub(super) fn is_active(&self, generation: Generation) -> bool {
        generation == self.active_generation.load(SeqCst)
    }

    fn retire_active(&mut self) {
        if let Some(handle) = self.worker.take() {
            self.retired_workers.push_back(handle);
        }
    }

    fn outstanding_worker_count(&self) -> usize {
        usize::from(self.worker.is_some()) + self.retired_workers.len()
    }

    #[cfg(test)]
    pub(super) fn active_generation_arc(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.active_generation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc as std_mpsc;
    use std::time::Duration;

    #[test]
    fn next_generation_starts_at_one() {
        let mut supervisor = ConnectionSupervisor::new();
        assert_eq!(supervisor.next_generation(), 1);
    }

    #[test]
    fn next_generation_is_strictly_increasing() {
        let mut supervisor = ConnectionSupervisor::new();
        let first = supervisor.next_generation();
        let second = supervisor.next_generation();
        let third = supervisor.next_generation();
        assert!(first < second && second < third);
    }

    #[test]
    fn generation_wrap_skips_zero_sentinel() {
        let mut supervisor = ConnectionSupervisor::new();
        supervisor.current = u64::MAX;

        assert_eq!(supervisor.next_generation(), 1);
        assert!(supervisor.is_active(1));
    }

    #[test]
    fn is_active_rejects_stale_generation() {
        let mut supervisor = ConnectionSupervisor::new();
        let old = supervisor.next_generation();
        let current = supervisor.next_generation();

        assert!(!supervisor.is_active(old));
        assert!(supervisor.is_active(current));
    }

    #[test]
    fn abandon_sets_active_generation_to_zero() {
        let mut supervisor = ConnectionSupervisor::new();
        let generation = supervisor.next_generation();
        assert!(supervisor.is_active(generation));

        supervisor.abandon();

        assert!(!supervisor.is_active(generation));
        assert_eq!(supervisor.active_generation.load(SeqCst), 0);
    }

    #[test]
    fn active_generation_arc_shares_same_counter() {
        let mut supervisor = ConnectionSupervisor::new();
        let shared = supervisor.active_generation_arc();
        let generation = supervisor.next_generation();
        assert_eq!(shared.load(SeqCst), generation);
    }

    #[test]
    fn retire_active_keeps_handle_until_worker_finishes() {
        let mut supervisor = ConnectionSupervisor::new();
        let (release_tx, release_rx) = std_mpsc::channel::<()>();
        supervisor.worker = Some(std::thread::spawn(move || {
            let _ = release_rx.recv();
        }));

        supervisor.retire_active();

        assert_eq!(supervisor.outstanding_worker_count(), 1);
        assert!(supervisor.worker.is_none());
        assert_eq!(supervisor.retired_workers.len(), 1);

        release_tx.send(()).unwrap();
        for _ in 0..50 {
            supervisor.reap_finished();
            if supervisor.outstanding_worker_count() == 0 {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(supervisor.outstanding_worker_count(), 0);
    }

    #[test]
    fn reap_finished_keeps_running_workers_and_joins_finished_workers() {
        let mut supervisor = ConnectionSupervisor::new();
        let (release_tx, release_rx) = std_mpsc::channel::<()>();
        supervisor
            .retired_workers
            .push_back(std::thread::spawn(|| {}));
        supervisor
            .retired_workers
            .push_back(std::thread::spawn(move || {
                let _ = release_rx.recv();
            }));

        for _ in 0..50 {
            supervisor.reap_finished();
            if supervisor.retired_workers.len() == 1 {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(supervisor.retired_workers.len(), 1);

        release_tx.send(()).unwrap();
        for _ in 0..50 {
            supervisor.reap_finished();
            if supervisor.retired_workers.is_empty() {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(supervisor.retired_workers.is_empty());
    }

    #[test]
    fn outstanding_worker_count_includes_active_and_retired() {
        let mut supervisor = ConnectionSupervisor::new();
        supervisor.worker = Some(std::thread::spawn(|| {}));
        supervisor
            .retired_workers
            .push_back(std::thread::spawn(|| {}));

        assert_eq!(supervisor.outstanding_worker_count(), 2);
    }

    use proptest::prelude::*;

    proptest! {
        #[test]
        fn prop_next_generation_strictly_monotonic(count in 1usize..=100usize) {
            let mut supervisor = ConnectionSupervisor::new();
            let mut previous = 0u64;
            for _ in 0..count {
                let generation = supervisor.next_generation();
                prop_assert!(generation > previous);
                previous = generation;
            }
        }
    }
}