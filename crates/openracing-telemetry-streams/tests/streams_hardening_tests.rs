//! Hardening tests for openracing-telemetry-streams.
//!
//! Covers edge cases, ordering invariants, overflow behavior, concurrent
//! access, and error display for `TelemetryBuffer`, `RingBuffer`,
//! `MovingAverage`, `RateLimiter`, `RateCounter`, and `StreamError`.

use openracing_telemetry_streams::{
    MovingAverage, RateCounter, RateLimiter, RingBuffer, StreamError, TelemetryBuffer,
};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ============================================================================
// StreamError
// ============================================================================

#[test]
fn stream_error_display_buffer_overflow() {
    let err = StreamError::BufferOverflow;
    assert_eq!(format!("{err}"), "Buffer overflow");
}

#[test]
fn stream_error_display_stream_closed() {
    let err = StreamError::StreamClosed;
    assert_eq!(format!("{err}"), "Stream closed");
}

#[test]
fn stream_error_display_processing_error() {
    let err = StreamError::ProcessingError("bad data".to_string());
    assert_eq!(format!("{err}"), "Processing error: bad data");
}

#[test]
fn stream_error_processing_error_empty_message() {
    let err = StreamError::ProcessingError(String::new());
    assert_eq!(format!("{err}"), "Processing error: ");
}

// ============================================================================
// TelemetryBuffer — edge cases
// ============================================================================

#[test]
fn telemetry_buffer_push_pop_fifo_ordering() -> TestResult {
    let buf = TelemetryBuffer::new(5);
    for i in 0..5 {
        buf.push(i);
    }
    for i in 0..5 {
        assert_eq!(buf.pop(), Some(i), "FIFO violated at {i}");
    }
    assert_eq!(buf.pop(), None);
    Ok(())
}

#[test]
fn telemetry_buffer_eviction_preserves_recent() -> TestResult {
    let buf = TelemetryBuffer::new(3);
    for i in 0..10 {
        buf.push(i);
    }
    // Only the last 3 should remain
    assert_eq!(buf.len(), 3);
    assert_eq!(buf.oldest(), Some(7));
    assert_eq!(buf.latest(), Some(9));
    Ok(())
}

#[test]
fn telemetry_buffer_iter_after_eviction() -> TestResult {
    let buf = TelemetryBuffer::new(3);
    for i in 0..6 {
        buf.push(i);
    }
    let items: Vec<i32> = buf.iter().collect();
    assert_eq!(items, vec![3, 4, 5]);
    Ok(())
}

#[test]
fn telemetry_buffer_clear_then_push() -> TestResult {
    let buf = TelemetryBuffer::new(5);
    buf.push(1);
    buf.push(2);
    buf.clear();
    assert!(buf.is_empty());

    buf.push(10);
    assert_eq!(buf.len(), 1);
    assert_eq!(buf.pop(), Some(10));
    Ok(())
}

#[test]
fn telemetry_buffer_concurrent_push_does_not_panic() -> TestResult {
    let buf = Arc::new(TelemetryBuffer::new(100));
    let mut handles = Vec::new();

    for t in 0..4 {
        let b = Arc::clone(&buf);
        handles.push(thread::spawn(move || {
            for i in 0..1000 {
                b.push(t * 1000 + i);
            }
        }));
    }

    for h in handles {
        h.join().map_err(|_| "thread panicked")?;
    }

    // Buffer should have at most 100 items due to eviction
    assert!(buf.len() <= 100);
    Ok(())
}

#[test]
fn telemetry_buffer_concurrent_push_and_pop() -> TestResult {
    let buf = Arc::new(TelemetryBuffer::new(50));
    let buf_writer = Arc::clone(&buf);
    let buf_reader = Arc::clone(&buf);

    let writer = thread::spawn(move || {
        for i in 0..500 {
            buf_writer.push(i);
        }
    });

    let reader = thread::spawn(move || {
        let mut count = 0u32;
        for _ in 0..1000 {
            if buf_reader.pop().is_some() {
                count += 1;
            }
            thread::yield_now();
        }
        count
    });

    writer.join().map_err(|_| "writer panicked")?;
    let popped = reader.join().map_err(|_| "reader panicked")?;
    // Some items must have been popped
    assert!(popped > 0 || !buf.is_empty(), "no items observed");
    Ok(())
}

// ============================================================================
// RingBuffer — edge cases
// ============================================================================

#[test]
fn ring_buffer_wrap_around_correctness() -> TestResult {
    let mut ring = RingBuffer::new(3);
    // Fill
    ring.write(1);
    ring.write(2);
    ring.write(3);
    assert!(ring.is_full());

    // Overwrite oldest
    let evicted = ring.write(4);
    assert_eq!(evicted, Some(1));
    assert_eq!(ring.read(), Some(2));
    assert_eq!(ring.read(), Some(3));
    assert_eq!(ring.read(), Some(4));
    assert!(ring.is_empty());
    Ok(())
}

#[test]
fn ring_buffer_multiple_wrap_arounds() -> TestResult {
    let mut ring = RingBuffer::new(2);
    for i in 0..20 {
        ring.write(i);
    }
    // Only last 2 remain
    assert_eq!(ring.len(), 2);
    assert_eq!(ring.read(), Some(18));
    assert_eq!(ring.read(), Some(19));
    Ok(())
}

#[test]
fn ring_buffer_interleaved_read_write() -> TestResult {
    let mut ring = RingBuffer::new(4);

    ring.write(1);
    ring.write(2);
    assert_eq!(ring.read(), Some(1));

    ring.write(3);
    ring.write(4);
    assert_eq!(ring.read(), Some(2));
    assert_eq!(ring.read(), Some(3));
    assert_eq!(ring.read(), Some(4));
    assert_eq!(ring.read(), None);
    Ok(())
}

#[test]
fn ring_buffer_clear_resets_indices() -> TestResult {
    let mut ring = RingBuffer::new(3);
    ring.write(1);
    ring.write(2);
    ring.write(3);
    ring.write(4); // overwrites 1
    ring.clear();

    assert!(ring.is_empty());
    assert_eq!(ring.len(), 0);

    // After clear write/read should work normally
    ring.write(100);
    assert_eq!(ring.len(), 1);
    assert_eq!(ring.read(), Some(100));
    Ok(())
}

#[test]
fn ring_buffer_capacity_one_repeated() -> TestResult {
    let mut ring = RingBuffer::new(1);
    for i in 0..10 {
        let evicted = ring.write(i);
        if i > 0 {
            assert_eq!(evicted, Some(i - 1));
        }
    }
    assert_eq!(ring.read(), Some(9));
    assert!(ring.is_empty());
    Ok(())
}

// ============================================================================
// MovingAverage — edge cases
// ============================================================================

#[test]
fn moving_average_window_size_one() -> TestResult {
    let mut avg = MovingAverage::new(1);
    avg.push(10.0);
    assert!((avg.average() - 10.0).abs() < f32::EPSILON);

    avg.push(20.0);
    assert!((avg.average() - 20.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn moving_average_sliding_window_eviction() -> TestResult {
    let mut avg = MovingAverage::new(3);
    avg.push(1.0);
    avg.push(2.0);
    avg.push(3.0);
    // avg = 2.0

    avg.push(4.0); // evicts 1.0, window = [2,3,4], avg = 3.0
    assert!((avg.average() - 3.0).abs() < 0.01);

    avg.push(5.0); // evicts 2.0, window = [3,4,5], avg = 4.0
    assert!((avg.average() - 4.0).abs() < 0.01);
    Ok(())
}

#[test]
fn moving_average_negative_values() -> TestResult {
    let mut avg = MovingAverage::new(4);
    avg.push(-10.0);
    avg.push(-20.0);
    avg.push(10.0);
    avg.push(20.0);
    assert!((avg.average() - 0.0).abs() < 0.01);
    Ok(())
}

#[test]
fn moving_average_reset_between_series() -> TestResult {
    let mut avg = MovingAverage::new(3);
    avg.push(100.0);
    avg.push(200.0);
    avg.push(300.0);
    assert!((avg.average() - 200.0).abs() < 0.01);

    avg.reset();
    assert!((avg.average() - 0.0).abs() < f32::EPSILON);

    avg.push(1.0);
    assert!((avg.average() - 1.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn moving_average_large_window_partially_filled() -> TestResult {
    let mut avg = MovingAverage::new(1000);
    avg.push(10.0);
    avg.push(20.0);
    // Only 2 samples, average = 15.0
    assert!((avg.average() - 15.0).abs() < 0.01);
    Ok(())
}

// ============================================================================
// RateLimiter — edge cases
// ============================================================================

#[test]
fn rate_limiter_first_call_always_true() -> TestResult {
    let mut limiter = RateLimiter::new(1.0); // 1 Hz
    assert!(limiter.should_update());
    Ok(())
}

#[test]
fn rate_limiter_immediate_second_call_false() -> TestResult {
    let mut limiter = RateLimiter::new(10.0); // 10 Hz = 100ms interval
    assert!(limiter.should_update());
    assert!(!limiter.should_update());
    Ok(())
}

#[test]
fn rate_limiter_reset_allows_immediate_update() -> TestResult {
    let mut limiter = RateLimiter::new(1.0);
    assert!(limiter.should_update());
    assert!(!limiter.should_update());

    limiter.reset();
    assert!(limiter.should_update());
    Ok(())
}

#[test]
fn rate_limiter_high_frequency_always_passes_after_interval() -> TestResult {
    let mut limiter = RateLimiter::new(1000.0); // 1kHz = 1ms interval
    assert!(limiter.should_update());

    // Sleep well beyond interval
    thread::sleep(Duration::from_millis(5));
    assert!(limiter.should_update());
    Ok(())
}

// ============================================================================
// RateCounter — edge cases
// ============================================================================

#[test]
fn rate_counter_zero_increments() -> TestResult {
    let counter = RateCounter::new(Duration::from_secs(1));
    thread::sleep(Duration::from_millis(5));
    assert!((counter.rate() - 0.0).abs() < f64::EPSILON);
    Ok(())
}

#[test]
fn rate_counter_reset_clears_count() -> TestResult {
    let mut counter = RateCounter::new(Duration::from_secs(1));
    for _ in 0..100 {
        counter.increment();
    }
    counter.reset();
    thread::sleep(Duration::from_millis(5));
    assert!((counter.rate() - 0.0).abs() < f64::EPSILON);
    Ok(())
}

#[test]
fn rate_counter_rate_increases_with_increments() -> TestResult {
    let mut counter = RateCounter::new(Duration::from_secs(1));
    for _ in 0..100 {
        counter.increment();
    }
    thread::sleep(Duration::from_millis(10));
    let rate = counter.rate();
    assert!(rate > 0.0, "rate should be positive after increments");
    Ok(())
}
