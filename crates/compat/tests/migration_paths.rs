//! Integration tests for compat migration paths using the real engine TelemetryData.
//!
//! These tests exercise the TelemetryCompat implementation on a thin wrapper
//! around the engine's TelemetryData struct, verifying that deprecated API shims
//! produce values consistent with direct field access during migration.
//!
//! A newtype wrapper is used because the canonical impl lives in
//! `engine::compat_impl` (cfg(test)-only) and the orphan rule prevents
//! re-implementing the trait on a foreign type in an integration test.

use compat::TelemetryCompat;
use racing_wheel_engine::TelemetryData;
use std::time::Instant;

// ---------------------------------------------------------------------------
// Newtype wrapper so we can implement TelemetryCompat in this crate
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

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Migration equivalence: old compat API ↔ direct new-field access
// ---------------------------------------------------------------------------

#[test]
fn migrate_temp_c_to_temperature_c() {
    let t = sample(0.0, 0.0, 72, 0);
    assert_eq!(t.temp_c(), t.0.temperature_c);
}

#[test]
fn migrate_faults_to_fault_flags() {
    let t = sample(0.0, 0.0, 0, 0xCD);
    assert_eq!(t.faults(), t.0.fault_flags);
}

#[test]
fn migrate_wheel_angle_mdeg_to_deg() {
    let t = sample(123.456, 0.0, 0, 0);
    let via_compat = t.wheel_angle_mdeg();
    let via_new = (t.0.wheel_angle_deg * 1000.0) as i32;
    assert_eq!(via_compat, via_new);
}

#[test]
fn migrate_wheel_speed_mrad_s_to_rad_s() {
    let t = sample(0.0, 7.89, 0, 0);
    let via_compat = t.wheel_speed_mrad_s();
    let via_new = (t.0.wheel_speed_rad_s * 1000.0) as i32;
    assert_eq!(via_compat, via_new);
}

#[test]
fn migrate_sequence_removed_field() {
    let t = sample(90.0, 5.0, 45, 0x02);
    // Removed field always returns 0 regardless of telemetry state
    assert_eq!(t.sequence(), 0);
}

// ---------------------------------------------------------------------------
// Migration equivalence across a range of realistic values
// ---------------------------------------------------------------------------

#[test]
fn migration_equivalence_sweep_angles() {
    let angles = [
        -900.0, -720.0, -360.0, -180.0, -90.0, -45.0, -1.0, -0.5, 0.0, 0.5, 1.0, 45.0, 90.0, 180.0,
        360.0, 720.0, 900.0,
    ];
    for &deg in &angles {
        let t = sample(deg, 0.0, 0, 0);
        assert_eq!(
            t.wheel_angle_mdeg(),
            (t.0.wheel_angle_deg * 1000.0) as i32,
            "angle migration mismatch at {deg} deg"
        );
    }
}

#[test]
fn migration_equivalence_sweep_speeds() {
    let speeds = [
        -100.0, -50.0, -10.0, -1.0, -0.1, 0.0, 0.1, 1.0, 10.0, 50.0, 100.0,
    ];
    for &rad_s in &speeds {
        let t = sample(0.0, rad_s, 0, 0);
        assert_eq!(
            t.wheel_speed_mrad_s(),
            (t.0.wheel_speed_rad_s * 1000.0) as i32,
            "speed migration mismatch at {rad_s} rad/s"
        );
    }
}

#[test]
fn migration_equivalence_all_temp_boundaries() {
    for temp in [0_u8, 1, 25, 50, 100, 127, 200, 254, 255] {
        let t = sample(0.0, 0.0, temp, 0);
        assert_eq!(
            t.temp_c(),
            t.0.temperature_c,
            "temp migration mismatch at {temp}"
        );
    }
}

#[test]
fn migration_equivalence_all_fault_patterns() {
    for faults in [
        0x00_u8, 0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x80, 0xFF,
    ] {
        let t = sample(0.0, 0.0, 0, faults);
        assert_eq!(
            t.faults(),
            t.0.fault_flags,
            "fault migration mismatch at {faults:#04X}"
        );
    }
}

// ---------------------------------------------------------------------------
// Deprecated shim: combined state snapshot via compat layer
// ---------------------------------------------------------------------------

#[test]
fn deprecated_shim_full_snapshot_matches_direct_access() {
    let t = sample(270.0, 12.5, 88, 0x13);

    // Compat (deprecated) API
    let old_temp = t.temp_c();
    let old_faults = t.faults();
    let old_angle = t.wheel_angle_mdeg();
    let old_speed = t.wheel_speed_mrad_s();
    let old_seq = t.sequence();

    // New direct API
    let new_temp = t.0.temperature_c;
    let new_faults = t.0.fault_flags;
    let new_angle = (t.0.wheel_angle_deg * 1000.0) as i32;
    let new_speed = (t.0.wheel_speed_rad_s * 1000.0) as i32;

    assert_eq!(old_temp, new_temp);
    assert_eq!(old_faults, new_faults);
    assert_eq!(old_angle, new_angle);
    assert_eq!(old_speed, new_speed);
    assert_eq!(old_seq, 0);
}

// ---------------------------------------------------------------------------
// Trait-object migration: code using `&dyn TelemetryCompat` still works
// ---------------------------------------------------------------------------

#[test]
fn trait_object_on_real_telemetry_data() {
    let t = sample(45.0, 3.15, 55, 0x0F);
    let dyn_ref: &dyn TelemetryCompat = &t;

    assert_eq!(dyn_ref.temp_c(), 55);
    assert_eq!(dyn_ref.faults(), 0x0F);
    assert_eq!(dyn_ref.wheel_angle_mdeg(), 45_000);
    assert_eq!(dyn_ref.wheel_speed_mrad_s(), 3_150);
    assert_eq!(dyn_ref.sequence(), 0);
}

fn read_via_compat(src: &dyn TelemetryCompat) -> (u8, u8, i32, i32, u32) {
    (
        src.temp_c(),
        src.faults(),
        src.wheel_angle_mdeg(),
        src.wheel_speed_mrad_s(),
        src.sequence(),
    )
}

#[test]
fn helper_fn_accepting_dyn_compat() {
    let t = sample(-30.0, -2.0, 10, 0x44);
    let (temp, faults, angle, speed, seq) = read_via_compat(&t);

    assert_eq!(temp, 10);
    assert_eq!(faults, 0x44);
    assert_eq!(angle, -30_000);
    assert_eq!(speed, -2_000);
    assert_eq!(seq, 0);
}

// ---------------------------------------------------------------------------
// Backwards-compatible type conversions: verify conversion factor consistency
// ---------------------------------------------------------------------------

#[test]
fn conversion_factor_is_consistent_1000() {
    let t = sample(1.0, 1.0, 0, 0);
    // Both angle and speed use ×1000 conversion
    assert_eq!(t.wheel_angle_mdeg(), 1_000);
    assert_eq!(t.wheel_speed_mrad_s(), 1_000);
}

#[test]
fn conversion_preserves_sign_on_real_telemetry() {
    let positive = sample(90.0, 5.0, 0, 0);
    let negative = sample(-90.0, -5.0, 0, 0);

    assert!(positive.wheel_angle_mdeg() > 0);
    assert!(negative.wheel_angle_mdeg() < 0);
    assert_eq!(positive.wheel_angle_mdeg(), -negative.wheel_angle_mdeg());

    assert!(positive.wheel_speed_mrad_s() > 0);
    assert!(negative.wheel_speed_mrad_s() < 0);
    assert_eq!(
        positive.wheel_speed_mrad_s(),
        -negative.wheel_speed_mrad_s()
    );
}

#[test]
fn conversion_truncates_toward_zero() {
    // f32 0.999 * 1000 = 999.0 → 999 (no rounding up)
    let t = sample(0.999, 0.999, 0, 0);
    assert_eq!(t.wheel_angle_mdeg(), 999);
    assert_eq!(t.wheel_speed_mrad_s(), 999);

    // Negative: -0.999 * 1000 = -999.0 → -999
    let tn = sample(-0.999, -0.999, 0, 0);
    assert_eq!(tn.wheel_angle_mdeg(), -999);
    assert_eq!(tn.wheel_speed_mrad_s(), -999);
}

// ---------------------------------------------------------------------------
// Cloned telemetry preserves compat behaviour
// ---------------------------------------------------------------------------

#[test]
fn cloned_inner_telemetry_has_same_compat_values() {
    let original = sample(50.0, 8.0, 77, 0xBB);
    let cloned = Compat(original.0.clone());

    assert_eq!(original.temp_c(), cloned.temp_c());
    assert_eq!(original.faults(), cloned.faults());
    assert_eq!(original.wheel_angle_mdeg(), cloned.wheel_angle_mdeg());
    assert_eq!(original.wheel_speed_mrad_s(), cloned.wheel_speed_mrad_s());
    assert_eq!(original.sequence(), cloned.sequence());
}

// ---------------------------------------------------------------------------
// hands_on field is not part of compat layer (no shim needed)
// ---------------------------------------------------------------------------

#[test]
fn hands_on_not_affected_by_compat_layer() {
    let mut t = sample(0.0, 0.0, 0, 0);
    t.0.hands_on = true;
    // Changing hands_on does not alter any compat method output
    assert_eq!(t.temp_c(), 0);
    assert_eq!(t.faults(), 0);
    assert_eq!(t.wheel_angle_mdeg(), 0);
    assert_eq!(t.wheel_speed_mrad_s(), 0);
    assert_eq!(t.sequence(), 0);
}

// ---------------------------------------------------------------------------
// Multiple independent TelemetryData instances via compat
// ---------------------------------------------------------------------------

#[test]
fn independent_real_instances_do_not_interfere() {
    let a = sample(10.0, 1.0, 20, 0x01);
    let b = sample(20.0, 2.0, 40, 0x02);
    let c = sample(30.0, 3.0, 60, 0x04);

    assert_eq!(a.temp_c(), 20);
    assert_eq!(b.temp_c(), 40);
    assert_eq!(c.temp_c(), 60);

    assert_eq!(a.wheel_angle_mdeg(), 10_000);
    assert_eq!(b.wheel_angle_mdeg(), 20_000);
    assert_eq!(c.wheel_angle_mdeg(), 30_000);

    assert_eq!(a.wheel_speed_mrad_s(), 1_000);
    assert_eq!(b.wheel_speed_mrad_s(), 2_000);
    assert_eq!(c.wheel_speed_mrad_s(), 3_000);
}

// ---------------------------------------------------------------------------
// Compat layer works on zero-initialised telemetry
// ---------------------------------------------------------------------------

#[test]
fn zero_state_telemetry_compat() {
    let t = sample(0.0, 0.0, 0, 0);
    assert_eq!(t.temp_c(), 0);
    assert_eq!(t.faults(), 0);
    assert_eq!(t.wheel_angle_mdeg(), 0);
    assert_eq!(t.wheel_speed_mrad_s(), 0);
    assert_eq!(t.sequence(), 0);
}

// ---------------------------------------------------------------------------
// Compat layer works on max-stress telemetry values
// ---------------------------------------------------------------------------

#[test]
fn high_stress_telemetry_values() {
    let t = sample(900.0, 100.0, u8::MAX, u8::MAX);
    assert_eq!(t.temp_c(), 255);
    assert_eq!(t.faults(), 255);
    assert_eq!(t.wheel_angle_mdeg(), 900_000);
    assert_eq!(t.wheel_speed_mrad_s(), 100_000);
    assert_eq!(t.sequence(), 0);
}
