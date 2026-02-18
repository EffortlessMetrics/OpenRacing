//! Generic lock-free seqlock-style mailbox for copy types.

use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicU32, Ordering};

/// Lock-free, single-writer/multi-reader mailbox.
///
/// The writer increments a sequence counter, writes payload, and publishes an
/// even sequence value when the snapshot is complete.
pub struct SnapshotMailbox<T: Copy> {
    seq: AtomicU32,
    data: UnsafeCell<T>,
}

unsafe impl<T: Copy> Sync for SnapshotMailbox<T> {}

impl<T: Copy> SnapshotMailbox<T> {
    pub const fn new(value: T) -> Self {
        Self {
            seq: AtomicU32::new(0),
            data: UnsafeCell::new(value),
        }
    }

    pub fn write(&self, value: T) {
        let seq = self.seq.load(Ordering::Relaxed);
        self.seq.store(seq.saturating_add(1), Ordering::Release);
        unsafe {
            *self.data.get() = value;
        }
        self.seq.store(seq.saturating_add(2), Ordering::Release);
    }

    pub fn read(&self) -> T {
        loop {
            let start = self.seq.load(Ordering::Acquire);
            if (start & 1) != 0 {
                continue;
            }

            let value = unsafe { *self.data.get() };
            let end = self.seq.load(Ordering::Acquire);
            if start == end {
                return value;
            }
        }
    }
}

