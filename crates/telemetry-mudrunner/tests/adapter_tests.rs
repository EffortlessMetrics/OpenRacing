//! Integration tests for the `racing-wheel-telemetry-mudrunner` crate.
//!
//! Tests verify MudRunner / SnowRunner SimHub JSON parsing via the public API.

use racing_wheel_telemetry_mudrunner::{
    MudRunnerAdapter, MudRunnerVariant, NormalizedTelemetry, TelemetryAdapter,
};
use std::time::Duration;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const VALID_JSON: &[u8] = br#"{"SpeedMs":0.0,"Rpms":0.0,"MaxRpms":0.0,"Gear":"N","Throttle":0.0,"Brake":0.0,"Clutch":0.0,"SteeringAngle":0.0,"FuelPercent":0.0,"LateralGForce":0.0,"LongitudinalGForce":0.0,"FFBValue":0.0,"IsRunning":false,"IsInPit":false}"#;

#[test]
fn test_game_id_mudrunner() {
    let adapter = MudRunnerAdapter::new();
    assert_eq!(adapter.game_id(), "mudrunner");
}

#[test]
fn test_game_id_snowrunner() {
    let adapter = MudRunnerAdapter::with_variant(MudRunnerVariant::SnowRunner);
    assert_eq!(adapter.game_id(), "snowrunner");
}

#[test]
fn test_variant_equality() {
    assert_eq!(MudRunnerVariant::MudRunner, MudRunnerVariant::MudRunner);
    assert_ne!(MudRunnerVariant::MudRunner, MudRunnerVariant::SnowRunner);
}

#[test]
fn test_default_is_mudrunner() {
    let adapter = MudRunnerAdapter::default();
    assert_eq!(adapter.game_id(), "mudrunner");
}

#[test]
fn test_update_rate_20hz() {
    let adapter = MudRunnerAdapter::new();
    assert_eq!(adapter.expected_update_rate(), Duration::from_millis(50));
}

#[test]
fn test_parse_zero_packet() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    let t = adapter.normalize(VALID_JSON)?;
    assert_eq!(t.speed_ms, 0.0);
    assert_eq!(t.rpm, 0.0);
    assert_eq!(t.gear, 0);
    Ok(())
}

#[test]
fn test_parse_packet_with_values() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    let json = br#"{"SpeedMs":8.5,"Rpms":2500.0,"MaxRpms":4500.0,"Gear":"2","Throttle":60.0,"Brake":20.0,"Clutch":0.0,"SteeringAngle":-90.0,"FuelPercent":70.0,"LateralGForce":0.3,"LongitudinalGForce":0.5,"FFBValue":0.2,"IsRunning":true,"IsInPit":false}"#;
    let t = adapter.normalize(json)?;
    assert!((t.speed_ms - 8.5).abs() < 0.01, "speed_ms={}", t.speed_ms);
    assert!((t.rpm - 2500.0).abs() < 0.1, "rpm={}", t.rpm);
    assert_eq!(t.gear, 2);
    // Throttle: 60/100 = 0.6
    assert!((t.throttle - 0.6).abs() < 0.001, "throttle={}", t.throttle);
    // Brake: 20/100 = 0.2
    assert!((t.brake - 0.2).abs() < 0.001, "brake={}", t.brake);
    Ok(())
}

#[test]
fn test_empty_input_rejected() {
    let adapter = MudRunnerAdapter::new();
    assert!(
        adapter.normalize(&[]).is_err(),
        "empty input must be rejected"
    );
}

#[test]
fn test_invalid_json_rejected() {
    let adapter = MudRunnerAdapter::new();
    assert!(
        adapter.normalize(b"not json at all").is_err(),
        "invalid JSON must be rejected"
    );
}

#[test]
fn test_non_utf8_rejected() {
    let adapter = MudRunnerAdapter::new();
    assert!(
        adapter.normalize(&[0xFF, 0xFE, 0xFD]).is_err(),
        "non-UTF8 must be rejected"
    );
}

#[test]
fn test_reverse_gear() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    let json = br#"{"SpeedMs":2.0,"Rpms":1500.0,"MaxRpms":4500.0,"Gear":"R","Throttle":30.0,"Brake":0.0,"Clutch":0.0,"SteeringAngle":0.0,"FuelPercent":50.0,"LateralGForce":0.0,"LongitudinalGForce":0.0,"FFBValue":0.0,"IsRunning":true,"IsInPit":false}"#;
    let t = adapter.normalize(json)?;
    assert_eq!(t.gear, -1, "gear 'R' should normalize to -1");
    Ok(())
}

#[test]
fn test_snowrunner_parses_same_protocol() -> TestResult {
    let adapter = MudRunnerAdapter::with_variant(MudRunnerVariant::SnowRunner);
    let t = adapter.normalize(VALID_JSON)?;
    assert_eq!(t.speed_ms, 0.0);
    Ok(())
}

#[test]
fn test_normalized_telemetry_default() {
    let t = NormalizedTelemetry::default();
    assert_eq!(t.speed_ms, 0.0);
    assert_eq!(t.rpm, 0.0);
    assert_eq!(t.gear, 0);
    assert_eq!(t.throttle, 0.0);
    assert_eq!(t.brake, 0.0);
}

#[tokio::test]
async fn test_is_game_running_returns_false() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    assert!(!adapter.is_game_running().await?);
    Ok(())
}

#[tokio::test]
async fn test_stop_monitoring_is_noop() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    adapter.stop_monitoring().await?;
    Ok(())
}
