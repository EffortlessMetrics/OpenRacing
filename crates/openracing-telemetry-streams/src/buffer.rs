//! Telemetry buffer implementations

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

pub struct TelemetryBuffer<T> {
    buffer: Arc<Mutex<VecDeque<T>>>,
    max_size: usize,
}

impl<T> TelemetryBuffer<T> {
    pub fn new(max_size: usize) -> Self {
        Self {
            buffer: Arc::new(Mutex::new(VecDeque::with_capacity(max_size))),
            max_size,
        }
    }

    pub fn push(&self, item: T) {
        // Recover from mutex poisoning: telemetry data loss is acceptable,
        // panicking is not.
        let mut buffer = self.buffer.lock().unwrap_or_else(|e| e.into_inner());

        if buffer.len() >= self.max_size {
            buffer.pop_front();
        }

        buffer.push_back(item);
    }

    pub fn pop(&self) -> Option<T> {
        let mut buffer = self.buffer.lock().unwrap_or_else(|e| e.into_inner());
        buffer.pop_front()
    }

    pub fn len(&self) -> usize {
        let buffer = self.buffer.lock().unwrap_or_else(|e| e.into_inner());
        buffer.len()
    }

    pub fn is_empty(&self) -> bool {
        let buffer = self.buffer.lock().unwrap_or_else(|e| e.into_inner());
        buffer.is_empty()
    }

    pub fn clear(&self) {
        let mut buffer = self.buffer.lock().unwrap_or_else(|e| e.into_inner());
        buffer.clear();
    }

    pub fn iter(&self) -> impl Iterator<Item = T>
    where
        T: Clone,
    {
        let buffer = self.buffer.lock().unwrap_or_else(|e| e.into_inner());
        buffer.iter().cloned().collect::<Vec<_>>().into_iter()
    }

    pub fn latest(&self) -> Option<T>
    where
        T: Clone,
    {
        let buffer = self.buffer.lock().unwrap_or_else(|e| e.into_inner());
        buffer.back().cloned()
    }

    pub fn oldest(&self) -> Option<T>
    where
        T: Clone,
    {
        let buffer = self.buffer.lock().unwrap_or_else(|e| e.into_inner());
        buffer.front().cloned()
    }
}

impl<T> Default for TelemetryBuffer<T> {
    fn default() -> Self {
        Self::new(1000)
    }
}

pub struct RingBuffer<T> {
    data: Vec<Option<T>>,
    write_index: usize,
    read_index: usize,
    count: usize,
    capacity: usize,
}

impl<T> RingBuffer<T> {
    pub fn new(capacity: usize) -> Self {
        Self {
            data: (0..capacity).map(|_| None).collect(),
            write_index: 0,
            read_index: 0,
            count: 0,
            capacity,
        }
    }

    pub fn write(&mut self, item: T) -> Option<T> {
        let old = self.data[self.write_index].take();
        self.data[self.write_index] = Some(item);

        self.write_index = (self.write_index + 1) % self.capacity;

        if self.count < self.capacity {
            self.count += 1;
        } else {
            self.read_index = (self.read_index + 1) % self.capacity;
        }

        old
    }

    pub fn read(&mut self) -> Option<T> {
        if self.count == 0 {
            return None;
        }

        let item = self.data[self.read_index].take();
        self.read_index = (self.read_index + 1) % self.capacity;
        self.count -= 1;

        item
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub fn is_full(&self) -> bool {
        self.count == self.capacity
    }

    pub fn len(&self) -> usize {
        self.count
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn clear(&mut self) {
        for item in self.data.iter_mut() {
            *item = None;
        }
        self.write_index = 0;
        self.read_index = 0;
        self.count = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // TelemetryBuffer
    // -----------------------------------------------------------------------

    #[test]
    fn test_telemetry_buffer_basic() {
        let buffer = TelemetryBuffer::new(3);

        buffer.push(1);
        buffer.push(2);
        buffer.push(3);

        assert_eq!(buffer.len(), 3);

        buffer.push(4);

        assert_eq!(buffer.len(), 3);
        assert_eq!(buffer.pop(), Some(2));
    }

    #[test]
    fn test_telemetry_buffer_latest() {
        let buffer = TelemetryBuffer::new(3);

        buffer.push(1);
        buffer.push(2);
        buffer.push(3);

        assert_eq!(buffer.latest(), Some(3));
        assert_eq!(buffer.oldest(), Some(1));
    }

    #[test]
    fn test_telemetry_buffer_empty_operations() {
        let buffer: TelemetryBuffer<i32> = TelemetryBuffer::new(5);

        assert!(buffer.is_empty());
        assert_eq!(buffer.len(), 0);
        assert_eq!(buffer.pop(), None);
        assert_eq!(buffer.latest(), None);
        assert_eq!(buffer.oldest(), None);
    }

    #[test]
    fn test_telemetry_buffer_clear() {
        let buffer = TelemetryBuffer::new(5);
        buffer.push(10);
        buffer.push(20);
        assert_eq!(buffer.len(), 2);

        buffer.clear();
        assert!(buffer.is_empty());
        assert_eq!(buffer.len(), 0);
        assert_eq!(buffer.pop(), None);
    }

    #[test]
    fn test_telemetry_buffer_iter() {
        let buffer = TelemetryBuffer::new(5);
        buffer.push(1);
        buffer.push(2);
        buffer.push(3);

        let items: Vec<i32> = buffer.iter().collect();
        assert_eq!(items, vec![1, 2, 3]);
    }

    #[test]
    fn test_telemetry_buffer_default_capacity() {
        let buffer: TelemetryBuffer<i32> = TelemetryBuffer::default();
        // Default is 1000, fill beyond it
        for i in 0..1001 {
            buffer.push(i);
        }
        assert_eq!(buffer.len(), 1000);
    }

    #[test]
    fn test_telemetry_buffer_size_one() {
        let buffer = TelemetryBuffer::new(1);
        buffer.push(10);
        assert_eq!(buffer.len(), 1);

        buffer.push(20);
        assert_eq!(buffer.len(), 1);
        assert_eq!(buffer.latest(), Some(20));
        assert_eq!(buffer.oldest(), Some(20));
    }

    // -----------------------------------------------------------------------
    // RingBuffer
    // -----------------------------------------------------------------------

    #[test]
    fn test_ring_buffer() {
        let mut buffer = RingBuffer::new(3);

        assert!(buffer.is_empty());

        buffer.write(1);
        buffer.write(2);

        assert_eq!(buffer.len(), 2);

        assert_eq!(buffer.read(), Some(1));
        assert_eq!(buffer.read(), Some(2));

        assert!(buffer.is_empty());
    }

    #[test]
    fn test_ring_buffer_overflow() {
        let mut buffer = RingBuffer::new(2);

        buffer.write(1);
        buffer.write(2);
        buffer.write(3);

        assert_eq!(buffer.len(), 2);
        assert_eq!(buffer.read(), Some(2));
        assert_eq!(buffer.read(), Some(3));
    }

    #[test]
    fn test_ring_buffer_clear() {
        let mut buffer = RingBuffer::new(5);
        buffer.write(1);
        buffer.write(2);
        buffer.write(3);
        assert_eq!(buffer.len(), 3);

        buffer.clear();
        assert!(buffer.is_empty());
        assert_eq!(buffer.len(), 0);
        assert_eq!(buffer.read(), None);
    }

    #[test]
    fn test_ring_buffer_capacity() {
        let buffer: RingBuffer<i32> = RingBuffer::new(7);
        assert_eq!(buffer.capacity(), 7);
        assert!(buffer.is_empty());
        assert!(!buffer.is_full());
    }

    #[test]
    fn test_ring_buffer_full_detection() {
        let mut buffer = RingBuffer::new(2);
        assert!(!buffer.is_full());

        buffer.write(1);
        assert!(!buffer.is_full());

        buffer.write(2);
        assert!(buffer.is_full());
    }

    #[test]
    fn test_ring_buffer_write_returns_overwritten_item() {
        let mut buffer = RingBuffer::new(2);

        assert!(buffer.write(1).is_none());
        assert!(buffer.write(2).is_none());
        // Next write should overwrite and return the old item
        let old = buffer.write(3);
        assert_eq!(old, Some(1));
    }

    #[test]
    fn test_ring_buffer_read_after_clear() {
        let mut buffer = RingBuffer::new(3);
        buffer.write(10);
        buffer.write(20);
        buffer.clear();

        assert_eq!(buffer.read(), None);
        // After clear, writing should work normally
        buffer.write(30);
        assert_eq!(buffer.read(), Some(30));
    }
}
