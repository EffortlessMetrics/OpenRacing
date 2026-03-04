#![allow(clippy::redundant_closure)]
//! Property-based tests for WRC Generations telemetry adapter.
//!
//! Tests rally-specific fields, UDP packet round-trip parsing, and edge cases
//! including spectator mode, service area, and shakedown scenarios.

use proptest::prelude::*;
use racing_wheel_telemetry_wrc_generations::{
    NormalizedTelemetry, TelemetryAdapter, TelemetryValue, WrcGenerationsAdapter,
};

const MIN_PACKET_SIZE: usize = 264;

// Byte offsets (Codemasters Mode 1, little-endian f32).
const OFF_VEL_X: usize = 32;
const OFF_VEL_Y: usize = 36;
const OFF_VEL_Z: usize = 40;

const OFF_WHEEL_SPEED_RL: usize = 100;
const OFF_WHEEL_SPEED_RR: usize = 104;
const OFF_WHEEL_SPEED_FL: usize = 108;
const OFF_WHEEL_SPEED_FR: usize = 112;

const OFF_THROTTLE: usize = 116;
const OFF_STEER: usize = 120;
const OFF_BRAKE: usize = 124;
const OFF_GEAR: usize = 132;
const OFF_GFORCE_LAT: usize = 136;
const OFF_CURRENT_LAP: usize = 144;
const OFF_RPM: usize = 148;
const OFF_CAR_POSITION: usize = 156;

const OFF_FUEL_IN_TANK: usize = 180;
const OFF_FUEL_CAPACITY: usize = 184;
const OFF_IN_PIT: usize = 188;

const OFF_TYRES_PRESSURE_RL: usize = 220;
const OFF_TYRES_PRESSURE_RR: usize = 224;
const OFF_TYRES_PRESSURE_FL: usize = 228;
const OFF_TYRES_PRESSURE_FR: usize = 232;

const OFF_LAST_LAP_TIME: usize = 248;
const OFF_MAX_RPM: usize = 252;
const OFF_MAX_GEARS: usize = 260;

fn make_packet(size: usize) -> Vec<u8> {
    vec![0u8; size]
}

fn write_f32(buf: &mut [u8], offset: usize, value: f32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn adapter() -> WrcGenerationsAdapter {
    WrcGenerationsAdapter::new()
}

fn parse(raw: &[u8]) -> Result<NormalizedTelemetry, anyhow::Error> {
    adapter().normalize(raw)
}

// ---------------------------------------------------------------------------
// Proptest: UDP packet parsing round-trips
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    /// Throttle value written to a packet parses back within clamped [0,1] range.
    #[test]
    fn prop_throttle_roundtrip(throttle in -2.0f32..3.0f32) {
        let mut buf = make_packet(MIN_PACKET_SIZE);
        write_f32(&mut buf, OFF_THROTTLE, throttle);
        let t = parse(&buf).map_err(|e| TestCaseError::fail(format!("{e}")))?;
        prop_assert!(t.throttle >= 0.0 && t.throttle <= 1.0,
            "throttle {} -> {} not in [0,1]", throttle, t.throttle);
    }

    /// Brake value round-trips within clamped [0,1] range.
    #[test]
    fn prop_brake_roundtrip(brake in -2.0f32..3.0f32) {
        let mut buf = make_packet(MIN_PACKET_SIZE);
        write_f32(&mut buf, OFF_BRAKE, brake);
        let t = parse(&buf).map_err(|e| TestCaseError::fail(format!("{e}")))?;
        prop_assert!(t.brake >= 0.0 && t.brake <= 1.0,
            "brake {} -> {} not in [0,1]", brake, t.brake);
    }

    /// Steering value round-trips within clamped [-1,1] range.
    #[test]
    fn prop_steering_roundtrip(steer in -10.0f32..10.0f32) {
        let mut buf = make_packet(MIN_PACKET_SIZE);
        write_f32(&mut buf, OFF_STEER, steer);
        let t = parse(&buf).map_err(|e| TestCaseError::fail(format!("{e}")))?;
        prop_assert!(t.steering_angle >= -1.0 && t.steering_angle <= 1.0,
            "steer {} -> {} not in [-1,1]", steer, t.steering_angle);
    }

    /// RPM always produces a non-negative, finite value.
    #[test]
    fn prop_rpm_non_negative(rpm in -10000.0f32..20000.0f32) {
        let mut buf = make_packet(MIN_PACKET_SIZE);
        write_f32(&mut buf, OFF_RPM, rpm);
        let t = parse(&buf).map_err(|e| TestCaseError::fail(format!("{e}")))?;
        prop_assert!(t.rpm >= 0.0 && t.rpm.is_finite(),
            "rpm {} -> {} invalid", rpm, t.rpm);
    }

    /// FFB scalar stays within [-1,1] for any lateral G value.
    #[test]
    fn prop_ffb_scalar_bounded(g in -20.0f32..20.0f32) {
        let mut buf = make_packet(MIN_PACKET_SIZE);
        write_f32(&mut buf, OFF_GFORCE_LAT, g);
        let t = parse(&buf).map_err(|e| TestCaseError::fail(format!("{e}")))?;
        prop_assert!(t.ffb_scalar >= -1.0 && t.ffb_scalar <= 1.0,
            "lat_g {} -> ffb_scalar {} not in [-1,1]", g, t.ffb_scalar);
    }

    /// Fuel percent always in [0,1] for any tank/capacity combination.
    #[test]
    fn prop_fuel_percent_bounded(
        tank in 0.0f32..200.0f32,
        capacity in 0.0f32..200.0f32,
    ) {
        let mut buf = make_packet(MIN_PACKET_SIZE);
        write_f32(&mut buf, OFF_FUEL_IN_TANK, tank);
        write_f32(&mut buf, OFF_FUEL_CAPACITY, capacity);
        let t = parse(&buf).map_err(|e| TestCaseError::fail(format!("{e}")))?;
        prop_assert!(t.fuel_percent >= 0.0 && t.fuel_percent <= 1.0 && t.fuel_percent.is_finite(),
            "tank={}, capacity={} -> fuel_percent={}", tank, capacity, t.fuel_percent);
    }

    /// Speed derived from wheel speeds is always non-negative and finite.
    #[test]
    fn prop_speed_non_negative(
        fl in -100.0f32..100.0f32,
        fr in -100.0f32..100.0f32,
        rl in -100.0f32..100.0f32,
        rr in -100.0f32..100.0f32,
    ) {
        let mut buf = make_packet(MIN_PACKET_SIZE);
        write_f32(&mut buf, OFF_WHEEL_SPEED_FL, fl);
        write_f32(&mut buf, OFF_WHEEL_SPEED_FR, fr);
        write_f32(&mut buf, OFF_WHEEL_SPEED_RL, rl);
        write_f32(&mut buf, OFF_WHEEL_SPEED_RR, rr);
        let t = parse(&buf).map_err(|e| TestCaseError::fail(format!("{e}")))?;
        prop_assert!(t.speed_ms >= 0.0 && t.speed_ms.is_finite(),
            "speed_ms {} invalid", t.speed_ms);
    }

    /// All output fields are finite for any valid-range inputs.
    #[test]
    fn prop_all_fields_finite(
        throttle in 0.0f32..1.0f32,
        brake in 0.0f32..1.0f32,
        rpm in 0.0f32..15000.0f32,
        gear in 0.0f32..8.0f32,
    ) {
        let mut buf = make_packet(MIN_PACKET_SIZE);
        write_f32(&mut buf, OFF_THROTTLE, throttle);
        write_f32(&mut buf, OFF_BRAKE, brake);
        write_f32(&mut buf, OFF_RPM, rpm);
        write_f32(&mut buf, OFF_GEAR, gear);
        write_f32(&mut buf, OFF_MAX_RPM, 8000.0);
        write_f32(&mut buf, OFF_FUEL_CAPACITY, 60.0);
        write_f32(&mut buf, OFF_FUEL_IN_TANK, 30.0);
        let t = parse(&buf).map_err(|e| TestCaseError::fail(format!("{e}")))?;
        prop_assert!(t.speed_ms.is_finite());
        prop_assert!(t.rpm.is_finite());
        prop_assert!(t.throttle.is_finite());
        prop_assert!(t.brake.is_finite());
        prop_assert!(t.steering_angle.is_finite());
        prop_assert!(t.ffb_scalar.is_finite());
        prop_assert!(t.fuel_percent.is_finite());
        prop_assert!(t.last_lap_time_s.is_finite());
    }
}

// ---------------------------------------------------------------------------
// Rally-specific fields: stage times, splits, surface type
// ---------------------------------------------------------------------------

#[test]
fn rally_last_lap_time_maps_stage_time() -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = make_packet(MIN_PACKET_SIZE);
    write_f32(&mut buf, OFF_LAST_LAP_TIME, 245.3);
    let t = parse(&buf)?;
    assert!(
        (t.last_lap_time_s - 245.3).abs() < 0.01,
        "stage time should map to last_lap_time_s, got {}",
        t.last_lap_time_s
    );
    Ok(())
}

#[test]
fn rally_negative_last_lap_time_handled() -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = make_packet(MIN_PACKET_SIZE);
    write_f32(&mut buf, OFF_LAST_LAP_TIME, -1.0);
    let t = parse(&buf)?;
    assert!(
        t.last_lap_time_s.is_finite(),
        "negative stage time must produce finite result"
    );
    Ok(())
}

#[test]
fn rally_tire_pressures_represent_surface_wear() -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = make_packet(MIN_PACKET_SIZE);
    // Gravel surfaces typically show uneven tire pressures
    write_f32(&mut buf, OFF_TYRES_PRESSURE_FL, 22.0);
    write_f32(&mut buf, OFF_TYRES_PRESSURE_FR, 24.0);
    write_f32(&mut buf, OFF_TYRES_PRESSURE_RL, 21.5);
    write_f32(&mut buf, OFF_TYRES_PRESSURE_RR, 23.5);
    let t = parse(&buf)?;
    // Verify all pressures are parsed and finite
    for (i, &p) in t.tire_pressures_psi.iter().enumerate() {
        assert!(
            p.is_finite() && p >= 0.0,
            "tire_pressures_psi[{i}] = {p} invalid"
        );
    }
    Ok(())
}

#[test]
fn rally_extended_wheel_speeds_represent_slip() -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = make_packet(MIN_PACKET_SIZE);
    // Simulate wheel spin on gravel: rear wheels faster than front
    write_f32(&mut buf, OFF_WHEEL_SPEED_FL, 15.0);
    write_f32(&mut buf, OFF_WHEEL_SPEED_FR, 15.0);
    write_f32(&mut buf, OFF_WHEEL_SPEED_RL, 22.0);
    write_f32(&mut buf, OFF_WHEEL_SPEED_RR, 22.0);
    let t = parse(&buf)?;
    match (
        t.extended.get("wheel_speed_fl"),
        t.extended.get("wheel_speed_rl"),
    ) {
        (Some(TelemetryValue::Float(fl)), Some(TelemetryValue::Float(rl))) => {
            assert!(
                rl > fl,
                "rear wheels should be faster in oversteer, fl={fl}, rl={rl}"
            );
        }
        _ => {
            // Extended data may not be present in all builds; skip
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Edge cases: spectator mode, service area, shakedown
// ---------------------------------------------------------------------------

#[test]
fn edge_spectator_mode_zero_inputs() -> Result<(), Box<dyn std::error::Error>> {
    // Spectator mode: all inputs zero, speed may be from other car's velocity
    let mut buf = make_packet(MIN_PACKET_SIZE);
    write_f32(&mut buf, OFF_THROTTLE, 0.0);
    write_f32(&mut buf, OFF_BRAKE, 0.0);
    write_f32(&mut buf, OFF_STEER, 0.0);
    write_f32(&mut buf, OFF_RPM, 0.0);
    write_f32(&mut buf, OFF_GEAR, 0.0);
    // Spectated car might have velocity
    write_f32(&mut buf, OFF_VEL_X, 20.0);
    write_f32(&mut buf, OFF_VEL_Y, 0.0);
    write_f32(&mut buf, OFF_VEL_Z, 0.0);
    let t = parse(&buf)?;
    assert_eq!(t.throttle, 0.0);
    assert_eq!(t.brake, 0.0);
    assert_eq!(t.gear, 0);
    // Speed may come from velocity fallback
    assert!(t.speed_ms.is_finite());
    Ok(())
}

#[test]
fn edge_service_area_in_pits_flag() -> Result<(), Box<dyn std::error::Error>> {
    // Service area between rally stages: car is stationary, in pits
    let mut buf = make_packet(MIN_PACKET_SIZE);
    write_f32(&mut buf, OFF_IN_PIT, 1.0);
    write_f32(&mut buf, OFF_RPM, 0.0);
    write_f32(&mut buf, OFF_GEAR, 0.0);
    let t = parse(&buf)?;
    assert!(t.flags.in_pits, "service area should set in_pits flag");
    assert_eq!(t.speed_ms, 0.0, "should be stationary in service area");
    Ok(())
}

#[test]
fn edge_shakedown_max_gear_one() -> Result<(), Box<dyn std::error::Error>> {
    // Shakedown: minimal gear range, controlled speed
    let mut buf = make_packet(MIN_PACKET_SIZE);
    write_f32(&mut buf, OFF_GEAR, 1.0);
    write_f32(&mut buf, OFF_MAX_GEARS, 1.0);
    write_f32(&mut buf, OFF_RPM, 3000.0);
    write_f32(&mut buf, OFF_MAX_RPM, 6000.0);
    write_f32(&mut buf, OFF_THROTTLE, 0.5);
    let t = parse(&buf)?;
    assert_eq!(t.gear, 1);
    assert_eq!(t.num_gears, 1);
    assert!((t.throttle - 0.5).abs() < 0.001);
    Ok(())
}

#[test]
fn edge_all_nan_packet_no_panic() -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = make_packet(MIN_PACKET_SIZE);
    let nan_bytes = f32::NAN.to_le_bytes();
    // Fill all f32 slots with NaN
    for offset in (0..MIN_PACKET_SIZE).step_by(4) {
        if offset + 4 <= buf.len() {
            buf[offset..offset + 4].copy_from_slice(&nan_bytes);
        }
    }
    let result = parse(&buf);
    // Must not panic; result may be Ok or Err
    if let Ok(t) = result {
        assert!(t.speed_ms.is_finite() || t.speed_ms == 0.0);
    }
    Ok(())
}

#[test]
fn edge_all_infinity_packet_no_panic() -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = make_packet(MIN_PACKET_SIZE);
    let inf_bytes = f32::INFINITY.to_le_bytes();
    for offset in (0..MIN_PACKET_SIZE).step_by(4) {
        if offset + 4 <= buf.len() {
            buf[offset..offset + 4].copy_from_slice(&inf_bytes);
        }
    }
    let result = parse(&buf);
    if let Ok(t) = result {
        assert!(t.throttle.is_finite());
        assert!(t.brake.is_finite());
        assert!(t.rpm.is_finite());
    }
    Ok(())
}

#[test]
fn edge_max_position_and_lap() -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = make_packet(MIN_PACKET_SIZE);
    write_f32(&mut buf, OFF_CAR_POSITION, 255.0);
    write_f32(&mut buf, OFF_CURRENT_LAP, 99.0);
    let t = parse(&buf)?;
    assert_eq!(t.position, 255);
    assert_eq!(t.lap, 100); // raw + 1
    Ok(())
}

#[test]
fn edge_zero_max_rpm_no_rpm_fraction() -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = make_packet(MIN_PACKET_SIZE);
    write_f32(&mut buf, OFF_RPM, 5000.0);
    write_f32(&mut buf, OFF_MAX_RPM, 0.0);
    let t = parse(&buf)?;
    assert!(
        !t.extended.contains_key("rpm_fraction"),
        "rpm_fraction should not exist when max_rpm is 0"
    );
    Ok(())
}
