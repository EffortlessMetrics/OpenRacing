#![allow(clippy::redundant_closure)]
#![allow(clippy::too_many_arguments)]
//! Property-based fuzz tests for Forza telemetry packet parsing.
//!
//! Covers all four Forza packet formats (Sled 232, CarDash 311, FM8 331, FH4 324)
//! with random bytes, valid-field round-trips, truncation, boundary conditions,
//! and deterministic normalization.

use proptest::prelude::*;
use racing_wheel_telemetry_forza::{ForzaAdapter, NormalizedTelemetry, TelemetryAdapter};

// ── Packet size constants ───────────────────────────────────────────────────

const SLED_SIZE: usize = 232;
const CARDASH_SIZE: usize = 311;
const FM8_CARDASH_SIZE: usize = 331;
const FH4_CARDASH_SIZE: usize = 324;

// ── Sled byte offsets ───────────────────────────────────────────────────────

const OFF_IS_RACE_ON: usize = 0;
const OFF_ENGINE_MAX_RPM: usize = 8;
const OFF_CURRENT_RPM: usize = 16;
const OFF_VEL_X: usize = 32;
const OFF_VEL_Y: usize = 36;
const OFF_VEL_Z: usize = 40;

// ── CarDash extension offsets ───────────────────────────────────────────────

const OFF_DASH_ACCEL: usize = 303;
const OFF_DASH_BRAKE: usize = 304;
const OFF_DASH_CLUTCH: usize = 305;
const OFF_DASH_GEAR: usize = 307;
const OFF_DASH_STEER: usize = 308;

// ── Helpers ─────────────────────────────────────────────────────────────────

fn write_f32(buf: &mut [u8], offset: usize, value: f32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_i32(buf: &mut [u8], offset: usize, value: i32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn make_sled(is_race_on: i32, rpm: f32, max_rpm: f32, vel: (f32, f32, f32)) -> Vec<u8> {
    let mut data = vec![0u8; SLED_SIZE];
    write_i32(&mut data, OFF_IS_RACE_ON, is_race_on);
    write_f32(&mut data, OFF_ENGINE_MAX_RPM, max_rpm);
    write_f32(&mut data, OFF_CURRENT_RPM, rpm);
    write_f32(&mut data, OFF_VEL_X, vel.0);
    write_f32(&mut data, OFF_VEL_Y, vel.1);
    write_f32(&mut data, OFF_VEL_Z, vel.2);
    data
}

fn make_cardash(
    is_race_on: i32,
    rpm: f32,
    max_rpm: f32,
    vel: (f32, f32, f32),
    throttle: u8,
    brake: u8,
    clutch: u8,
    gear: u8,
    steer: i8,
) -> Vec<u8> {
    let mut data = vec![0u8; CARDASH_SIZE];
    write_i32(&mut data, OFF_IS_RACE_ON, is_race_on);
    write_f32(&mut data, OFF_ENGINE_MAX_RPM, max_rpm);
    write_f32(&mut data, OFF_CURRENT_RPM, rpm);
    write_f32(&mut data, OFF_VEL_X, vel.0);
    write_f32(&mut data, OFF_VEL_Y, vel.1);
    write_f32(&mut data, OFF_VEL_Z, vel.2);
    data[OFF_DASH_ACCEL] = throttle;
    data[OFF_DASH_BRAKE] = brake;
    data[OFF_DASH_CLUTCH] = clutch;
    data[OFF_DASH_GEAR] = gear;
    data[OFF_DASH_STEER] = steer as u8;
    data
}

fn assert_telemetry_invariants(t: &NormalizedTelemetry) {
    assert!(
        t.speed_ms >= 0.0 && t.speed_ms.is_finite(),
        "speed_ms invalid: {}",
        t.speed_ms
    );
    assert!(t.rpm >= 0.0 && t.rpm.is_finite(), "rpm invalid: {}", t.rpm);
    assert!(
        t.throttle >= 0.0 && t.throttle <= 1.0,
        "throttle out of 0.0..=1.0: {}",
        t.throttle
    );
    assert!(
        t.brake >= 0.0 && t.brake <= 1.0,
        "brake out of 0.0..=1.0: {}",
        t.brake
    );
    assert!(
        t.clutch >= 0.0 && t.clutch <= 1.0,
        "clutch out of 0.0..=1.0: {}",
        t.clutch
    );
    assert!(
        t.slip_ratio >= 0.0 && t.slip_ratio.is_finite(),
        "slip_ratio invalid: {}",
        t.slip_ratio
    );
}

// ── 1. Random byte arrays → parse never panics ─────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Arbitrary random bytes of any length must never cause a panic.
    #[test]
    fn prop_random_bytes_no_panic(
        data in proptest::collection::vec(any::<u8>(), 0..512)
    ) {
        let adapter = ForzaAdapter::new();
        let _ = adapter.normalize(&data);
    }

    /// Random bytes at each exact packet size must not panic.
    #[test]
    fn prop_sled_size_random_no_panic(
        data in proptest::collection::vec(any::<u8>(), SLED_SIZE..=SLED_SIZE)
    ) {
        let adapter = ForzaAdapter::new();
        let _ = adapter.normalize(&data);
    }

    #[test]
    fn prop_cardash_size_random_no_panic(
        data in proptest::collection::vec(any::<u8>(), CARDASH_SIZE..=CARDASH_SIZE)
    ) {
        let adapter = ForzaAdapter::new();
        let _ = adapter.normalize(&data);
    }

    #[test]
    fn prop_fm8_size_random_no_panic(
        data in proptest::collection::vec(any::<u8>(), FM8_CARDASH_SIZE..=FM8_CARDASH_SIZE)
    ) {
        let adapter = ForzaAdapter::new();
        let _ = adapter.normalize(&data);
    }

    #[test]
    fn prop_fh4_size_random_no_panic(
        data in proptest::collection::vec(any::<u8>(), FH4_CARDASH_SIZE..=FH4_CARDASH_SIZE)
    ) {
        let adapter = ForzaAdapter::new();
        let _ = adapter.normalize(&data);
    }
}

// ── 2. Valid telemetry frame → normalize → verify invariants ────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    /// Sled packet with valid physics data must produce correct invariants.
    #[test]
    fn prop_sled_valid_invariants(
        rpm in 0.0f32..20000.0,
        max_rpm in 100.0f32..25000.0,
        vel_x in -100.0f32..100.0,
        vel_y in -100.0f32..100.0,
        vel_z in -100.0f32..100.0,
    ) {
        let data = make_sled(1, rpm, max_rpm, (vel_x, vel_y, vel_z));
        let adapter = ForzaAdapter::new();
        let result = adapter.normalize(&data);
        prop_assert!(result.is_ok(), "valid sled must parse: {:?}", result);
        let t = result.map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;

        assert_telemetry_invariants(&t);

        // RPM preserved
        prop_assert!((t.rpm - rpm).abs() < 0.1,
            "rpm mismatch: {} vs {}", t.rpm, rpm);

        // Speed = magnitude of velocity vector
        let expected_speed = (vel_x * vel_x + vel_y * vel_y + vel_z * vel_z).sqrt();
        prop_assert!((t.speed_ms - expected_speed).abs() < 0.1,
            "speed_ms {} vs expected {}", t.speed_ms, expected_speed);
    }

    /// CarDash packet round-trip: throttle/brake/gear preserved.
    #[test]
    fn prop_cardash_round_trip(
        rpm in 0.0f32..18000.0,
        throttle_raw in 0u8..=255u8,
        brake_raw in 0u8..=255u8,
        clutch_raw in 0u8..=255u8,
        gear in 0u8..=10u8,
        steer in -127i8..=127i8,
    ) {
        let data = make_cardash(
            1, rpm, 20000.0, (30.0, 0.0, 0.0),
            throttle_raw, brake_raw, clutch_raw, gear, steer,
        );
        let adapter = ForzaAdapter::new();
        let result = adapter.normalize(&data);
        prop_assert!(result.is_ok(), "valid cardash must parse: {:?}", result);
        let t = result.map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;

        assert_telemetry_invariants(&t);

        // Throttle: u8/255 → [0,1]
        let expected_throttle = f32::from(throttle_raw) / 255.0;
        prop_assert!((t.throttle - expected_throttle).abs() < 0.01,
            "throttle {} vs expected {}", t.throttle, expected_throttle);

        // Brake: u8/255 → [0,1]
        let expected_brake = f32::from(brake_raw) / 255.0;
        prop_assert!((t.brake - expected_brake).abs() < 0.01,
            "brake {} vs expected {}", t.brake, expected_brake);
    }

    /// Race-off packets should produce zeroed telemetry.
    #[test]
    fn prop_race_off_zeros(
        rpm in 0.0f32..20000.0,
        vel in -100.0f32..100.0,
    ) {
        let data = make_sled(0, rpm, 20000.0, (vel, 0.0, 0.0));
        let adapter = ForzaAdapter::new();
        let t = adapter.normalize(&data)
            .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        prop_assert_eq!(t.rpm, 0.0, "race_off: rpm should be 0");
        prop_assert_eq!(t.speed_ms, 0.0, "race_off: speed_ms should be 0");
    }
}

// ── 3. Partial/truncated packets → graceful error ───────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    /// Any packet shorter than Sled size (232) must be rejected.
    #[test]
    fn prop_truncated_below_sled_rejected(len in 0usize..SLED_SIZE) {
        let data = vec![0u8; len];
        let adapter = ForzaAdapter::new();
        prop_assert!(adapter.normalize(&data).is_err(),
            "packet of len {} should be rejected", len);
    }

    /// Packets between known sizes must be rejected (not a valid format).
    #[test]
    fn prop_inter_format_sizes_rejected(
        len in (SLED_SIZE + 1)..CARDASH_SIZE,
    ) {
        let data = vec![0u8; len];
        let adapter = ForzaAdapter::new();
        prop_assert!(adapter.normalize(&data).is_err(),
            "packet of len {} (between Sled and CarDash) should be rejected", len);
    }
}

// ── 4. UDP packet boundary conditions ───────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// Off-by-one below each valid packet size must be rejected.
    #[test]
    fn prop_off_by_one_below_rejected(
        format_idx in 0usize..4,
    ) {
        let sizes = [SLED_SIZE, CARDASH_SIZE, FM8_CARDASH_SIZE, FH4_CARDASH_SIZE];
        let size = sizes[format_idx];
        let data = vec![0u8; size - 1];
        let adapter = ForzaAdapter::new();
        prop_assert!(adapter.normalize(&data).is_err(),
            "size {} (off-by-one below {}) should be rejected", size - 1, size);
    }

    /// Oversized packets (beyond all known formats) should be rejected.
    #[test]
    fn prop_oversized_rejected(extra in 1usize..128) {
        let total = FM8_CARDASH_SIZE + extra;
        // Skip exact match sizes
        prop_assume!(total != SLED_SIZE && total != CARDASH_SIZE
            && total != FM8_CARDASH_SIZE && total != FH4_CARDASH_SIZE);
        let data = vec![0u8; total];
        let adapter = ForzaAdapter::new();
        prop_assert!(adapter.normalize(&data).is_err(),
            "oversized buffer of {} must be rejected", total);
    }

    /// Exact valid sizes must parse (smoke test).
    #[test]
    fn prop_exact_sled_parses(
        _dummy in 0u8..1u8,
    ) {
        let data = make_sled(1, 5000.0, 10000.0, (20.0, 0.0, 0.0));
        let adapter = ForzaAdapter::new();
        let result = adapter.normalize(&data);
        prop_assert!(result.is_ok(), "exact sled must parse");
        if let Ok(t) = result {
            prop_assert!((t.rpm - 5000.0).abs() < 0.1);
        }
    }

    /// Maximum-size packet (512 bytes, typical UDP buffer) must not panic.
    #[test]
    fn prop_max_udp_buffer_no_panic(
        data in proptest::collection::vec(any::<u8>(), 512..=512)
    ) {
        let adapter = ForzaAdapter::new();
        let _ = adapter.normalize(&data);
    }
}

// ── 5. Deterministic normalization across repeated calls ─────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// Multiple consecutive normalizations of the same packet produce
    /// identical NormalizedTelemetry fields (determinism).
    #[test]
    fn prop_repeated_normalize_deterministic(
        rpm in 0.0f32..15000.0,
        vel in 0.0f32..80.0,
        count in 2usize..10,
    ) {
        let adapter = ForzaAdapter::new();
        let data = make_sled(1, rpm, 18000.0, (vel, 0.0, 0.0));
        let first = adapter.normalize(&data)
            .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        for _ in 1..count {
            let t = adapter.normalize(&data)
                .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
            prop_assert!((t.rpm - first.rpm).abs() < f32::EPSILON,
                "rpm not deterministic: {} vs {}", t.rpm, first.rpm);
            prop_assert!((t.speed_ms - first.speed_ms).abs() < f32::EPSILON,
                "speed_ms not deterministic: {} vs {}", t.speed_ms, first.speed_ms);
        }
    }
}

// ── Extreme value tests ─────────────────────────────────────────────────────

#[test]
fn extreme_nan_in_all_sled_fields() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = vec![0u8; SLED_SIZE];
    write_i32(&mut data, OFF_IS_RACE_ON, 1);
    let nan_bytes = f32::NAN.to_le_bytes();
    for offset in (4..SLED_SIZE).step_by(4) {
        if offset + 4 <= SLED_SIZE {
            data[offset..offset + 4].copy_from_slice(&nan_bytes);
        }
    }
    let adapter = ForzaAdapter::new();
    // Must not panic
    let _ = adapter.normalize(&data);
    Ok(())
}

#[test]
fn extreme_infinity_in_all_sled_fields() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = vec![0u8; SLED_SIZE];
    write_i32(&mut data, OFF_IS_RACE_ON, 1);
    let inf_bytes = f32::INFINITY.to_le_bytes();
    for offset in (4..SLED_SIZE).step_by(4) {
        if offset + 4 <= SLED_SIZE {
            data[offset..offset + 4].copy_from_slice(&inf_bytes);
        }
    }
    let adapter = ForzaAdapter::new();
    let _ = adapter.normalize(&data);
    Ok(())
}

#[test]
fn deterministic_normalization() -> Result<(), Box<dyn std::error::Error>> {
    let adapter = ForzaAdapter::new();
    let data = make_cardash(1, 5000.0, 10000.0, (25.0, 0.0, 0.0), 200, 50, 0, 4, 30);
    let a = adapter.normalize(&data)?;
    let b = adapter.normalize(&data)?;
    assert!((a.rpm - b.rpm).abs() < f32::EPSILON);
    assert!((a.speed_ms - b.speed_ms).abs() < f32::EPSILON);
    assert!((a.throttle - b.throttle).abs() < f32::EPSILON);
    assert!((a.brake - b.brake).abs() < f32::EPSILON);
    assert_eq!(a.gear, b.gear);
    Ok(())
}
