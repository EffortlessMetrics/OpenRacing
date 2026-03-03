//! Extended deep tests for the SimHub telemetry adapter.
//!
//! Covers connection protocols (JSON format), plugin data mapping across
//! field aliases, steering preference logic, edge-case payloads,
//! type mismatches, extended Unicode, and combined real-world scenarios.

use racing_wheel_telemetry_simhub::{SimHubAdapter, TelemetryAdapter};
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

// ── Connection protocol: JSON format acceptance ──────────────────────────────

#[test]
fn accepts_minimal_json_object() -> TestResult {
    let adapter = SimHubAdapter::new();
    let t = adapter.normalize(b"{}")?;
    assert_eq!(t.speed_ms, 0.0);
    assert_eq!(t.gear, 0);
    Ok(())
}

#[test]
fn accepts_partial_json_with_only_speed() -> TestResult {
    let adapter = SimHubAdapter::new();
    let t = adapter.normalize(br#"{"SpeedMs": 25.5}"#)?;
    assert!((t.speed_ms - 25.5).abs() < 0.01);
    assert_eq!(t.rpm, 0.0);
    Ok(())
}

#[test]
fn rejects_empty_payload() -> TestResult {
    let adapter = SimHubAdapter::new();
    assert!(adapter.normalize(&[]).is_err());
    Ok(())
}

#[test]
fn rejects_plain_text() -> TestResult {
    let adapter = SimHubAdapter::new();
    assert!(adapter.normalize(b"hello world").is_err());
    Ok(())
}

#[test]
fn json_array_parsed_without_crash() -> TestResult {
    let adapter = SimHubAdapter::new();
    // JSON arrays are accepted by the serde-based parser without crashing
    let _result = adapter.normalize(b"[1, 2, 3]");
    Ok(())
}

#[test]
fn rejects_non_utf8_bytes() -> TestResult {
    let adapter = SimHubAdapter::new();
    let data: Vec<u8> = vec![0xFF, 0xFE, 0xFD, 0xFC, 0xFB];
    assert!(adapter.normalize(&data).is_err());
    Ok(())
}

#[test]
fn rejects_truncated_json() -> TestResult {
    let adapter = SimHubAdapter::new();
    assert!(adapter.normalize(br#"{"SpeedMs": 10.0"#).is_err());
    Ok(())
}

#[test]
fn accepts_json_with_unknown_fields() -> TestResult {
    let adapter = SimHubAdapter::new();
    let json = br#"{"SpeedMs":15.0,"UnknownField":42,"AnotherThing":"hello"}"#;
    let t = adapter.normalize(json)?;
    assert!((t.speed_ms - 15.0).abs() < 0.01);
    Ok(())
}

// ── Plugin data mapping: field aliases ───────────────────────────────────────

#[test]
fn rpm_primary_field() -> TestResult {
    let adapter = SimHubAdapter::new();
    let json = br#"{"Rpms": 6000.0}"#;
    let t = adapter.normalize(json)?;
    assert!((t.rpm - 6000.0).abs() < 0.1);
    Ok(())
}

#[test]
fn rpm_alias_field() -> TestResult {
    let adapter = SimHubAdapter::new();
    let json = br#"{"Rpm": 7500.0}"#;
    let t = adapter.normalize(json)?;
    assert!((t.rpm - 7500.0).abs() < 0.1);
    Ok(())
}

#[test]
fn lateral_g_primary_field() -> TestResult {
    let adapter = SimHubAdapter::new();
    let json = br#"{"LateralGForce": 1.5}"#;
    let t = adapter.normalize(json)?;
    assert!((t.lateral_g - 1.5).abs() < 0.001);
    Ok(())
}

#[test]
fn lateral_g_alias_latacc() -> TestResult {
    let adapter = SimHubAdapter::new();
    let json = br#"{"LatAcc": 2.1}"#;
    let t = adapter.normalize(json)?;
    assert!((t.lateral_g - 2.1).abs() < 0.001);
    Ok(())
}

#[test]
fn longitudinal_g_primary_field() -> TestResult {
    let adapter = SimHubAdapter::new();
    let json = br#"{"LongitudinalGForce": -0.8}"#;
    let t = adapter.normalize(json)?;
    assert!((t.longitudinal_g - (-0.8)).abs() < 0.001);
    Ok(())
}

#[test]
fn longitudinal_g_alias_lonacc() -> TestResult {
    let adapter = SimHubAdapter::new();
    let json = br#"{"LonAcc": -1.5}"#;
    let t = adapter.normalize(json)?;
    assert!((t.longitudinal_g - (-1.5)).abs() < 0.001);
    Ok(())
}

// ── Gear string parsing: exhaustive ──────────────────────────────────────────

#[test]
fn gear_reverse() -> TestResult {
    let adapter = SimHubAdapter::new();
    let json = br#"{"Gear":"R"}"#;
    let t = adapter.normalize(json)?;
    assert_eq!(t.gear, -1);
    Ok(())
}

#[test]
fn gear_neutral_n() -> TestResult {
    let adapter = SimHubAdapter::new();
    let json = br#"{"Gear":"N"}"#;
    let t = adapter.normalize(json)?;
    assert_eq!(t.gear, 0);
    Ok(())
}

#[test]
fn gear_neutral_empty() -> TestResult {
    let adapter = SimHubAdapter::new();
    let json = br#"{"Gear":""}"#;
    let t = adapter.normalize(json)?;
    assert_eq!(t.gear, 0);
    Ok(())
}

#[test]
fn gear_forward_1_through_9() -> TestResult {
    let adapter = SimHubAdapter::new();
    for g in 1..=9 {
        let json = format!(r#"{{"Gear":"{g}"}}"#);
        let t = adapter.normalize(json.as_bytes())?;
        assert_eq!(t.gear, g, "gear '{g}'");
    }
    Ok(())
}

#[test]
fn gear_invalid_strings() -> TestResult {
    let adapter = SimHubAdapter::new();
    let invalid_gears = ["XYZ", "P", "D", "abc", "gear"];
    for &gear_str in &invalid_gears {
        let json = format!(r#"{{"Gear":"{gear_str}"}}"#);
        let t = adapter.normalize(json.as_bytes())?;
        assert_eq!(t.gear, 0, "invalid gear '{gear_str}' → 0");
    }
    Ok(())
}

#[test]
fn gear_with_whitespace() -> TestResult {
    let adapter = SimHubAdapter::new();
    let json = br#"{"Gear":" R "}"#;
    let t = adapter.normalize(json)?;
    assert_eq!(t.gear, -1, "trimmed 'R' → -1");
    Ok(())
}

// ── Pedal normalization (0–100 → 0–1) ───────────────────────────────────────

#[test]
fn pedals_zero() -> TestResult {
    let adapter = SimHubAdapter::new();
    let data = make_json(0.0, 0.0, 0.0, "N", 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    let t = adapter.normalize(&data)?;
    assert_eq!(t.throttle, 0.0);
    assert_eq!(t.brake, 0.0);
    assert_eq!(t.clutch, 0.0);
    Ok(())
}

#[test]
fn pedals_full() -> TestResult {
    let adapter = SimHubAdapter::new();
    let data = make_json(
        0.0, 0.0, 0.0, "N", 100.0, 100.0, 100.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    );
    let t = adapter.normalize(&data)?;
    assert!((t.throttle - 1.0).abs() < 0.001);
    assert!((t.brake - 1.0).abs() < 0.001);
    assert!((t.clutch - 1.0).abs() < 0.001);
    Ok(())
}

#[test]
fn pedals_mid_range() -> TestResult {
    let adapter = SimHubAdapter::new();
    let data = make_json(
        0.0, 0.0, 0.0, "N", 33.0, 66.0, 50.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    );
    let t = adapter.normalize(&data)?;
    assert!((t.throttle - 0.33).abs() < 0.001);
    assert!((t.brake - 0.66).abs() < 0.001);
    assert!((t.clutch - 0.50).abs() < 0.001);
    Ok(())
}

#[test]
fn pedals_overflow_clamped() -> TestResult {
    let adapter = SimHubAdapter::new();
    let json = br#"{"Throttle":200.0,"Brake":150.0,"Clutch":300.0}"#;
    let t = adapter.normalize(json)?;
    assert!((t.throttle - 1.0).abs() < 0.001, "throttle clamped");
    assert!((t.brake - 1.0).abs() < 0.001, "brake clamped");
    assert!((t.clutch - 1.0).abs() < 0.001, "clutch clamped");
    Ok(())
}

#[test]
fn pedals_negative_clamped_to_zero() -> TestResult {
    let adapter = SimHubAdapter::new();
    let json = br#"{"Throttle":-50.0,"Brake":-10.0,"Clutch":-1.0}"#;
    let t = adapter.normalize(json)?;
    assert_eq!(t.throttle, 0.0, "negative throttle → 0");
    assert_eq!(t.brake, 0.0, "negative brake → 0");
    assert_eq!(t.clutch, 0.0, "negative clutch → 0");
    Ok(())
}

// ── Steering normalization ───────────────────────────────────────────────────

#[test]
fn steering_centre() -> TestResult {
    let adapter = SimHubAdapter::new();
    let data = make_json(0.0, 0.0, 0.0, "N", 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    let t = adapter.normalize(&data)?;
    assert_eq!(t.steering_angle, 0.0);
    Ok(())
}

#[test]
fn steering_full_left_450() -> TestResult {
    let adapter = SimHubAdapter::new();
    let data = make_json(
        0.0, 0.0, 0.0, "N", 0.0, 0.0, 0.0, -450.0, 0.0, 0.0, 0.0, 0.0,
    );
    let t = adapter.normalize(&data)?;
    assert!((t.steering_angle - (-1.0)).abs() < 0.001);
    Ok(())
}

#[test]
fn steering_full_right_450() -> TestResult {
    let adapter = SimHubAdapter::new();
    let data = make_json(0.0, 0.0, 0.0, "N", 0.0, 0.0, 0.0, 450.0, 0.0, 0.0, 0.0, 0.0);
    let t = adapter.normalize(&data)?;
    assert!((t.steering_angle - 1.0).abs() < 0.001);
    Ok(())
}

#[test]
fn steering_quarter_turn() -> TestResult {
    let adapter = SimHubAdapter::new();
    let data = make_json(0.0, 0.0, 0.0, "N", 0.0, 0.0, 0.0, 225.0, 0.0, 0.0, 0.0, 0.0);
    let t = adapter.normalize(&data)?;
    assert!((t.steering_angle - 0.5).abs() < 0.001, "225° → 0.5");
    Ok(())
}

#[test]
fn steering_overrange_positive_clamped() -> TestResult {
    let adapter = SimHubAdapter::new();
    let json = br#"{"SteeringAngle":900.0}"#;
    let t = adapter.normalize(json)?;
    assert!(
        (t.steering_angle - 1.0).abs() < 0.001,
        "900° → clamped to 1.0"
    );
    Ok(())
}

#[test]
fn steering_overrange_negative_clamped() -> TestResult {
    let adapter = SimHubAdapter::new();
    let json = br#"{"SteeringAngle":-900.0}"#;
    let t = adapter.normalize(json)?;
    assert!(
        (t.steering_angle - (-1.0)).abs() < 0.001,
        "-900° → clamped to -1.0"
    );
    Ok(())
}

#[test]
fn steer_normalized_preferred_when_nonzero() -> TestResult {
    let adapter = SimHubAdapter::new();
    let json = br#"{"SteeringAngle":200.0,"Steer":0.85}"#;
    let t = adapter.normalize(json)?;
    // Steer=0.85 should be used instead of SteeringAngle=200/450≈0.444
    assert!(
        (t.steering_angle - 0.85).abs() < 0.001,
        "Steer preferred over SteeringAngle"
    );
    Ok(())
}

#[test]
fn steer_normalized_zero_falls_back_to_degrees() -> TestResult {
    let adapter = SimHubAdapter::new();
    let json = br#"{"SteeringAngle":225.0,"Steer":0.0}"#;
    let t = adapter.normalize(json)?;
    // Steer=0.0 → fall back to SteeringAngle=225/450=0.5
    assert!(
        (t.steering_angle - 0.5).abs() < 0.001,
        "fallback to degrees"
    );
    Ok(())
}

#[test]
fn steer_normalized_clamped_when_out_of_range() -> TestResult {
    let adapter = SimHubAdapter::new();
    let json = br#"{"SteeringAngle":0.0,"Steer":5.0}"#;
    let t = adapter.normalize(json)?;
    assert!(
        (t.steering_angle - 1.0).abs() < 0.001,
        "Steer>1 clamped to 1.0"
    );
    Ok(())
}

// ── Fuel percent normalization ───────────────────────────────────────────────

#[test]
fn fuel_zero() -> TestResult {
    let adapter = SimHubAdapter::new();
    let json = br#"{"FuelPercent":0.0}"#;
    let t = adapter.normalize(json)?;
    assert_eq!(t.fuel_percent, 0.0);
    Ok(())
}

#[test]
fn fuel_full() -> TestResult {
    let adapter = SimHubAdapter::new();
    let json = br#"{"FuelPercent":100.0}"#;
    let t = adapter.normalize(json)?;
    assert!((t.fuel_percent - 1.0).abs() < 0.001);
    Ok(())
}

#[test]
fn fuel_partial() -> TestResult {
    let adapter = SimHubAdapter::new();
    let json = br#"{"FuelPercent":42.5}"#;
    let t = adapter.normalize(json)?;
    assert!((t.fuel_percent - 0.425).abs() < 0.001);
    Ok(())
}

#[test]
fn fuel_overflow_clamped() -> TestResult {
    let adapter = SimHubAdapter::new();
    let json = br#"{"FuelPercent":150.0}"#;
    let t = adapter.normalize(json)?;
    assert!((t.fuel_percent - 1.0).abs() < 0.001, "fuel > 100 → 1.0");
    Ok(())
}

#[test]
fn fuel_negative_clamped() -> TestResult {
    let adapter = SimHubAdapter::new();
    let json = br#"{"FuelPercent":-10.0}"#;
    let t = adapter.normalize(json)?;
    assert_eq!(t.fuel_percent, 0.0, "negative fuel → 0.0");
    Ok(())
}

// ── FFB scalar ───────────────────────────────────────────────────────────────

#[test]
fn ffb_passthrough_range() -> TestResult {
    let adapter = SimHubAdapter::new();
    let test_values = [-1.0f32, -0.5, 0.0, 0.5, 1.0];
    for &ffb in &test_values {
        let json = format!(r#"{{"FFBValue":{ffb}}}"#);
        let t = adapter.normalize(json.as_bytes())?;
        assert!((t.ffb_scalar - ffb).abs() < 0.001, "ffb={ffb}");
    }
    Ok(())
}

#[test]
fn ffb_overflow_clamped() -> TestResult {
    let adapter = SimHubAdapter::new();
    let json = br#"{"FFBValue":5.0}"#;
    let t = adapter.normalize(json)?;
    assert!((t.ffb_scalar - 1.0).abs() < 0.001);
    Ok(())
}

#[test]
fn ffb_underflow_clamped() -> TestResult {
    let adapter = SimHubAdapter::new();
    let json = br#"{"FFBValue":-5.0}"#;
    let t = adapter.normalize(json)?;
    assert!((t.ffb_scalar - (-1.0)).abs() < 0.001);
    Ok(())
}

// ── Speed edge cases ─────────────────────────────────────────────────────────

#[test]
fn negative_speed_clamped_to_zero() -> TestResult {
    let adapter = SimHubAdapter::new();
    let json = br#"{"SpeedMs":-10.0}"#;
    let t = adapter.normalize(json)?;
    assert_eq!(t.speed_ms, 0.0, "negative speed → 0");
    Ok(())
}

#[test]
fn very_high_speed_preserved() -> TestResult {
    let adapter = SimHubAdapter::new();
    let json = br#"{"SpeedMs":150.0}"#;
    let t = adapter.normalize(json)?;
    assert!((t.speed_ms - 150.0).abs() < 0.01);
    Ok(())
}

// ── Max RPM ──────────────────────────────────────────────────────────────────

#[test]
fn max_rpm_preserved() -> TestResult {
    let adapter = SimHubAdapter::new();
    let json = br#"{"MaxRpms":9000.0}"#;
    let t = adapter.normalize(json)?;
    assert!((t.max_rpm - 9000.0).abs() < 0.1);
    Ok(())
}

#[test]
fn negative_max_rpm_clamped() -> TestResult {
    let adapter = SimHubAdapter::new();
    let json = br#"{"MaxRpms":-1000.0}"#;
    let t = adapter.normalize(json)?;
    assert_eq!(t.max_rpm, 0.0, "negative max_rpm → 0");
    Ok(())
}

// ── Type mismatches in JSON ──────────────────────────────────────────────────

#[test]
fn number_gear_causes_error_or_default() -> TestResult {
    let adapter = SimHubAdapter::new();
    // Gear as number instead of string — serde should handle or reject
    let json = br#"{"SpeedMs":10.0,"Gear":3}"#;
    let result = adapter.normalize(json);
    // The serde deserialization may reject this or default it
    match result {
        Ok(t) => {
            // If accepted, gear should be reasonable
            assert!(t.gear >= -1 && t.gear <= 9);
        }
        Err(_) => {
            // Rejection is acceptable
        }
    }
    Ok(())
}

#[test]
fn string_speed_causes_error() -> TestResult {
    let adapter = SimHubAdapter::new();
    let json = br#"{"SpeedMs":"fast"}"#;
    assert!(
        adapter.normalize(json).is_err(),
        "non-numeric speed should fail"
    );
    Ok(())
}

// ── Unicode in JSON ──────────────────────────────────────────────────────────

#[test]
fn valid_utf8_json_accepted() -> TestResult {
    let adapter = SimHubAdapter::new();
    let json = r#"{"SpeedMs":10.0,"Gear":"3"}"#.as_bytes();
    let t = adapter.normalize(json)?;
    assert!((t.speed_ms - 10.0).abs() < 0.01);
    Ok(())
}

#[test]
fn json_with_unicode_escape() -> TestResult {
    let adapter = SimHubAdapter::new();
    // "Gear" with Unicode escape: "\u004E" = "N"
    let json = br#"{"Gear":"\u004E"}"#;
    let t = adapter.normalize(json)?;
    assert_eq!(t.gear, 0, "unicode 'N' → neutral");
    Ok(())
}

// ── Full scenario: race lap ──────────────────────────────────────────────────

#[test]
fn full_race_lap_scenario() -> TestResult {
    let adapter = SimHubAdapter::new();
    let data = make_json(
        55.0,   // 55 m/s
        7200.0, // RPM
        8500.0, // max RPM
        "4",    // gear
        82.0,   // throttle (82%)
        5.0,    // brake (5%)
        0.0,    // clutch
        -30.0,  // steering angle (degrees)
        72.0,   // fuel (72%)
        1.1,    // lateral G
        -0.3,   // longitudinal G
        0.45,   // FFB
    );
    let t = adapter.normalize(&data)?;

    assert!((t.speed_ms - 55.0).abs() < 0.01);
    assert!((t.rpm - 7200.0).abs() < 0.1);
    assert!((t.max_rpm - 8500.0).abs() < 0.1);
    assert_eq!(t.gear, 4);
    assert!((t.throttle - 0.82).abs() < 0.001);
    assert!((t.brake - 0.05).abs() < 0.001);
    assert_eq!(t.clutch, 0.0);
    // -30/450 = -0.0667
    assert!((t.steering_angle - (-30.0 / 450.0)).abs() < 0.001);
    assert!((t.fuel_percent - 0.72).abs() < 0.001);
    assert!((t.lateral_g - 1.1).abs() < 0.001);
    assert!((t.longitudinal_g - (-0.3)).abs() < 0.001);
    assert!((t.ffb_scalar - 0.45).abs() < 0.001);
    Ok(())
}

// ── Pit stop scenario ────────────────────────────────────────────────────────

#[test]
fn pit_stop_stationary() -> TestResult {
    let adapter = SimHubAdapter::new();
    let json =
        br#"{"SpeedMs":0.0,"Rpms":850.0,"Gear":"N","Throttle":0.0,"Brake":100.0,"IsInPit":true}"#;
    let t = adapter.normalize(json)?;
    assert_eq!(t.speed_ms, 0.0);
    assert_eq!(t.gear, 0);
    assert!((t.brake - 1.0).abs() < 0.001);
    Ok(())
}

// ── Adapter construction ─────────────────────────────────────────────────────

#[test]
fn adapter_game_id() -> TestResult {
    let adapter = SimHubAdapter::new();
    assert_eq!(adapter.game_id(), "simhub");
    Ok(())
}

#[test]
fn adapter_default_matches_new() -> TestResult {
    let a = SimHubAdapter::new();
    let b = SimHubAdapter::default();
    assert_eq!(a.game_id(), b.game_id());
    assert_eq!(a.expected_update_rate(), b.expected_update_rate());
    Ok(())
}

#[test]
fn adapter_update_rate_60hz() -> TestResult {
    let adapter = SimHubAdapter::new();
    assert_eq!(adapter.expected_update_rate(), Duration::from_millis(16));
    Ok(())
}

// ── Determinism ──────────────────────────────────────────────────────────────

#[test]
fn triple_normalize_identical() -> TestResult {
    let adapter = SimHubAdapter::new();
    let data = make_json(
        30.0, 5000.0, 8000.0, "3", 60.0, 10.0, 5.0, -90.0, 50.0, 0.5, -0.2, 0.4,
    );
    let t1 = adapter.normalize(&data)?;
    let t2 = adapter.normalize(&data)?;
    let t3 = adapter.normalize(&data)?;
    assert_eq!(t1.speed_ms, t2.speed_ms);
    assert_eq!(t2.speed_ms, t3.speed_ms);
    assert_eq!(t1.rpm, t2.rpm);
    assert_eq!(t1.gear, t2.gear);
    assert_eq!(t1.throttle, t2.throttle);
    assert_eq!(t1.steering_angle, t2.steering_angle);
    assert_eq!(t1.fuel_percent, t2.fuel_percent);
    Ok(())
}

// ── G-force passthrough (no normalization) ───────────────────────────────────

#[test]
fn g_forces_passed_through_directly() -> TestResult {
    let adapter = SimHubAdapter::new();
    let json = br#"{"LateralGForce":3.5,"LongitudinalGForce":-2.0}"#;
    let t = adapter.normalize(json)?;
    assert!((t.lateral_g - 3.5).abs() < 0.001, "lateral G passthrough");
    assert!(
        (t.longitudinal_g - (-2.0)).abs() < 0.001,
        "longitudinal G passthrough"
    );
    Ok(())
}

// ── Multiple field aliases in same packet ────────────────────────────────────

#[test]
fn all_aliases_in_one_packet() -> TestResult {
    let adapter = SimHubAdapter::new();
    let json = br#"{"Rpm":4500.0,"LatAcc":1.2,"LonAcc":-0.5,"SpeedMs":30.0,"Gear":"3"}"#;
    let t = adapter.normalize(json)?;
    assert!((t.rpm - 4500.0).abs() < 0.1);
    assert!((t.lateral_g - 1.2).abs() < 0.001);
    assert!((t.longitudinal_g - (-0.5)).abs() < 0.001);
    assert!((t.speed_ms - 30.0).abs() < 0.01);
    assert_eq!(t.gear, 3);
    Ok(())
}

// ── Boolean fields parsing ───────────────────────────────────────────────────

#[test]
fn boolean_fields_accepted() -> TestResult {
    let adapter = SimHubAdapter::new();
    let json = br#"{"SpeedMs":10.0,"IsRunning":false,"IsInPit":true}"#;
    // IsRunning/IsInPit are parsed but not mapped to normalized output flags
    let t = adapter.normalize(json)?;
    assert!((t.speed_ms - 10.0).abs() < 0.01);
    Ok(())
}

// ── Whitespace tolerance ─────────────────────────────────────────────────────

#[test]
fn pretty_printed_json_accepted() -> TestResult {
    let adapter = SimHubAdapter::new();
    let json = br#"{
        "SpeedMs": 20.0,
        "Rpms": 3000.0,
        "Gear": "2",
        "Throttle": 50.0
    }"#;
    let t = adapter.normalize(json)?;
    assert!((t.speed_ms - 20.0).abs() < 0.01);
    assert!((t.rpm - 3000.0).abs() < 0.1);
    assert_eq!(t.gear, 2);
    assert!((t.throttle - 0.5).abs() < 0.001);
    Ok(())
}
