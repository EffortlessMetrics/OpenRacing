//! Deep individual tests for the SimHub telemetry adapter.
//!
//! Covers JSON parsing, normalization of all fields, gear string handling,
//! steering preference logic, field aliases, malformed payloads, and edge cases.

use racing_wheel_telemetry_simhub::{SimHubAdapter, TelemetryAdapter};

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

// ── Valid packet parsing ─────────────────────────────────────────────────────

#[test]
fn deep_parse_race_pace_packet() -> TestResult {
    let adapter = SimHubAdapter::new();
    let data = make_json(55.0, 7200.0, 8500.0, "4", 80.0, 0.0, 0.0, -45.0, 65.0, 0.8, -0.3, 0.5);
    let t = adapter.normalize(&data)?;
    assert!((t.speed_ms - 55.0).abs() < 0.01);
    assert!((t.rpm - 7200.0).abs() < 0.1);
    assert!((t.max_rpm - 8500.0).abs() < 0.1);
    assert_eq!(t.gear, 4);
    // Throttle: 80/100 = 0.80
    assert!((t.throttle - 0.80).abs() < 0.001);
    assert_eq!(t.brake, 0.0);
    // Steer: -45/450 = -0.1
    assert!((t.steering_angle - (-0.1)).abs() < 0.001);
    // Fuel: 65/100 = 0.65
    assert!((t.fuel_percent - 0.65).abs() < 0.001);
    assert!((t.lateral_g - 0.8).abs() < 0.001);
    assert!((t.longitudinal_g - (-0.3)).abs() < 0.001);
    assert!((t.ffb_scalar - 0.5).abs() < 0.001);
    Ok(())
}

#[test]
fn deep_parse_zero_packet() -> TestResult {
    let adapter = SimHubAdapter::new();
    let data = make_json(0.0, 0.0, 0.0, "N", 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    let t = adapter.normalize(&data)?;
    assert_eq!(t.speed_ms, 0.0);
    assert_eq!(t.rpm, 0.0);
    assert_eq!(t.gear, 0);
    assert_eq!(t.throttle, 0.0);
    assert_eq!(t.steering_angle, 0.0);
    Ok(())
}

// ── Gear string parsing ─────────────────────────────────────────────────────

#[test]
fn deep_gear_reverse() -> TestResult {
    let adapter = SimHubAdapter::new();
    let data = make_json(0.0, 1000.0, 8000.0, "R", 0.0, 0.0, 100.0, 0.0, 50.0, 0.0, 0.0, 0.0);
    let t = adapter.normalize(&data)?;
    assert_eq!(t.gear, -1, "R → -1");
    // clutch=100 → 1.0
    assert!((t.clutch - 1.0).abs() < 0.001);
    Ok(())
}

#[test]
fn deep_gear_neutral_variants() -> TestResult {
    let adapter = SimHubAdapter::new();
    for gear_str in ["N", ""] {
        let json = format!(
            r#"{{"SpeedMs":0.0,"Rpms":0.0,"MaxRpms":0.0,"Gear":"{gear_str}","Throttle":0.0,"Brake":0.0,"Clutch":0.0,"SteeringAngle":0.0,"FuelPercent":0.0,"LateralGForce":0.0,"LongitudinalGForce":0.0,"FFBValue":0.0,"IsRunning":false,"IsInPit":false}}"#
        );
        let t = adapter.normalize(json.as_bytes())?;
        assert_eq!(t.gear, 0, "gear '{gear_str}' → 0");
    }
    Ok(())
}

#[test]
fn deep_gear_forward_1_through_9() -> TestResult {
    let adapter = SimHubAdapter::new();
    for g in 1..=9 {
        let json = format!(
            r#"{{"SpeedMs":0.0,"Rpms":0.0,"MaxRpms":0.0,"Gear":"{g}","Throttle":0.0,"Brake":0.0,"Clutch":0.0,"SteeringAngle":0.0,"FuelPercent":0.0,"LateralGForce":0.0,"LongitudinalGForce":0.0,"FFBValue":0.0,"IsRunning":false,"IsInPit":false}}"#
        );
        let t = adapter.normalize(json.as_bytes())?;
        assert_eq!(t.gear, g, "gear '{g}' should parse to {g}");
    }
    Ok(())
}

#[test]
fn deep_gear_invalid_string_defaults_zero() -> TestResult {
    let adapter = SimHubAdapter::new();
    let json = br#"{"SpeedMs":0.0,"Rpms":0.0,"MaxRpms":0.0,"Gear":"XYZ","Throttle":0.0,"Brake":0.0,"Clutch":0.0,"SteeringAngle":0.0,"FuelPercent":0.0,"LateralGForce":0.0,"LongitudinalGForce":0.0,"FFBValue":0.0,"IsRunning":false,"IsInPit":false}"#;
    let t = adapter.normalize(json)?;
    assert_eq!(t.gear, 0, "unparsable gear string → 0");
    Ok(())
}

// ── Normalization: throttle/brake/clutch (0–100 → 0–1) ──────────────────────

#[test]
fn deep_pedal_normalization() -> TestResult {
    let adapter = SimHubAdapter::new();
    let data = make_json(0.0, 0.0, 0.0, "N", 50.0, 25.0, 75.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    let t = adapter.normalize(&data)?;
    assert!((t.throttle - 0.5).abs() < 0.001);
    assert!((t.brake - 0.25).abs() < 0.001);
    assert!((t.clutch - 0.75).abs() < 0.001);
    Ok(())
}

#[test]
fn deep_pedal_overclamped() -> TestResult {
    let adapter = SimHubAdapter::new();
    // Values > 100 → clamp to 1.0; values < 0 → clamp to 0.0
    let json = br#"{"SpeedMs":0.0,"Rpms":0.0,"MaxRpms":0.0,"Gear":"N","Throttle":150.0,"Brake":-10.0,"Clutch":200.0,"SteeringAngle":0.0,"FuelPercent":0.0,"LateralGForce":0.0,"LongitudinalGForce":0.0,"FFBValue":0.0,"IsRunning":false,"IsInPit":false}"#;
    let t = adapter.normalize(json)?;
    assert!((t.throttle - 1.0).abs() < 0.001, "throttle clamped to 1.0");
    assert!(t.brake >= 0.0, "brake clamped to 0.0");
    assert!((t.clutch - 1.0).abs() < 0.001, "clutch clamped to 1.0");
    Ok(())
}

// ── Normalization: steering ──────────────────────────────────────────────────

#[test]
fn deep_steering_full_lock_left() -> TestResult {
    let adapter = SimHubAdapter::new();
    let data = make_json(0.0, 0.0, 0.0, "N", 0.0, 0.0, 0.0, -450.0, 0.0, 0.0, 0.0, 0.0);
    let t = adapter.normalize(&data)?;
    assert!((t.steering_angle - (-1.0)).abs() < 0.001, "full left lock");
    Ok(())
}

#[test]
fn deep_steering_full_lock_right() -> TestResult {
    let adapter = SimHubAdapter::new();
    let data = make_json(0.0, 0.0, 0.0, "N", 0.0, 0.0, 0.0, 450.0, 0.0, 0.0, 0.0, 0.0);
    let t = adapter.normalize(&data)?;
    assert!((t.steering_angle - 1.0).abs() < 0.001, "full right lock");
    Ok(())
}

#[test]
fn deep_steering_overrange_clamped() -> TestResult {
    let adapter = SimHubAdapter::new();
    // 900° → should clamp to 1.0 (since 900/450 = 2.0 → clamp to 1.0)
    let data = make_json(0.0, 0.0, 0.0, "N", 0.0, 0.0, 0.0, 900.0, 0.0, 0.0, 0.0, 0.0);
    let t = adapter.normalize(&data)?;
    assert!((t.steering_angle - 1.0).abs() < 0.001, "overrange clamped");
    Ok(())
}

#[test]
fn deep_steer_normalized_preferred_over_degrees() -> TestResult {
    let adapter = SimHubAdapter::new();
    // When "Steer" (pre-normalised) is non-zero, it takes priority over SteeringAngle.
    let json = br#"{"SpeedMs":0.0,"Rpms":0.0,"MaxRpms":0.0,"Gear":"N","Throttle":0.0,"Brake":0.0,"Clutch":0.0,"SteeringAngle":200.0,"Steer":0.75,"FuelPercent":0.0,"LateralGForce":0.0,"LongitudinalGForce":0.0,"FFBValue":0.0,"IsRunning":false,"IsInPit":false}"#;
    let t = adapter.normalize(json)?;
    // Should use Steer=0.75 instead of SteeringAngle=200/450≈0.444
    assert!((t.steering_angle - 0.75).abs() < 0.001, "Steer preferred");
    Ok(())
}

// ── Normalization: fuel percent ──────────────────────────────────────────────

#[test]
fn deep_fuel_percent_normalization() -> TestResult {
    let adapter = SimHubAdapter::new();
    let data = make_json(0.0, 0.0, 0.0, "N", 0.0, 0.0, 0.0, 0.0, 42.5, 0.0, 0.0, 0.0);
    let t = adapter.normalize(&data)?;
    assert!((t.fuel_percent - 0.425).abs() < 0.001);
    Ok(())
}

// ── Normalization: FFB scalar clamping ───────────────────────────────────────

#[test]
fn deep_ffb_scalar_passthrough() -> TestResult {
    let adapter = SimHubAdapter::new();
    let data = make_json(0.0, 0.0, 0.0, "N", 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, -0.8);
    let t = adapter.normalize(&data)?;
    assert!((t.ffb_scalar - (-0.8)).abs() < 0.001);
    Ok(())
}

// ── Field aliases ────────────────────────────────────────────────────────────

#[test]
fn deep_rpm_alias() -> TestResult {
    let adapter = SimHubAdapter::new();
    // "Rpm" instead of "Rpms"
    let json = br#"{"SpeedMs":0.0,"Rpm":6500.0,"MaxRpms":0.0,"Gear":"N","Throttle":0.0,"Brake":0.0,"Clutch":0.0,"SteeringAngle":0.0,"FuelPercent":0.0,"LateralGForce":0.0,"LongitudinalGForce":0.0,"FFBValue":0.0,"IsRunning":false,"IsInPit":false}"#;
    let t = adapter.normalize(json)?;
    assert!((t.rpm - 6500.0).abs() < 0.1, "Rpm alias");
    Ok(())
}

#[test]
fn deep_lat_acc_alias() -> TestResult {
    let adapter = SimHubAdapter::new();
    // "LatAcc" alias for "LateralGForce"
    let json = br#"{"SpeedMs":0.0,"Rpms":0.0,"MaxRpms":0.0,"Gear":"N","Throttle":0.0,"Brake":0.0,"Clutch":0.0,"SteeringAngle":0.0,"FuelPercent":0.0,"LatAcc":2.5,"LongitudinalGForce":0.0,"FFBValue":0.0,"IsRunning":false,"IsInPit":false}"#;
    let t = adapter.normalize(json)?;
    assert!((t.lateral_g - 2.5).abs() < 0.001, "LatAcc alias");
    Ok(())
}

#[test]
fn deep_lon_acc_alias() -> TestResult {
    let adapter = SimHubAdapter::new();
    // "LonAcc" alias for "LongitudinalGForce"
    let json = br#"{"SpeedMs":0.0,"Rpms":0.0,"MaxRpms":0.0,"Gear":"N","Throttle":0.0,"Brake":0.0,"Clutch":0.0,"SteeringAngle":0.0,"FuelPercent":0.0,"LateralGForce":0.0,"LonAcc":-1.3,"FFBValue":0.0,"IsRunning":false,"IsInPit":false}"#;
    let t = adapter.normalize(json)?;
    assert!((t.longitudinal_g - (-1.3)).abs() < 0.001, "LonAcc alias");
    Ok(())
}

// ── Malformed / edge-case payloads ───────────────────────────────────────────

#[test]
fn deep_empty_bytes_rejected() -> TestResult {
    let adapter = SimHubAdapter::new();
    assert!(adapter.normalize(&[]).is_err());
    Ok(())
}

#[test]
fn deep_invalid_json_rejected() -> TestResult {
    let adapter = SimHubAdapter::new();
    assert!(adapter.normalize(b"not json at all").is_err());
    Ok(())
}

#[test]
fn deep_non_utf8_rejected() -> TestResult {
    let adapter = SimHubAdapter::new();
    let data: Vec<u8> = vec![0xFF, 0xFE, 0xFD, 0xFC];
    assert!(adapter.normalize(&data).is_err());
    Ok(())
}

#[test]
fn deep_empty_json_object_uses_defaults() -> TestResult {
    let adapter = SimHubAdapter::new();
    let t = adapter.normalize(b"{}")?;
    assert_eq!(t.speed_ms, 0.0);
    assert_eq!(t.rpm, 0.0);
    assert_eq!(t.gear, 0);
    assert_eq!(t.throttle, 0.0);
    Ok(())
}

#[test]
fn deep_missing_optional_fields() -> TestResult {
    let adapter = SimHubAdapter::new();
    // Only mandatory-ish fields present
    let json = br#"{"SpeedMs":10.0,"Rpms":3000.0,"Gear":"2"}"#;
    let t = adapter.normalize(json)?;
    assert!((t.speed_ms - 10.0).abs() < 0.01);
    assert!((t.rpm - 3000.0).abs() < 0.1);
    assert_eq!(t.gear, 2);
    // Defaults for missing fields
    assert_eq!(t.throttle, 0.0);
    assert_eq!(t.brake, 0.0);
    Ok(())
}

// ── Speed edge cases ─────────────────────────────────────────────────────────

#[test]
fn deep_negative_speed_clamped() -> TestResult {
    let adapter = SimHubAdapter::new();
    let data = make_json(-10.0, 0.0, 0.0, "N", 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    let t = adapter.normalize(&data)?;
    assert!(t.speed_ms >= 0.0, "negative speed clamped");
    Ok(())
}

// ── Determinism ──────────────────────────────────────────────────────────────

#[test]
fn deep_deterministic_output() -> TestResult {
    let adapter = SimHubAdapter::new();
    let data = make_json(30.0, 5000.0, 8000.0, "3", 60.0, 10.0, 5.0, -90.0, 50.0, 0.5, -0.2, 0.4);
    let t1 = adapter.normalize(&data)?;
    let t2 = adapter.normalize(&data)?;
    assert_eq!(t1.speed_ms, t2.speed_ms);
    assert_eq!(t1.rpm, t2.rpm);
    assert_eq!(t1.gear, t2.gear);
    assert_eq!(t1.throttle, t2.throttle);
    assert_eq!(t1.steering_angle, t2.steering_angle);
    Ok(())
}
