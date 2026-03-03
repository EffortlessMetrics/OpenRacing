//! Integration tests for the `racing-wheel-telemetry-simhub` crate.
//!
//! Tests verify SimHub JSON UDP parsing via the public API.

use racing_wheel_telemetry_simhub::{NormalizedTelemetry, SimHubAdapter, TelemetryAdapter};
use std::time::Duration;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn full_packet() -> &'static [u8] {
    br#"{"SpeedMs":22.5,"Rpms":4500.0,"MaxRpms":8000.0,"Gear":"3","Throttle":75.0,"Brake":10.0,"Clutch":0.0,"SteeringAngle":-90.0,"FuelPercent":82.3,"LateralGForce":1.2,"LongitudinalGForce":-0.5,"FFBValue":0.35,"IsRunning":true,"IsInPit":false}"#
}

fn zero_packet() -> &'static [u8] {
    br#"{"SpeedMs":0.0,"Rpms":0.0,"MaxRpms":0.0,"Gear":"N","Throttle":0.0,"Brake":0.0,"Clutch":0.0,"SteeringAngle":0.0,"FuelPercent":0.0,"LateralGForce":0.0,"LongitudinalGForce":0.0,"FFBValue":0.0,"IsRunning":false,"IsInPit":false}"#
}

#[test]
fn test_game_id() {
    let adapter = SimHubAdapter::new();
    assert_eq!(adapter.game_id(), "simhub");
}

#[test]
fn test_default_trait() {
    let adapter = SimHubAdapter::default();
    assert_eq!(adapter.game_id(), "simhub");
}

#[test]
fn test_update_rate_60hz() {
    let adapter = SimHubAdapter::new();
    assert_eq!(adapter.expected_update_rate(), Duration::from_millis(16));
}

#[test]
fn test_parse_full_packet() -> TestResult {
    let adapter = SimHubAdapter::new();
    let t = adapter.normalize(full_packet())?;
    assert!((t.speed_ms - 22.5).abs() < 0.01, "speed_ms");
    assert!((t.rpm - 4500.0).abs() < 0.1, "rpm");
    assert!((t.max_rpm - 8000.0).abs() < 0.1, "max_rpm");
    assert_eq!(t.gear, 3, "gear");
    // Throttle: 75/100 = 0.75
    assert!((t.throttle - 0.75).abs() < 0.001, "throttle");
    // Brake: 10/100 = 0.10
    assert!((t.brake - 0.10).abs() < 0.001, "brake");
    assert_eq!(t.clutch, 0.0, "clutch");
    // SteeringAngle: -90/450 = -0.2
    assert!(
        (t.steering_angle - (-0.2)).abs() < 0.001,
        "steering_angle={}",
        t.steering_angle
    );
    assert!((t.fuel_percent - 0.823).abs() < 0.001, "fuel_percent");
    assert!((t.lateral_g - 1.2).abs() < 0.001, "lateral_g");
    assert!((t.longitudinal_g - (-0.5)).abs() < 0.001, "longitudinal_g");
    assert!((t.ffb_scalar - 0.35).abs() < 0.001, "ffb_scalar");
    Ok(())
}

#[test]
fn test_parse_zero_packet() -> TestResult {
    let adapter = SimHubAdapter::new();
    let t = adapter.normalize(zero_packet())?;
    assert_eq!(t.speed_ms, 0.0);
    assert_eq!(t.gear, 0);
    assert_eq!(t.throttle, 0.0);
    Ok(())
}

#[test]
fn test_empty_bytes_rejected() {
    let adapter = SimHubAdapter::new();
    assert!(
        adapter.normalize(&[]).is_err(),
        "empty bytes must be rejected"
    );
}

#[test]
fn test_invalid_json_rejected() {
    let adapter = SimHubAdapter::new();
    assert!(
        adapter.normalize(b"{broken json").is_err(),
        "invalid JSON must be rejected"
    );
}

#[test]
fn test_non_utf8_rejected() {
    let adapter = SimHubAdapter::new();
    assert!(
        adapter.normalize(&[0xFF, 0xFE, 0xFD]).is_err(),
        "non-UTF8 must be rejected"
    );
}

#[test]
fn test_gear_reverse() -> TestResult {
    let adapter = SimHubAdapter::new();
    let json = br#"{"SpeedMs":5.0,"Rpms":2000.0,"MaxRpms":8000.0,"Gear":"R","Throttle":20.0,"Brake":0.0,"Clutch":0.0,"SteeringAngle":0.0,"FuelPercent":50.0,"LateralGForce":0.0,"LongitudinalGForce":0.0,"FFBValue":0.0,"IsRunning":true,"IsInPit":false}"#;
    let t = adapter.normalize(json)?;
    assert_eq!(t.gear, -1, "gear 'R' should normalize to -1");
    Ok(())
}

#[test]
fn test_gear_neutral() -> TestResult {
    let adapter = SimHubAdapter::new();
    let json = br#"{"SpeedMs":0.0,"Rpms":800.0,"MaxRpms":8000.0,"Gear":"N","Throttle":0.0,"Brake":0.0,"Clutch":0.0,"SteeringAngle":0.0,"FuelPercent":50.0,"LateralGForce":0.0,"LongitudinalGForce":0.0,"FFBValue":0.0,"IsRunning":true,"IsInPit":false}"#;
    let t = adapter.normalize(json)?;
    assert_eq!(t.gear, 0, "gear 'N' should normalize to 0");
    Ok(())
}

#[test]
fn test_throttle_normalised_from_percentage() -> TestResult {
    let adapter = SimHubAdapter::new();
    let json = br#"{"SpeedMs":0.0,"Rpms":0.0,"MaxRpms":0.0,"Gear":"N","Throttle":100.0,"Brake":50.0,"Clutch":25.0,"SteeringAngle":0.0,"FuelPercent":100.0,"LateralGForce":0.0,"LongitudinalGForce":0.0,"FFBValue":0.0,"IsRunning":true,"IsInPit":false}"#;
    let t = adapter.normalize(json)?;
    assert!((t.throttle - 1.0).abs() < 0.001, "100% → 1.0");
    assert!((t.brake - 0.5).abs() < 0.001, "50% → 0.5");
    assert!((t.clutch - 0.25).abs() < 0.001, "25% → 0.25");
    assert!((t.fuel_percent - 1.0).abs() < 0.001, "100% fuel → 1.0");
    Ok(())
}

#[test]
fn test_steering_full_lock_clamped() -> TestResult {
    let adapter = SimHubAdapter::new();
    // -450° should clamp to -1.0 normalised
    let json = br#"{"SpeedMs":0.0,"Rpms":0.0,"MaxRpms":0.0,"Gear":"N","Throttle":0.0,"Brake":0.0,"Clutch":0.0,"SteeringAngle":-450.0,"FuelPercent":0.0,"LateralGForce":0.0,"LongitudinalGForce":0.0,"FFBValue":0.0,"IsRunning":true,"IsInPit":false}"#;
    let t = adapter.normalize(json)?;
    assert!(
        (t.steering_angle - (-1.0)).abs() < 0.001,
        "full left lock={}",
        t.steering_angle
    );
    Ok(())
}

#[test]
fn test_rpm_alias() -> TestResult {
    let adapter = SimHubAdapter::new();
    // Some SimHub configs send "Rpm" instead of "Rpms"
    let json = br#"{"SpeedMs":0.0,"Rpm":3000.0,"MaxRpms":0.0,"Gear":"N","Throttle":0.0,"Brake":0.0,"Clutch":0.0,"SteeringAngle":0.0,"FuelPercent":0.0,"LateralGForce":0.0,"LongitudinalGForce":0.0,"FFBValue":0.0,"IsRunning":true,"IsInPit":false}"#;
    let t = adapter.normalize(json)?;
    assert!((t.rpm - 3000.0).abs() < 0.1, "Rpm alias should work");
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
    assert_eq!(t.ffb_scalar, 0.0);
}
