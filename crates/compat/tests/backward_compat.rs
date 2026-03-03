//! Backward compatibility guarantees and deprecation tracking tests.
//!
//! These tests verify:
//! - The deprecation registry: which old field names map to which new names
//! - Cross-field isolation: compat methods depend only on their own field
//! - Idempotent access: repeated compat calls return identical results
//! - API contract stability: trait surface and behavior guarantees
//! - Conversion edge cases near precision and overflow boundaries

use compat::TelemetryCompat;
use racing_wheel_engine::TelemetryData;
use std::time::Instant;

// ---------------------------------------------------------------------------
// Newtype wrapper (orphan rule)
// ---------------------------------------------------------------------------

struct Compat(TelemetryData);

impl TelemetryCompat for Compat {
    fn temp_c(&self) -> u8 {
        self.0.temperature_c
    }
    fn faults(&self) -> u8 {
        self.0.fault_flags
    }
    fn wheel_angle_mdeg(&self) -> i32 {
        (self.0.wheel_angle_deg * 1000.0) as i32
    }
    fn wheel_speed_mrad_s(&self) -> i32 {
        (self.0.wheel_speed_rad_s * 1000.0) as i32
    }
    fn sequence(&self) -> u32 {
        0
    }
}

fn sample(angle_deg: f32, speed_rad_s: f32, temp: u8, faults: u8) -> Compat {
    Compat(TelemetryData {
        wheel_angle_deg: angle_deg,
        wheel_speed_rad_s: speed_rad_s,
        temperature_c: temp,
        fault_flags: faults,
        hands_on: false,
        timestamp: Instant::now(),
    })
}

// ===========================================================================
// Deprecation registry: document and verify each deprecated → new mapping
// ===========================================================================

/// Verifies the complete deprecation mapping table.
/// Each deprecated API method must return the equivalent of its new-field counterpart.
#[test]
fn deprecation_registry_complete() {
    let t = sample(45.0, 3.0, 72, 0xAB);

    // temp_c → temperature_c (direct mapping, no conversion)
    assert_eq!(t.temp_c(), t.0.temperature_c);

    // faults → fault_flags (direct mapping, no conversion)
    assert_eq!(t.faults(), t.0.fault_flags);

    // wheel_angle_mdeg → wheel_angle_deg (×1000 unit conversion)
    assert_eq!(
        t.wheel_angle_mdeg(),
        (t.0.wheel_angle_deg * 1000.0) as i32
    );

    // wheel_speed_mrad_s → wheel_speed_rad_s (×1000 unit conversion)
    assert_eq!(
        t.wheel_speed_mrad_s(),
        (t.0.wheel_speed_rad_s * 1000.0) as i32
    );

    // sequence → (removed, always 0)
    assert_eq!(t.sequence(), 0);
}

/// Direct-mapped fields must return values identical to the new field (no transformation).
#[test]
fn direct_mapped_fields_have_no_conversion() {
    for temp in [0_u8, 1, 50, 100, 127, 200, 255] {
        for faults in [0x00_u8, 0x01, 0x0F, 0x55, 0xAA, 0xFF] {
            let t = sample(0.0, 0.0, temp, faults);
            assert_eq!(
                t.temp_c(),
                temp,
                "temp_c should equal temperature_c for {temp}"
            );
            assert_eq!(
                t.faults(),
                faults,
                "faults should equal fault_flags for {faults:#04X}"
            );
        }
    }
}

/// Unit-converted fields use exactly ×1000 factor, not some other multiplier.
#[test]
fn conversion_factor_is_exactly_1000() {
    let t = sample(1.0, 1.0, 0, 0);
    assert_eq!(
        t.wheel_angle_mdeg(),
        1000,
        "angle conversion factor must be 1000"
    );
    assert_eq!(
        t.wheel_speed_mrad_s(),
        1000,
        "speed conversion factor must be 1000"
    );
}

/// The sequence field was removed from the schema; its compat shim always returns 0.
#[test]
fn removed_field_sequence_always_zero() {
    let values = [
        (0.0, 0.0, 0_u8, 0_u8),
        (900.0, 100.0, 255, 255),
        (-900.0, -100.0, 128, 0x80),
        (1.5, -2.5, 42, 0x0F),
    ];
    for (angle, speed, temp, faults) in values {
        let t = sample(angle, speed, temp, faults);
        assert_eq!(
            t.sequence(),
            0,
            "sequence must be 0 regardless of telemetry state"
        );
    }
}

// ===========================================================================
// Cross-field isolation: compat methods depend only on their own field
// ===========================================================================

#[test]
fn temp_c_isolated_from_other_fields() {
    let base = sample(0.0, 0.0, 42, 0);
    let varied_angle = sample(900.0, 0.0, 42, 0);
    let varied_speed = sample(0.0, 100.0, 42, 0);
    let varied_faults = sample(0.0, 0.0, 42, 0xFF);

    assert_eq!(base.temp_c(), varied_angle.temp_c());
    assert_eq!(base.temp_c(), varied_speed.temp_c());
    assert_eq!(base.temp_c(), varied_faults.temp_c());
}

#[test]
fn faults_isolated_from_other_fields() {
    let base = sample(0.0, 0.0, 0, 0x55);
    let varied_angle = sample(900.0, 0.0, 0, 0x55);
    let varied_speed = sample(0.0, 100.0, 0, 0x55);
    let varied_temp = sample(0.0, 0.0, 255, 0x55);

    assert_eq!(base.faults(), varied_angle.faults());
    assert_eq!(base.faults(), varied_speed.faults());
    assert_eq!(base.faults(), varied_temp.faults());
}

#[test]
fn wheel_angle_mdeg_isolated_from_other_fields() {
    let base = sample(45.0, 0.0, 0, 0);
    let varied_speed = sample(45.0, 100.0, 0, 0);
    let varied_temp = sample(45.0, 0.0, 255, 0);
    let varied_faults = sample(45.0, 0.0, 0, 0xFF);

    assert_eq!(base.wheel_angle_mdeg(), varied_speed.wheel_angle_mdeg());
    assert_eq!(base.wheel_angle_mdeg(), varied_temp.wheel_angle_mdeg());
    assert_eq!(
        base.wheel_angle_mdeg(),
        varied_faults.wheel_angle_mdeg()
    );
}

#[test]
fn wheel_speed_mrad_s_isolated_from_other_fields() {
    let base = sample(0.0, 7.5, 0, 0);
    let varied_angle = sample(900.0, 7.5, 0, 0);
    let varied_temp = sample(0.0, 7.5, 255, 0);
    let varied_faults = sample(0.0, 7.5, 0, 0xFF);

    assert_eq!(
        base.wheel_speed_mrad_s(),
        varied_angle.wheel_speed_mrad_s()
    );
    assert_eq!(
        base.wheel_speed_mrad_s(),
        varied_temp.wheel_speed_mrad_s()
    );
    assert_eq!(
        base.wheel_speed_mrad_s(),
        varied_faults.wheel_speed_mrad_s()
    );
}

// ===========================================================================
// Idempotent access: repeated calls return the same result
// ===========================================================================

#[test]
fn repeated_calls_return_identical_results() {
    let t = sample(123.456, -7.89, 99, 0x3C);

    let temp1 = t.temp_c();
    let temp2 = t.temp_c();
    let temp3 = t.temp_c();
    assert_eq!(temp1, temp2);
    assert_eq!(temp2, temp3);

    let faults1 = t.faults();
    let faults2 = t.faults();
    assert_eq!(faults1, faults2);

    let angle1 = t.wheel_angle_mdeg();
    let angle2 = t.wheel_angle_mdeg();
    assert_eq!(angle1, angle2);

    let speed1 = t.wheel_speed_mrad_s();
    let speed2 = t.wheel_speed_mrad_s();
    assert_eq!(speed1, speed2);

    let seq1 = t.sequence();
    let seq2 = t.sequence();
    assert_eq!(seq1, seq2);
}

// ===========================================================================
// API contract: trait provides the expected surface and guarantees
// ===========================================================================

/// The trait is object-safe and can be used with dynamic dispatch.
#[test]
fn trait_is_object_safe() {
    let t = sample(10.0, 2.0, 30, 0x08);
    let _: &dyn TelemetryCompat = &t;
}

/// A generic function can accept any TelemetryCompat implementor.
fn sum_compat_values<T: TelemetryCompat>(t: &T) -> i64 {
    i64::from(t.temp_c())
        + i64::from(t.faults())
        + i64::from(t.wheel_angle_mdeg())
        + i64::from(t.wheel_speed_mrad_s())
        + i64::from(t.sequence())
}

#[test]
fn generic_compat_consumer_works() {
    let t = sample(90.0, 5.0, 45, 2);
    let sum = sum_compat_values(&t);
    let expected = 45_i64 + 2 + 90_000 + 5_000 + 0;
    assert_eq!(sum, expected);
}

/// Boxed trait objects work for owned dynamic dispatch.
#[test]
fn boxed_trait_object_works() {
    let t = sample(60.0, 4.0, 33, 0x11);
    let boxed: Box<dyn TelemetryCompat> = Box::new(t);
    assert_eq!(boxed.temp_c(), 33);
    assert_eq!(boxed.faults(), 0x11);
    assert_eq!(boxed.wheel_angle_mdeg(), 60_000);
    assert_eq!(boxed.wheel_speed_mrad_s(), 4_000);
    assert_eq!(boxed.sequence(), 0);
}

/// A Vec of heterogeneous trait objects all respond correctly.
#[test]
fn heterogeneous_trait_object_collection() {
    let items: Vec<Box<dyn TelemetryCompat>> = vec![
        Box::new(sample(10.0, 1.0, 20, 0x01)),
        Box::new(sample(20.0, 2.0, 40, 0x02)),
        Box::new(sample(30.0, 3.0, 60, 0x04)),
    ];
    let temps: Vec<u8> = items.iter().map(|i| i.temp_c()).collect();
    assert_eq!(temps, vec![20, 40, 60]);

    let angles: Vec<i32> = items.iter().map(|i| i.wheel_angle_mdeg()).collect();
    assert_eq!(angles, vec![10_000, 20_000, 30_000]);
}

// ===========================================================================
// Conversion edge cases near precision boundaries
// ===========================================================================

/// Typical racing wheel range: ±900° angle, ±50 rad/s speed.
#[test]
fn conversion_within_typical_racing_range() {
    let cases: &[(f32, i32)] = &[
        (900.0, 900_000),
        (-900.0, -900_000),
        (450.0, 450_000),
        (-450.0, -450_000),
        (0.5, 500),
        (-0.5, -500),
    ];
    for &(deg, expected_mdeg) in cases {
        let t = sample(deg, 0.0, 0, 0);
        assert_eq!(
            t.wheel_angle_mdeg(),
            expected_mdeg,
            "angle conversion mismatch at {deg}\u{00B0}"
        );
    }
}

/// Sub-millidegree precision is lost by truncation toward zero (not rounding).
#[test]
fn sub_unit_precision_truncates() {
    // 0.4999 * 1000 = 499.9 → truncates to 499
    let t = sample(0.4999, 0.4999, 0, 0);
    assert_eq!(t.wheel_angle_mdeg(), 499);
    assert_eq!(t.wheel_speed_mrad_s(), 499);

    // Negative: -0.4999 * 1000 = -499.9 → truncates to -499
    let tn = sample(-0.4999, -0.4999, 0, 0);
    assert_eq!(tn.wheel_angle_mdeg(), -499);
    assert_eq!(tn.wheel_speed_mrad_s(), -499);
}

/// Powers of two are exactly representable in f32 and should convert exactly.
#[test]
fn exact_f32_values_convert_exactly() {
    let powers: &[f32] = &[
        0.125, 0.25, 0.5, 1.0, 2.0, 4.0, 8.0, 16.0, 32.0, 64.0, 128.0, 256.0, 512.0,
    ];
    for &deg in powers {
        let t = sample(deg, deg, 0, 0);
        let expected = (deg * 1000.0) as i32;
        assert_eq!(t.wheel_angle_mdeg(), expected, "f32-exact angle {deg}");
        assert_eq!(t.wheel_speed_mrad_s(), expected, "f32-exact speed {deg}");
    }
}

// ===========================================================================
// Backward compat: old code patterns still compile and work alongside new code
// ===========================================================================

/// Simulates old test code that relied on deprecated field names via compat.
#[test]
fn old_test_code_pattern_still_works() {
    let t = sample(270.0, 12.5, 88, 0x13);

    // Old-style assertions (using deprecated compat names)
    assert_eq!(t.temp_c(), 88);
    assert_eq!(t.faults(), 0x13);
    assert_eq!(t.wheel_angle_mdeg(), 270_000);
    assert_eq!(t.wheel_speed_mrad_s(), 12_500);
    assert_eq!(t.sequence(), 0);
}

/// Simulates migrated test code using new field names directly.
#[test]
fn new_test_code_pattern_works() {
    let t = sample(270.0, 12.5, 88, 0x13);

    // New-style assertions (direct field access)
    assert_eq!(t.0.temperature_c, 88);
    assert_eq!(t.0.fault_flags, 0x13);
    assert_eq!((t.0.wheel_angle_deg * 1000.0) as i32, 270_000);
    assert_eq!((t.0.wheel_speed_rad_s * 1000.0) as i32, 12_500);
}

/// Old and new code can coexist: compat and direct access yield same results.
#[test]
fn old_and_new_code_coexist() {
    let t = sample(135.0, 6.28, 50, 0x07);

    assert_eq!(t.temp_c(), t.0.temperature_c);
    assert_eq!(t.faults(), t.0.fault_flags);
    assert_eq!(
        t.wheel_angle_mdeg(),
        (t.0.wheel_angle_deg * 1000.0) as i32
    );
    assert_eq!(
        t.wheel_speed_mrad_s(),
        (t.0.wheel_speed_rad_s * 1000.0) as i32
    );
}
