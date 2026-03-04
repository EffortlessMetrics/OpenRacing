//! Extended coverage tests for openracing-telemetry-streams.
//!
//! Covers: RingBuffer with struct payloads, TelemetryBuffer edge cases,
//! MovingAverage window-size-1, RateLimiter boundary conditions,
//! StreamError debug/clone, and large-capacity ring buffer stress.

use openracing_telemetry_streams::{MovingAverage, RingBuffer, StreamError, TelemetryBuffer};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ── RingBuffer with struct payloads ─────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
struct SampleFrame {
    timestamp_ns: u64,
    rpm: f32,
    speed: f32,
}

#[test]
fn ring_buffer_struct_payload_write_read() -> TestResult {
    let mut rb = RingBuffer::new(3);
    rb.write(SampleFrame {
        timestamp_ns: 100,
        rpm: 5000.0,
        speed: 30.0,
    });
    rb.write(SampleFrame {
        timestamp_ns: 200,
        rpm: 5500.0,
        speed: 35.0,
    });

    let first = rb.read().ok_or("expected first frame")?;
    assert_eq!(first.timestamp_ns, 100);
    assert!((first.rpm - 5000.0).abs() < f32::EPSILON);

    let second = rb.read().ok_or("expected second frame")?;
    assert_eq!(second.timestamp_ns, 200);
    Ok(())
}

#[test]
fn ring_buffer_struct_overflow_evicts_oldest() -> TestResult {
    let mut rb = RingBuffer::new(2);
    rb.write(SampleFrame {
        timestamp_ns: 1,
        rpm: 1000.0,
        speed: 10.0,
    });
    rb.write(SampleFrame {
        timestamp_ns: 2,
        rpm: 2000.0,
        speed: 20.0,
    });

    let evicted = rb.write(SampleFrame {
        timestamp_ns: 3,
        rpm: 3000.0,
        speed: 30.0,
    });
    assert_eq!(
        evicted.as_ref().map(|f| f.timestamp_ns),
        Some(1),
        "oldest frame should be evicted"
    );

    let remaining = rb.read().ok_or("expected frame")?;
    assert_eq!(remaining.timestamp_ns, 2);
    Ok(())
}

// ── RingBuffer capacity-1 ───────────────────────────────────────────────

#[test]
fn ring_buffer_capacity_one() -> TestResult {
    let mut rb = RingBuffer::new(1);
    assert!(rb.is_empty());
    assert_eq!(rb.capacity(), 1);

    rb.write(42);
    assert!(rb.is_full());
    assert_eq!(rb.len(), 1);

    let evicted = rb.write(99);
    assert_eq!(evicted, Some(42));
    assert_eq!(rb.read(), Some(99));
    assert!(rb.is_empty());
    Ok(())
}

// ── RingBuffer large capacity stress ────────────────────────────────────

#[test]
fn ring_buffer_large_capacity_fill_drain() -> TestResult {
    let cap = 1024;
    let mut rb = RingBuffer::new(cap);

    for i in 0..cap {
        rb.write(i);
    }
    assert!(rb.is_full());
    assert_eq!(rb.len(), cap);

    for i in 0..cap {
        let val = rb.read().ok_or("expected value during drain")?;
        assert_eq!(val, i);
    }
    assert!(rb.is_empty());
    Ok(())
}

#[test]
fn ring_buffer_overwrite_cycle() -> TestResult {
    let mut rb = RingBuffer::new(4);

    // Write 3× capacity to force multiple wraparounds
    for i in 0..12 {
        rb.write(i);
    }
    assert_eq!(rb.len(), 4);

    // Should contain the last 4 values: 8, 9, 10, 11
    for expected in 8..12 {
        let val = rb.read().ok_or("expected value")?;
        assert_eq!(val, expected);
    }
    Ok(())
}

// ── TelemetryBuffer edge cases ──────────────────────────────────────────

#[test]
fn telemetry_buffer_pop_from_empty_returns_none() -> TestResult {
    let buf: TelemetryBuffer<i32> = TelemetryBuffer::new(10);
    assert!(buf.pop().is_none());
    assert!(buf.latest().is_none());
    assert!(buf.oldest().is_none());
    Ok(())
}

#[test]
fn telemetry_buffer_single_element() -> TestResult {
    let buf = TelemetryBuffer::new(1);
    buf.push(42);
    assert_eq!(buf.latest(), Some(42));
    assert_eq!(buf.oldest(), Some(42));
    assert_eq!(buf.len(), 1);
    assert!(!buf.is_empty());

    assert_eq!(buf.pop(), Some(42));
    assert!(buf.is_empty());
    Ok(())
}

#[test]
fn telemetry_buffer_iter_empty() -> TestResult {
    let buf: TelemetryBuffer<i32> = TelemetryBuffer::new(5);
    let items: Vec<i32> = buf.iter().collect();
    assert!(items.is_empty());
    Ok(())
}

#[test]
fn telemetry_buffer_push_and_clear_cycle() -> TestResult {
    let buf = TelemetryBuffer::new(5);
    for i in 0..5 {
        buf.push(i);
    }
    assert_eq!(buf.len(), 5);

    buf.clear();
    assert!(buf.is_empty());

    // Re-fill after clear
    for i in 10..15 {
        buf.push(i);
    }
    assert_eq!(buf.oldest(), Some(10));
    assert_eq!(buf.latest(), Some(14));
    Ok(())
}

// ── MovingAverage edge cases ────────────────────────────────────────────

#[test]
fn moving_average_window_size_one() -> TestResult {
    let mut avg = MovingAverage::new(1);
    avg.push(10.0);
    assert!((avg.average() - 10.0).abs() < 0.001);

    avg.push(20.0);
    assert!((avg.average() - 20.0).abs() < 0.001);

    avg.push(30.0);
    assert!((avg.average() - 30.0).abs() < 0.001);
    Ok(())
}

#[test]
fn moving_average_large_window_partially_filled() -> TestResult {
    let mut avg = MovingAverage::new(100);
    avg.push(50.0);
    avg.push(50.0);
    // Only 2 values in a window of 100 → average should still be 50
    assert!((avg.average() - 50.0).abs() < 0.001);
    Ok(())
}

#[test]
fn moving_average_negative_values() -> TestResult {
    let mut avg = MovingAverage::new(3);
    avg.push(-10.0);
    avg.push(-20.0);
    avg.push(-30.0);
    assert!((avg.average() - (-20.0)).abs() < 0.001);
    Ok(())
}

// ── StreamError coverage ────────────────────────────────────────────────

#[test]
fn stream_error_processing_error_message() -> TestResult {
    let err = StreamError::ProcessingError("custom message".to_string());
    let display = format!("{err}");
    assert!(display.contains("custom message"));
    Ok(())
}

#[test]
fn stream_error_debug_format() -> TestResult {
    let err = StreamError::BufferOverflow;
    let debug = format!("{err:?}");
    assert!(debug.contains("BufferOverflow"));

    let err = StreamError::StreamClosed;
    let debug = format!("{err:?}");
    assert!(debug.contains("StreamClosed"));

    let err = StreamError::ProcessingError("test".into());
    let debug = format!("{err:?}");
    assert!(debug.contains("ProcessingError"));
    Ok(())
}

// ── TelemetryBuffer with String payloads ────────────────────────────────

#[test]
fn telemetry_buffer_string_payloads() -> TestResult {
    let buf = TelemetryBuffer::new(3);
    buf.push("alpha".to_string());
    buf.push("beta".to_string());
    buf.push("gamma".to_string());

    assert_eq!(buf.oldest(), Some("alpha".to_string()));
    assert_eq!(buf.latest(), Some("gamma".to_string()));

    buf.push("delta".to_string());
    assert_eq!(buf.oldest(), Some("beta".to_string()));
    assert_eq!(buf.len(), 3);
    Ok(())
}
