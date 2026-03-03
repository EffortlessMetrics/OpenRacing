//! Property-based tests for the MudRunner / SnowRunner telemetry adapter.
//!
//! Exercises JSON parsing round-trips, terrain/vehicle state edge cases, and
//! ensures the adapter never panics on arbitrary input.

#![allow(clippy::redundant_closure)]

use proptest::prelude::*;
use racing_wheel_telemetry_mudrunner::{
    MudRunnerAdapter, MudRunnerVariant, NormalizedTelemetry, TelemetryAdapter,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a SimHub-style JSON packet with the given telemetry values.
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
        (1..=8i32).prop_map(|g| g.to_string()),
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
        let adapter = MudRunnerAdapter::new();
        let _ = adapter.normalize(&data);
    }

    /// SnowRunner variant must also handle arbitrary bytes without panic.
    #[test]
    fn prop_snowrunner_random_bytes_no_panic(
        data in proptest::collection::vec(any::<u8>(), 0..4096)
    ) {
        let adapter = MudRunnerAdapter::with_variant(MudRunnerVariant::SnowRunner);
        let _ = adapter.normalize(&data);
    }
}

// ---------------------------------------------------------------------------
// Round-trip: generated JSON → normalize → verify invariants
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    /// Any valid JSON with finite values must parse without panic and produce
    /// finite output fields.
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
        let adapter = MudRunnerAdapter::new();
        let result = adapter.normalize(json.as_bytes());
        prop_assert!(result.is_ok(), "valid JSON must parse: {:?}", result);
        let t = result.map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;

        // All output fields must be finite
        prop_assert!(t.speed_ms.is_finite(), "speed_ms not finite: {}", t.speed_ms);
        prop_assert!(t.rpm.is_finite(), "rpm not finite: {}", t.rpm);
        prop_assert!(t.throttle.is_finite(), "throttle not finite: {}", t.throttle);
        prop_assert!(t.brake.is_finite(), "brake not finite: {}", t.brake);
        prop_assert!(t.steering_angle.is_finite(), "steering not finite: {}", t.steering_angle);
        prop_assert!(t.fuel_percent.is_finite(), "fuel not finite: {}", t.fuel_percent);
        prop_assert!(t.ffb_scalar.is_finite(), "ffb not finite: {}", t.ffb_scalar);
    }

    /// Throttle/Brake/Clutch normalised from percentage (0-100) to 0.0-1.0.
    #[test]
    fn prop_percentage_normalisation(
        throttle in 0.0f64..100.0,
        brake in 0.0f64..100.0,
        clutch in 0.0f64..100.0,
        fuel in 0.0f64..100.0,
    ) {
        let json = make_json(0.0, 0.0, 8000.0, "N", throttle, brake, clutch, 0.0, fuel, 0.0, 0.0, 0.0);
        let adapter = MudRunnerAdapter::new();
        let t = adapter.normalize(json.as_bytes())
            .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;

        prop_assert!(t.throttle >= 0.0 && t.throttle <= 1.0,
            "throttle out of range: {}", t.throttle);
        prop_assert!(t.brake >= 0.0 && t.brake <= 1.0,
            "brake out of range: {}", t.brake);
        prop_assert!(t.clutch >= 0.0 && t.clutch <= 1.0,
            "clutch out of range: {}", t.clutch);
        prop_assert!(t.fuel_percent >= 0.0 && t.fuel_percent <= 1.0,
            "fuel_percent out of range: {}", t.fuel_percent);
    }
}

// ---------------------------------------------------------------------------
// Deterministic edge-case tests
// ---------------------------------------------------------------------------

#[test]
fn test_missing_optional_fields_accepted() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    // Minimal JSON with only required fields (varies by adapter impl)
    let json = br#"{"SpeedMs":0.0,"Rpms":0.0,"MaxRpms":0.0,"Gear":"N","Throttle":0.0,"Brake":0.0,"Clutch":0.0,"SteeringAngle":0.0,"FuelPercent":0.0,"LateralGForce":0.0,"LongitudinalGForce":0.0,"FFBValue":0.0}"#;
    let result = adapter.normalize(json);
    assert!(result.is_ok(), "packet without IsRunning/IsInPit should parse");
    Ok(())
}

#[test]
fn test_extreme_speed_value() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    let json = make_json(99999.0, 0.0, 0.0, "N", 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    let t = adapter.normalize(json.as_bytes())?;
    assert!(t.speed_ms.is_finite(), "extreme speed must still be finite");
    Ok(())
}

#[test]
fn test_extreme_rpm_value() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    let json = make_json(0.0, 999999.0, 999999.0, "N", 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    let t = adapter.normalize(json.as_bytes())?;
    assert!(t.rpm.is_finite(), "extreme RPM must still be finite");
    Ok(())
}

#[test]
fn test_negative_values_in_json() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    let json = make_json(-10.0, -100.0, 8000.0, "N", -50.0, -50.0, -50.0, -900.0, -10.0, -20.0, -20.0, -5.0);
    // Should not panic regardless of whether it succeeds or fails
    let _ = adapter.normalize(json.as_bytes());
    Ok(())
}

#[test]
fn test_zero_max_rpms() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    let json = make_json(10.0, 3000.0, 0.0, "2", 50.0, 0.0, 0.0, 0.0, 50.0, 0.0, 0.0, 0.0);
    let t = adapter.normalize(json.as_bytes())?;
    assert!(t.rpm.is_finite());
    Ok(())
}

#[test]
fn test_all_gears_parse() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    for gear in &["R", "N", "1", "2", "3", "4", "5", "6"] {
        let json = make_json(10.0, 3000.0, 8000.0, gear, 50.0, 0.0, 0.0, 0.0, 50.0, 0.0, 0.0, 0.0);
        let t = adapter.normalize(json.as_bytes())?;
        match *gear {
            "R" => assert_eq!(t.gear, -1, "gear R → -1"),
            "N" => assert_eq!(t.gear, 0, "gear N → 0"),
            g => assert_eq!(t.gear, g.parse::<i8>()?, "gear {g} roundtrip"),
        }
    }
    Ok(())
}

#[test]
fn test_both_variants_same_output() -> TestResult {
    let mud = MudRunnerAdapter::new();
    let snow = MudRunnerAdapter::with_variant(MudRunnerVariant::SnowRunner);
    let json = make_json(20.0, 3000.0, 6000.0, "3", 70.0, 10.0, 0.0, 45.0, 60.0, 0.5, 0.3, 0.2);
    let bytes = json.as_bytes();

    let t1 = mud.normalize(bytes)?;
    let t2 = snow.normalize(bytes)?;

    assert!((t1.speed_ms - t2.speed_ms).abs() < f32::EPSILON);
    assert!((t1.rpm - t2.rpm).abs() < f32::EPSILON);
    assert_eq!(t1.gear, t2.gear);
    assert!((t1.throttle - t2.throttle).abs() < f32::EPSILON);
    assert!((t1.brake - t2.brake).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn test_empty_json_object_defaults() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    // Empty JSON object may be accepted with default values
    let result = adapter.normalize(b"{}");
    if let Ok(t) = result {
        assert!(t.speed_ms.abs() < 0.001, "empty JSON should default speed to 0");
        assert!(t.rpm.abs() < 0.001, "empty JSON should default rpm to 0");
        assert_eq!(t.gear, 0, "empty JSON should default gear to 0");
    }
    Ok(())
}

#[test]
fn test_unicode_in_gear_field() {
    let adapter = MudRunnerAdapter::new();
    let json = r#"{"SpeedMs":0.0,"Rpms":0.0,"MaxRpms":0.0,"Gear":"XYZ","Throttle":0.0,"Brake":0.0,"Clutch":0.0,"SteeringAngle":0.0,"FuelPercent":0.0,"LateralGForce":0.0,"LongitudinalGForce":0.0,"FFBValue":0.0,"IsRunning":false,"IsInPit":false}"#;
    // Should not panic; may error or default to gear 0
    let _ = adapter.normalize(json.as_bytes());
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
    assert_eq!(t.lateral_g, 0.0);
    assert_eq!(t.longitudinal_g, 0.0);
}
