//! Lock-free sample queues for RT metrics collection.
//!
//! This module provides bounded, lock-free queues for collecting samples
//! from the RT hot path. Samples are pushed without blocking and later
//! drained by a non-RT collector for histogram construction.
//!
//! # RT Safety
//!
//! All `push_*` methods are RT-safe:
//! - Bounded capacity (no allocation on push)
//! - Lock-free implementation
//! - Drop on overflow (acceptable for metrics)
//! - Deterministic execution time
//!
//! # Overflow Behavior
//!
//! When a queue is full, new samples are silently dropped. This is acceptable
//! for metrics collection where:
//! - Sample loss is preferable to blocking
//! - Statistical accuracy is maintained with sufficient samples
//! - Overflow indicates a collection backlog (diagnostic info)

use crossbeam::queue::ArrayQueue;

/// Default capacity for sample queues.
pub const DEFAULT_QUEUE_CAPACITY: usize = 10_000;

/// Lock-free sample queues for RT metrics collection.
///
/// This struct provides bounded SPSC (single-producer, single-consumer) queues
/// for collecting latency and jitter samples from the RT hot path. The RT thread
/// pushes samples without blocking, and a collector thread drains them for
/// histogram construction.
///
/// # RT Safety
///
/// Push operations are RT-safe. They perform a single atomic compare-and-swap
/// on success, or return immediately on failure (queue full).
///
/// # Example
///
/// ```rust
/// use openracing_atomic::queues::RTSampleQueues;
///
/// let queues = RTSampleQueues::new();
///
/// // RT hot path - push samples (no blocking)
/// queues.push_jitter(100_000);  // 100μs in nanoseconds
/// queues.push_hid_latency(50_000);
///
/// // Collector path - drain samples
/// while let Some(sample) = queues.pop_jitter() {
///     // Process sample...
/// }
/// ```
#[derive(Debug)]
#[allow(clippy::struct_field_names)]
pub struct RTSampleQueues {
    /// Jitter samples in nanoseconds.
    jitter_ns: ArrayQueue<u64>,
    /// Processing time samples in nanoseconds.
    processing_time_ns: ArrayQueue<u64>,
    /// HID latency samples in nanoseconds.
    hid_latency_ns: ArrayQueue<u64>,
}

impl Default for RTSampleQueues {
    fn default() -> Self {
        Self::new()
    }
}

impl RTSampleQueues {
    /// Create new sample queues with default capacity.
    ///
    /// This is an initialization-time operation that allocates the queue storage.
    /// After creation, no further allocations occur.
    #[must_use]
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_QUEUE_CAPACITY)
    }

    /// Create new sample queues with a specific capacity.
    ///
    /// # Panics
    ///
    /// Panics if capacity is 0.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            jitter_ns: ArrayQueue::new(capacity),
            processing_time_ns: ArrayQueue::new(capacity),
            hid_latency_ns: ArrayQueue::new(capacity),
        }
    }

    /// Push a jitter sample (nanoseconds).
    ///
    /// # RT Safety
    ///
    /// RT-safe. Non-blocking push with O(1) complexity.
    /// Returns immediately if queue is full (sample is dropped).
    ///
    /// # Returns
    ///
    /// `Ok(())` if the sample was pushed, `Err(sample)` if the queue is full.
    ///
    /// # Errors
    ///
    /// Returns `Err(sample)` if the queue is full.
    #[inline]
    pub fn push_jitter(&self, ns: u64) -> Result<(), u64> {
        self.jitter_ns.push(ns)
    }

    /// Push a jitter sample, dropping on overflow.
    ///
    /// # RT Safety
    ///
    /// RT-safe. Non-blocking, always returns immediately.
    #[inline]
    pub fn push_jitter_drop(&self, ns: u64) {
        let _ = self.jitter_ns.push(ns);
    }

    /// Push a processing time sample (nanoseconds).
    ///
    /// # RT Safety
    ///
    /// RT-safe. Non-blocking push with O(1) complexity.
    /// Returns immediately if queue is full (sample is dropped).
    ///
    /// # Errors
    ///
    /// Returns `Err(sample)` if the queue is full.
    #[inline]
    pub fn push_processing_time(&self, ns: u64) -> Result<(), u64> {
        self.processing_time_ns.push(ns)
    }

    /// Push a processing time sample, dropping on overflow.
    ///
    /// # RT Safety
    ///
    /// RT-safe. Non-blocking, always returns immediately.
    #[inline]
    pub fn push_processing_time_drop(&self, ns: u64) {
        let _ = self.processing_time_ns.push(ns);
    }

    /// Push a HID latency sample (nanoseconds).
    ///
    /// # RT Safety
    ///
    /// RT-safe. Non-blocking push with O(1) complexity.
    /// Returns immediately if queue is full (sample is dropped).
    ///
    /// # Errors
    ///
    /// Returns `Err(sample)` if the queue is full.
    #[inline]
    pub fn push_hid_latency(&self, ns: u64) -> Result<(), u64> {
        self.hid_latency_ns.push(ns)
    }

    /// Push a HID latency sample, dropping on overflow.
    ///
    /// # RT Safety
    ///
    /// RT-safe. Non-blocking, always returns immediately.
    #[inline]
    pub fn push_hid_latency_drop(&self, ns: u64) {
        let _ = self.hid_latency_ns.push(ns);
    }

    /// Pop a jitter sample.
    ///
    /// # RT Safety
    ///
    /// RT-safe but typically called from non-RT collector code.
    #[inline]
    pub fn pop_jitter(&self) -> Option<u64> {
        self.jitter_ns.pop()
    }

    /// Pop a processing time sample.
    ///
    /// # RT Safety
    ///
    /// RT-safe but typically called from non-RT collector code.
    #[inline]
    pub fn pop_processing_time(&self) -> Option<u64> {
        self.processing_time_ns.pop()
    }

    /// Pop a HID latency sample.
    ///
    /// # RT Safety
    ///
    /// RT-safe but typically called from non-RT collector code.
    #[inline]
    pub fn pop_hid_latency(&self) -> Option<u64> {
        self.hid_latency_ns.pop()
    }

    /// Get the number of jitter samples in the queue.
    #[inline]
    #[must_use]
    pub fn jitter_len(&self) -> usize {
        self.jitter_ns.len()
    }

    /// Get the number of processing time samples in the queue.
    #[inline]
    #[must_use]
    pub fn processing_time_len(&self) -> usize {
        self.processing_time_ns.len()
    }

    /// Get the number of HID latency samples in the queue.
    #[inline]
    #[must_use]
    pub fn hid_latency_len(&self) -> usize {
        self.hid_latency_ns.len()
    }

    /// Check if the jitter queue is empty.
    #[inline]
    #[must_use]
    pub fn jitter_is_empty(&self) -> bool {
        self.jitter_ns.is_empty()
    }

    /// Check if the processing time queue is empty.
    #[inline]
    #[must_use]
    pub fn processing_time_is_empty(&self) -> bool {
        self.processing_time_ns.is_empty()
    }

    /// Check if the HID latency queue is empty.
    #[inline]
    #[must_use]
    pub fn hid_latency_is_empty(&self) -> bool {
        self.hid_latency_ns.is_empty()
    }

    /// Drain all jitter samples into a vector.
    ///
    /// **NOT RT-safe**. Allocates a vector.
    #[cfg(feature = "std")]
    pub fn drain_jitter(&self) -> alloc::vec::Vec<u64> {
        let mut samples = alloc::vec::Vec::with_capacity(self.jitter_ns.len());
        while let Some(sample) = self.jitter_ns.pop() {
            samples.push(sample);
        }
        samples
    }

    /// Drain all processing time samples into a vector.
    ///
    /// **NOT RT-safe**. Allocates a vector.
    #[cfg(feature = "std")]
    pub fn drain_processing_time(&self) -> alloc::vec::Vec<u64> {
        let mut samples = alloc::vec::Vec::with_capacity(self.processing_time_ns.len());
        while let Some(sample) = self.processing_time_ns.pop() {
            samples.push(sample);
        }
        samples
    }

    /// Drain all HID latency samples into a vector.
    ///
    /// **NOT RT-safe**. Allocates a vector.
    #[cfg(feature = "std")]
    pub fn drain_hid_latency(&self) -> alloc::vec::Vec<u64> {
        let mut samples = alloc::vec::Vec::with_capacity(self.hid_latency_ns.len());
        while let Some(sample) = self.hid_latency_ns.pop() {
            samples.push(sample);
        }
        samples
    }
}

/// Statistics about queue usage.
#[derive(Debug, Clone, Copy, Default)]
pub struct QueueStats {
    /// Number of jitter samples currently in queue.
    pub jitter_count: usize,
    /// Number of processing time samples currently in queue.
    pub processing_time_count: usize,
    /// Number of HID latency samples currently in queue.
    pub hid_latency_count: usize,
}

impl RTSampleQueues {
    /// Get statistics about current queue usage.
    #[must_use]
    pub fn stats(&self) -> QueueStats {
        QueueStats {
            jitter_count: self.jitter_ns.len(),
            processing_time_count: self.processing_time_ns.len(),
            hid_latency_count: self.hid_latency_ns.len(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_and_pop_jitter() {
        let queues = RTSampleQueues::with_capacity(10);

        queues.push_jitter(100).unwrap();
        queues.push_jitter(200).unwrap();
        queues.push_jitter(300).unwrap();

        assert_eq!(queues.pop_jitter(), Some(100));
        assert_eq!(queues.pop_jitter(), Some(200));
        assert_eq!(queues.pop_jitter(), Some(300));
        assert_eq!(queues.pop_jitter(), None);
    }

    #[test]
    fn test_queue_overflow() {
        let queues = RTSampleQueues::with_capacity(2);

        assert!(queues.push_jitter(1).is_ok());
        assert!(queues.push_jitter(2).is_ok());
        assert!(queues.push_jitter(3).is_err());

        queues.push_jitter_drop(4);

        assert_eq!(queues.pop_jitter(), Some(1));
        assert_eq!(queues.pop_jitter(), Some(2));
        assert_eq!(queues.pop_jitter(), None);
    }

    #[test]
    fn test_queue_lengths() {
        let queues = RTSampleQueues::with_capacity(10);

        assert!(queues.jitter_is_empty());
        assert_eq!(queues.jitter_len(), 0);

        queues.push_jitter(100).unwrap();
        assert!(!queues.jitter_is_empty());
        assert_eq!(queues.jitter_len(), 1);

        queues.push_jitter(200).unwrap();
        assert_eq!(queues.jitter_len(), 2);
    }

    #[test]
    fn test_queue_stats() {
        let queues = RTSampleQueues::with_capacity(10);

        queues.push_jitter(1).unwrap();
        queues.push_jitter(2).unwrap();
        queues.push_processing_time(3).unwrap();
        queues.push_hid_latency(4).unwrap();
        queues.push_hid_latency(5).unwrap();

        let stats = queues.stats();
        assert_eq!(stats.jitter_count, 2);
        assert_eq!(stats.processing_time_count, 1);
        assert_eq!(stats.hid_latency_count, 2);
    }

    #[cfg(feature = "std")]
    #[test]
    fn test_drain() {
        let queues = RTSampleQueues::with_capacity(10);

        queues.push_jitter(1).unwrap();
        queues.push_jitter(2).unwrap();
        queues.push_jitter(3).unwrap();

        let samples = queues.drain_jitter();
        assert_eq!(samples, alloc::vec![1, 2, 3]);
        assert!(queues.jitter_is_empty());
    }
}
