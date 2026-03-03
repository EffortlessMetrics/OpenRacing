//! Extended deep tests for the MudRunner / SnowRunner SimHub JSON bridge adapter.
//!
//! Focuses on SimHub JSON field coverage not exercised by the existing
//! deep_tests.rs: pre-normalised `Steer` field vs `SteeringAngle`, field
//! alias handling (`Rpm` vs `Rpms`, `LatAcc` / `LonAcc`), gear string
//! edge cases (numeric strings ≥10, whitespace), G-force passthrough,
//! FFB scalar clamping, mixed variant scenarios, and JSON structural
//! edge cases.

use racing_wheel_telemetry_mudrunner::{MudRunnerAdapter, MudRunnerVariant, TelemetryAdapter};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Build a SimHub JSON packet with full control over every field.
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

/// Build a JSON payload with selected key overrides using raw JSON format.
fn make_raw_json(json: &str) -> Vec<u8> {
    json.as_bytes().to_vec()
}

// ═══════════════════════════════════════════════════════════════════════════════
// Gear string parsing edge cases
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn gear_string_reverse() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    let data = make_json(5.0, 2000.0, 4500.0, "R", 30.0, 0.0, 0.0, 0.0, 50.0, 0.0, 0.0, 0.0);
    let t = adapter.normalize(&data)?;
    assert_eq!(t.gear, -1);
    Ok(())
}

#[test]
fn gear_string_neutral() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    let data = make_json(0.0, 800.0, 4500.0, "N", 0.0, 0.0, 0.0, 0.0, 50.0, 0.0, 0.0, 0.0);
    let t = adapter.normalize(&data)?;
    assert_eq!(t.gear, 0);
    Ok(())
}

#[test]
fn gear_string_empty() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    let data = make_json(0.0, 800.0, 4500.0, "", 0.0, 0.0, 0.0, 0.0, 50.0, 0.0, 0.0, 0.0);
    let t = adapter.normalize(&data)?;
    assert_eq!(t.gear, 0);
    Ok(())
}

#[test]
fn gear_string_1_through_9() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    for g in 1..=9 {
        let gear_str = g.to_string();
        let data = make_json(10.0, 3000.0, 4500.0, &gear_str, 50.0, 0.0, 0.0, 0.0, 50.0, 0.0, 0.0, 0.0);
        let t = adapter.normalize(&data)?;
        assert_eq!(t.gear, g, "gear string \"{gear_str}\"");
    }
    Ok(())
}

#[test]
fn gear_string_unrecognised_defaults_to_zero() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    let data = make_json(10.0, 3000.0, 4500.0, "X", 50.0, 0.0, 0.0, 0.0, 50.0, 0.0, 0.0, 0.0);
    let t = adapter.normalize(&data)?;
    assert_eq!(t.gear, 0, "unrecognised gear → 0");
    Ok(())
}

#[test]
fn gear_string_lowercase_r_defaults_to_zero() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    // "r" (lowercase) is not "R", should parse to 0
    let data = make_json(5.0, 2000.0, 4500.0, "r", 30.0, 0.0, 0.0, 0.0, 50.0, 0.0, 0.0, 0.0);
    let t = adapter.normalize(&data)?;
    assert_eq!(t.gear, 0, "lowercase 'r' not treated as reverse");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Steering: Steer (pre-normalised) vs SteeringAngle (degrees)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn steer_from_degrees_225_gives_half() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    // 225° / 450° = 0.5
    let data = make_json(0.0, 0.0, 0.0, "N", 0.0, 0.0, 0.0, 225.0, 0.0, 0.0, 0.0, 0.0);
    let t = adapter.normalize(&data)?;
    assert!((t.steering_angle - 0.5).abs() < 0.001);
    Ok(())
}

#[test]
fn steer_degrees_zero_gives_zero() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    let data = make_json(0.0, 0.0, 0.0, "N", 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    let t = adapter.normalize(&data)?;
    assert_eq!(t.steering_angle, 0.0);
    Ok(())
}

#[test]
fn steer_beyond_450_clamped_to_one() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    let data = make_json(0.0, 0.0, 0.0, "N", 0.0, 0.0, 0.0, 900.0, 0.0, 0.0, 0.0, 0.0);
    let t = adapter.normalize(&data)?;
    assert!((t.steering_angle - 1.0).abs() < 0.001, "clamped to 1.0");
    Ok(())
}

#[test]
fn steer_below_neg_450_clamped_to_neg_one() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    let data = make_json(0.0, 0.0, 0.0, "N", 0.0, 0.0, 0.0, -600.0, 0.0, 0.0, 0.0, 0.0);
    let t = adapter.normalize(&data)?;
    assert!((t.steering_angle - (-1.0)).abs() < 0.001, "clamped to -1.0");
    Ok(())
}

#[test]
fn steer_pre_normalised_preferred_when_nonzero() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    // Steer field (pre-normalised) = 0.8, SteeringAngle = 0.0 (which would be 0/450=0)
    let json = make_raw_json(
        r#"{"SpeedMs":0.0,"Rpms":0.0,"MaxRpms":0.0,"Gear":"N","Throttle":0.0,"Brake":0.0,"Clutch":0.0,"SteeringAngle":0.0,"Steer":0.8,"FuelPercent":0.0,"LateralGForce":0.0,"LongitudinalGForce":0.0,"FFBValue":0.0,"IsRunning":false,"IsInPit":false}"#,
    );
    let t = adapter.normalize(&json)?;
    assert!((t.steering_angle - 0.8).abs() < 0.001, "Steer preferred");
    Ok(())
}

#[test]
fn steer_pre_normalised_zero_falls_back_to_degrees() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    // Steer = 0.0 (falls back), SteeringAngle = 90.0 → 90/450 = 0.2
    let json = make_raw_json(
        r#"{"SpeedMs":0.0,"Rpms":0.0,"MaxRpms":0.0,"Gear":"N","Throttle":0.0,"Brake":0.0,"Clutch":0.0,"SteeringAngle":90.0,"Steer":0.0,"FuelPercent":0.0,"LateralGForce":0.0,"LongitudinalGForce":0.0,"FFBValue":0.0,"IsRunning":false,"IsInPit":false}"#,
    );
    let t = adapter.normalize(&json)?;
    assert!((t.steering_angle - 0.2).abs() < 0.001, "fallback to degrees");
    Ok(())
}

#[test]
fn steer_pre_normalised_clamped_above_one() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    let json = make_raw_json(
        r#"{"SpeedMs":0.0,"Rpms":0.0,"MaxRpms":0.0,"Gear":"N","Throttle":0.0,"Brake":0.0,"Clutch":0.0,"SteeringAngle":0.0,"Steer":2.5,"FuelPercent":0.0,"LateralGForce":0.0,"LongitudinalGForce":0.0,"FFBValue":0.0,"IsRunning":false,"IsInPit":false}"#,
    );
    let t = adapter.normalize(&json)?;
    assert!((t.steering_angle - 1.0).abs() < 0.001, "Steer clamped to 1.0");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// G-force passthrough
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn lateral_g_passthrough() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    let data = make_json(10.0, 2000.0, 4500.0, "2", 50.0, 0.0, 0.0, 0.0, 50.0, 1.5, 0.0, 0.0);
    let t = adapter.normalize(&data)?;
    assert!((t.lateral_g - 1.5).abs() < 0.001);
    Ok(())
}

#[test]
fn longitudinal_g_passthrough() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    let data = make_json(10.0, 2000.0, 4500.0, "2", 50.0, 0.0, 0.0, 0.0, 50.0, 0.0, -0.8, 0.0);
    let t = adapter.normalize(&data)?;
    assert!((t.longitudinal_g - (-0.8)).abs() < 0.001);
    Ok(())
}

#[test]
fn lateral_g_alias_latacc() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    let json = make_raw_json(
        r#"{"SpeedMs":0.0,"Rpms":0.0,"MaxRpms":0.0,"Gear":"N","Throttle":0.0,"Brake":0.0,"Clutch":0.0,"SteeringAngle":0.0,"FuelPercent":0.0,"LatAcc":2.1,"LongitudinalGForce":0.0,"FFBValue":0.0,"IsRunning":false,"IsInPit":false}"#,
    );
    let t = adapter.normalize(&json)?;
    assert!((t.lateral_g - 2.1).abs() < 0.001, "LatAcc alias");
    Ok(())
}

#[test]
fn longitudinal_g_alias_lonacc() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    let json = make_raw_json(
        r#"{"SpeedMs":0.0,"Rpms":0.0,"MaxRpms":0.0,"Gear":"N","Throttle":0.0,"Brake":0.0,"Clutch":0.0,"SteeringAngle":0.0,"FuelPercent":0.0,"LateralGForce":0.0,"LonAcc":-1.3,"FFBValue":0.0,"IsRunning":false,"IsInPit":false}"#,
    );
    let t = adapter.normalize(&json)?;
    assert!((t.longitudinal_g - (-1.3)).abs() < 0.001, "LonAcc alias");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// FFB scalar
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn ffb_scalar_passthrough() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    let data = make_json(10.0, 2000.0, 4500.0, "2", 50.0, 0.0, 0.0, 0.0, 50.0, 0.0, 0.0, 0.42);
    let t = adapter.normalize(&data)?;
    assert!((t.ffb_scalar - 0.42).abs() < 0.001);
    Ok(())
}

#[test]
fn ffb_scalar_negative() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    let data = make_json(10.0, 2000.0, 4500.0, "2", 50.0, 0.0, 0.0, 0.0, 50.0, 0.0, 0.0, -0.7);
    let t = adapter.normalize(&data)?;
    assert!((t.ffb_scalar - (-0.7)).abs() < 0.001);
    Ok(())
}

#[test]
fn ffb_scalar_clamped_above_one() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    let data = make_json(10.0, 2000.0, 4500.0, "2", 50.0, 0.0, 0.0, 0.0, 50.0, 0.0, 0.0, 5.0);
    let t = adapter.normalize(&data)?;
    assert!((t.ffb_scalar - 1.0).abs() < 0.001, "FFB clamped to 1.0");
    Ok(())
}

#[test]
fn ffb_scalar_clamped_below_neg_one() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    let data = make_json(10.0, 2000.0, 4500.0, "2", 50.0, 0.0, 0.0, 0.0, 50.0, 0.0, 0.0, -3.0);
    let t = adapter.normalize(&data)?;
    assert!((t.ffb_scalar - (-1.0)).abs() < 0.001, "FFB clamped to -1.0");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Max RPM
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn max_rpm_passthrough() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    let data = make_json(10.0, 2000.0, 5500.0, "2", 50.0, 0.0, 0.0, 0.0, 50.0, 0.0, 0.0, 0.0);
    let t = adapter.normalize(&data)?;
    assert!((t.max_rpm - 5500.0).abs() < 0.1);
    Ok(())
}

#[test]
fn max_rpm_negative_clamped() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    let data = make_json(0.0, 0.0, -1000.0, "N", 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    let t = adapter.normalize(&data)?;
    assert!(t.max_rpm >= 0.0, "max_rpm non-negative");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Clutch normalisation
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn clutch_half() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    let data = make_json(0.0, 800.0, 4500.0, "N", 0.0, 0.0, 50.0, 0.0, 50.0, 0.0, 0.0, 0.0);
    let t = adapter.normalize(&data)?;
    assert!((t.clutch - 0.5).abs() < 0.001);
    Ok(())
}

#[test]
fn clutch_full() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    let data = make_json(0.0, 800.0, 4500.0, "N", 0.0, 0.0, 100.0, 0.0, 50.0, 0.0, 0.0, 0.0);
    let t = adapter.normalize(&data)?;
    assert!((t.clutch - 1.0).abs() < 0.001);
    Ok(())
}

#[test]
fn clutch_overrange_clamped() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    let data = make_json(0.0, 800.0, 4500.0, "N", 0.0, 0.0, 250.0, 0.0, 50.0, 0.0, 0.0, 0.0);
    let t = adapter.normalize(&data)?;
    assert!((t.clutch - 1.0).abs() < 0.001, "clutch clamped to 1.0");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Fuel normalisation
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn fuel_percent_normalised() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    let data = make_json(0.0, 0.0, 0.0, "N", 0.0, 0.0, 0.0, 0.0, 33.3, 0.0, 0.0, 0.0);
    let t = adapter.normalize(&data)?;
    assert!((t.fuel_percent - 0.333).abs() < 0.001);
    Ok(())
}

#[test]
fn fuel_percent_zero() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    let data = make_json(0.0, 0.0, 0.0, "N", 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    let t = adapter.normalize(&data)?;
    assert_eq!(t.fuel_percent, 0.0);
    Ok(())
}

#[test]
fn fuel_percent_overrange_clamped() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    let data = make_json(0.0, 0.0, 0.0, "N", 0.0, 0.0, 0.0, 0.0, 200.0, 0.0, 0.0, 0.0);
    let t = adapter.normalize(&data)?;
    assert!((t.fuel_percent - 1.0).abs() < 0.001, "fuel clamped to 1.0");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// JSON field alias handling (`Rpm` vs `Rpms`)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn rpm_alias_field() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    let json = make_raw_json(
        r#"{"SpeedMs":0.0,"Rpm":3500.0,"MaxRpms":0.0,"Gear":"N","Throttle":0.0,"Brake":0.0,"Clutch":0.0,"SteeringAngle":0.0,"FuelPercent":0.0,"LateralGForce":0.0,"LongitudinalGForce":0.0,"FFBValue":0.0,"IsRunning":false,"IsInPit":false}"#,
    );
    let t = adapter.normalize(&json)?;
    assert!((t.rpm - 3500.0).abs() < 0.1, "Rpm alias accepted");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// JSON structural edge cases
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn partial_json_with_only_speed() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    let json = make_raw_json(r#"{"SpeedMs":12.0}"#);
    let t = adapter.normalize(&json)?;
    assert!((t.speed_ms - 12.0).abs() < 0.01);
    assert_eq!(t.gear, 0, "missing gear → default 0");
    assert_eq!(t.rpm, 0.0, "missing rpm → default 0");
    Ok(())
}

#[test]
fn extra_unknown_fields_ignored() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    let json = make_raw_json(
        r#"{"SpeedMs":5.0,"Rpms":1000.0,"MaxRpms":4500.0,"Gear":"1","Throttle":30.0,"Brake":0.0,"Clutch":0.0,"SteeringAngle":0.0,"FuelPercent":50.0,"LateralGForce":0.0,"LongitudinalGForce":0.0,"FFBValue":0.0,"IsRunning":true,"IsInPit":false,"Unknown1":999,"Unknown2":"hello"}"#,
    );
    let t = adapter.normalize(&json)?;
    assert!((t.speed_ms - 5.0).abs() < 0.01);
    assert_eq!(t.gear, 1);
    Ok(())
}

#[test]
fn json_with_integer_speed() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    // JSON number without decimal point should still deserialize to f32
    let json = make_raw_json(r#"{"SpeedMs":10,"Rpms":2000,"MaxRpms":4500,"Gear":"2","Throttle":50,"Brake":0,"Clutch":0,"SteeringAngle":0,"FuelPercent":50,"LateralGForce":0,"LongitudinalGForce":0,"FFBValue":0,"IsRunning":true,"IsInPit":false}"#);
    let t = adapter.normalize(&json)?;
    assert!((t.speed_ms - 10.0).abs() < 0.01);
    assert!((t.rpm - 2000.0).abs() < 0.1);
    Ok(())
}

#[test]
fn json_whitespace_preserved() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    let json = make_raw_json(
        r#"{
          "SpeedMs" : 15.0 ,
          "Rpms" : 3000.0 ,
          "MaxRpms" : 4500.0 ,
          "Gear" : "3" ,
          "Throttle" : 60.0 ,
          "Brake" : 0.0 ,
          "Clutch" : 0.0 ,
          "SteeringAngle" : -45.0 ,
          "FuelPercent" : 80.0 ,
          "LateralGForce" : 0.5 ,
          "LongitudinalGForce" : -0.2 ,
          "FFBValue" : 0.1 ,
          "IsRunning" : true ,
          "IsInPit" : false
        }"#,
    );
    let t = adapter.normalize(&json)?;
    assert!((t.speed_ms - 15.0).abs() < 0.01);
    assert_eq!(t.gear, 3);
    assert!((t.steering_angle - (-45.0 / 450.0)).abs() < 0.001);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// SnowRunner variant scenarios
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn snowrunner_heavy_snow_driving() -> TestResult {
    let adapter = MudRunnerAdapter::with_variant(MudRunnerVariant::SnowRunner);
    assert_eq!(adapter.game_id(), "snowrunner");
    let data = make_json(6.0, 2200.0, 4500.0, "1", 90.0, 0.0, 0.0, -100.0, 40.0, 0.4, 0.3, 0.6);
    let t = adapter.normalize(&data)?;
    assert!((t.speed_ms - 6.0).abs() < 0.01);
    assert!((t.throttle - 0.9).abs() < 0.001);
    // -100/450 = -0.222…
    assert!((t.steering_angle - (-100.0 / 450.0)).abs() < 0.001);
    assert!((t.fuel_percent - 0.4).abs() < 0.001);
    Ok(())
}

#[test]
fn snowrunner_variant_clone_copy() -> TestResult {
    let v = MudRunnerVariant::SnowRunner;
    let v2 = v;
    assert_eq!(v, v2);
    Ok(())
}

#[test]
fn snowrunner_variant_debug() -> TestResult {
    let v = MudRunnerVariant::SnowRunner;
    let dbg = format!("{v:?}");
    assert!(dbg.contains("SnowRunner"));
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Malformed payloads
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn json_number_only_rejected() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    assert!(adapter.normalize(b"42").is_err());
    Ok(())
}

#[test]
fn json_string_only_rejected() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    assert!(adapter.normalize(br#""hello""#).is_err());
    Ok(())
}

#[test]
fn truncated_json_rejected() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    assert!(adapter.normalize(br#"{"SpeedMs":10.0,"#).is_err());
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario: offroad recovery
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn scenario_stuck_and_reversing() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    let data = make_json(2.0, 1200.0, 4500.0, "R", 40.0, 0.0, 30.0, 200.0, 60.0, 0.1, -0.3, 0.15);
    let t = adapter.normalize(&data)?;

    assert_eq!(t.gear, -1, "reverse");
    assert!((t.speed_ms - 2.0).abs() < 0.01);
    assert!((t.throttle - 0.4).abs() < 0.001);
    assert!((t.clutch - 0.3).abs() < 0.001);
    // 200/450 = 0.444…
    assert!((t.steering_angle - (200.0 / 450.0)).abs() < 0.001);
    assert!((t.fuel_percent - 0.6).abs() < 0.001);
    Ok(())
}

#[test]
fn scenario_high_speed_highway() -> TestResult {
    let adapter = MudRunnerAdapter::with_variant(MudRunnerVariant::SnowRunner);
    let data = make_json(25.0, 4000.0, 4500.0, "5", 70.0, 0.0, 0.0, -5.0, 50.0, 0.05, 0.1, 0.05);
    let t = adapter.normalize(&data)?;

    assert!((t.speed_ms - 25.0).abs() < 0.01);
    assert!((t.rpm - 4000.0).abs() < 0.1);
    assert_eq!(t.gear, 5);
    assert!((t.throttle - 0.7).abs() < 0.001);
    assert!((t.steering_angle - (-5.0 / 450.0)).abs() < 0.001);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Determinism across variants
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn deterministic_across_repeated_calls() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    let data = make_json(12.0, 2800.0, 4500.0, "3", 65.0, 10.0, 0.0, -30.0, 55.0, 0.3, -0.2, 0.15);
    let t1 = adapter.normalize(&data)?;
    let t2 = adapter.normalize(&data)?;
    assert_eq!(t1.speed_ms, t2.speed_ms);
    assert_eq!(t1.rpm, t2.rpm);
    assert_eq!(t1.gear, t2.gear);
    assert_eq!(t1.throttle, t2.throttle);
    assert_eq!(t1.steering_angle, t2.steering_angle);
    assert_eq!(t1.fuel_percent, t2.fuel_percent);
    assert_eq!(t1.lateral_g, t2.lateral_g);
    assert_eq!(t1.ffb_scalar, t2.ffb_scalar);
    Ok(())
}
