use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

/// Bounded Producer-Consumer circular byte queue (Resiliency Buffer)
pub(super) struct BufferQueue {
    queue: Mutex<VecDeque<u8>>,
    cv_read: Condvar,
    cv_write: Condvar,
    pub(super) capacity: usize,
    disconnected: AtomicBool,
}

impl BufferQueue {
    pub(super) fn new(capacity: usize) -> Self {
        Self {
            queue: Mutex::new(VecDeque::with_capacity(capacity)),
            cv_read: Condvar::new(),
            cv_write: Condvar::new(),
            capacity,
            disconnected: AtomicBool::new(false),
        }
    }

    pub(super) fn push(&self, bytes: &[u8]) {
        let mut queue = self.queue.lock().unwrap();
        // Block downloader thread if buffer capacity limit is reached
        while queue.len() + bytes.len() > self.capacity && !self.disconnected.load(Ordering::SeqCst)
        {
            queue = self.cv_write.wait(queue).unwrap();
        }
        if self.disconnected.load(Ordering::SeqCst) {
            return;
        }
        queue.extend(bytes);
        self.cv_read.notify_all();
    }

    pub(super) fn pop(&self, buf: &mut [u8]) -> std::io::Result<usize> {
        let mut queue = self.queue.lock().unwrap();
        while queue.is_empty() && !self.disconnected.load(Ordering::SeqCst) {
            queue = self.cv_read.wait(queue).unwrap();
        }
        if queue.is_empty() && self.disconnected.load(Ordering::SeqCst) {
            return Ok(0); // EOF or network disconnection
        }

        let count = std::cmp::min(buf.len(), queue.len());
        for slot in buf.iter_mut().take(count) {
            *slot = queue.pop_front().unwrap();
        }
        self.cv_write.notify_all();
        Ok(count)
    }

    pub(super) fn wait_until_at_least(&self, target_len: usize, max_wait: Duration) -> usize {
        let deadline = Instant::now() + max_wait;
        let mut queue = self.queue.lock().unwrap();

        while queue.len() < target_len && !self.disconnected.load(Ordering::SeqCst) {
            let now = Instant::now();
            if now >= deadline {
                break;
            }

            let remaining = deadline.saturating_duration_since(now);
            let (next_queue, timeout) = self.cv_read.wait_timeout(queue, remaining).unwrap();
            queue = next_queue;

            if timeout.timed_out() {
                break;
            }
        }

        queue.len()
    }

    pub(super) fn len(&self) -> usize {
        self.queue.lock().unwrap().len()
    }

    pub(super) fn set_disconnected(&self, disc: bool) {
        self.disconnected.store(disc, Ordering::SeqCst);
        let _queue = self.queue.lock().unwrap();
        // Wake up both consumer and producer to terminate gracefully
        self.cv_read.notify_all();
        self.cv_write.notify_all();
    }
}
