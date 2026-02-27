//! Integration tests for the `racing-wheel-telemetry-ams2` crate.
//!
//! AMS2 uses Windows shared memory, so most behavioral tests are adapter-level
//! (game_id, update_rate, etc.). Shared memory tests require a running AMS2 instance.

use racing_wheel_telemetry_ams2::{AMS2Adapter, TelemetryAdapter};
use std::time::Duration;

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn test_game_id() {
    let adapter = AMS2Adapter::new();
    assert_eq!(adapter.game_id(), "ams2");
}

#[test]
fn test_default_update_rate() {
    let adapter = AMS2Adapter::new();
    assert_eq!(
        adapter.expected_update_rate(),
        Duration::from_millis(16),
        "AMS2 default update rate should be ~60Hz (16ms)"
    );
}

#[test]
fn test_adapter_is_default() {
    let a = AMS2Adapter::new();
    let b = AMS2Adapter::default();
    // Both should have the same game_id and update rate.
    assert_eq!(a.game_id(), b.game_id());
    assert_eq!(a.expected_update_rate(), b.expected_update_rate());
}

/// Normalizing an empty slice must return an error, not panic.
#[test]
fn test_normalize_empty_returns_error() {
    let adapter = AMS2Adapter::new();
    assert!(
        adapter.normalize(&[]).is_err(),
        "empty raw data must return error"
    );
}

/// Normalizing arbitrary bytes must not panic.
#[test]
fn test_normalize_arbitrary_bytes_no_panic() {
    let adapter = AMS2Adapter::new();
    // Fill with junk data — result is unspecified but must not panic.
    let _ = adapter.normalize(&vec![0xAB; 2048]);
}

#[tokio::test]
async fn test_is_game_running_returns_result() -> TestResult {
    let adapter = AMS2Adapter::new();
    // Should return Ok(bool) regardless of whether AMS2 is actually running.
    let _ = adapter.is_game_running().await?;
    Ok(())
}

#[tokio::test]
async fn test_stop_monitoring_is_safe() -> TestResult {
    let adapter = AMS2Adapter::new();
    // stop_monitoring should always succeed (no-op when not started).
    adapter.stop_monitoring().await?;
    Ok(())
}
