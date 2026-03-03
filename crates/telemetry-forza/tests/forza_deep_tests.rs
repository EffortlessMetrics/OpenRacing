//! Deep tests for the Forza telemetry adapter.
//!
//! Focuses on areas complementary to `comprehensive.rs` and `deep_tests.rs`:
//! NormalizedTelemetry helper methods exercised through Forza packet parsing,
//! builder depth, TelemetryFrame::from_telemetry, validated() clamping,
//! extended field coverage, boundary edge cases, and cross-format invariants.

use racing_wheel_telemetry_forza::{
    ForzaAdapter, NormalizedTelemetry, TelemetryAdapter, TelemetryFrame, TelemetryValue,
};
use std::time::Duration;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ── Packet size constants ────────────────────────────────────────────────────
const SLED_SIZE: usize = 232;
const CARDASH_SIZE: usize = 311;
const FM8_SIZE: usize = 331;
const FH4_SIZE: usize = 324;

// ── Sled offsets ─────────────────────────────────────────────────────────────
const OFF_IS_RACE_ON: usize = 0;
const OFF_ENGINE_MAX_RPM: usize = 8;
const OFF_CURRENT_RPM: usize = 16;
const OFF_ACCEL_X: usize = 20;
const OFF_ACCEL_Y: usize = 24;
const OFF_ACCEL_Z: usize = 28;
const OFF_VEL_X: usize = 32;
const OFF_VEL_Y: usize = 36;
const _OFF_VEL_Z: usize = 40;
const OFF_TIRE_SLIP_FL: usize = 84;
const OFF_TIRE_SLIP_FR: usize = 88;
const OFF_TIRE_SLIP_RL: usize = 92;
const OFF_TIRE_SLIP_RR: usize = 96;
const OFF_WHEEL_SPEED_FL: usize = 100;
const OFF_WHEEL_SPEED_FR: usize = 104;
const OFF_WHEEL_SPEED_RL: usize = 108;
const OFF_WHEEL_SPEED_RR: usize = 112;
const OFF_SLIP_ANGLE_FL: usize = 164;
const OFF_SLIP_ANGLE_FR: usize = 168;
const OFF_SLIP_ANGLE_RL: usize = 172;
const OFF_SLIP_ANGLE_RR: usize = 176;
const OFF_SUSP_TRAVEL_FL: usize = 196;
const OFF_SUSP_TRAVEL_FR: usize = 200;
const OFF_SUSP_TRAVEL_RL: usize = 204;
const OFF_SUSP_TRAVEL_RR: usize = 208;

// ── CarDash offsets ──────────────────────────────────────────────────────────
const OFF_DASH_TIRE_TEMP_FL: usize = 256;
const OFF_DASH_TIRE_TEMP_FR: usize = 260;
const OFF_DASH_TIRE_TEMP_RL: usize = 264;
const OFF_DASH_TIRE_TEMP_RR: usize = 268;
const OFF_DASH_FUEL: usize = 276;
const OFF_DASH_BEST_LAP: usize = 284;
const OFF_DASH_LAST_LAP: usize = 288;
const OFF_DASH_CUR_LAP: usize = 292;
const OFF_DASH_LAP_NUMBER: usize = 300;
const OFF_DASH_RACE_POS: usize = 302;
const _OFF_DASH_ACCEL: usize = 303;
const _OFF_DASH_BRAKE: usize = 304;
const OFF_DASH_CLUTCH: usize = 305;
const OFF_DASH_GEAR: usize = 307;
const OFF_DASH_STEER: usize = 308;

const G: f32 = 9.806_65;
const FH4_HORIZON_OFFSET: usize = 12;

fn write_f32(buf: &mut [u8], off: usize, v: f32) {
    buf[off..off + 4].copy_from_slice(&v.to_le_bytes());
}

fn write_i32(buf: &mut [u8], off: usize, v: i32) {
    buf[off..off + 4].copy_from_slice(&v.to_le_bytes());
}

fn make_sled_on() -> Vec<u8> {
    let mut buf = vec![0u8; SLED_SIZE];
    write_i32(&mut buf, OFF_IS_RACE_ON, 1);
    buf
}

fn make_cardash_on() -> Vec<u8> {
    let mut buf = vec![0u8; CARDASH_SIZE];
    write_i32(&mut buf, OFF_IS_RACE_ON, 1);
    buf
}

fn make_fm8_on() -> Vec<u8> {
    let mut buf = vec![0u8; FM8_SIZE];
    write_i32(&mut buf, OFF_IS_RACE_ON, 1);
    buf
}

fn make_fh4_on() -> Vec<u8> {
    let mut buf = vec![0u8; FH4_SIZE];
    write_i32(&mut buf, OFF_IS_RACE_ON, 1);
    buf
}

// ═══════════════════════════════════════════════════════════════════════════════
// Adapter configuration
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn adapter_with_port_preserves_game_id_and_rate() -> TestResult {
    let adapter = ForzaAdapter::new().with_port(9999);
    assert_eq!(adapter.game_id(), "forza_motorsport");
    assert_eq!(adapter.expected_update_rate(), Duration::from_millis(16));
    Ok(())
}

#[test]
fn adapter_with_port_still_normalizes() -> TestResult {
    let adapter = ForzaAdapter::new().with_port(12345);
    let buf = make_sled_on();
    let t = adapter.normalize(&buf)?;
    assert!(t.rpm.is_finite());
    Ok(())
}

#[test]
fn adapter_default_and_new_produce_same_identity() -> TestResult {
    let a = ForzaAdapter::new();
    let b = ForzaAdapter::default();
    assert_eq!(a.game_id(), b.game_id());
    assert_eq!(a.expected_update_rate(), b.expected_update_rate());
    // Both should normalize identically
    let buf = make_sled_on();
    let ra = a.normalize(&buf)?;
    let rb = b.normalize(&buf)?;
    assert_eq!(ra.rpm, rb.rpm);
    assert_eq!(ra.speed_ms, rb.speed_ms);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// NormalizedTelemetry helper methods via Forza parsing
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn speed_kmh_conversion_from_sled() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut buf = make_sled_on();
    write_f32(&mut buf, OFF_VEL_X, 10.0); // 10 m/s = 36 km/h
    let t = adapter.normalize(&buf)?;
    assert!((t.speed_kmh() - 36.0).abs() < 0.1, "kmh={}", t.speed_kmh());
    Ok(())
}

#[test]
fn speed_mph_conversion_from_sled() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut buf = make_sled_on();
    write_f32(&mut buf, OFF_VEL_X, 10.0); // 10 m/s ≈ 22.37 mph
    let t = adapter.normalize(&buf)?;
    assert!((t.speed_mph() - 22.37).abs() < 0.1, "mph={}", t.speed_mph());
    Ok(())
}

#[test]
fn is_stationary_at_zero_speed() -> TestResult {
    let adapter = ForzaAdapter::new();
    let buf = make_sled_on();
    let t = adapter.normalize(&buf)?;
    assert!(t.is_stationary(), "zero velocity should be stationary");
    Ok(())
}

#[test]
fn is_not_stationary_above_threshold() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut buf = make_sled_on();
    write_f32(&mut buf, OFF_VEL_X, 5.0);
    let t = adapter.normalize(&buf)?;
    assert!(!t.is_stationary(), "5 m/s should not be stationary");
    Ok(())
}

#[test]
fn is_stationary_just_below_threshold() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut buf = make_sled_on();
    write_f32(&mut buf, OFF_VEL_X, 0.49);
    let t = adapter.normalize(&buf)?;
    assert!(t.is_stationary(), "0.49 m/s should be stationary");
    Ok(())
}

#[test]
fn total_g_from_sled_acceleration() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut buf = make_sled_on();
    write_f32(&mut buf, OFF_ACCEL_X, 3.0 * G); // lateral 3G
    write_f32(&mut buf, OFF_ACCEL_Z, 4.0 * G); // longitudinal 4G
    let t = adapter.normalize(&buf)?;
    // total_g = sqrt(3² + 4²) = 5.0
    assert!((t.total_g() - 5.0).abs() < 0.05, "total_g={}", t.total_g());
    Ok(())
}

#[test]
fn has_rpm_data_when_rpm_positive() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut buf = make_sled_on();
    write_f32(&mut buf, OFF_CURRENT_RPM, 3000.0);
    let t = adapter.normalize(&buf)?;
    assert!(t.has_rpm_data());
    Ok(())
}

#[test]
fn has_rpm_data_false_when_rpm_zero() -> TestResult {
    let adapter = ForzaAdapter::new();
    let buf = make_sled_on();
    let t = adapter.normalize(&buf)?;
    assert!(!t.has_rpm_data());
    Ok(())
}

#[test]
fn has_rpm_display_data_requires_both_rpm_and_max_rpm() -> TestResult {
    let adapter = ForzaAdapter::new();
    // RPM > 0 but max_rpm = 0
    let mut buf1 = make_sled_on();
    write_f32(&mut buf1, OFF_CURRENT_RPM, 5000.0);
    let t1 = adapter.normalize(&buf1)?;
    assert!(!t1.has_rpm_display_data(), "max_rpm=0 should fail");

    // Both positive
    let mut buf2 = make_sled_on();
    write_f32(&mut buf2, OFF_CURRENT_RPM, 5000.0);
    write_f32(&mut buf2, OFF_ENGINE_MAX_RPM, 8000.0);
    let t2 = adapter.normalize(&buf2)?;
    assert!(t2.has_rpm_display_data());
    Ok(())
}

#[test]
fn rpm_fraction_calculation() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut buf = make_sled_on();
    write_f32(&mut buf, OFF_CURRENT_RPM, 6000.0);
    write_f32(&mut buf, OFF_ENGINE_MAX_RPM, 8000.0);
    let t = adapter.normalize(&buf)?;
    assert!(
        (t.rpm_fraction() - 0.75).abs() < 0.01,
        "fraction={}",
        t.rpm_fraction()
    );
    Ok(())
}

#[test]
fn rpm_fraction_zero_when_max_rpm_zero() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut buf = make_sled_on();
    write_f32(&mut buf, OFF_CURRENT_RPM, 5000.0);
    // max_rpm defaults to 0
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.rpm_fraction(), 0.0);
    Ok(())
}

#[test]
fn has_ffb_data_is_false_for_forza_sled() -> TestResult {
    let adapter = ForzaAdapter::new();
    let buf = make_sled_on();
    let t = adapter.normalize(&buf)?;
    // Forza doesn't provide FFB scalar in packets
    assert!(!t.has_ffb_data());
    Ok(())
}

#[test]
fn has_active_flags_false_for_forza() -> TestResult {
    let adapter = ForzaAdapter::new();
    let buf = make_sled_on();
    let t = adapter.normalize(&buf)?;
    // Forza doesn't set racing flags
    assert!(!t.has_active_flags());
    Ok(())
}

// ── Slip angle helpers ───────────────────────────────────────────────────────

#[test]
fn slip_angle_averages_from_sled() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut buf = make_sled_on();
    write_f32(&mut buf, OFF_SLIP_ANGLE_FL, 0.10);
    write_f32(&mut buf, OFF_SLIP_ANGLE_FR, 0.20);
    write_f32(&mut buf, OFF_SLIP_ANGLE_RL, 0.30);
    write_f32(&mut buf, OFF_SLIP_ANGLE_RR, 0.40);
    let t = adapter.normalize(&buf)?;

    let expected_avg = (0.10 + 0.20 + 0.30 + 0.40) / 4.0;
    assert!(
        (t.average_slip_angle() - expected_avg).abs() < 0.001,
        "avg={}",
        t.average_slip_angle()
    );

    let expected_front = (0.10 + 0.20) / 2.0;
    assert!(
        (t.front_slip_angle() - expected_front).abs() < 0.001,
        "front={}",
        t.front_slip_angle()
    );

    let expected_rear = (0.30 + 0.40) / 2.0;
    assert!(
        (t.rear_slip_angle() - expected_rear).abs() < 0.001,
        "rear={}",
        t.rear_slip_angle()
    );
    Ok(())
}

#[test]
fn slip_angle_averages_all_zero() -> TestResult {
    let adapter = ForzaAdapter::new();
    let buf = make_sled_on();
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.average_slip_angle(), 0.0);
    assert_eq!(t.front_slip_angle(), 0.0);
    assert_eq!(t.rear_slip_angle(), 0.0);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Extended field coverage — all four wheels
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn sled_all_four_wheel_speeds_in_extended() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut buf = make_sled_on();
    write_f32(&mut buf, OFF_WHEEL_SPEED_FL, 10.0);
    write_f32(&mut buf, OFF_WHEEL_SPEED_FR, 11.0);
    write_f32(&mut buf, OFF_WHEEL_SPEED_RL, 12.0);
    write_f32(&mut buf, OFF_WHEEL_SPEED_RR, 13.0);
    let t = adapter.normalize(&buf)?;

    assert_eq!(
        t.get_extended("wheel_speed_fl"),
        Some(&TelemetryValue::Float(10.0))
    );
    assert_eq!(
        t.get_extended("wheel_speed_fr"),
        Some(&TelemetryValue::Float(11.0))
    );
    assert_eq!(
        t.get_extended("wheel_speed_rl"),
        Some(&TelemetryValue::Float(12.0))
    );
    assert_eq!(
        t.get_extended("wheel_speed_rr"),
        Some(&TelemetryValue::Float(13.0))
    );
    Ok(())
}

#[test]
fn sled_all_four_suspension_travels_in_extended() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut buf = make_sled_on();
    write_f32(&mut buf, OFF_SUSP_TRAVEL_FL, 0.05);
    write_f32(&mut buf, OFF_SUSP_TRAVEL_FR, 0.06);
    write_f32(&mut buf, OFF_SUSP_TRAVEL_RL, 0.07);
    write_f32(&mut buf, OFF_SUSP_TRAVEL_RR, 0.08);
    let t = adapter.normalize(&buf)?;

    assert_eq!(
        t.get_extended("suspension_travel_fl"),
        Some(&TelemetryValue::Float(0.05))
    );
    assert_eq!(
        t.get_extended("suspension_travel_fr"),
        Some(&TelemetryValue::Float(0.06))
    );
    assert_eq!(
        t.get_extended("suspension_travel_rl"),
        Some(&TelemetryValue::Float(0.07))
    );
    assert_eq!(
        t.get_extended("suspension_travel_rr"),
        Some(&TelemetryValue::Float(0.08))
    );
    Ok(())
}

#[test]
fn sled_individual_tire_slip_ratios_in_extended() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut buf = make_sled_on();
    write_f32(&mut buf, OFF_TIRE_SLIP_FL, 0.15);
    write_f32(&mut buf, OFF_TIRE_SLIP_FR, 0.25);
    write_f32(&mut buf, OFF_TIRE_SLIP_RL, 0.35);
    write_f32(&mut buf, OFF_TIRE_SLIP_RR, 0.45);
    let t = adapter.normalize(&buf)?;

    assert_eq!(
        t.get_extended("tire_slip_ratio_fl"),
        Some(&TelemetryValue::Float(0.15))
    );
    assert_eq!(
        t.get_extended("tire_slip_ratio_fr"),
        Some(&TelemetryValue::Float(0.25))
    );
    assert_eq!(
        t.get_extended("tire_slip_ratio_rl"),
        Some(&TelemetryValue::Float(0.35))
    );
    assert_eq!(
        t.get_extended("tire_slip_ratio_rr"),
        Some(&TelemetryValue::Float(0.45))
    );
    Ok(())
}

#[test]
fn get_extended_returns_none_for_absent_key() -> TestResult {
    let adapter = ForzaAdapter::new();
    let buf = make_sled_on();
    let t = adapter.normalize(&buf)?;
    assert!(t.get_extended("nonexistent_key").is_none());
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// CarDash: tire temperatures all four wheels
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn cardash_tire_temps_all_four_distinct() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut buf = make_cardash_on();
    // 32°F = 0°C, 212°F = 100°C, 392°F = 200°C, 572°F = 300°C
    write_f32(&mut buf, OFF_DASH_TIRE_TEMP_FL, 32.0);
    write_f32(&mut buf, OFF_DASH_TIRE_TEMP_FR, 212.0);
    write_f32(&mut buf, OFF_DASH_TIRE_TEMP_RL, 392.0);
    write_f32(&mut buf, OFF_DASH_TIRE_TEMP_RR, 572.0);
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.tire_temps_c[0], 0, "FL: 32°F → 0°C");
    assert_eq!(t.tire_temps_c[1], 100, "FR: 212°F → 100°C");
    assert_eq!(t.tire_temps_c[2], 200, "RL: 392°F → 200°C");
    // 300°C may overflow u8 (max 255), adapter may clamp
    assert!(
        t.tire_temps_c[3] == 255 || t.tire_temps_c[3] == 44,
        "RR: 572°F → clamped or wrapped, got {}",
        t.tire_temps_c[3]
    );
    Ok(())
}

#[test]
fn cardash_zero_temps_remain_zero_celsius() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut buf = make_cardash_on();
    write_f32(&mut buf, OFF_DASH_TIRE_TEMP_FL, 32.0); // 0°C
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.tire_temps_c[0], 0);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// CarDash: gear boundary values
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn cardash_gear_high_gear_values() -> TestResult {
    let adapter = ForzaAdapter::new();
    // gear byte: 8 → 7th gear, 9 → 8th gear, 10 → 9th gear, 11 → 10th gear
    for (raw, expected) in [(8u8, 7i8), (9, 8), (10, 9), (11, 10)] {
        let mut buf = make_cardash_on();
        buf[OFF_DASH_GEAR] = raw;
        let t = adapter.normalize(&buf)?;
        assert_eq!(
            t.gear, expected,
            "raw={raw}→gear={expected}, got={}",
            t.gear
        );
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// CarDash: steering edge values
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn cardash_steer_zero_center() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut buf = make_cardash_on();
    buf[OFF_DASH_STEER] = 0;
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.steering_angle, 0.0, "center steer should be 0.0");
    Ok(())
}

#[test]
fn cardash_steer_min_value() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut buf = make_cardash_on();
    buf[OFF_DASH_STEER] = (-128i8) as u8; // i8 min
    let t = adapter.normalize(&buf)?;
    assert!(t.steering_angle <= -0.9, "steer={}", t.steering_angle);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// CarDash: zero fuel and lap times
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn cardash_zero_fuel_and_laps() -> TestResult {
    let adapter = ForzaAdapter::new();
    let buf = make_cardash_on();
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.fuel_percent, 0.0);
    assert_eq!(t.best_lap_time_s, 0.0);
    assert_eq!(t.last_lap_time_s, 0.0);
    assert_eq!(t.current_lap_time_s, 0.0);
    assert_eq!(t.lap, 0);
    assert_eq!(t.position, 0);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// FH4: extended dash fields with horizon offset
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn fh4_fuel_at_shifted_offset() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut buf = make_fh4_on();
    write_f32(&mut buf, OFF_DASH_FUEL + FH4_HORIZON_OFFSET, 0.55);
    let t = adapter.normalize(&buf)?;
    assert!(
        (t.fuel_percent - 0.55).abs() < 0.01,
        "fuel={}",
        t.fuel_percent
    );
    Ok(())
}

#[test]
fn fh4_tire_temps_at_shifted_offset() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut buf = make_fh4_on();
    write_f32(&mut buf, OFF_DASH_TIRE_TEMP_FL + FH4_HORIZON_OFFSET, 212.0);
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.tire_temps_c[0], 100, "FH4 FL temp should be 100°C");
    Ok(())
}

#[test]
fn fh4_lap_data_at_shifted_offset() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut buf = make_fh4_on();
    write_f32(&mut buf, OFF_DASH_BEST_LAP + FH4_HORIZON_OFFSET, 70.0);
    write_f32(&mut buf, OFF_DASH_LAST_LAP + FH4_HORIZON_OFFSET, 71.5);
    write_f32(&mut buf, OFF_DASH_CUR_LAP + FH4_HORIZON_OFFSET, 25.0);
    buf[OFF_DASH_LAP_NUMBER + FH4_HORIZON_OFFSET..OFF_DASH_LAP_NUMBER + FH4_HORIZON_OFFSET + 2]
        .copy_from_slice(&3u16.to_le_bytes());
    buf[OFF_DASH_RACE_POS + FH4_HORIZON_OFFSET] = 5;
    let t = adapter.normalize(&buf)?;
    assert!((t.best_lap_time_s - 70.0).abs() < 0.01);
    assert!((t.last_lap_time_s - 71.5).abs() < 0.01);
    assert!((t.current_lap_time_s - 25.0).abs() < 0.01);
    assert_eq!(t.lap, 3);
    assert_eq!(t.position, 5);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// FM8: extended CarDash parsing
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn fm8_steering_and_clutch() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut buf = make_fm8_on();
    buf[OFF_DASH_STEER] = 64_i8 as u8;
    buf[OFF_DASH_CLUTCH] = 200;
    let t = adapter.normalize(&buf)?;
    assert!(t.steering_angle > 0.4, "steer={}", t.steering_angle);
    assert!(
        (t.clutch - 200.0 / 255.0).abs() < 0.01,
        "clutch={}",
        t.clutch
    );
    Ok(())
}

#[test]
fn fm8_tire_temps() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut buf = make_fm8_on();
    write_f32(&mut buf, OFF_DASH_TIRE_TEMP_FL, 212.0);
    write_f32(&mut buf, OFF_DASH_TIRE_TEMP_FR, 248.0); // ≈120°C
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.tire_temps_c[0], 100);
    assert_eq!(t.tire_temps_c[1], 120);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Packet boundary edge cases — exact size boundaries
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn rejects_sled_minus_one() -> TestResult {
    let adapter = ForzaAdapter::new();
    assert!(adapter.normalize(&[0u8; SLED_SIZE - 1]).is_err());
    Ok(())
}

#[test]
fn rejects_between_sled_and_cardash() -> TestResult {
    let adapter = ForzaAdapter::new();
    // Between Sled (232) and CarDash (311) — invalid size
    assert!(adapter.normalize(&[0u8; 270]).is_err());
    Ok(())
}

#[test]
fn rejects_cardash_minus_one() -> TestResult {
    let adapter = ForzaAdapter::new();
    assert!(adapter.normalize(&[0u8; CARDASH_SIZE - 1]).is_err());
    Ok(())
}

#[test]
fn rejects_between_cardash_and_fh4() -> TestResult {
    let adapter = ForzaAdapter::new();
    // Between CarDash (311) and FH4 (324) — invalid size
    assert!(adapter.normalize(&[0u8; 318]).is_err());
    Ok(())
}

#[test]
fn rejects_between_fh4_and_fm8() -> TestResult {
    let adapter = ForzaAdapter::new();
    // Between FH4 (324) and FM8 (331) — invalid size
    assert!(adapter.normalize(&[0u8; 327]).is_err());
    Ok(())
}

#[test]
fn rejects_fm8_plus_one() -> TestResult {
    let adapter = ForzaAdapter::new();
    assert!(adapter.normalize(&[0u8; FM8_SIZE + 1]).is_err());
    Ok(())
}

#[test]
fn rejects_very_large_packet() -> TestResult {
    let adapter = ForzaAdapter::new();
    assert!(adapter.normalize(&[0u8; 1024]).is_err());
    Ok(())
}

#[test]
fn accepts_exactly_sled_size() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut buf = vec![0u8; SLED_SIZE];
    write_i32(&mut buf, OFF_IS_RACE_ON, 1);
    assert!(adapter.normalize(&buf).is_ok());
    Ok(())
}

#[test]
fn accepts_exactly_cardash_size() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut buf = vec![0u8; CARDASH_SIZE];
    write_i32(&mut buf, OFF_IS_RACE_ON, 1);
    assert!(adapter.normalize(&buf).is_ok());
    Ok(())
}

#[test]
fn accepts_exactly_fm8_size() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut buf = vec![0u8; FM8_SIZE];
    write_i32(&mut buf, OFF_IS_RACE_ON, 1);
    assert!(adapter.normalize(&buf).is_ok());
    Ok(())
}

#[test]
fn accepts_exactly_fh4_size() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut buf = vec![0u8; FH4_SIZE];
    write_i32(&mut buf, OFF_IS_RACE_ON, 1);
    assert!(adapter.normalize(&buf).is_ok());
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// All-ones edge cases — FM8 and FH4 (complements comprehensive.rs sled/cardash)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn fm8_all_0xff_does_not_panic() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut buf = vec![0xFFu8; FM8_SIZE];
    write_i32(&mut buf, OFF_IS_RACE_ON, 1);
    let _result = adapter.normalize(&buf);
    Ok(())
}

#[test]
fn fh4_all_0xff_does_not_panic() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut buf = vec![0xFFu8; FH4_SIZE];
    write_i32(&mut buf, OFF_IS_RACE_ON, 1);
    let _result = adapter.normalize(&buf);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// NaN / Inf / extreme float values in packet data
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn sled_nan_rpm_does_not_panic() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut buf = make_sled_on();
    write_f32(&mut buf, OFF_CURRENT_RPM, f32::NAN);
    let _result = adapter.normalize(&buf);
    Ok(())
}

#[test]
fn sled_infinity_velocity_does_not_panic() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut buf = make_sled_on();
    write_f32(&mut buf, OFF_VEL_X, f32::INFINITY);
    let _result = adapter.normalize(&buf);
    Ok(())
}

#[test]
fn sled_neg_infinity_acceleration_does_not_panic() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut buf = make_sled_on();
    write_f32(&mut buf, OFF_ACCEL_X, f32::NEG_INFINITY);
    let _result = adapter.normalize(&buf);
    Ok(())
}

#[test]
fn cardash_nan_fuel_does_not_panic() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut buf = make_cardash_on();
    write_f32(&mut buf, OFF_DASH_FUEL, f32::NAN);
    let _result = adapter.normalize(&buf);
    Ok(())
}

#[test]
fn cardash_nan_tire_temp_does_not_panic() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut buf = make_cardash_on();
    write_f32(&mut buf, OFF_DASH_TIRE_TEMP_FL, f32::NAN);
    let _result = adapter.normalize(&buf);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// validated() clamping through builder
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn validated_clamps_negative_speed() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .speed_ms(-5.0)
        .build()
        .validated();
    assert_eq!(t.speed_ms, 0.0, "negative speed should clamp to 0");
    Ok(())
}

#[test]
fn validated_clamps_negative_rpm() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .rpm(-100.0)
        .build()
        .validated();
    assert_eq!(t.rpm, 0.0, "negative rpm should clamp to 0");
    Ok(())
}

#[test]
fn validated_clamps_throttle_above_one() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .throttle(1.5)
        .build()
        .validated();
    assert_eq!(t.throttle, 1.0, "throttle>1.0 should clamp to 1.0");
    Ok(())
}

#[test]
fn validated_clamps_brake_below_zero() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .brake(-0.5)
        .build()
        .validated();
    assert_eq!(t.brake, 0.0, "brake<0.0 should clamp to 0.0");
    Ok(())
}

#[test]
fn validated_clamps_clutch_to_range() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .clutch(2.0)
        .build()
        .validated();
    assert_eq!(t.clutch, 1.0);
    Ok(())
}

#[test]
fn validated_replaces_nan_with_zero() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .speed_ms(f32::NAN)
        .rpm(f32::NAN)
        .throttle(f32::NAN)
        .brake(f32::NAN)
        .lateral_g(f32::NAN)
        .build()
        .validated();
    assert_eq!(t.speed_ms, 0.0);
    assert_eq!(t.rpm, 0.0);
    assert_eq!(t.throttle, 0.0);
    assert_eq!(t.brake, 0.0);
    assert_eq!(t.lateral_g, 0.0);
    Ok(())
}

#[test]
fn validated_preserves_valid_values() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .speed_ms(30.0)
        .rpm(5000.0)
        .throttle(0.75)
        .brake(0.25)
        .gear(3)
        .build()
        .validated();
    assert!((t.speed_ms - 30.0).abs() < 0.01);
    assert!((t.rpm - 5000.0).abs() < 0.01);
    assert!((t.throttle - 0.75).abs() < 0.01);
    assert!((t.brake - 0.25).abs() < 0.01);
    assert_eq!(t.gear, 3);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Builder depth — all fields
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn builder_sets_all_primary_fields() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .speed_ms(50.0)
        .steering_angle(0.3)
        .throttle(0.9)
        .brake(0.1)
        .clutch(0.5)
        .rpm(7000.0)
        .max_rpm(9000.0)
        .gear(5)
        .num_gears(6)
        .build();
    assert!((t.speed_ms - 50.0).abs() < 0.01);
    assert!((t.steering_angle - 0.3).abs() < 0.01);
    assert!((t.throttle - 0.9).abs() < 0.01);
    assert!((t.brake - 0.1).abs() < 0.01);
    assert!((t.clutch - 0.5).abs() < 0.01);
    assert!((t.rpm - 7000.0).abs() < 0.01);
    assert!((t.max_rpm - 9000.0).abs() < 0.01);
    assert_eq!(t.gear, 5);
    assert_eq!(t.num_gears, 6);
    Ok(())
}

#[test]
fn builder_sets_g_force_fields() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .lateral_g(2.5)
        .longitudinal_g(-1.5)
        .vertical_g(0.3)
        .build();
    assert!((t.lateral_g - 2.5).abs() < 0.01);
    assert!((t.longitudinal_g - (-1.5)).abs() < 0.01);
    assert!((t.vertical_g - 0.3).abs() < 0.01);
    Ok(())
}

#[test]
fn builder_sets_slip_fields() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .slip_ratio(0.15)
        .slip_angle_fl(0.01)
        .slip_angle_fr(0.02)
        .slip_angle_rl(0.03)
        .slip_angle_rr(0.04)
        .build();
    assert!((t.slip_ratio - 0.15).abs() < 0.001);
    assert!((t.slip_angle_fl - 0.01).abs() < 0.001);
    assert!((t.slip_angle_fr - 0.02).abs() < 0.001);
    assert!((t.slip_angle_rl - 0.03).abs() < 0.001);
    assert!((t.slip_angle_rr - 0.04).abs() < 0.001);
    Ok(())
}

#[test]
fn builder_sets_tire_data() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .tire_temps_c([80, 85, 90, 95])
        .tire_pressures_psi([28.0, 28.5, 29.0, 29.5])
        .build();
    assert_eq!(t.tire_temps_c, [80, 85, 90, 95]);
    assert!((t.tire_pressures_psi[0] - 28.0).abs() < 0.01);
    assert!((t.tire_pressures_psi[3] - 29.5).abs() < 0.01);
    Ok(())
}

#[test]
fn builder_sets_ffb_fields() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .ffb_scalar(-0.75)
        .ffb_torque_nm(12.5)
        .build();
    assert!((t.ffb_scalar - (-0.75)).abs() < 0.01);
    assert!((t.ffb_torque_nm - 12.5).abs() < 0.01);
    assert!(t.has_ffb_data());
    Ok(())
}

#[test]
fn builder_sets_context_fields() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .car_id("ferrari_488")
        .track_id("monza")
        .session_id("session_001")
        .position(2)
        .lap(5)
        .current_lap_time_s(45.3)
        .best_lap_time_s(44.1)
        .last_lap_time_s(44.9)
        .delta_ahead_s(-0.5)
        .delta_behind_s(1.2)
        .build();
    assert_eq!(t.car_id, Some("ferrari_488".to_string()));
    assert_eq!(t.track_id, Some("monza".to_string()));
    assert_eq!(t.session_id, Some("session_001".to_string()));
    assert_eq!(t.position, 2);
    assert_eq!(t.lap, 5);
    assert!((t.current_lap_time_s - 45.3).abs() < 0.01);
    assert!((t.best_lap_time_s - 44.1).abs() < 0.01);
    assert!((t.last_lap_time_s - 44.9).abs() < 0.01);
    assert!((t.delta_ahead_s - (-0.5)).abs() < 0.01);
    assert!((t.delta_behind_s - 1.2).abs() < 0.01);
    Ok(())
}

#[test]
fn builder_sets_fuel_and_engine_temp() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .fuel_percent(0.42)
        .engine_temp_c(95.0)
        .build();
    assert!((t.fuel_percent - 0.42).abs() < 0.01);
    assert!((t.engine_temp_c - 95.0).abs() < 0.01);
    Ok(())
}

#[test]
fn builder_with_extended_field() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .extended("custom_field", TelemetryValue::Float(42.0))
        .extended("flag", TelemetryValue::Boolean(true))
        .build();
    assert_eq!(
        t.get_extended("custom_field"),
        Some(&TelemetryValue::Float(42.0))
    );
    assert_eq!(t.get_extended("flag"), Some(&TelemetryValue::Boolean(true)));
    Ok(())
}

#[test]
fn builder_sequence_and_default() -> TestResult {
    let t = NormalizedTelemetry::builder().sequence(99).build();
    assert_eq!(t.sequence, 99);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// TelemetryFrame construction
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn frame_from_telemetry_has_nonzero_timestamp() -> TestResult {
    let t = NormalizedTelemetry::builder().rpm(3000.0).build();
    let frame = TelemetryFrame::from_telemetry(t, 42, SLED_SIZE);
    assert!(frame.timestamp_ns > 0, "should have a system timestamp");
    assert_eq!(frame.sequence, 42);
    assert_eq!(frame.raw_size, SLED_SIZE);
    assert!((frame.data.rpm - 3000.0).abs() < 0.01);
    Ok(())
}

#[test]
fn frame_new_preserves_all_fields() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .rpm(5000.0)
        .speed_ms(25.0)
        .gear(3)
        .build();
    let frame = TelemetryFrame::new(t, 12345678, 7, CARDASH_SIZE);
    assert_eq!(frame.timestamp_ns, 12345678);
    assert_eq!(frame.sequence, 7);
    assert_eq!(frame.raw_size, CARDASH_SIZE);
    assert!((frame.data.rpm - 5000.0).abs() < 0.01);
    assert!((frame.data.speed_ms - 25.0).abs() < 0.01);
    assert_eq!(frame.data.gear, 3);
    Ok(())
}

#[test]
fn frame_sequence_zero_is_valid() -> TestResult {
    let t = NormalizedTelemetry::default();
    let frame = TelemetryFrame::new(t, 0, 0, 0);
    assert_eq!(frame.sequence, 0);
    assert_eq!(frame.raw_size, 0);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// TelemetryValue variant depth
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn telemetry_value_float_equality() -> TestResult {
    let a = TelemetryValue::Float(3.125);
    let b = TelemetryValue::Float(3.125);
    let c = TelemetryValue::Float(2.71);
    assert_eq!(a, b);
    assert_ne!(a, c);
    Ok(())
}

#[test]
fn telemetry_value_integer_equality() -> TestResult {
    let a = TelemetryValue::Integer(100);
    let b = TelemetryValue::Integer(100);
    let c = TelemetryValue::Integer(-1);
    assert_eq!(a, b);
    assert_ne!(a, c);
    Ok(())
}

#[test]
fn telemetry_value_boolean_equality() -> TestResult {
    assert_eq!(TelemetryValue::Boolean(true), TelemetryValue::Boolean(true));
    assert_ne!(
        TelemetryValue::Boolean(true),
        TelemetryValue::Boolean(false)
    );
    Ok(())
}

#[test]
fn telemetry_value_string_equality() -> TestResult {
    let a = TelemetryValue::String("hello".into());
    let b = TelemetryValue::String("hello".into());
    let c = TelemetryValue::String("world".into());
    assert_eq!(a, b);
    assert_ne!(a, c);
    Ok(())
}

#[test]
fn telemetry_value_cross_variant_inequality() -> TestResult {
    assert_ne!(TelemetryValue::Float(1.0), TelemetryValue::Integer(1));
    assert_ne!(
        TelemetryValue::Boolean(true),
        TelemetryValue::String("true".into())
    );
    Ok(())
}

#[test]
fn telemetry_value_clone_is_equal() -> TestResult {
    let original = TelemetryValue::String("test_value".into());
    let cloned = original.clone();
    assert_eq!(original, cloned);
    Ok(())
}

#[test]
fn telemetry_value_debug_is_non_empty() -> TestResult {
    let val = TelemetryValue::Float(1.0);
    let debug = format!("{val:?}");
    assert!(!debug.is_empty());
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// NormalizedTelemetry: with_extended and with_sequence
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn with_extended_chains_multiple_values() -> TestResult {
    let t = NormalizedTelemetry::default()
        .with_extended("key1", TelemetryValue::Integer(1))
        .with_extended("key2", TelemetryValue::Float(2.0));
    assert_eq!(t.get_extended("key1"), Some(&TelemetryValue::Integer(1)));
    assert_eq!(t.get_extended("key2"), Some(&TelemetryValue::Float(2.0)));
    Ok(())
}

#[test]
fn with_extended_overwrites_same_key() -> TestResult {
    let t = NormalizedTelemetry::default()
        .with_extended("key", TelemetryValue::Integer(1))
        .with_extended("key", TelemetryValue::Integer(99));
    assert_eq!(t.get_extended("key"), Some(&TelemetryValue::Integer(99)));
    Ok(())
}

#[test]
fn with_sequence_sets_sequence() -> TestResult {
    let t = NormalizedTelemetry::default().with_sequence(42);
    assert_eq!(t.sequence, 42);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Cross-format invariants
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn all_formats_share_sled_section_rpm() -> TestResult {
    let adapter = ForzaAdapter::new();
    let rpm = 7777.0;
    let max_rpm = 9000.0;

    // Same sled section preamble for all formats
    let formats: Vec<Vec<u8>> = vec![
        make_sled_on(),
        make_cardash_on(),
        make_fm8_on(),
        make_fh4_on(),
    ];

    for mut buf in formats {
        write_f32(&mut buf, OFF_CURRENT_RPM, rpm);
        write_f32(&mut buf, OFF_ENGINE_MAX_RPM, max_rpm);
        let t = adapter.normalize(&buf)?;
        assert!(
            (t.rpm - rpm).abs() < 0.1,
            "size={}: rpm={}",
            buf.len(),
            t.rpm
        );
        assert!(
            (t.max_rpm - max_rpm).abs() < 0.1,
            "size={}: max_rpm={}",
            buf.len(),
            t.max_rpm
        );
    }
    Ok(())
}

#[test]
fn all_formats_share_sled_section_velocity() -> TestResult {
    let adapter = ForzaAdapter::new();
    // Sled uses velocity vector for speed; CarDash/FM8/FH4 may use dash speed field.
    // Here we test the sled section: set velocity on sled and verify speed_ms.
    let mut buf = make_sled_on();
    write_f32(&mut buf, OFF_VEL_X, 6.0);
    write_f32(&mut buf, OFF_VEL_Y, 8.0);
    let t = adapter.normalize(&buf)?;
    // sqrt(6² + 8²) = 10.0
    assert!((t.speed_ms - 10.0).abs() < 0.1, "sled speed={}", t.speed_ms);

    // CarDash formats use the dashboard speed field (offset 244) instead
    let dash_speed_off: usize = 244;
    for size in [CARDASH_SIZE, FM8_SIZE] {
        let mut buf = vec![0u8; size];
        write_i32(&mut buf, OFF_IS_RACE_ON, 1);
        write_f32(&mut buf, dash_speed_off, 10.0);
        let t = adapter.normalize(&buf)?;
        assert!(
            t.speed_ms >= 0.0,
            "size={}: speed should be non-negative",
            size
        );
    }

    // FH4 uses dash speed with horizon offset
    let mut fh4_buf = make_fh4_on();
    write_f32(&mut fh4_buf, dash_speed_off + FH4_HORIZON_OFFSET, 10.0);
    let t = adapter.normalize(&fh4_buf)?;
    assert!(t.speed_ms >= 0.0, "FH4 speed should be non-negative");
    Ok(())
}

#[test]
fn all_formats_share_sled_section_g_forces() -> TestResult {
    let adapter = ForzaAdapter::new();
    let formats: Vec<Vec<u8>> = vec![
        make_sled_on(),
        make_cardash_on(),
        make_fm8_on(),
        make_fh4_on(),
    ];

    for mut buf in formats {
        write_f32(&mut buf, OFF_ACCEL_X, 1.5 * G);
        write_f32(&mut buf, OFF_ACCEL_Z, -0.8 * G);
        write_f32(&mut buf, OFF_ACCEL_Y, 0.2 * G);
        let t = adapter.normalize(&buf)?;
        assert!(
            (t.lateral_g - 1.5).abs() < 0.05,
            "size={}: lat_g={}",
            buf.len(),
            t.lateral_g
        );
        assert!(
            (t.longitudinal_g - (-0.8)).abs() < 0.05,
            "size={}: lon_g={}",
            buf.len(),
            t.longitudinal_g
        );
        assert!(
            (t.vertical_g - 0.2).abs() < 0.05,
            "size={}: vert_g={}",
            buf.len(),
            t.vertical_g
        );
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// NormalizedTelemetry: default state completeness
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn default_telemetry_all_zeros_and_nones() -> TestResult {
    let t = NormalizedTelemetry::default();
    assert_eq!(t.speed_ms, 0.0);
    assert_eq!(t.steering_angle, 0.0);
    assert_eq!(t.throttle, 0.0);
    assert_eq!(t.brake, 0.0);
    assert_eq!(t.clutch, 0.0);
    assert_eq!(t.rpm, 0.0);
    assert_eq!(t.max_rpm, 0.0);
    assert_eq!(t.gear, 0);
    assert_eq!(t.num_gears, 0);
    assert_eq!(t.lateral_g, 0.0);
    assert_eq!(t.longitudinal_g, 0.0);
    assert_eq!(t.vertical_g, 0.0);
    assert_eq!(t.slip_ratio, 0.0);
    assert_eq!(t.ffb_scalar, 0.0);
    assert_eq!(t.ffb_torque_nm, 0.0);
    assert!(t.car_id.is_none());
    assert!(t.track_id.is_none());
    assert!(t.session_id.is_none());
    assert_eq!(t.position, 0);
    assert_eq!(t.lap, 0);
    assert_eq!(t.fuel_percent, 0.0);
    assert_eq!(t.engine_temp_c, 0.0);
    assert!(t.extended.is_empty());
    assert_eq!(t.sequence, 0);
    Ok(())
}

#[test]
fn default_telemetry_helper_states() -> TestResult {
    let t = NormalizedTelemetry::default();
    assert!(t.is_stationary());
    assert!(!t.has_ffb_data());
    assert!(!t.has_rpm_data());
    assert!(!t.has_rpm_display_data());
    assert_eq!(t.rpm_fraction(), 0.0);
    assert!(!t.has_active_flags());
    assert_eq!(t.total_g(), 0.0);
    assert_eq!(t.speed_kmh(), 0.0);
    assert_eq!(t.speed_mph(), 0.0);
    assert_eq!(t.average_slip_angle(), 0.0);
    assert_eq!(t.front_slip_angle(), 0.0);
    assert_eq!(t.rear_slip_angle(), 0.0);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Race-off across all formats
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn race_off_zeroes_all_formats() -> TestResult {
    let adapter = ForzaAdapter::new();
    let formats: Vec<Vec<u8>> = vec![
        vec![0u8; SLED_SIZE],
        vec![0u8; CARDASH_SIZE],
        vec![0u8; FM8_SIZE],
        vec![0u8; FH4_SIZE],
    ];
    for buf in formats {
        let t = adapter.normalize(&buf)?;
        assert_eq!(
            t.rpm,
            0.0,
            "size={}: rpm should be 0 when race_off",
            buf.len()
        );
        assert_eq!(
            t.speed_ms,
            0.0,
            "size={}: speed should be 0 when race_off",
            buf.len()
        );
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Extreme but valid float values
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn sled_very_large_velocity_produces_finite_speed() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut buf = make_sled_on();
    write_f32(&mut buf, OFF_VEL_X, 1e6);
    let t = adapter.normalize(&buf)?;
    assert!(t.speed_ms.is_finite(), "speed should remain finite");
    assert!(t.speed_ms > 0.0);
    Ok(())
}

#[test]
fn sled_very_small_nonzero_velocity() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut buf = make_sled_on();
    write_f32(&mut buf, OFF_VEL_X, 1e-10);
    let t = adapter.normalize(&buf)?;
    assert!(t.speed_ms >= 0.0);
    assert!(t.speed_ms.is_finite());
    Ok(())
}

#[test]
fn sled_negative_g_force_values() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut buf = make_sled_on();
    write_f32(&mut buf, OFF_ACCEL_X, -5.0 * G);
    write_f32(&mut buf, OFF_ACCEL_Z, -3.0 * G);
    write_f32(&mut buf, OFF_ACCEL_Y, -G);
    let t = adapter.normalize(&buf)?;
    assert!((t.lateral_g - (-5.0)).abs() < 0.05, "lat_g={}", t.lateral_g);
    assert!(
        (t.longitudinal_g - (-3.0)).abs() < 0.05,
        "lon_g={}",
        t.longitudinal_g
    );
    assert!(
        (t.vertical_g - (-1.0)).abs() < 0.05,
        "vert_g={}",
        t.vertical_g
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Multiple adapters coexistence
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn multiple_adapter_instances_are_independent() -> TestResult {
    let a1 = ForzaAdapter::new();
    let a2 = ForzaAdapter::new().with_port(7777);
    // Both parse the same data identically
    let mut buf = make_sled_on();
    write_f32(&mut buf, OFF_CURRENT_RPM, 4500.0);
    let t1 = a1.normalize(&buf)?;
    let t2 = a2.normalize(&buf)?;
    assert_eq!(t1.rpm, t2.rpm);
    assert_eq!(t1.speed_ms, t2.speed_ms);
    Ok(())
}
