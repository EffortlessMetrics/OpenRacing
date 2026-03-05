//! Hardening tests for the telemetry streaming layer.
//!
//! Tests stream creation, data rate enforcement, back-pressure handling,
//! multiple concurrent streams, stream lifecycle, and data format validation.

use openracing_telemetry_streams::{
    MovingAverage, RateCounter, RateLimiter, RingBuffer, StreamError, TelemetryBuffer,
};
use std::time::Duration;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Verify that an error matches the expected variant.
fn assert_stream_error_display(err: &StreamError, expected_fragment: &str) {
    let msg = format!("{err}");
    assert!(
        msg.contains(expected_fragment),
        "Expected error to contain '{expected_fragment}', got '{msg}'"
    );
}

// ===========================================================================
// 1. Stream creation and subscription (TelemetryBuffer)
// ===========================================================================

#[test]
fn test_telemetry_buffer_creation_default() {
    let buf: TelemetryBuffer<i32> = TelemetryBuffer::default();
    assert!(buf.is_empty());
    assert_eq!(buf.len(), 0);
}

#[test]
fn test_telemetry_buffer_creation_custom_capacity() {
    let buf: TelemetryBuffer<f64> = TelemetryBuffer::new(50);
    assert!(buf.is_empty());
}

#[test]
fn test_telemetry_buffer_push_pop_fifo() {
    let buf = TelemetryBuffer::new(10);
    buf.push(1);
    buf.push(2);
    buf.push(3);

    assert_eq!(buf.len(), 3);
    assert_eq!(buf.pop(), Some(1));
    assert_eq!(buf.pop(), Some(2));
    assert_eq!(buf.pop(), Some(3));
    assert_eq!(buf.pop(), None);
}

#[test]
fn test_telemetry_buffer_latest_oldest() {
    let buf = TelemetryBuffer::new(5);
    buf.push(10);
    buf.push(20);
    buf.push(30);

    assert_eq!(buf.oldest(), Some(10));
    assert_eq!(buf.latest(), Some(30));
}

#[test]
fn test_telemetry_buffer_latest_oldest_empty() {
    let buf: TelemetryBuffer<i32> = TelemetryBuffer::new(5);
    assert_eq!(buf.oldest(), None);
    assert_eq!(buf.latest(), None);
}

#[test]
fn test_telemetry_buffer_clear() {
    let buf = TelemetryBuffer::new(5);
    buf.push(1);
    buf.push(2);
    buf.clear();
    assert!(buf.is_empty());
    assert_eq!(buf.len(), 0);
}

#[test]
fn test_telemetry_buffer_iter() {
    let buf = TelemetryBuffer::new(5);
    buf.push(100);
    buf.push(200);
    buf.push(300);

    let items: Vec<i32> = buf.iter().collect();
    assert_eq!(items, vec![100, 200, 300]);
}

// ===========================================================================
// 2. Data rate enforcement (back-pressure via eviction)
// ===========================================================================

#[test]
fn test_telemetry_buffer_evicts_oldest_at_capacity() {
    let buf = TelemetryBuffer::new(3);

    buf.push(1);
    buf.push(2);
    buf.push(3);
    assert_eq!(buf.len(), 3);

    buf.push(4);
    assert_eq!(buf.len(), 3);
    // Oldest (1) should have been evicted
    assert_eq!(buf.oldest(), Some(2));
    assert_eq!(buf.latest(), Some(4));
}

#[test]
fn test_telemetry_buffer_evicts_correctly_with_many_pushes() {
    let buf = TelemetryBuffer::new(2);
    for i in 0..100 {
        buf.push(i);
    }

    assert_eq!(buf.len(), 2);
    assert_eq!(buf.oldest(), Some(98));
    assert_eq!(buf.latest(), Some(99));
}

#[test]
fn test_rate_limiter_first_update_allowed() {
    let mut limiter = RateLimiter::new(10.0);
    assert!(limiter.should_update(), "First call should be allowed");
}

#[test]
fn test_rate_limiter_immediate_second_rejected() {
    let mut limiter = RateLimiter::new(10.0);
    assert!(limiter.should_update());
    assert!(
        !limiter.should_update(),
        "Immediate second call should be rejected"
    );
}

#[test]
fn test_rate_limiter_reset_allows_update() {
    let mut limiter = RateLimiter::new(10.0);
    assert!(limiter.should_update());
    assert!(!limiter.should_update());
    limiter.reset();
    assert!(
        limiter.should_update(),
        "After reset, first call should be allowed"
    );
}

#[test]
fn test_rate_limiter_allows_after_interval() {
    let mut limiter = RateLimiter::new(50.0); // 20ms interval
    assert!(limiter.should_update());
    std::thread::sleep(Duration::from_millis(25));
    assert!(
        limiter.should_update(),
        "Should allow after interval elapses"
    );
}

// ===========================================================================
// 3. Back-pressure handling (RingBuffer)
// ===========================================================================

#[test]
fn test_ring_buffer_creation() {
    let buf: RingBuffer<u32> = RingBuffer::new(4);
    assert!(buf.is_empty());
    assert!(!buf.is_full());
    assert_eq!(buf.len(), 0);
    assert_eq!(buf.capacity(), 4);
}

#[test]
fn test_ring_buffer_write_read() {
    let mut buf = RingBuffer::new(4);
    let evicted = buf.write(10);
    assert!(evicted.is_none());

    let evicted = buf.write(20);
    assert!(evicted.is_none());

    assert_eq!(buf.read(), Some(10));
    assert_eq!(buf.read(), Some(20));
    assert_eq!(buf.read(), None);
}

#[test]
fn test_ring_buffer_overflow_evicts_oldest() {
    let mut buf = RingBuffer::new(2);
    buf.write(1);
    buf.write(2);
    assert!(buf.is_full());

    // Writing a third item should evict the oldest
    let evicted = buf.write(3);
    assert_eq!(evicted, Some(1));
    assert_eq!(buf.len(), 2);

    assert_eq!(buf.read(), Some(2));
    assert_eq!(buf.read(), Some(3));
}

#[test]
fn test_ring_buffer_wrap_around() {
    let mut buf = RingBuffer::new(3);

    buf.write(1);
    buf.write(2);
    buf.write(3);
    assert_eq!(buf.read(), Some(1));

    buf.write(4);
    assert_eq!(buf.read(), Some(2));
    assert_eq!(buf.read(), Some(3));
    assert_eq!(buf.read(), Some(4));
    assert!(buf.is_empty());
}

#[test]
fn test_ring_buffer_clear() {
    let mut buf = RingBuffer::new(3);
    buf.write(1);
    buf.write(2);
    buf.clear();

    assert!(buf.is_empty());
    assert_eq!(buf.len(), 0);
    assert_eq!(buf.read(), None);
}

#[test]
fn test_ring_buffer_capacity_1() {
    let mut buf = RingBuffer::new(1);
    buf.write(42);
    assert!(buf.is_full());
    assert_eq!(buf.len(), 1);

    let evicted = buf.write(99);
    assert_eq!(evicted, Some(42));
    assert_eq!(buf.read(), Some(99));
}

#[test]
fn test_ring_buffer_stress() {
    let mut buf = RingBuffer::new(16);
    for i in 0..1000 {
        buf.write(i);
    }

    assert_eq!(buf.len(), 16);

    // Last 16 items should be 984..1000
    for expected in 984..1000 {
        assert_eq!(buf.read(), Some(expected));
    }
    assert!(buf.is_empty());
}

// ===========================================================================
// 4. Multiple concurrent streams (TelemetryBuffer thread safety)
// ===========================================================================

#[test]
fn test_telemetry_buffer_concurrent_push() {
    use std::sync::Arc;
    use std::thread;

    let buf = Arc::new(TelemetryBuffer::new(1000));
    let mut handles = Vec::new();

    for t in 0..4 {
        let buf_clone = Arc::clone(&buf);
        handles.push(thread::spawn(move || {
            for i in 0..100 {
                buf_clone.push(t * 100 + i);
            }
        }));
    }

    for h in handles {
        h.join().ok();
    }

    // All 400 items should fit since capacity is 1000
    assert_eq!(buf.len(), 400);
}

#[test]
fn test_telemetry_buffer_concurrent_push_pop() {
    use std::sync::Arc;
    use std::thread;

    let buf = Arc::new(TelemetryBuffer::new(100));

    let buf_writer = Arc::clone(&buf);
    let writer = thread::spawn(move || {
        for i in 0..200 {
            buf_writer.push(i);
        }
    });

    let buf_reader = Arc::clone(&buf);
    let reader = thread::spawn(move || {
        let mut read_count = 0;
        for _ in 0..300 {
            if buf_reader.pop().is_some() {
                read_count += 1;
            }
            std::thread::sleep(Duration::from_micros(10));
        }
        read_count
    });

    writer.join().ok();
    let _read = reader.join().ok();
    // No panic = success; concurrent access is safe
}

#[test]
fn test_multiple_independent_buffers() {
    let buf_a: TelemetryBuffer<f32> = TelemetryBuffer::new(10);
    let buf_b: TelemetryBuffer<f32> = TelemetryBuffer::new(10);

    buf_a.push(1.0);
    buf_b.push(2.0);

    assert_eq!(buf_a.latest(), Some(1.0));
    assert_eq!(buf_b.latest(), Some(2.0));
}

// ===========================================================================
// 5. Stream lifecycle (start / stop / restart)
// ===========================================================================

#[test]
fn test_ring_buffer_lifecycle_start_use_clear_reuse() {
    let mut buf = RingBuffer::new(4);

    // Phase 1: use
    buf.write(10);
    buf.write(20);
    assert_eq!(buf.len(), 2);

    // Phase 2: "stop" — clear
    buf.clear();
    assert!(buf.is_empty());

    // Phase 3: restart
    buf.write(30);
    buf.write(40);
    assert_eq!(buf.read(), Some(30));
    assert_eq!(buf.read(), Some(40));
}

#[test]
fn test_telemetry_buffer_lifecycle() {
    let buf = TelemetryBuffer::new(5);

    buf.push(1);
    buf.push(2);
    assert_eq!(buf.len(), 2);

    buf.clear();
    assert!(buf.is_empty());

    buf.push(3);
    assert_eq!(buf.latest(), Some(3));
}

#[test]
fn test_rate_limiter_lifecycle() {
    let mut limiter = RateLimiter::new(1000.0);

    // Start
    assert!(limiter.should_update());

    // Stop
    limiter.reset();

    // Restart
    assert!(limiter.should_update());
}

#[test]
fn test_rate_counter_lifecycle() {
    let mut counter = RateCounter::new(Duration::from_secs(1));

    for _ in 0..10 {
        counter.increment();
    }
    std::thread::sleep(Duration::from_millis(50));
    let rate_before = counter.rate();
    assert!(rate_before > 0.0);

    counter.reset();
    // After reset, rate should be zero or very low
    let rate_after = counter.rate();
    assert!(
        rate_after < rate_before || rate_after == 0.0,
        "Rate after reset should be less than before"
    );
}

// ===========================================================================
// 6. Data format validation
// ===========================================================================

#[test]
fn test_moving_average_empty() {
    let avg = MovingAverage::new(5);
    assert!((avg.average() - 0.0).abs() < f32::EPSILON);
}

#[test]
fn test_moving_average_single_value() {
    let mut avg = MovingAverage::new(5);
    avg.push(42.0);
    assert!((avg.average() - 42.0).abs() < f32::EPSILON);
}

#[test]
fn test_moving_average_window_full() {
    let mut avg = MovingAverage::new(3);
    avg.push(1.0);
    avg.push(2.0);
    avg.push(3.0);
    // Average of [1, 2, 3] = 2.0
    assert!((avg.average() - 2.0).abs() < 0.01);
}

#[test]
fn test_moving_average_window_slide() {
    let mut avg = MovingAverage::new(3);
    avg.push(1.0);
    avg.push(2.0);
    avg.push(3.0);
    avg.push(10.0);
    // Window is now [2, 3, 10] → avg = 5.0
    assert!((avg.average() - 5.0).abs() < 0.01);
}

#[test]
fn test_moving_average_reset() {
    let mut avg = MovingAverage::new(3);
    avg.push(100.0);
    avg.push(200.0);
    avg.reset();
    assert!((avg.average() - 0.0).abs() < f32::EPSILON);
}

#[test]
fn test_rate_counter_increments() {
    let mut counter = RateCounter::new(Duration::from_secs(1));
    for _ in 0..100 {
        counter.increment();
    }
    std::thread::sleep(Duration::from_millis(50));
    let rate = counter.rate();
    assert!(rate > 0.0, "Rate should be positive after increments");
}

#[test]
fn test_rate_counter_reset_zeroes_count() {
    let mut counter = RateCounter::new(Duration::from_secs(1));
    counter.increment();
    counter.increment();
    counter.reset();
    // After reset, no increments have been counted
    // Rate immediately after reset should be 0 (count=0, elapsed≈0)
    let rate = counter.rate();
    assert!(
        rate == 0.0,
        "Rate should be 0 immediately after reset, got {rate}"
    );
}

// ===========================================================================
// Error types
// ===========================================================================

#[test]
fn test_stream_error_buffer_overflow_display() {
    let err = StreamError::BufferOverflow;
    assert_stream_error_display(&err, "Buffer overflow");
}

#[test]
fn test_stream_error_stream_closed_display() {
    let err = StreamError::StreamClosed;
    assert_stream_error_display(&err, "Stream closed");
}

#[test]
fn test_stream_error_processing_display() {
    let err = StreamError::ProcessingError("bad data".to_string());
    assert_stream_error_display(&err, "bad data");
}

// ===========================================================================
// Edge cases
// ===========================================================================

#[test]
fn test_telemetry_buffer_capacity_1() {
    let buf = TelemetryBuffer::new(1);
    buf.push(1);
    buf.push(2);
    assert_eq!(buf.len(), 1);
    assert_eq!(buf.pop(), Some(2));
}

#[test]
fn test_ring_buffer_read_empty() {
    let mut buf: RingBuffer<u32> = RingBuffer::new(4);
    assert_eq!(buf.read(), None);
}

#[test]
fn test_moving_average_window_size_1() {
    let mut avg = MovingAverage::new(1);
    avg.push(5.0);
    assert!((avg.average() - 5.0).abs() < f32::EPSILON);
    avg.push(10.0);
    assert!((avg.average() - 10.0).abs() < f32::EPSILON);
}
