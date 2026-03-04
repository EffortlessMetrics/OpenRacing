//! Deep individual tests for the MudRunner / SnowRunner telemetry adapter.
//!
//! Covers variant handling, JSON parsing via the SimHub bridge,
//! normalization of all fields, edge cases, and malformed payloads.

use racing_wheel_telemetry_mudrunner::{MudRunnerAdapter, MudRunnerVariant, TelemetryAdapter};
use std::time::Duration;

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[allow(clippy::too_many_arguments)]
fn make_json(
    speed: f32,
    rpms: f32,
    max_rpms: f32,
    gear: &str,
    throttle: f32,
    brake: f32,
    clutch: f32,
    steer_deg: f32,
    fuel_pct: f32,
    lat_g: f32,
    lon_g: f32,
    ffb: f32,
) -> Vec<u8> {
    format!(
        r#"{{"SpeedMs":{speed},"Rpms":{rpms},"MaxRpms":{max_rpms},"Gear":"{gear}","Throttle":{throttle},"Brake":{brake},"Clutch":{clutch},"SteeringAngle":{steer_deg},"FuelPercent":{fuel_pct},"LateralGForce":{lat_g},"LongitudinalGForce":{lon_g},"FFBValue":{ffb},"IsRunning":true,"IsInPit":false}}"#
    )
    .into_bytes()
}

// ── Variant identity tests ───────────────────────────────────────────────────

#[test]
fn deep_mudrunner_variant_game_id() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    assert_eq!(adapter.game_id(), "mudrunner");
    Ok(())
}

#[test]
fn deep_snowrunner_variant_game_id() -> TestResult {
    let adapter = MudRunnerAdapter::with_variant(MudRunnerVariant::SnowRunner);
    assert_eq!(adapter.game_id(), "snowrunner");
    Ok(())
}

#[test]
fn deep_update_rate_20hz() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    assert_eq!(adapter.expected_update_rate(), Duration::from_millis(50));
    Ok(())
}

#[test]
fn deep_default_is_mudrunner() -> TestResult {
    let adapter = MudRunnerAdapter::default();
    assert_eq!(adapter.game_id(), "mudrunner");
    Ok(())
}

#[test]
fn deep_variant_equality() -> TestResult {
    assert_eq!(MudRunnerVariant::MudRunner, MudRunnerVariant::MudRunner);
    assert_ne!(MudRunnerVariant::MudRunner, MudRunnerVariant::SnowRunner);
    Ok(())
}

// ── Packet parsing: valid ────────────────────────────────────────────────────

#[test]
fn deep_parse_offroad_driving_packet() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    // Typical MudRunner: low speed, moderate RPM, gear 2, heavy throttle
    let data = make_json(
        8.5, 2500.0, 4500.0, "2", 80.0, 0.0, 0.0, -30.0, 70.0, 0.3, 0.5, 0.2,
    );
    let t = adapter.normalize(&data)?;
    assert!((t.speed_ms - 8.5).abs() < 0.01, "speed_ms={}", t.speed_ms);
    assert!((t.rpm - 2500.0).abs() < 0.1);
    assert_eq!(t.gear, 2);
    // Throttle: 80/100
    assert!((t.throttle - 0.80).abs() < 0.001);
    // Steer: -30/450
    assert!((t.steering_angle - (-30.0 / 450.0)).abs() < 0.001);
    // Fuel: 70/100
    assert!((t.fuel_percent - 0.70).abs() < 0.001);
    Ok(())
}

#[test]
fn deep_parse_stationary_packet() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    let data = make_json(
        0.0, 800.0, 4500.0, "N", 0.0, 0.0, 0.0, 0.0, 100.0, 0.0, 0.0, 0.0,
    );
    let t = adapter.normalize(&data)?;
    assert_eq!(t.speed_ms, 0.0);
    assert!((t.rpm - 800.0).abs() < 0.1);
    assert_eq!(t.gear, 0);
    assert!((t.fuel_percent - 1.0).abs() < 0.001);
    Ok(())
}

#[test]
fn deep_parse_reverse_gear() -> TestResult {
    let adapter = MudRunnerAdapter::with_variant(MudRunnerVariant::SnowRunner);
    let data = make_json(
        3.0, 1500.0, 4500.0, "R", 30.0, 0.0, 50.0, 10.0, 60.0, 0.0, 0.0, 0.0,
    );
    let t = adapter.normalize(&data)?;
    assert_eq!(t.gear, -1, "R → -1");
    assert!((t.clutch - 0.50).abs() < 0.001, "clutch 50%");
    Ok(())
}

// ── Normalization: pedals ────────────────────────────────────────────────────

#[test]
fn deep_pedal_full_range() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    let data = make_json(
        0.0, 0.0, 0.0, "N", 100.0, 100.0, 100.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    );
    let t = adapter.normalize(&data)?;
    assert!((t.throttle - 1.0).abs() < 0.001, "full throttle");
    assert!((t.brake - 1.0).abs() < 0.001, "full brake");
    assert!((t.clutch - 1.0).abs() < 0.001, "full clutch");
    Ok(())
}

#[test]
fn deep_pedal_overrange_clamped() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    let json = br#"{"SpeedMs":0.0,"Rpms":0.0,"MaxRpms":0.0,"Gear":"N","Throttle":200.0,"Brake":150.0,"Clutch":300.0,"SteeringAngle":0.0,"FuelPercent":0.0,"LateralGForce":0.0,"LongitudinalGForce":0.0,"FFBValue":0.0,"IsRunning":false,"IsInPit":false}"#;
    let t = adapter.normalize(json)?;
    assert!((t.throttle - 1.0).abs() < 0.001);
    assert!((t.brake - 1.0).abs() < 0.001);
    assert!((t.clutch - 1.0).abs() < 0.001);
    Ok(())
}

// ── Normalization: steering ──────────────────────────────────────────────────

#[test]
fn deep_steering_full_lock() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    // Full left: -450/450 = -1.0
    let left = make_json(
        0.0, 0.0, 0.0, "N", 0.0, 0.0, 0.0, -450.0, 0.0, 0.0, 0.0, 0.0,
    );
    let t = adapter.normalize(&left)?;
    assert!((t.steering_angle - (-1.0)).abs() < 0.001, "full left");

    // Full right: 450/450 = 1.0
    let right = make_json(0.0, 0.0, 0.0, "N", 0.0, 0.0, 0.0, 450.0, 0.0, 0.0, 0.0, 0.0);
    let t = adapter.normalize(&right)?;
    assert!((t.steering_angle - 1.0).abs() < 0.001, "full right");
    Ok(())
}

// ── Normalization: speed and RPM ─────────────────────────────────────────────

#[test]
fn deep_negative_speed_clamped() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    let data = make_json(-5.0, 0.0, 0.0, "N", 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    let t = adapter.normalize(&data)?;
    assert!(t.speed_ms >= 0.0, "speed clamped");
    Ok(())
}

#[test]
fn deep_negative_rpm_clamped() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    let data = make_json(
        0.0, -500.0, 0.0, "N", 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    );
    let t = adapter.normalize(&data)?;
    assert!(t.rpm >= 0.0, "rpm clamped");
    Ok(())
}

// ── Malformed payloads ───────────────────────────────────────────────────────

#[test]
fn deep_empty_bytes_rejected() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    assert!(adapter.normalize(&[]).is_err());
    Ok(())
}

#[test]
fn deep_invalid_json_rejected() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    assert!(adapter.normalize(b"{{{{invalid json").is_err());
    Ok(())
}

#[test]
fn deep_non_utf8_rejected() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    assert!(adapter.normalize(&[0xFF, 0xFE, 0xFD]).is_err());
    Ok(())
}

#[test]
fn deep_empty_json_uses_defaults() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    let t = adapter.normalize(b"{}")?;
    assert_eq!(t.speed_ms, 0.0);
    assert_eq!(t.rpm, 0.0);
    assert_eq!(t.gear, 0);
    Ok(())
}

// ── Both variants parse identically ─────────────────────────────────────────

#[test]
fn deep_both_variants_same_normalization() -> TestResult {
    let mud = MudRunnerAdapter::new();
    let snow = MudRunnerAdapter::with_variant(MudRunnerVariant::SnowRunner);
    let data = make_json(
        15.0, 3000.0, 4500.0, "3", 60.0, 10.0, 0.0, 45.0, 80.0, 0.1, -0.2, 0.3,
    );

    let tm = mud.normalize(&data)?;
    let ts = snow.normalize(&data)?;

    assert_eq!(tm.speed_ms, ts.speed_ms, "speed");
    assert_eq!(tm.rpm, ts.rpm, "rpm");
    assert_eq!(tm.gear, ts.gear, "gear");
    assert_eq!(tm.throttle, ts.throttle, "throttle");
    assert_eq!(tm.steering_angle, ts.steering_angle, "steer");
    Ok(())
}

// ── Determinism ──────────────────────────────────────────────────────────────

#[test]
fn deep_deterministic_output() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    let data = make_json(
        12.0, 2800.0, 4500.0, "2", 70.0, 5.0, 0.0, -20.0, 55.0, 0.2, -0.1, 0.15,
    );
    let t1 = adapter.normalize(&data)?;
    let t2 = adapter.normalize(&data)?;
    assert_eq!(t1.speed_ms, t2.speed_ms);
    assert_eq!(t1.gear, t2.gear);
    assert_eq!(t1.throttle, t2.throttle);
    Ok(())
}

// ── Async stop_monitoring is a no-op ─────────────────────────────────────────

#[tokio::test]
async fn deep_stop_monitoring_succeeds() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    adapter.stop_monitoring().await?;
    Ok(())
}

#[tokio::test]
async fn deep_is_game_running_returns_false() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    assert!(!adapter.is_game_running().await?);
    Ok(())
}
