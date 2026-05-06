//! Hardening tests for the openracing-native-plugin SPSC shared memory channel.
//!
//! Covers ring buffer wrap-around, capacity exhaustion, concurrent
//! producer-consumer correctness, try_write/try_read semantics,
//! shutdown flag propagation, and frame size mismatch handling.

use openracing_native_plugin::error::NativePluginError;
use openracing_native_plugin::spsc::SpscChannel;
use std::sync::Arc;
use std::thread;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ============================================================================
// Basic read/write edge cases
// ============================================================================

#[test]
fn spsc_read_empty_returns_no_data() -> TestResult {
    let channel = SpscChannel::with_capacity(64, 8)?;
    let reader = channel.reader();
    let mut buf = vec![0u8; 64];
    let result = reader.read(&mut buf);
    assert!(
        matches!(result, Err(NativePluginError::NoDataAvailable)),
        "reading empty channel should return NoDataAvailable"
    );
    Ok(())
}

#[test]
fn spsc_single_frame_round_trip() -> TestResult {
    let frame_size = 32;
    let channel = SpscChannel::with_capacity(frame_size, 4)?;
    let writer = channel.writer();
    let reader = channel.reader();

    let data: Vec<u8> = (0..frame_size as u8).collect();
    writer.write(&data)?;

    let mut buf = vec![0u8; frame_size];
    reader.read(&mut buf)?;
    assert_eq!(buf, data);
    Ok(())
}

#[test]
fn spsc_fifo_ordering_preserved() -> TestResult {
    let frame_size = 8;
    let capacity = 16;
    let channel = SpscChannel::with_capacity(frame_size, capacity)?;
    let writer = channel.writer();
    let reader = channel.reader();

    for i in 0u8..capacity as u8 {
        writer.write(&[i; 8])?;
    }

    for i in 0u8..capacity as u8 {
        let mut buf = vec![0u8; frame_size];
        reader.read(&mut buf)?;
        assert_eq!(buf, vec![i; 8], "FIFO ordering violated at frame {i}");
    }
    Ok(())
}

#[test]
fn spsc_full_buffer_returns_ring_buffer_full() -> TestResult {
    let frame_size = 16;
    let capacity = 4u32;
    let channel = SpscChannel::with_capacity(frame_size, capacity)?;
    let writer = channel.writer();

    // Fill
    for i in 0u8..capacity as u8 {
        writer.write(&vec![i; frame_size])?;
    }

    // Next write should fail
    let result = writer.write(&vec![0xFF; frame_size]);
    assert!(
        matches!(result, Err(NativePluginError::RingBufferFull)),
        "expected RingBufferFull"
    );
    Ok(())
}

#[test]
fn spsc_drain_then_refill() -> TestResult {
    let frame_size = 8;
    let capacity = 4u32;
    let channel = SpscChannel::with_capacity(frame_size, capacity)?;
    let writer = channel.writer();
    let reader = channel.reader();

    // Fill, drain, refill
    for round in 0u8..3 {
        for i in 0..capacity as u8 {
            writer.write(&vec![round * 10 + i; frame_size])?;
        }
        for i in 0..capacity as u8 {
            let mut buf = vec![0u8; frame_size];
            reader.read(&mut buf)?;
            assert_eq!(buf[0], round * 10 + i);
        }
    }
    Ok(())
}

// ============================================================================
// try_write / try_read semantics
// ============================================================================

#[test]
fn spsc_try_write_returns_true_when_space_available() -> TestResult {
    let frame_size = 8;
    let channel = SpscChannel::with_capacity(frame_size, 4)?;
    let writer = channel.writer();

    let result = writer.try_write(&vec![0; frame_size])?;
    assert!(result, "try_write should return true when space available");
    Ok(())
}

#[test]
fn spsc_try_write_returns_false_when_full() -> TestResult {
    let frame_size = 8;
    let capacity = 2u32;
    let channel = SpscChannel::with_capacity(frame_size, capacity)?;
    let writer = channel.writer();

    writer.write(&vec![0; frame_size])?;
    writer.write(&vec![0; frame_size])?;

    let result = writer.try_write(&vec![0; frame_size])?;
    assert!(!result, "try_write should return false when full");
    Ok(())
}

#[test]
fn spsc_try_read_returns_false_when_empty() -> TestResult {
    let frame_size = 8;
    let channel = SpscChannel::with_capacity(frame_size, 4)?;
    let reader = channel.reader();

    let mut buf = vec![0u8; frame_size];
    let result = reader.try_read(&mut buf)?;
    assert!(!result, "try_read should return false when empty");
    Ok(())
}

#[test]
fn spsc_try_read_returns_true_when_data_available() -> TestResult {
    let frame_size = 8;
    let channel = SpscChannel::with_capacity(frame_size, 4)?;
    let writer = channel.writer();
    let reader = channel.reader();

    writer.write(&vec![42; frame_size])?;

    let mut buf = vec![0u8; frame_size];
    let result = reader.try_read(&mut buf)?;
    assert!(result, "try_read should return true when data available");
    assert_eq!(buf, vec![42; frame_size]);
    Ok(())
}

// ============================================================================
// has_data
// ============================================================================

#[test]
fn spsc_has_data_false_when_empty() -> TestResult {
    let channel = SpscChannel::with_capacity(16, 4)?;
    let reader = channel.reader();
    assert!(!reader.has_data());
    Ok(())
}

#[test]
fn spsc_has_data_true_after_write() -> TestResult {
    let frame_size = 16;
    let channel = SpscChannel::with_capacity(frame_size, 4)?;
    let writer = channel.writer();
    let reader = channel.reader();

    writer.write(&vec![0; frame_size])?;
    assert!(reader.has_data());
    Ok(())
}

#[test]
fn spsc_has_data_false_after_drain() -> TestResult {
    let frame_size = 16;
    let channel = SpscChannel::with_capacity(frame_size, 4)?;
    let writer = channel.writer();
    let reader = channel.reader();

    writer.write(&vec![0; frame_size])?;
    let mut buf = vec![0u8; frame_size];
    reader.read(&mut buf)?;
    assert!(!reader.has_data());
    Ok(())
}

// ============================================================================
// Shutdown flag
// ============================================================================

#[test]
fn spsc_shutdown_flag_propagates() -> TestResult {
    let channel = SpscChannel::with_capacity(16, 4)?;
    assert!(!channel.is_shutdown());

    channel.shutdown();
    assert!(channel.is_shutdown());
    Ok(())
}

// ============================================================================
// Frame size mismatch
// ============================================================================

#[test]
fn spsc_write_undersized_frame_returns_error() -> TestResult {
    let frame_size = 64;
    let channel = SpscChannel::with_capacity(frame_size, 4)?;
    let writer = channel.writer();

    let small_frame = vec![0u8; 32];
    let result = writer.write(&small_frame);
    assert!(matches!(
        result,
        Err(NativePluginError::FrameSizeMismatch {
            expected: 64,
            actual: 32
        })
    ));
    Ok(())
}

#[test]
fn spsc_write_oversized_frame_returns_error() -> TestResult {
    let frame_size = 64;
    let channel = SpscChannel::with_capacity(frame_size, 4)?;
    let writer = channel.writer();

    let big_frame = vec![0u8; 128];
    let result = writer.write(&big_frame);
    assert!(matches!(
        result,
        Err(NativePluginError::FrameSizeMismatch {
            expected: 64,
            actual: 128
        })
    ));
    Ok(())
}

#[test]
fn spsc_read_undersized_buffer_returns_error() -> TestResult {
    let frame_size = 64;
    let channel = SpscChannel::with_capacity(frame_size, 4)?;
    let writer = channel.writer();
    let reader = channel.reader();

    writer.write(&vec![0; frame_size])?;

    let mut small_buf = vec![0u8; 32];
    let result = reader.read(&mut small_buf);
    assert!(matches!(
        result,
        Err(NativePluginError::BufferSizeMismatch {
            expected: 64,
            actual: 32
        })
    ));
    Ok(())
}

// ============================================================================
// Shared memory size limit
// ============================================================================

#[test]
fn spsc_oversized_allocation_fails() -> TestResult {
    // 4MB max → frame_size * capacity > 4MB should fail
    let result = SpscChannel::with_capacity(1024 * 1024, 8);
    assert!(result.is_err(), "creating a channel over 4MB should fail");
    Ok(())
}

// ============================================================================
// Channel metadata
// ============================================================================

#[test]
fn spsc_channel_metadata_consistent() -> TestResult {
    let frame_size = 128;
    let capacity = 16u32;
    let channel = SpscChannel::with_capacity(frame_size, capacity)?;

    assert_eq!(channel.frame_size(), frame_size);
    assert_eq!(channel.max_frames(), capacity);
    assert!(!channel.os_id().is_empty());
    Ok(())
}

// ============================================================================
// Concurrent producer-consumer
// ============================================================================

#[test]
fn spsc_concurrent_single_producer_single_consumer() -> TestResult {
    let frame_size = 8;
    let capacity = 64u32;
    let total_frames = 500u32;
    let channel = Arc::new(SpscChannel::with_capacity(frame_size, capacity)?);

    let producer_ch = Arc::clone(&channel);
    let producer = thread::spawn(move || -> u32 {
        let writer = producer_ch.writer();
        let mut written = 0u32;
        for i in 0..total_frames {
            let frame = (i as u64).to_le_bytes();
            loop {
                match writer.try_write(&frame) {
                    Ok(true) => {
                        written += 1;
                        break;
                    }
                    Ok(false) => thread::yield_now(),
                    Err(_) => break,
                }
            }
        }
        written
    });

    let consumer_ch = Arc::clone(&channel);
    let consumer = thread::spawn(move || -> u32 {
        let reader = consumer_ch.reader();
        let mut read_count = 0u32;
        let mut last_value: Option<u64> = None;

        loop {
            let mut buf = vec![0u8; frame_size];
            match reader.try_read(&mut buf) {
                Ok(true) => {
                    let val = u64::from_le_bytes(buf.try_into().map_err(|_| "bad len").unwrap());
                    // Verify FIFO ordering
                    if let Some(prev) = last_value {
                        assert!(val > prev, "FIFO order violated: {val} <= {prev}");
                    }
                    last_value = Some(val);
                    read_count += 1;
                    if read_count >= total_frames {
                        break;
                    }
                }
                Ok(false) => thread::yield_now(),
                Err(_) => break,
            }
        }
        read_count
    });

    let written = producer.join().map_err(|_| "producer panicked")?;
    let read = consumer.join().map_err(|_| "consumer panicked")?;

    assert_eq!(written, total_frames);
    assert_eq!(read, total_frames);
    Ok(())
}
