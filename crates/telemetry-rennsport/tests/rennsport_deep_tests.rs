//! Extended deep tests for the Rennsport telemetry adapter.
//!
//! Covers connection handling, data normalization edge cases, field isolation,
//! boundary values, multi-gear scenarios, packet structure integrity,
//! and combined real-world scenarios.

use racing_wheel_telemetry_rennsport::{RennsportAdapter, TelemetryAdapter};
use std::time::Duration;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const MIN_PACKET_SIZE: usize = 24;
const IDENTIFIER: u8 = 0x52; // 'R'
const OFF_IDENTIFIER: usize = 0;
const OFF_SPEED_KMH: usize = 4;
const OFF_RPM: usize = 8;
const OFF_GEAR: usize = 12;
const OFF_FFB_SCALAR: usize = 16;
const OFF_SLIP_RATIO: usize = 20;

fn make_packet(speed_kmh: f32, rpm: f32, gear: i8, ffb: f32, slip: f32) -> Vec<u8> {
    let mut data = vec![0u8; MIN_PACKET_SIZE];
    data[OFF_IDENTIFIER] = IDENTIFIER;
    data[OFF_SPEED_KMH..OFF_SPEED_KMH + 4].copy_from_slice(&speed_kmh.to_le_bytes());
    data[OFF_RPM..OFF_RPM + 4].copy_from_slice(&rpm.to_le_bytes());
    data[OFF_GEAR] = gear as u8;
    data[OFF_FFB_SCALAR..OFF_FFB_SCALAR + 4].copy_from_slice(&ffb.to_le_bytes());
    data[OFF_SLIP_RATIO..OFF_SLIP_RATIO + 4].copy_from_slice(&slip.to_le_bytes());
    data
}

// ── Connection handling ──────────────────────────────────────────────────────

#[test]
fn adapter_default_port_is_9000() -> TestResult {
    let adapter = RennsportAdapter::new();
    // We verify the adapter can be built; port is internal but with_port works.
    assert_eq!(adapter.game_id(), "rennsport");
    Ok(())
}

#[test]
fn adapter_with_port_override() -> TestResult {
    let adapter = RennsportAdapter::new().with_port(12345);
    assert_eq!(adapter.game_id(), "rennsport");
    assert_eq!(adapter.expected_update_rate(), Duration::from_millis(16));
    Ok(())
}

#[test]
fn adapter_default_trait() -> TestResult {
    let a = RennsportAdapter::new();
    let b = RennsportAdapter::default();
    assert_eq!(a.game_id(), b.game_id());
    assert_eq!(a.expected_update_rate(), b.expected_update_rate());
    Ok(())
}

#[test]
fn adapter_update_rate_is_60hz() -> TestResult {
    let adapter = RennsportAdapter::new();
    assert_eq!(adapter.expected_update_rate(), Duration::from_millis(16));
    Ok(())
}

// ── Packet rejection: size boundaries ────────────────────────────────────────

#[test]
fn reject_zero_bytes() -> TestResult {
    let adapter = RennsportAdapter::new();
    assert!(adapter.normalize(&[]).is_err());
    Ok(())
}

#[test]
fn reject_1_byte() -> TestResult {
    let adapter = RennsportAdapter::new();
    assert!(adapter.normalize(&[IDENTIFIER]).is_err());
    Ok(())
}

#[test]
fn reject_every_size_below_minimum() -> TestResult {
    let adapter = RennsportAdapter::new();
    for size in 0..MIN_PACKET_SIZE {
        let data = vec![IDENTIFIER; size];
        assert!(
            adapter.normalize(&data).is_err(),
            "size {size} should be rejected"
        );
    }
    Ok(())
}

#[test]
fn accept_exactly_minimum_size() -> TestResult {
    let adapter = RennsportAdapter::new();
    let data = make_packet(0.0, 0.0, 0, 0.0, 0.0);
    assert_eq!(data.len(), MIN_PACKET_SIZE);
    let _t = adapter.normalize(&data)?;
    Ok(())
}

#[test]
fn accept_oversized_with_trailing_junk() -> TestResult {
    let adapter = RennsportAdapter::new();
    let mut data = make_packet(100.0, 5000.0, 3, 0.5, 0.1);
    data.extend_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE]);
    let t = adapter.normalize(&data)?;
    assert!((t.speed_ms - 100.0 / 3.6).abs() < 0.1);
    Ok(())
}

// ── Identifier validation ────────────────────────────────────────────────────

#[test]
fn reject_zero_identifier() -> TestResult {
    let adapter = RennsportAdapter::new();
    let mut data = make_packet(0.0, 0.0, 0, 0.0, 0.0);
    data[OFF_IDENTIFIER] = 0x00;
    assert!(adapter.normalize(&data).is_err());
    Ok(())
}

#[test]
fn reject_every_wrong_identifier_byte() -> TestResult {
    let adapter = RennsportAdapter::new();
    for id in 0..=255u8 {
        if id == IDENTIFIER {
            continue;
        }
        let mut data = make_packet(0.0, 0.0, 0, 0.0, 0.0);
        data[OFF_IDENTIFIER] = id;
        assert!(
            adapter.normalize(&data).is_err(),
            "identifier 0x{id:02X} should be rejected"
        );
    }
    Ok(())
}

// ── Speed conversion: data normalization ─────────────────────────────────────

#[test]
fn speed_conversion_table() -> TestResult {
    let adapter = RennsportAdapter::new();
    let cases: &[(f32, f32)] = &[
        (0.0, 0.0),
        (1.0, 1.0 / 3.6),
        (3.6, 1.0),
        (36.0, 10.0),
        (72.0, 20.0),
        (108.0, 30.0),
        (180.0, 50.0),
        (252.0, 70.0),
        (288.0, 80.0),
        (360.0, 100.0),
    ];
    for &(kmh, expected_ms) in cases {
        let data = make_packet(kmh, 0.0, 0, 0.0, 0.0);
        let t = adapter.normalize(&data)?;
        assert!(
            (t.speed_ms - expected_ms).abs() < 0.02,
            "speed {kmh} km/h → expected {expected_ms} m/s, got {}",
            t.speed_ms
        );
    }
    Ok(())
}

#[test]
fn negative_speed_becomes_zero() -> TestResult {
    let adapter = RennsportAdapter::new();
    let data = make_packet(-100.0, 0.0, 0, 0.0, 0.0);
    let t = adapter.normalize(&data)?;
    assert_eq!(t.speed_ms, 0.0, "negative speed → 0");
    Ok(())
}

#[test]
fn very_high_speed_preserved() -> TestResult {
    let adapter = RennsportAdapter::new();
    let data = make_packet(500.0, 0.0, 0, 0.0, 0.0);
    let t = adapter.normalize(&data)?;
    assert!((t.speed_ms - 500.0 / 3.6).abs() < 0.1);
    Ok(())
}

// ── RPM normalization ────────────────────────────────────────────────────────

#[test]
fn rpm_preserved_exactly() -> TestResult {
    let adapter = RennsportAdapter::new();
    let data = make_packet(0.0, 7500.0, 0, 0.0, 0.0);
    let t = adapter.normalize(&data)?;
    assert!((t.rpm - 7500.0).abs() < 0.1);
    Ok(())
}

#[test]
fn negative_rpm_clamped_to_zero() -> TestResult {
    let adapter = RennsportAdapter::new();
    let data = make_packet(0.0, -500.0, 0, 0.0, 0.0);
    let t = adapter.normalize(&data)?;
    assert_eq!(t.rpm, 0.0, "negative RPM → 0");
    Ok(())
}

#[test]
fn very_high_rpm_preserved() -> TestResult {
    let adapter = RennsportAdapter::new();
    let data = make_packet(0.0, 20_000.0, 0, 0.0, 0.0);
    let t = adapter.normalize(&data)?;
    assert!((t.rpm - 20_000.0).abs() < 1.0);
    Ok(())
}

// ── Gear handling ────────────────────────────────────────────────────────────

#[test]
fn gear_all_valid_values() -> TestResult {
    let adapter = RennsportAdapter::new();
    for gear in -1i8..=8 {
        let data = make_packet(50.0, 5000.0, gear, 0.0, 0.0);
        let t = adapter.normalize(&data)?;
        assert_eq!(t.gear, gear, "gear {gear}");
    }
    Ok(())
}

#[test]
fn gear_reverse_at_low_speed() -> TestResult {
    let adapter = RennsportAdapter::new();
    let data = make_packet(5.0, 1200.0, -1, -0.3, 0.0);
    let t = adapter.normalize(&data)?;
    assert_eq!(t.gear, -1);
    assert!((t.speed_ms - 5.0 / 3.6).abs() < 0.1);
    Ok(())
}

#[test]
fn gear_neutral_stationary() -> TestResult {
    let adapter = RennsportAdapter::new();
    let data = make_packet(0.0, 800.0, 0, 0.0, 0.0);
    let t = adapter.normalize(&data)?;
    assert_eq!(t.gear, 0);
    assert_eq!(t.speed_ms, 0.0);
    Ok(())
}

// ── FFB scalar clamping ─────────────────────────────────────────────────────

#[test]
fn ffb_within_range_passed_through() -> TestResult {
    let adapter = RennsportAdapter::new();
    let test_values = [-1.0f32, -0.5, 0.0, 0.5, 1.0];
    for &ffb in &test_values {
        let data = make_packet(0.0, 0.0, 0, ffb, 0.0);
        let t = adapter.normalize(&data)?;
        assert!((t.ffb_scalar - ffb).abs() < 0.001, "ffb={ffb}");
    }
    Ok(())
}

#[test]
fn ffb_positive_overflow_clamped_to_1() -> TestResult {
    let adapter = RennsportAdapter::new();
    for &ffb in &[1.1f32, 2.0, 10.0, 100.0, f32::MAX] {
        let data = make_packet(0.0, 0.0, 0, ffb, 0.0);
        let t = adapter.normalize(&data)?;
        assert!(
            (t.ffb_scalar - 1.0).abs() < 0.001,
            "ffb={ffb} should clamp to 1.0, got {}",
            t.ffb_scalar
        );
    }
    Ok(())
}

#[test]
fn ffb_negative_overflow_clamped_to_neg1() -> TestResult {
    let adapter = RennsportAdapter::new();
    for &ffb in &[-1.1f32, -2.0, -10.0, -100.0, f32::MIN] {
        let data = make_packet(0.0, 0.0, 0, ffb, 0.0);
        let t = adapter.normalize(&data)?;
        assert!(
            (t.ffb_scalar - (-1.0)).abs() < 0.001,
            "ffb={ffb} should clamp to -1.0, got {}",
            t.ffb_scalar
        );
    }
    Ok(())
}

// ── Slip ratio clamping ─────────────────────────────────────────────────────

#[test]
fn slip_within_range_passed_through() -> TestResult {
    let adapter = RennsportAdapter::new();
    let test_values = [0.0f32, 0.1, 0.5, 0.9, 1.0];
    for &slip in &test_values {
        let data = make_packet(0.0, 0.0, 0, 0.0, slip);
        let t = adapter.normalize(&data)?;
        assert!((t.slip_ratio - slip).abs() < 0.001, "slip={slip}");
    }
    Ok(())
}

#[test]
fn slip_negative_clamped_to_zero() -> TestResult {
    let adapter = RennsportAdapter::new();
    let data = make_packet(0.0, 0.0, 0, 0.0, -0.5);
    let t = adapter.normalize(&data)?;
    assert_eq!(t.slip_ratio, 0.0, "negative slip → 0");
    Ok(())
}

#[test]
fn slip_above_one_clamped() -> TestResult {
    let adapter = RennsportAdapter::new();
    let data = make_packet(0.0, 0.0, 0, 0.0, 5.0);
    let t = adapter.normalize(&data)?;
    assert!((t.slip_ratio - 1.0).abs() < 0.001, "slip>1 → 1.0");
    Ok(())
}

// ── NaN / Infinity handling ──────────────────────────────────────────────────

#[test]
fn nan_in_all_float_fields() -> TestResult {
    let adapter = RennsportAdapter::new();
    let mut data = make_packet(0.0, 0.0, 0, 0.0, 0.0);
    // Write NaN into every float field
    data[OFF_SPEED_KMH..OFF_SPEED_KMH + 4].copy_from_slice(&f32::NAN.to_le_bytes());
    data[OFF_RPM..OFF_RPM + 4].copy_from_slice(&f32::NAN.to_le_bytes());
    data[OFF_FFB_SCALAR..OFF_FFB_SCALAR + 4].copy_from_slice(&f32::NAN.to_le_bytes());
    data[OFF_SLIP_RATIO..OFF_SLIP_RATIO + 4].copy_from_slice(&f32::NAN.to_le_bytes());

    let t = adapter.normalize(&data)?;
    assert!(t.speed_ms.is_finite(), "NaN speed → finite");
    assert!(t.rpm.is_finite(), "NaN rpm → finite");
    assert!(t.ffb_scalar.is_finite(), "NaN ffb → finite");
    assert!(t.slip_ratio.is_finite(), "NaN slip → finite");
    Ok(())
}

#[test]
fn infinity_in_all_float_fields() -> TestResult {
    let adapter = RennsportAdapter::new();
    let mut data = make_packet(0.0, 0.0, 0, 0.0, 0.0);
    data[OFF_SPEED_KMH..OFF_SPEED_KMH + 4].copy_from_slice(&f32::INFINITY.to_le_bytes());
    data[OFF_RPM..OFF_RPM + 4].copy_from_slice(&f32::INFINITY.to_le_bytes());
    data[OFF_FFB_SCALAR..OFF_FFB_SCALAR + 4].copy_from_slice(&f32::INFINITY.to_le_bytes());
    data[OFF_SLIP_RATIO..OFF_SLIP_RATIO + 4].copy_from_slice(&f32::INFINITY.to_le_bytes());

    let t = adapter.normalize(&data)?;
    assert!(t.speed_ms.is_finite(), "Inf speed → finite");
    assert!(t.rpm.is_finite(), "Inf rpm → finite");
    assert!(t.ffb_scalar.is_finite(), "Inf ffb → finite");
    assert!(t.slip_ratio.is_finite(), "Inf slip → finite");
    Ok(())
}

#[test]
fn neg_infinity_in_all_float_fields() -> TestResult {
    let adapter = RennsportAdapter::new();
    let mut data = make_packet(0.0, 0.0, 0, 0.0, 0.0);
    data[OFF_SPEED_KMH..OFF_SPEED_KMH + 4].copy_from_slice(&f32::NEG_INFINITY.to_le_bytes());
    data[OFF_RPM..OFF_RPM + 4].copy_from_slice(&f32::NEG_INFINITY.to_le_bytes());
    data[OFF_FFB_SCALAR..OFF_FFB_SCALAR + 4].copy_from_slice(&f32::NEG_INFINITY.to_le_bytes());
    data[OFF_SLIP_RATIO..OFF_SLIP_RATIO + 4].copy_from_slice(&f32::NEG_INFINITY.to_le_bytes());

    let t = adapter.normalize(&data)?;
    assert!(t.speed_ms.is_finite(), "-Inf speed → finite");
    assert!(t.rpm.is_finite(), "-Inf rpm → finite");
    assert!(t.ffb_scalar.is_finite(), "-Inf ffb → finite");
    assert!(t.slip_ratio.is_finite(), "-Inf slip → finite");
    Ok(())
}

// ── Field isolation ──────────────────────────────────────────────────────────

#[test]
fn changing_speed_only_does_not_affect_other_fields() -> TestResult {
    let adapter = RennsportAdapter::new();
    let base = make_packet(100.0, 5000.0, 3, 0.5, 0.2);
    let modified = make_packet(200.0, 5000.0, 3, 0.5, 0.2);

    let t_base = adapter.normalize(&base)?;
    let t_mod = adapter.normalize(&modified)?;

    assert_ne!(t_base.speed_ms, t_mod.speed_ms, "speed should differ");
    assert_eq!(t_base.rpm, t_mod.rpm, "rpm unchanged");
    assert_eq!(t_base.gear, t_mod.gear, "gear unchanged");
    assert_eq!(t_base.ffb_scalar, t_mod.ffb_scalar, "ffb unchanged");
    assert_eq!(t_base.slip_ratio, t_mod.slip_ratio, "slip unchanged");
    Ok(())
}

#[test]
fn changing_gear_only() -> TestResult {
    let adapter = RennsportAdapter::new();
    let base = make_packet(100.0, 5000.0, 3, 0.5, 0.2);
    let modified = make_packet(100.0, 5000.0, 5, 0.5, 0.2);

    let t_base = adapter.normalize(&base)?;
    let t_mod = adapter.normalize(&modified)?;

    assert_eq!(t_base.speed_ms, t_mod.speed_ms, "speed unchanged");
    assert_ne!(t_base.gear, t_mod.gear, "gear should differ");
    Ok(())
}

// ── Combined scenarios ───────────────────────────────────────────────────────

#[test]
fn race_start_acceleration() -> TestResult {
    let adapter = RennsportAdapter::new();

    // Standing start
    let start = make_packet(0.0, 7000.0, 1, 0.0, 0.8);
    let t = adapter.normalize(&start)?;
    assert_eq!(t.speed_ms, 0.0);
    assert_eq!(t.gear, 1);
    assert!((t.slip_ratio - 0.8).abs() < 0.001, "high slip at launch");

    // First gear acceleration
    let accel = make_packet(60.0, 8500.0, 1, 0.4, 0.3);
    let t = adapter.normalize(&accel)?;
    assert!((t.speed_ms - 60.0 / 3.6).abs() < 0.1);
    assert!((t.rpm - 8500.0).abs() < 0.1);

    // Shift to second
    let shift = make_packet(80.0, 5000.0, 2, 0.3, 0.1);
    let t = adapter.normalize(&shift)?;
    assert_eq!(t.gear, 2);
    assert!((t.rpm - 5000.0).abs() < 0.1);

    Ok(())
}

#[test]
fn high_speed_braking_zone() -> TestResult {
    let adapter = RennsportAdapter::new();
    let data = make_packet(280.0, 9500.0, 6, -0.9, 0.4);
    let t = adapter.normalize(&data)?;
    assert!((t.speed_ms - 280.0 / 3.6).abs() < 0.1);
    assert_eq!(t.gear, 6);
    assert!(
        (t.ffb_scalar - (-0.9)).abs() < 0.001,
        "strong negative FFB under braking"
    );
    assert!(
        (t.slip_ratio - 0.4).abs() < 0.001,
        "moderate slip under braking"
    );
    Ok(())
}

#[test]
fn pit_lane_crawl() -> TestResult {
    let adapter = RennsportAdapter::new();
    let data = make_packet(60.0, 2000.0, 2, 0.05, 0.0);
    let t = adapter.normalize(&data)?;
    assert!((t.speed_ms - 60.0 / 3.6).abs() < 0.1);
    assert_eq!(t.gear, 2);
    assert!((t.ffb_scalar - 0.05).abs() < 0.001);
    assert_eq!(t.slip_ratio, 0.0);
    Ok(())
}

// ── Determinism ──────────────────────────────────────────────────────────────

#[test]
fn triple_normalize_identical() -> TestResult {
    let adapter = RennsportAdapter::new();
    let data = make_packet(150.0, 7000.0, 4, 0.6, 0.15);
    let t1 = adapter.normalize(&data)?;
    let t2 = adapter.normalize(&data)?;
    let t3 = adapter.normalize(&data)?;

    assert_eq!(t1.speed_ms, t2.speed_ms);
    assert_eq!(t2.speed_ms, t3.speed_ms);
    assert_eq!(t1.rpm, t2.rpm);
    assert_eq!(t1.gear, t2.gear);
    assert_eq!(t1.ffb_scalar, t2.ffb_scalar);
    assert_eq!(t1.slip_ratio, t2.slip_ratio);
    Ok(())
}

// ── Default fields are zeroed ────────────────────────────────────────────────

#[test]
fn non_protocol_fields_default_to_zero() -> TestResult {
    let adapter = RennsportAdapter::new();
    let data = make_packet(100.0, 5000.0, 3, 0.5, 0.1);
    let t = adapter.normalize(&data)?;

    // Fields not in the Rennsport protocol should be default zero
    assert_eq!(t.throttle, 0.0, "throttle not in protocol");
    assert_eq!(t.brake, 0.0, "brake not in protocol");
    assert_eq!(t.clutch, 0.0, "clutch not in protocol");
    assert_eq!(t.steering_angle, 0.0, "steering not in protocol");
    assert_eq!(t.fuel_percent, 0.0, "fuel not in protocol");
    assert_eq!(t.lateral_g, 0.0, "lateral_g not in protocol");
    assert_eq!(t.longitudinal_g, 0.0, "longitudinal_g not in protocol");
    Ok(())
}
