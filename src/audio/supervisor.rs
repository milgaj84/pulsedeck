use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering::SeqCst};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use super::types::{ConnectRequest, EngineEvent, Generation};

// ---------------------------------------------------------------------------
// ConnectionSupervisor
// ---------------------------------------------------------------------------

/// Manages generation IDs and the lifecycle of connection/decode workers.
///
/// Every `Play` command allocates a new monotonically increasing `Generation`.
/// Workers carry their generation and check the shared `AtomicU64` on each
/// blocking step; any result from a non-active generation is discarded as
/// `Abandoned`.  This makes rapid station switching safe and eliminates
/// stale-retry storms.
///
/// # Invariants
/// - Generation 0 is the "abandoned / none" sentinel; real generations start at 1.
/// - `active_generation` always equals `current` except during `abandon()`,
///   where it is reset to 0.
/// - The control thread never calls `join()` on a stale worker (non-blocking
///   discard via `drop`).
pub(super) struct ConnectionSupervisor {
    /// Shared atomic counter read by workers to detect abandonment.
    active_generation: Arc<AtomicU64>,
    /// The most recently allocated generation (mirrors `active_generation`
    /// while a worker is live).
    current: Generation,
    /// Handle to the most recently spawned worker thread, if any.
    worker: Option<JoinHandle<()>>,
}

impl ConnectionSupervisor {
    /// Creates a new supervisor with generation 0 (no active worker).
    pub(super) fn new() -> Self {
        Self {
            active_generation: Arc::new(AtomicU64::new(0)),
            current: 0,
            worker: None,
        }
    }

    /// Atomically bumps the active generation to the next value and returns it.
    ///
    /// The returned value is strictly greater than all previously returned
    /// generations.  Generation 0 is skipped (reserved as the sentinel value
    /// used by `abandon()`).
    pub(super) fn next_generation(&mut self) -> Generation {
        self.current += 1;
        self.active_generation.store(self.current, SeqCst);
        self.current
    }

    /// Spawns a worker thread for the given `ConnectRequest`.
    ///
    /// Any previously held worker handle is dropped (detached) without
    /// joining, so the control thread is never blocked on the hot path.
    /// The old worker will detect that its generation is no longer active on
    /// its next `guard_active` check and exit promptly.
    ///
    /// The `sample_buffer` is cloned into the worker so it can push decoded
    /// PCM samples into the shared visualizer tap.
    pub(super) fn spawn(
        &mut self,
        req: ConnectRequest,
        event_tx: mpsc::Sender<EngineEvent>,
        sample_buffer: Arc<Mutex<VecDeque<f32>>>,
    ) {
        // Drop the old handle without joining — non-blocking detach.
        let _old = self.worker.take();

        let active_gen_arc = Arc::clone(&self.active_generation);

        let handle = std::thread::spawn(move || {
            super::decode::run_worker(req, event_tx, active_gen_arc, sample_buffer);
        });

        self.worker = Some(handle);
    }

    /// Abandons the current generation without blocking.
    ///
    /// Stores 0 to `active_generation` (SeqCst) so that any running worker
    /// detects the change on its next `is_active` check and exits promptly.
    /// The worker handle is dropped without joining.
    pub(super) fn abandon(&mut self) {
        self.active_generation.store(0, SeqCst);
        let _handle = self.worker.take(); // dropped here, no join
    }

    /// Returns `true` iff `gen` is the currently active generation.
    ///
    /// This is a pure read — no mutation occurs.
    pub(super) fn is_active(&self, gen: Generation) -> bool {
        gen == self.active_generation.load(SeqCst)
    }

    /// Clones the `Arc<AtomicU64>` so workers can share access to the active
    /// generation counter without taking a lock.
    #[cfg(test)]
    pub(super) fn active_generation_arc(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.active_generation)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Unit tests
    // -----------------------------------------------------------------------

    #[test]
    fn next_generation_starts_at_one() {
        let mut sup = ConnectionSupervisor::new();
        let g = sup.next_generation();
        assert_eq!(g, 1, "first generation should be 1 (0 is the sentinel)");
    }

    #[test]
    fn next_generation_is_strictly_increasing() {
        let mut sup = ConnectionSupervisor::new();
        let g1 = sup.next_generation();
        let g2 = sup.next_generation();
        let g3 = sup.next_generation();
        assert!(g1 < g2, "g1={g1} must be < g2={g2}");
        assert!(g2 < g3, "g2={g2} must be < g3={g3}");
    }

    #[test]
    fn is_active_true_for_current_generation() {
        let mut sup = ConnectionSupervisor::new();
        let g = sup.next_generation();
        assert!(sup.is_active(g), "current generation should be active");
    }

    #[test]
    fn is_active_false_for_stale_generation() {
        let mut sup = ConnectionSupervisor::new();
        let g1 = sup.next_generation();
        let _g2 = sup.next_generation();
        assert!(
            !sup.is_active(g1),
            "previous generation should no longer be active"
        );
    }

    #[test]
    fn is_active_false_for_zero() {
        let mut sup = ConnectionSupervisor::new();
        // Generation 0 is the sentinel; it should never be "active" after
        // a real generation has been allocated.
        let _g = sup.next_generation();
        assert!(
            !sup.is_active(0),
            "generation 0 is the sentinel, never active"
        );
    }

    #[test]
    fn abandon_sets_active_generation_to_zero() {
        let mut sup = ConnectionSupervisor::new();
        let g = sup.next_generation();
        assert!(sup.is_active(g));

        sup.abandon();

        assert!(
            !sup.is_active(g),
            "after abandon, prior generation should be inactive"
        );
        assert_eq!(
            sup.active_generation.load(SeqCst),
            0,
            "abandon must store 0 to active_generation"
        );
    }

    #[test]
    fn abandon_when_no_worker_does_not_panic() {
        let mut sup = ConnectionSupervisor::new();
        // Should be a no-op / no panic even with no worker.
        sup.abandon();
        assert_eq!(sup.active_generation.load(SeqCst), 0);
    }

    #[test]
    fn active_generation_arc_shares_same_counter() {
        let mut sup = ConnectionSupervisor::new();
        let arc = sup.active_generation_arc();
        let g = sup.next_generation();
        // The Arc should reflect the updated value.
        assert_eq!(arc.load(SeqCst), g);
    }

    // -----------------------------------------------------------------------
    // Property-based test 5.1: generation strict monotonicity
    //
    // Property 3: Generation strict monotonicity
    // For any N calls to `next_generation()`, the returned values form a
    // strictly monotonically increasing sequence.
    //
    // Validates: Requirements 4.1
    // -----------------------------------------------------------------------

    use proptest::prelude::*;

    proptest! {
        /// **Validates: Requirements 4.1**
        #[test]
        fn prop_next_generation_strictly_monotonic(n in 1usize..=100usize) {
            let mut sup = ConnectionSupervisor::new();
            let mut prev = 0u64; // 0 is the sentinel; first real gen will be 1
            for _ in 0..n {
                let g = sup.next_generation();
                prop_assert!(
                    g > prev,
                    "generation {g} must be strictly greater than previous {prev}"
                );
                prev = g;
            }
        }
    }
}
