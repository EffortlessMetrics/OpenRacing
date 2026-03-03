//! Property-based tests for the SimHub telemetry adapter.
//!
//! Exercises JSON data format round-trips, property mapping, and ensures
//! the adapter never panics on arbitrary input.

#![allow(clippy::redundant_closure)]

use proptest::prelude::*;
use racing_wheel_telemetry_simhub::{NormalizedTelemetry, SimHubAdapter, TelemetryAdapter};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a SimHub JSON telemetry packet.
#[allow(clippy::too_many_arguments)]
fn make_json(
    speed: f64,
    rpms: f64,
    max_rpms: f64,
    gear: &str,
    throttle: f64,
    brake: f64,
    clutch: f64,
    steering: f64,
    fuel: f64,
    lat_g: f64,
    lon_g: f64,
    ffb: f64,
) -> String {
    format!(
        r#"{{"SpeedMs":{speed},"Rpms":{rpms},"MaxRpms":{max_rpms},"Gear":"{gear}","Throttle":{throttle},"Brake":{brake},"Clutch":{clutch},"SteeringAngle":{steering},"FuelPercent":{fuel},"LateralGForce":{lat_g},"LongitudinalGForce":{lon_g},"FFBValue":{ffb},"IsRunning":true,"IsInPit":false}}"#
    )
}

/// Proptest strategy for a gear string.
fn gear_strategy() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("R".to_string()),
        Just("N".to_string()),
        (1..=6i32).prop_map(|g| g.to_string()),
    ]
}

// ---------------------------------------------------------------------------
// Fuzz: arbitrary bytes must never panic
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Arbitrary random bytes of any length must never cause a panic.
    #[test]
    fn prop_random_bytes_no_panic(
        data in proptest::collection::vec(any::<u8>(), 0..4096)
    ) {
        let adapter = SimHubAdapter::new();
        let _ = adapter.normalize(&data);
    }

    /// Random valid UTF-8 strings must not cause a panic.
    #[test]
    fn prop_random_utf8_no_panic(
        data in "\\PC{0,500}"
    ) {
        let adapter = SimHubAdapter::new();
        let _ = adapter.normalize(data.as_bytes());
    }
}

// ---------------------------------------------------------------------------
// Round-trip: generated JSON → normalize → verify invariants
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    /// Valid SimHub JSON with normal values must parse and produce bounded output.
    #[test]
    fn prop_valid_json_round_trip(
        speed in 0.0f64..500.0,
        rpms in 0.0f64..20000.0,
        max_rpms in 0.0f64..25000.0,
        gear in gear_strategy(),
        throttle in 0.0f64..100.0,
        brake in 0.0f64..100.0,
        clutch in 0.0f64..100.0,
        steering in -450.0f64..450.0,
        fuel in 0.0f64..100.0,
        lat_g in -10.0f64..10.0,
        lon_g in -10.0f64..10.0,
        ffb in -1.0f64..1.0,
    ) {
        let json = make_json(speed, rpms, max_rpms, &gear, throttle, brake, clutch, steering, fuel, lat_g, lon_g, ffb);
        let adapter = SimHubAdapter::new();
        let result = adapter.normalize(json.as_bytes());
        prop_assert!(result.is_ok(), "valid JSON must parse: {:?}", result);
        let t = result.map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;

        // All output fields must be finite
        prop_assert!(t.speed_ms.is_finite(), "speed_ms not finite");
        prop_assert!(t.rpm.is_finite(), "rpm not finite");
        prop_assert!(t.throttle.is_finite(), "throttle not finite");
        prop_assert!(t.brake.is_finite(), "brake not finite");
        prop_assert!(t.steering_angle.is_finite(), "steering_angle not finite");
        prop_assert!(t.fuel_percent.is_finite(), "fuel_percent not finite");
        prop_assert!(t.ffb_scalar.is_finite(), "ffb_scalar not finite");

        // Normalised ranges
        prop_assert!(t.throttle >= 0.0 && t.throttle <= 1.0,
            "throttle out of range: {}", t.throttle);
        prop_assert!(t.brake >= 0.0 && t.brake <= 1.0,
            "brake out of range: {}", t.brake);
        prop_assert!(t.clutch >= 0.0 && t.clutch <= 1.0,
            "clutch out of range: {}", t.clutch);
        prop_assert!(t.fuel_percent >= 0.0 && t.fuel_percent <= 1.0,
            "fuel_percent out of range: {}", t.fuel_percent);

        // Steering: -450..450° → -1.0..1.0
        prop_assert!(t.steering_angle >= -1.0 && t.steering_angle <= 1.0,
            "steering_angle out of range: {}", t.steering_angle);
    }

    /// Steering normalization: degrees / 450 clamped to [-1, 1].
    #[test]
    fn prop_steering_normalisation(
        steering_deg in -1000.0f64..1000.0,
    ) {
        let json = make_json(0.0, 0.0, 8000.0, "N", 0.0, 0.0, 0.0, steering_deg, 0.0, 0.0, 0.0, 0.0);
        let adapter = SimHubAdapter::new();
        let result = adapter.normalize(json.as_bytes());
        prop_assert!(result.is_ok());
        let t = result.map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        prop_assert!(t.steering_angle >= -1.0 && t.steering_angle <= 1.0,
            "steering_angle {} out of [-1, 1] for input {}", t.steering_angle, steering_deg);
    }

    /// Gear mapping: "R" → -1, "N" → 0, "1"-"6" → 1-6.
    #[test]
    fn prop_gear_mapping(
        gear in gear_strategy(),
    ) {
        let json = make_json(10.0, 3000.0, 8000.0, &gear, 50.0, 0.0, 0.0, 0.0, 50.0, 0.0, 0.0, 0.0);
        let adapter = SimHubAdapter::new();
        let t = adapter.normalize(json.as_bytes())
            .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;

        let expected: i8 = match gear.as_str() {
            "R" => -1,
            "N" => 0,
            g => g.parse::<i8>().map_err(|e| TestCaseError::Fail(format!("{e}").into()))?,
        };
        prop_assert_eq!(t.gear, expected, "gear '{}' should map to {}", gear, expected);
    }
}

// ---------------------------------------------------------------------------
// Missing properties / type mismatches
// ---------------------------------------------------------------------------

#[test]
fn test_missing_optional_fields() -> TestResult {
    let adapter = SimHubAdapter::new();
    // JSON without IsRunning/IsInPit
    let json = br#"{"SpeedMs":10.0,"Rpms":3000.0,"MaxRpms":8000.0,"Gear":"3","Throttle":50.0,"Brake":10.0,"Clutch":0.0,"SteeringAngle":0.0,"FuelPercent":50.0,"LateralGForce":0.0,"LongitudinalGForce":0.0,"FFBValue":0.0}"#;
    let result = adapter.normalize(json);
    assert!(
        result.is_ok(),
        "packet without optional fields should parse"
    );
    Ok(())
}

#[test]
fn test_rpm_alias_field() -> TestResult {
    let adapter = SimHubAdapter::new();
    // "Rpm" instead of "Rpms"
    let json = br#"{"SpeedMs":0.0,"Rpm":5000.0,"MaxRpms":8000.0,"Gear":"N","Throttle":0.0,"Brake":0.0,"Clutch":0.0,"SteeringAngle":0.0,"FuelPercent":0.0,"LateralGForce":0.0,"LongitudinalGForce":0.0,"FFBValue":0.0}"#;
    let t = adapter.normalize(json)?;
    assert!(
        (t.rpm - 5000.0).abs() < 0.1,
        "Rpm alias should work, got {}",
        t.rpm
    );
    Ok(())
}

#[test]
fn test_empty_json_object_defaults() -> TestResult {
    let adapter = SimHubAdapter::new();
    // Empty JSON object is accepted with default values
    let result = adapter.normalize(b"{}");
    if let Ok(t) = result {
        assert!(
            t.speed_ms.abs() < 0.001,
            "empty JSON should default speed to 0"
        );
        assert_eq!(t.gear, 0, "empty JSON should default gear to 0");
    }
    Ok(())
}

#[test]
fn test_json_array_handled() {
    let adapter = SimHubAdapter::new();
    // Should not panic regardless of acceptance
    let _ = adapter.normalize(b"[]");
}

#[test]
fn test_json_null_rejected() {
    let adapter = SimHubAdapter::new();
    assert!(
        adapter.normalize(b"null").is_err(),
        "JSON null must be rejected"
    );
}

#[test]
fn test_type_mismatch_string_speed() {
    let adapter = SimHubAdapter::new();
    let json = br#"{"SpeedMs":"fast","Rpms":0.0,"MaxRpms":0.0,"Gear":"N","Throttle":0.0,"Brake":0.0,"Clutch":0.0,"SteeringAngle":0.0,"FuelPercent":0.0,"LateralGForce":0.0,"LongitudinalGForce":0.0,"FFBValue":0.0}"#;
    // Should not panic
    let _ = adapter.normalize(json);
}

#[test]
fn test_type_mismatch_numeric_gear() {
    let adapter = SimHubAdapter::new();
    // Gear as number instead of string
    let json = br#"{"SpeedMs":0.0,"Rpms":0.0,"MaxRpms":0.0,"Gear":3,"Throttle":0.0,"Brake":0.0,"Clutch":0.0,"SteeringAngle":0.0,"FuelPercent":0.0,"LateralGForce":0.0,"LongitudinalGForce":0.0,"FFBValue":0.0}"#;
    let _ = adapter.normalize(json);
}

#[test]
fn test_extreme_steering_values() -> TestResult {
    let adapter = SimHubAdapter::new();
    // Over-range steering must be clamped
    let json_pos = br#"{"SpeedMs":0.0,"Rpms":0.0,"MaxRpms":0.0,"Gear":"N","Throttle":0.0,"Brake":0.0,"Clutch":0.0,"SteeringAngle":9999.0,"FuelPercent":0.0,"LateralGForce":0.0,"LongitudinalGForce":0.0,"FFBValue":0.0}"#;
    let t = adapter.normalize(json_pos)?;
    assert!(
        (t.steering_angle - 1.0).abs() < 0.001,
        "extreme positive steering should clamp to 1.0"
    );

    let json_neg = br#"{"SpeedMs":0.0,"Rpms":0.0,"MaxRpms":0.0,"Gear":"N","Throttle":0.0,"Brake":0.0,"Clutch":0.0,"SteeringAngle":-9999.0,"FuelPercent":0.0,"LateralGForce":0.0,"LongitudinalGForce":0.0,"FFBValue":0.0}"#;
    let t = adapter.normalize(json_neg)?;
    assert!(
        (t.steering_angle + 1.0).abs() < 0.001,
        "extreme negative steering should clamp to -1.0"
    );
    Ok(())
}

#[test]
fn test_over_100_percent_throttle() -> TestResult {
    let adapter = SimHubAdapter::new();
    let json = make_json(
        0.0, 0.0, 0.0, "N", 200.0, 200.0, 200.0, 0.0, 200.0, 0.0, 0.0, 0.0,
    );
    let t = adapter.normalize(json.as_bytes())?;
    assert!(
        t.throttle <= 1.0,
        "over-100% throttle should clamp to 1.0, got {}",
        t.throttle
    );
    assert!(
        t.brake <= 1.0,
        "over-100% brake should clamp to 1.0, got {}",
        t.brake
    );
    assert!(
        t.fuel_percent <= 1.0,
        "over-100% fuel should clamp to 1.0, got {}",
        t.fuel_percent
    );
    Ok(())
}

#[test]
fn test_normalize_is_deterministic() -> TestResult {
    let adapter = SimHubAdapter::new();
    let json = make_json(
        22.5, 4500.0, 8000.0, "3", 75.0, 10.0, 0.0, -90.0, 82.3, 1.2, -0.5, 0.35,
    );
    let bytes = json.as_bytes();
    let a = adapter.normalize(bytes)?;
    let b = adapter.normalize(bytes)?;
    assert!((a.speed_ms - b.speed_ms).abs() < f32::EPSILON);
    assert!((a.rpm - b.rpm).abs() < f32::EPSILON);
    assert_eq!(a.gear, b.gear);
    assert!((a.throttle - b.throttle).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn test_normalized_telemetry_default_invariants() {
    let t = NormalizedTelemetry::default();
    assert_eq!(t.speed_ms, 0.0);
    assert_eq!(t.rpm, 0.0);
    assert_eq!(t.gear, 0);
    assert_eq!(t.throttle, 0.0);
    assert_eq!(t.brake, 0.0);
    assert_eq!(t.clutch, 0.0);
    assert_eq!(t.steering_angle, 0.0);
    assert_eq!(t.fuel_percent, 0.0);
    assert_eq!(t.ffb_scalar, 0.0);
}
