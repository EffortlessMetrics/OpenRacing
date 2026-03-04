#![allow(clippy::redundant_closure)]

//! Comprehensive integration tests for the racing-wheel-telemetry-forza crate.
//!
//! Exercises packet parsing (Sled, CarDash, FM8, FH4), normalization to
//! `NormalizedTelemetry`, edge cases, and proptest fuzz coverage.

use racing_wheel_telemetry_forza::{
    ForzaAdapter, NormalizedTelemetry, TelemetryAdapter, TelemetryFrame, TelemetryValue,
};
use std::time::Duration;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ── Packet size constants (from Forza protocol spec) ─────────────────────────
const SLED_SIZE: usize = 232;
const CARDASH_SIZE: usize = 311;
const FM8_CARDASH_SIZE: usize = 331;
const FH4_CARDASH_SIZE: usize = 324;

// ── Sled byte offsets ────────────────────────────────────────────────────────
const OFF_IS_RACE_ON: usize = 0;
const OFF_ENGINE_MAX_RPM: usize = 8;
const OFF_CURRENT_RPM: usize = 16;
const OFF_ACCEL_X: usize = 20;
const OFF_ACCEL_Y: usize = 24;
const OFF_ACCEL_Z: usize = 28;
const OFF_VEL_X: usize = 32;
const OFF_VEL_Y: usize = 36;
const OFF_VEL_Z: usize = 40;
const OFF_TIRE_SLIP_RATIO_FL: usize = 84;
const OFF_TIRE_SLIP_RATIO_FR: usize = 88;
const OFF_TIRE_SLIP_RATIO_RL: usize = 92;
const OFF_TIRE_SLIP_RATIO_RR: usize = 96;
const OFF_WHEEL_SPEED_FL: usize = 100;
const OFF_SLIP_ANGLE_FL: usize = 164;
const OFF_SUSP_TRAVEL_FL: usize = 196;

// ── CarDash extension offsets ────────────────────────────────────────────────
const OFF_DASH_SPEED: usize = 244;
const OFF_DASH_TIRE_TEMP_FL: usize = 256;
const OFF_DASH_FUEL: usize = 276;
const OFF_DASH_BEST_LAP: usize = 284;
const OFF_DASH_LAST_LAP: usize = 288;
const OFF_DASH_CUR_LAP: usize = 292;
const OFF_DASH_LAP_NUMBER: usize = 300;
const OFF_DASH_RACE_POS: usize = 302;
const OFF_DASH_ACCEL: usize = 303;
const OFF_DASH_BRAKE: usize = 304;
const OFF_DASH_CLUTCH: usize = 305;
const OFF_DASH_GEAR: usize = 307;
const OFF_DASH_STEER: usize = 308;

const G: f32 = 9.806_65;

// ── Fixture helpers ──────────────────────────────────────────────────────────

fn write_f32(buf: &mut [u8], offset: usize, val: f32) {
    buf[offset..offset + 4].copy_from_slice(&val.to_le_bytes());
}

fn write_i32(buf: &mut [u8], offset: usize, val: i32) {
    buf[offset..offset + 4].copy_from_slice(&val.to_le_bytes());
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

fn make_cardash(rpm: f32, vel: f32, throttle: u8, brake: u8, gear: u8) -> Vec<u8> {
    let mut data = vec![0u8; CARDASH_SIZE];
    let sled = make_sled(1, rpm, 8000.0, (vel, 0.0, 0.0));
    data[..SLED_SIZE].copy_from_slice(&sled);
    write_f32(&mut data, OFF_DASH_SPEED, vel);
    data[OFF_DASH_ACCEL] = throttle;
    data[OFF_DASH_BRAKE] = brake;
    data[OFF_DASH_GEAR] = gear;
    data
}

fn make_fm8_cardash(rpm: f32, vel: f32, throttle: u8, gear: u8) -> Vec<u8> {
    let mut data = vec![0u8; FM8_CARDASH_SIZE];
    let sled = make_sled(1, rpm, 8000.0, (vel, 0.0, 0.0));
    data[..SLED_SIZE].copy_from_slice(&sled);
    write_f32(&mut data, OFF_DASH_SPEED, vel);
    data[OFF_DASH_ACCEL] = throttle;
    data[OFF_DASH_GEAR] = gear;
    data
}

fn make_fh4_cardash(rpm: f32, vel: f32, throttle: u8, gear: u8) -> Vec<u8> {
    let mut data = vec![0u8; FH4_CARDASH_SIZE];
    let sled = make_sled(1, rpm, 8000.0, (vel, 0.0, 0.0));
    data[..SLED_SIZE].copy_from_slice(&sled);
    // FH4: 12-byte horizon offset shifts all dash fields by +12
    write_f32(&mut data, OFF_DASH_SPEED + 12, vel);
    data[OFF_DASH_ACCEL + 12] = throttle;
    data[OFF_DASH_GEAR + 12] = gear;
    data
}

// ═══════════════════════════════════════════════════════════════════════════════
// Adapter identity and configuration
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn adapter_game_id_is_forza_motorsport() -> TestResult {
    let adapter = ForzaAdapter::new();
    assert_eq!(adapter.game_id(), "forza_motorsport");
    Ok(())
}

#[test]
fn adapter_update_rate_is_60hz() -> TestResult {
    let adapter = ForzaAdapter::new();
    assert_eq!(adapter.expected_update_rate(), Duration::from_millis(16));
    Ok(())
}

#[test]
fn adapter_default_matches_new() -> TestResult {
    let a = ForzaAdapter::new();
    let b = ForzaAdapter::default();
    assert_eq!(a.game_id(), b.game_id());
    assert_eq!(a.expected_update_rate(), b.expected_update_rate());
    Ok(())
}

#[test]
fn adapter_usable_as_trait_object() -> TestResult {
    let adapter: Box<dyn TelemetryAdapter> = Box::new(ForzaAdapter::new());
    assert_eq!(adapter.game_id(), "forza_motorsport");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Sled format parsing (232 bytes — FM7 and earlier)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn sled_basic_rpm_and_speed() -> TestResult {
    let adapter = ForzaAdapter::new();
    let data = make_sled(1, 5000.0, 8000.0, (20.0, 0.0, 0.0));
    let result = adapter.normalize(&data)?;
    assert!((result.rpm - 5000.0).abs() < 0.01);
    assert!((result.speed_ms - 20.0).abs() < 0.01);
    Ok(())
}

#[test]
fn sled_race_off_returns_zeroed() -> TestResult {
    let adapter = ForzaAdapter::new();
    let data = make_sled(0, 5000.0, 8000.0, (20.0, 0.0, 0.0));
    let result = adapter.normalize(&data)?;
    assert_eq!(result.rpm, 0.0);
    assert_eq!(result.speed_ms, 0.0);
    Ok(())
}

#[test]
fn sled_zero_filled_with_race_on() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut data = vec![0u8; SLED_SIZE];
    write_i32(&mut data, OFF_IS_RACE_ON, 1);
    let result = adapter.normalize(&data)?;
    assert_eq!(result.rpm, 0.0);
    assert_eq!(result.speed_ms, 0.0);
    assert_eq!(result.lateral_g, 0.0);
    Ok(())
}

#[test]
fn sled_3d_velocity_magnitude() -> TestResult {
    let adapter = ForzaAdapter::new();
    // sqrt(3² + 4² + 0²) = 5.0
    let data = make_sled(1, 1000.0, 8000.0, (3.0, 4.0, 0.0));
    let result = adapter.normalize(&data)?;
    assert!((result.speed_ms - 5.0).abs() < 0.01);
    Ok(())
}

#[test]
fn sled_negative_velocity_gives_positive_speed() -> TestResult {
    let adapter = ForzaAdapter::new();
    let data = make_sled(1, 1000.0, 8000.0, (-10.0, 0.0, 0.0));
    let result = adapter.normalize(&data)?;
    assert!(result.speed_ms >= 0.0);
    assert!((result.speed_ms - 10.0).abs() < 0.01);
    Ok(())
}

#[test]
fn sled_high_rpm_value() -> TestResult {
    let adapter = ForzaAdapter::new();
    let data = make_sled(1, 20000.0, 20000.0, (0.0, 0.0, 0.0));
    let result = adapter.normalize(&data)?;
    assert!((result.rpm - 20000.0).abs() < 0.01);
    Ok(())
}

#[test]
fn sled_g_forces_from_acceleration() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut data = make_sled(1, 1000.0, 8000.0, (0.0, 0.0, 0.0));
    write_f32(&mut data, OFF_ACCEL_X, 2.0 * G); // 2G lateral
    write_f32(&mut data, OFF_ACCEL_Z, 1.5 * G); // 1.5G longitudinal
    write_f32(&mut data, OFF_ACCEL_Y, 0.5 * G); // 0.5G vertical
    let result = adapter.normalize(&data)?;
    assert!((result.lateral_g - 2.0).abs() < 0.01);
    assert!((result.longitudinal_g - 1.5).abs() < 0.01);
    assert!((result.vertical_g - 0.5).abs() < 0.01);
    Ok(())
}

#[test]
fn sled_tire_slip_ratios_averaged() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut data = make_sled(1, 1000.0, 8000.0, (0.0, 0.0, 0.0));
    write_f32(&mut data, OFF_TIRE_SLIP_RATIO_FL, 0.1);
    write_f32(&mut data, OFF_TIRE_SLIP_RATIO_FR, 0.2);
    write_f32(&mut data, OFF_TIRE_SLIP_RATIO_RL, 0.3);
    write_f32(&mut data, OFF_TIRE_SLIP_RATIO_RR, 0.4);
    let result = adapter.normalize(&data)?;
    let expected = (0.1 + 0.2 + 0.3 + 0.4) / 4.0;
    assert!((result.slip_ratio - expected).abs() < 0.01);
    Ok(())
}

#[test]
fn sled_slip_angles_parsed() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut data = make_sled(1, 1000.0, 8000.0, (0.0, 0.0, 0.0));
    write_f32(&mut data, OFF_SLIP_ANGLE_FL, 0.05);
    let result = adapter.normalize(&data)?;
    assert!((result.slip_angle_fl - 0.05).abs() < 0.001);
    Ok(())
}

#[test]
fn sled_extended_wheel_speeds() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut data = make_sled(1, 1000.0, 8000.0, (0.0, 0.0, 0.0));
    write_f32(&mut data, OFF_WHEEL_SPEED_FL, 50.0);
    let result = adapter.normalize(&data)?;
    assert_eq!(
        result.get_extended("wheel_speed_fl"),
        Some(&TelemetryValue::Float(50.0))
    );
    Ok(())
}

#[test]
fn sled_extended_suspension_travel() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut data = make_sled(1, 1000.0, 8000.0, (0.0, 0.0, 0.0));
    write_f32(&mut data, OFF_SUSP_TRAVEL_FL, 0.12);
    let result = adapter.normalize(&data)?;
    assert_eq!(
        result.get_extended("suspension_travel_fl"),
        Some(&TelemetryValue::Float(0.12))
    );
    Ok(())
}

#[test]
fn sled_max_rpm_field_parsed() -> TestResult {
    let adapter = ForzaAdapter::new();
    let data = make_sled(1, 5000.0, 9000.0, (0.0, 0.0, 0.0));
    let result = adapter.normalize(&data)?;
    assert!((result.max_rpm - 9000.0).abs() < 0.01);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// CarDash format parsing (311 bytes — FM7 "Car Dash" / FH5)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn cardash_basic_fields() -> TestResult {
    let adapter = ForzaAdapter::new();
    let data = make_cardash(5000.0, 25.0, 200, 100, 4);
    let result = adapter.normalize(&data)?;
    assert!((result.rpm - 5000.0).abs() < 0.01);
    assert!((result.throttle - 200.0 / 255.0).abs() < 0.01);
    assert!((result.brake - 100.0 / 255.0).abs() < 0.01);
    assert_eq!(result.gear, 3); // gear byte 4 → gear 3
    Ok(())
}

#[test]
fn cardash_gear_mapping_reverse() -> TestResult {
    let adapter = ForzaAdapter::new();
    let data = make_cardash(1000.0, 0.0, 0, 0, 0); // gear 0 = Reverse
    let result = adapter.normalize(&data)?;
    assert_eq!(result.gear, -1);
    Ok(())
}

#[test]
fn cardash_gear_mapping_neutral() -> TestResult {
    let adapter = ForzaAdapter::new();
    let data = make_cardash(1000.0, 0.0, 0, 0, 1); // gear 1 = Neutral
    let result = adapter.normalize(&data)?;
    assert_eq!(result.gear, 0);
    Ok(())
}

#[test]
fn cardash_gear_mapping_first() -> TestResult {
    let adapter = ForzaAdapter::new();
    let data = make_cardash(1000.0, 0.0, 0, 0, 2); // gear 2 = 1st
    let result = adapter.normalize(&data)?;
    assert_eq!(result.gear, 1);
    Ok(())
}

#[test]
fn cardash_full_throttle_and_brake() -> TestResult {
    let adapter = ForzaAdapter::new();
    let data = make_cardash(5000.0, 20.0, 255, 255, 3);
    let result = adapter.normalize(&data)?;
    assert!((result.throttle - 1.0).abs() < 0.01);
    assert!((result.brake - 1.0).abs() < 0.01);
    Ok(())
}

#[test]
fn cardash_steering_parsed() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut data = make_cardash(3000.0, 15.0, 0, 0, 3);
    data[OFF_DASH_STEER] = 63_i8 as u8; // positive steer
    let result = adapter.normalize(&data)?;
    assert!((result.steering_angle - 63.0 / 127.0).abs() < 0.01);
    Ok(())
}

#[test]
fn cardash_clutch_parsed() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut data = make_cardash(3000.0, 15.0, 0, 0, 3);
    data[OFF_DASH_CLUTCH] = 128;
    let result = adapter.normalize(&data)?;
    assert!((result.clutch - 128.0 / 255.0).abs() < 0.01);
    Ok(())
}

#[test]
fn cardash_fuel_percent_parsed() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut data = make_cardash(3000.0, 15.0, 0, 0, 3);
    write_f32(&mut data, OFF_DASH_FUEL, 0.75);
    let result = adapter.normalize(&data)?;
    assert!((result.fuel_percent - 0.75).abs() < 0.01);
    Ok(())
}

#[test]
fn cardash_lap_data_parsed() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut data = make_cardash(3000.0, 15.0, 0, 0, 3);
    write_f32(&mut data, OFF_DASH_BEST_LAP, 62.5);
    write_f32(&mut data, OFF_DASH_LAST_LAP, 63.1);
    write_f32(&mut data, OFF_DASH_CUR_LAP, 30.0);
    data[OFF_DASH_LAP_NUMBER..OFF_DASH_LAP_NUMBER + 2].copy_from_slice(&5u16.to_le_bytes());
    data[OFF_DASH_RACE_POS] = 3;
    let result = adapter.normalize(&data)?;
    assert!((result.best_lap_time_s - 62.5).abs() < 0.01);
    assert!((result.last_lap_time_s - 63.1).abs() < 0.01);
    assert!((result.current_lap_time_s - 30.0).abs() < 0.01);
    assert_eq!(result.lap, 5);
    assert_eq!(result.position, 3);
    Ok(())
}

#[test]
fn cardash_tire_temps_fahrenheit_to_celsius() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut data = make_cardash(3000.0, 15.0, 0, 0, 3);
    // 212°F = 100°C
    write_f32(&mut data, OFF_DASH_TIRE_TEMP_FL, 212.0);
    let result = adapter.normalize(&data)?;
    assert_eq!(result.tire_temps_c[0], 100);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// FM8 CarDash format (331 bytes — Forza Motorsport 2023)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn fm8_cardash_basic_parse() -> TestResult {
    let adapter = ForzaAdapter::new();
    let data = make_fm8_cardash(7500.0, 30.0, 180, 5);
    let result = adapter.normalize(&data)?;
    assert!((result.rpm - 7500.0).abs() < 0.01);
    assert!((result.throttle - 180.0 / 255.0).abs() < 0.01);
    assert_eq!(result.gear, 4); // gear byte 5 → gear 4
    Ok(())
}

#[test]
fn fm8_cardash_race_off() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut data = make_fm8_cardash(7500.0, 30.0, 180, 5);
    write_i32(&mut data, OFF_IS_RACE_ON, 0);
    let result = adapter.normalize(&data)?;
    assert_eq!(result.rpm, 0.0);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// FH4 CarDash format (324 bytes — Forza Horizon 4)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn fh4_cardash_basic_parse() -> TestResult {
    let adapter = ForzaAdapter::new();
    let data = make_fh4_cardash(6000.0, 25.0, 200, 4);
    let result = adapter.normalize(&data)?;
    assert!((result.rpm - 6000.0).abs() < 0.01);
    assert!((result.throttle - 200.0 / 255.0).abs() < 0.01);
    assert_eq!(result.gear, 3); // gear byte 4 → gear 3
    Ok(())
}

#[test]
fn fh4_cardash_race_off() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut data = make_fh4_cardash(6000.0, 25.0, 200, 4);
    write_i32(&mut data, OFF_IS_RACE_ON, 0);
    let result = adapter.normalize(&data)?;
    assert_eq!(result.rpm, 0.0);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// NormalizedTelemetry construction & accessors
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn normalized_telemetry_default_is_zeroed() -> TestResult {
    let t = NormalizedTelemetry::default();
    assert_eq!(t.rpm, 0.0);
    assert_eq!(t.speed_ms, 0.0);
    assert_eq!(t.gear, 0);
    assert_eq!(t.throttle, 0.0);
    assert_eq!(t.brake, 0.0);
    Ok(())
}

#[test]
fn normalized_telemetry_builder_sets_fields() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .rpm(6000.0)
        .speed_ms(40.0)
        .gear(4)
        .throttle(0.8)
        .brake(0.2)
        .build();
    assert!((t.rpm - 6000.0).abs() < 0.01);
    assert!((t.speed_ms - 40.0).abs() < 0.01);
    assert_eq!(t.gear, 4);
    assert!((t.throttle - 0.8).abs() < 0.01);
    assert!((t.brake - 0.2).abs() < 0.01);
    Ok(())
}

#[test]
fn telemetry_value_variants() -> TestResult {
    let f = TelemetryValue::Float(1.5);
    let i = TelemetryValue::Integer(42);
    let b = TelemetryValue::Boolean(true);
    let s = TelemetryValue::String("test".to_string());
    assert_eq!(f, TelemetryValue::Float(1.5));
    assert_eq!(i, TelemetryValue::Integer(42));
    assert_eq!(b, TelemetryValue::Boolean(true));
    assert_eq!(s, TelemetryValue::String("test".to_string()));
    Ok(())
}

#[test]
fn telemetry_frame_creation() -> TestResult {
    let telemetry = NormalizedTelemetry::builder().rpm(3000.0).build();
    let frame = TelemetryFrame::new(telemetry, 12345, 1, 232);
    assert_eq!(frame.data.rpm, 3000.0);
    assert_eq!(frame.timestamp_ns, 12345);
    assert_eq!(frame.sequence, 1);
    assert_eq!(frame.raw_size, 232);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Edge cases: malformed / empty / truncated packets
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn empty_packet_returns_error() -> TestResult {
    let adapter = ForzaAdapter::new();
    assert!(adapter.normalize(&[]).is_err());
    Ok(())
}

#[test]
fn single_byte_packet_returns_error() -> TestResult {
    let adapter = ForzaAdapter::new();
    assert!(adapter.normalize(&[0xFF]).is_err());
    Ok(())
}

#[test]
fn truncated_sled_packet_returns_error() -> TestResult {
    let adapter = ForzaAdapter::new();
    assert!(adapter.normalize(&[0u8; 100]).is_err());
    Ok(())
}

#[test]
fn wrong_size_packet_returns_error() -> TestResult {
    let adapter = ForzaAdapter::new();
    // 250 bytes — not a valid Forza format
    assert!(adapter.normalize(&[0u8; 250]).is_err());
    Ok(())
}

#[test]
fn oversized_packet_returns_error() -> TestResult {
    let adapter = ForzaAdapter::new();
    assert!(adapter.normalize(&[0u8; 500]).is_err());
    Ok(())
}

#[test]
fn sled_all_zeros_race_on_does_not_panic() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut data = vec![0u8; SLED_SIZE];
    write_i32(&mut data, OFF_IS_RACE_ON, 1);
    let result = adapter.normalize(&data)?;
    assert!(result.rpm.is_finite());
    assert!(result.speed_ms.is_finite());
    Ok(())
}

#[test]
fn cardash_all_zeros_race_on_does_not_panic() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut data = vec![0u8; CARDASH_SIZE];
    write_i32(&mut data, OFF_IS_RACE_ON, 1);
    let result = adapter.normalize(&data)?;
    assert!(result.rpm.is_finite());
    assert!(result.speed_ms.is_finite());
    Ok(())
}

#[test]
fn fm8_all_zeros_race_on_does_not_panic() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut data = vec![0u8; FM8_CARDASH_SIZE];
    write_i32(&mut data, OFF_IS_RACE_ON, 1);
    let result = adapter.normalize(&data)?;
    assert!(result.rpm.is_finite());
    Ok(())
}

#[test]
fn fh4_all_zeros_race_on_does_not_panic() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut data = vec![0u8; FH4_CARDASH_SIZE];
    write_i32(&mut data, OFF_IS_RACE_ON, 1);
    let result = adapter.normalize(&data)?;
    assert!(result.rpm.is_finite());
    Ok(())
}

#[test]
fn sled_all_0xff_race_on_does_not_panic() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut data = vec![0xFFu8; SLED_SIZE];
    write_i32(&mut data, OFF_IS_RACE_ON, 1);
    // 0xFF bytes may decode to NaN/Inf — adapter should handle gracefully
    let _result = adapter.normalize(&data);
    Ok(())
}

#[test]
fn cardash_all_0xff_race_on_does_not_panic() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut data = vec![0xFFu8; CARDASH_SIZE];
    write_i32(&mut data, OFF_IS_RACE_ON, 1);
    let _result = adapter.normalize(&data);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Cross-format consistency
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn sled_and_cardash_share_rpm_from_sled_section() -> TestResult {
    let adapter = ForzaAdapter::new();
    let sled_data = make_sled(1, 5000.0, 8000.0, (20.0, 0.0, 0.0));
    let mut cardash_data = vec![0u8; CARDASH_SIZE];
    cardash_data[..SLED_SIZE].copy_from_slice(&sled_data);
    write_f32(&mut cardash_data, OFF_DASH_SPEED, 20.0);

    let sled_result = adapter.normalize(&sled_data)?;
    let cardash_result = adapter.normalize(&cardash_data)?;
    assert!((sled_result.rpm - cardash_result.rpm).abs() < 0.01);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Proptest: arbitrary byte sequences must never panic
// ═══════════════════════════════════════════════════════════════════════════════

mod proptest_fuzz {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn no_panic_on_arbitrary_bytes(data in proptest::collection::vec(any::<u8>(), 0..512)) {
            let adapter = ForzaAdapter::new();
            let _ = adapter.normalize(&data);
        }

        #[test]
        fn sled_size_arbitrary_bytes_no_panic(data in proptest::collection::vec(any::<u8>(), SLED_SIZE..=SLED_SIZE)) {
            let adapter = ForzaAdapter::new();
            let _ = adapter.normalize(&data);
        }

        #[test]
        fn cardash_size_arbitrary_bytes_no_panic(data in proptest::collection::vec(any::<u8>(), CARDASH_SIZE..=CARDASH_SIZE)) {
            let adapter = ForzaAdapter::new();
            let _ = adapter.normalize(&data);
        }

        #[test]
        fn fm8_size_arbitrary_bytes_no_panic(data in proptest::collection::vec(any::<u8>(), FM8_CARDASH_SIZE..=FM8_CARDASH_SIZE)) {
            let adapter = ForzaAdapter::new();
            let _ = adapter.normalize(&data);
        }

        #[test]
        fn fh4_size_arbitrary_bytes_no_panic(data in proptest::collection::vec(any::<u8>(), FH4_CARDASH_SIZE..=FH4_CARDASH_SIZE)) {
            let adapter = ForzaAdapter::new();
            let _ = adapter.normalize(&data);
        }

        #[test]
        fn valid_sled_with_random_physics(
            rpm in 0.0f32..20000.0,
            vel_x in -100.0f32..100.0,
            vel_y in -100.0f32..100.0,
            vel_z in -100.0f32..100.0,
        ) {
            let adapter = ForzaAdapter::new();
            let data = make_sled(1, rpm, 20000.0, (vel_x, vel_y, vel_z));
            let result = adapter.normalize(&data);
            assert!(result.is_ok());
            if let Ok(t) = result {
                assert!(t.rpm.is_finite());
                assert!(t.speed_ms.is_finite());
                assert!(t.speed_ms >= 0.0);
            }
        }
    }
}
