//! Extended deep tests for the WRC Generations / EA WRC telemetry adapter.
//!
//! Focuses on areas not covered by the existing deep_tests.rs: slip ratio
//! derivation from wheel-speed vs body-velocity, longitudinal G passthrough,
//! gear rounding thresholds (±0.5 boundary), fuel edge cases (zero-capacity),
//! NaN injection at individual offsets, max_gears extended field, lap
//! 0-based indexing, tire temperature negative-value clamping, and
//! full rally-stage scenarios.

use racing_wheel_telemetry_wrc_generations::{TelemetryAdapter, TelemetryValue, WrcGenerationsAdapter};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const MIN_PACKET: usize = 264;

// Byte offsets (Codemasters Mode 1, all f32 LE).
const OFF_LAP_TIME: usize = 4;
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
const OFF_GFORCE_LON: usize = 140;
const OFF_CURRENT_LAP: usize = 144;
const OFF_RPM: usize = 148;
const OFF_CAR_POSITION: usize = 156;
const OFF_FUEL_IN_TANK: usize = 180;
const OFF_FUEL_CAPACITY: usize = 184;
const OFF_BRAKES_TEMP_RL: usize = 204;
const OFF_BRAKES_TEMP_RR: usize = 208;
const OFF_BRAKES_TEMP_FL: usize = 212;
const OFF_BRAKES_TEMP_FR: usize = 216;
const OFF_TYRES_PRESSURE_RL: usize = 220;
const OFF_TYRES_PRESSURE_RR: usize = 224;
const OFF_TYRES_PRESSURE_FL: usize = 228;
const OFF_TYRES_PRESSURE_FR: usize = 232;
const OFF_LAST_LAP_TIME: usize = 248;
const OFF_MAX_RPM: usize = 252;
const OFF_MAX_GEARS: usize = 260;

fn write_f32(buf: &mut [u8], off: usize, v: f32) {
    buf[off..off + 4].copy_from_slice(&v.to_le_bytes());
}

fn make_packet() -> Vec<u8> {
    vec![0u8; MIN_PACKET]
}

// ═══════════════════════════════════════════════════════════════════════════════
// Slip ratio derivation
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn slip_ratio_zero_when_all_speeds_zero() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let buf = make_packet();
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.slip_ratio, 0.0, "both zero → slip_ratio = 0");
    Ok(())
}

#[test]
fn slip_ratio_zero_when_speeds_equal() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    // Wheel speeds and body velocity both 20 m/s
    for off in [OFF_WHEEL_SPEED_FL, OFF_WHEEL_SPEED_FR, OFF_WHEEL_SPEED_RL, OFF_WHEEL_SPEED_RR] {
        write_f32(&mut buf, off, 20.0);
    }
    write_f32(&mut buf, OFF_VEL_X, 20.0);
    write_f32(&mut buf, OFF_VEL_Y, 0.0);
    write_f32(&mut buf, OFF_VEL_Z, 0.0);
    let t = adapter.normalize(&buf)?;
    assert!(t.slip_ratio < 0.01, "equal speeds → ~0 slip, got {}", t.slip_ratio);
    Ok(())
}

#[test]
fn slip_ratio_nonzero_wheelspin() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    // Wheels spinning faster than body → wheelspin
    for off in [OFF_WHEEL_SPEED_FL, OFF_WHEEL_SPEED_FR, OFF_WHEEL_SPEED_RL, OFF_WHEEL_SPEED_RR] {
        write_f32(&mut buf, off, 30.0);
    }
    write_f32(&mut buf, OFF_VEL_X, 20.0);
    let t = adapter.normalize(&buf)?;
    // avg_wheel = 30, body = 20, denom = max(30,20) = 30
    // slip = |30-20| / 30 = 10/30 = 0.333…
    assert!((t.slip_ratio - (10.0 / 30.0)).abs() < 0.01, "wheelspin slip={}", t.slip_ratio);
    Ok(())
}

#[test]
fn slip_ratio_nonzero_lockup() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    // Wheels slower than body → lockup
    for off in [OFF_WHEEL_SPEED_FL, OFF_WHEEL_SPEED_FR, OFF_WHEEL_SPEED_RL, OFF_WHEEL_SPEED_RR] {
        write_f32(&mut buf, off, 10.0);
    }
    write_f32(&mut buf, OFF_VEL_X, 25.0);
    let t = adapter.normalize(&buf)?;
    // avg_wheel = 10, body = 25, denom = max(10,25) = 25
    // slip = |10-25| / 25 = 15/25 = 0.6
    assert!((t.slip_ratio - 0.6).abs() < 0.01, "lockup slip={}", t.slip_ratio);
    Ok(())
}

#[test]
fn slip_ratio_clamped_to_one() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    for off in [OFF_WHEEL_SPEED_FL, OFF_WHEEL_SPEED_FR, OFF_WHEEL_SPEED_RL, OFF_WHEEL_SPEED_RR] {
        write_f32(&mut buf, off, 100.0);
    }
    // body velocity = 0 (body_speed = 0), avg_wheel = 100, denom = 100
    // slip = |100-0|/100 = 1.0
    let t = adapter.normalize(&buf)?;
    assert!(t.slip_ratio <= 1.0, "clamped, got {}", t.slip_ratio);
    Ok(())
}

#[test]
fn slip_ratio_low_speed_threshold() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    // Both <1 → denom ≤ 1 → slip_ratio = 0
    for off in [OFF_WHEEL_SPEED_FL, OFF_WHEEL_SPEED_FR, OFF_WHEEL_SPEED_RL, OFF_WHEEL_SPEED_RR] {
        write_f32(&mut buf, off, 0.5);
    }
    write_f32(&mut buf, OFF_VEL_X, 0.3);
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.slip_ratio, 0.0, "below threshold → 0");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Gear rounding thresholds
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn gear_minus_0_49_is_neutral() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_GEAR, -0.49);
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.gear, 0, "-0.49 → neutral");
    Ok(())
}

#[test]
fn gear_minus_0_51_is_reverse() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_GEAR, -0.51);
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.gear, -1, "-0.51 → reverse");
    Ok(())
}

#[test]
fn gear_0_49_is_neutral() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_GEAR, 0.49);
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.gear, 0, "0.49 → neutral");
    Ok(())
}

#[test]
fn gear_0_51_is_first() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_GEAR, 0.51);
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.gear, 1, "0.51 → first");
    Ok(())
}

#[test]
fn gear_very_large_clamped_to_8() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_GEAR, 100.0);
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.gear, 8, "100.0 → clamped to 8");
    Ok(())
}

#[test]
fn gear_very_negative_still_reverse() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_GEAR, -10.0);
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.gear, -1, "-10.0 → reverse");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Longitudinal G-force
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn longitudinal_g_passthrough() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_GFORCE_LON, -2.5);
    let t = adapter.normalize(&buf)?;
    assert!((t.longitudinal_g - (-2.5)).abs() < 0.01);
    Ok(())
}

#[test]
fn lateral_g_passthrough() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_GFORCE_LAT, 1.8);
    let t = adapter.normalize(&buf)?;
    assert!((t.lateral_g - 1.8).abs() < 0.01);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// FFB scalar negative lateral G
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn ffb_scalar_negative_lat_g() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_GFORCE_LAT, -1.5);
    let t = adapter.normalize(&buf)?;
    // ffb = -1.5 / 3.0 = -0.5
    assert!((t.ffb_scalar - (-0.5)).abs() < 0.01, "ffb={}", t.ffb_scalar);
    Ok(())
}

#[test]
fn ffb_scalar_zero() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let buf = make_packet();
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.ffb_scalar, 0.0);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Fuel edge cases
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn fuel_zero_in_tank() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_FUEL_IN_TANK, 0.0);
    write_f32(&mut buf, OFF_FUEL_CAPACITY, 50.0);
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.fuel_percent, 0.0);
    Ok(())
}

#[test]
fn fuel_full_tank() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_FUEL_IN_TANK, 50.0);
    write_f32(&mut buf, OFF_FUEL_CAPACITY, 50.0);
    let t = adapter.normalize(&buf)?;
    assert!((t.fuel_percent - 1.0).abs() < 0.001);
    Ok(())
}

#[test]
fn fuel_overfull_clamped() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_FUEL_IN_TANK, 100.0);
    write_f32(&mut buf, OFF_FUEL_CAPACITY, 50.0);
    let t = adapter.normalize(&buf)?;
    assert!((t.fuel_percent - 1.0).abs() < 0.001, "clamped to 1.0");
    Ok(())
}

#[test]
fn fuel_zero_capacity_safe() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_FUEL_IN_TANK, 10.0);
    write_f32(&mut buf, OFF_FUEL_CAPACITY, 0.0);
    let t = adapter.normalize(&buf)?;
    // capacity max(0,1)=1 → fuel = 10/1 clamped to 1.0
    assert!(t.fuel_percent.is_finite());
    assert!(t.fuel_percent <= 1.0);
    Ok(())
}

#[test]
fn fuel_negative_in_tank_clamped() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_FUEL_IN_TANK, -5.0);
    write_f32(&mut buf, OFF_FUEL_CAPACITY, 50.0);
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.fuel_percent, 0.0, "negative fuel → 0");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Lap indexing (0-based + 1)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn lap_zero_based_to_one() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_CURRENT_LAP, 0.0);
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.lap, 1, "0-based(0) + 1 = 1");
    Ok(())
}

#[test]
fn lap_second_lap() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_CURRENT_LAP, 1.0);
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.lap, 2, "0-based(1) + 1 = 2");
    Ok(())
}

#[test]
fn lap_negative_clamped() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_CURRENT_LAP, -1.0);
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.lap, 1, "negative → 0 → +1 = 1");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Max gears extended field
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn max_gears_extracted() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_MAX_GEARS, 6.0);
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.num_gears, 6);
    Ok(())
}

#[test]
fn max_gears_zero() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let buf = make_packet();
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.num_gears, 0);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Car position
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn position_extracted() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_CAR_POSITION, 5.0);
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.position, 5);
    Ok(())
}

#[test]
fn position_negative_clamped_to_zero() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_CAR_POSITION, -3.0);
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.position, 0);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Tire temperatures: negative values clamped
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn tire_temps_negative_clamped_to_zero() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_BRAKES_TEMP_FL, -20.0);
    write_f32(&mut buf, OFF_BRAKES_TEMP_FR, -5.0);
    write_f32(&mut buf, OFF_BRAKES_TEMP_RL, 0.0);
    write_f32(&mut buf, OFF_BRAKES_TEMP_RR, 50.0);
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.tire_temps_c[0], 0, "negative → 0");
    assert_eq!(t.tire_temps_c[1], 0, "negative → 0");
    assert_eq!(t.tire_temps_c[2], 0);
    assert_eq!(t.tire_temps_c[3], 50);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// NaN injection at individual offsets
// ═══════════════════════════════════════════════════════════════════════════════

fn write_nan(buf: &mut [u8], off: usize) {
    write_f32(buf, off, f32::NAN);
}

fn write_inf(buf: &mut [u8], off: usize) {
    write_f32(buf, off, f32::INFINITY);
}

#[test]
fn nan_rpm_defaults_to_zero() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    write_nan(&mut buf, OFF_RPM);
    let t = adapter.normalize(&buf)?;
    assert!(t.rpm.is_finite());
    assert_eq!(t.rpm, 0.0);
    Ok(())
}

#[test]
fn nan_throttle_defaults_to_zero() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    write_nan(&mut buf, OFF_THROTTLE);
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.throttle, 0.0);
    Ok(())
}

#[test]
fn nan_brake_defaults_to_zero() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    write_nan(&mut buf, OFF_BRAKE);
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.brake, 0.0);
    Ok(())
}

#[test]
fn nan_steer_defaults_to_zero() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    write_nan(&mut buf, OFF_STEER);
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.steering_angle, 0.0);
    Ok(())
}

#[test]
fn nan_gear_defaults_to_neutral() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    write_nan(&mut buf, OFF_GEAR);
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.gear, 0, "NaN gear → neutral");
    Ok(())
}

#[test]
fn nan_wheel_speed_defaults_to_zero() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    write_nan(&mut buf, OFF_WHEEL_SPEED_FL);
    // Other wheels at 20 m/s, body at 20
    write_f32(&mut buf, OFF_WHEEL_SPEED_FR, 20.0);
    write_f32(&mut buf, OFF_WHEEL_SPEED_RL, 20.0);
    write_f32(&mut buf, OFF_WHEEL_SPEED_RR, 20.0);
    write_f32(&mut buf, OFF_VEL_X, 20.0);
    let t = adapter.normalize(&buf)?;
    assert!(t.speed_ms.is_finite());
    // avg = (0+20+20+20)/4 = 15 (NaN FL → 0)
    assert!((t.speed_ms - 15.0).abs() < 0.01);
    Ok(())
}

#[test]
fn inf_velocity_defaults_to_zero() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    write_inf(&mut buf, OFF_VEL_X);
    let t = adapter.normalize(&buf)?;
    assert!(t.speed_ms.is_finite());
    Ok(())
}

#[test]
fn nan_fuel_capacity_uses_default() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_FUEL_IN_TANK, 25.0);
    write_nan(&mut buf, OFF_FUEL_CAPACITY);
    let t = adapter.normalize(&buf)?;
    assert!(t.fuel_percent.is_finite());
    assert!(t.fuel_percent <= 1.0);
    Ok(())
}

#[test]
fn nan_lat_g_defaults_to_zero() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    write_nan(&mut buf, OFF_GFORCE_LAT);
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.lateral_g, 0.0);
    assert_eq!(t.ffb_scalar, 0.0, "NaN lat_g → ffb=0");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Wheel speed extended values
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn wheel_speed_extended_float_values() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_WHEEL_SPEED_FL, 18.5);
    write_f32(&mut buf, OFF_WHEEL_SPEED_FR, 19.0);
    write_f32(&mut buf, OFF_WHEEL_SPEED_RL, 17.5);
    write_f32(&mut buf, OFF_WHEEL_SPEED_RR, 18.0);
    let t = adapter.normalize(&buf)?;

    match t.extended.get("wheel_speed_fl") {
        Some(TelemetryValue::Float(f)) => assert!((*f - 18.5).abs() < 0.01),
        other => return Err(format!("expected Float, got {other:?}").into()),
    }
    match t.extended.get("wheel_speed_rr") {
        Some(TelemetryValue::Float(f)) => assert!((*f - 18.0).abs() < 0.01),
        other => return Err(format!("expected Float, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn wheel_speed_negative_uses_abs() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_WHEEL_SPEED_FL, -15.0);
    let t = adapter.normalize(&buf)?;
    match t.extended.get("wheel_speed_fl") {
        Some(TelemetryValue::Float(f)) => assert!((*f - 15.0).abs() < 0.01, "abs(-15)=15"),
        other => return Err(format!("expected Float, got {other:?}").into()),
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// RPM fraction extended field
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn rpm_fraction_at_redline() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_RPM, 8000.0);
    write_f32(&mut buf, OFF_MAX_RPM, 8000.0);
    let t = adapter.normalize(&buf)?;
    match t.extended.get("rpm_fraction") {
        Some(TelemetryValue::Float(f)) => assert!((*f - 1.0).abs() < 0.001),
        other => return Err(format!("expected Float, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn rpm_fraction_absent_when_max_rpm_zero() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_RPM, 5000.0);
    // max_rpm = 0 → no rpm_fraction extended field
    let t = adapter.normalize(&buf)?;
    assert!(!t.extended.contains_key("rpm_fraction"), "no rpm_fraction when max_rpm=0");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Speed asymmetric wheel speeds
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn speed_asymmetric_wheels() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_WHEEL_SPEED_FL, 25.0);
    write_f32(&mut buf, OFF_WHEEL_SPEED_FR, 30.0);
    write_f32(&mut buf, OFF_WHEEL_SPEED_RL, 20.0);
    write_f32(&mut buf, OFF_WHEEL_SPEED_RR, 25.0);
    let t = adapter.normalize(&buf)?;
    // avg = (25+30+20+25)/4 = 25.0
    assert!((t.speed_ms - 25.0).abs() < 0.01);
    Ok(())
}

#[test]
fn speed_body_velocity_3d() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    // All wheel speeds = 0 → fallback to body velocity
    write_f32(&mut buf, OFF_VEL_X, 2.0);
    write_f32(&mut buf, OFF_VEL_Y, 3.0);
    write_f32(&mut buf, OFF_VEL_Z, 6.0);
    let t = adapter.normalize(&buf)?;
    // sqrt(4+9+36) = 7.0
    assert!((t.speed_ms - 7.0).abs() < 0.01, "speed_ms={}", t.speed_ms);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Brake negative clamped
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn brake_negative_clamped() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_BRAKE, -0.5);
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.brake, 0.0);
    Ok(())
}

#[test]
fn throttle_negative_clamped() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_THROTTLE, -0.3);
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.throttle, 0.0);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Adapter async operations
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn async_is_game_running_false_by_default() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    assert!(!adapter.is_game_running().await?);
    Ok(())
}

#[tokio::test]
async fn async_stop_monitoring_succeeds() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    adapter.stop_monitoring().await?;
    Ok(())
}

#[test]
fn adapter_clone_preserves_identity() -> TestResult {
    let adapter = WrcGenerationsAdapter::new().with_port(9999);
    let cloned = adapter.clone();
    assert_eq!(adapter.game_id(), cloned.game_id());
    assert_eq!(adapter.expected_update_rate(), cloned.expected_update_rate());
    Ok(())
}

#[test]
fn adapter_default_trait() -> TestResult {
    let adapter = WrcGenerationsAdapter::default();
    assert_eq!(adapter.game_id(), "wrc_generations");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Timing fields
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn timing_negative_lap_time_clamped() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_LAP_TIME, -5.0);
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.current_lap_time_s, 0.0, "negative → 0");
    Ok(())
}

#[test]
fn timing_negative_last_lap_time_clamped() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_LAST_LAP_TIME, -10.0);
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.last_lap_time_s, 0.0, "negative → 0");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Full rally-stage scenario
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn scenario_monte_carlo_stage() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_LAP_TIME, 182.5);
    write_f32(&mut buf, OFF_LAST_LAP_TIME, 175.3);
    write_f32(&mut buf, OFF_CURRENT_LAP, 1.0);
    write_f32(&mut buf, OFF_CAR_POSITION, 2.0);
    write_f32(&mut buf, OFF_RPM, 6500.0);
    write_f32(&mut buf, OFF_MAX_RPM, 8500.0);
    write_f32(&mut buf, OFF_GEAR, 4.0);
    write_f32(&mut buf, OFF_THROTTLE, 0.85);
    write_f32(&mut buf, OFF_BRAKE, 0.0);
    write_f32(&mut buf, OFF_STEER, -0.15);
    write_f32(&mut buf, OFF_GFORCE_LAT, 0.9);
    write_f32(&mut buf, OFF_GFORCE_LON, 0.3);
    write_f32(&mut buf, OFF_FUEL_IN_TANK, 30.0);
    write_f32(&mut buf, OFF_FUEL_CAPACITY, 60.0);
    write_f32(&mut buf, OFF_MAX_GEARS, 6.0);
    for off in [OFF_WHEEL_SPEED_FL, OFF_WHEEL_SPEED_FR, OFF_WHEEL_SPEED_RL, OFF_WHEEL_SPEED_RR] {
        write_f32(&mut buf, off, 28.0);
    }
    write_f32(&mut buf, OFF_VEL_X, 28.0);
    write_f32(&mut buf, OFF_BRAKES_TEMP_FL, 180.0);
    write_f32(&mut buf, OFF_BRAKES_TEMP_FR, 175.0);
    write_f32(&mut buf, OFF_BRAKES_TEMP_RL, 160.0);
    write_f32(&mut buf, OFF_BRAKES_TEMP_RR, 165.0);
    write_f32(&mut buf, OFF_TYRES_PRESSURE_FL, 26.5);
    write_f32(&mut buf, OFF_TYRES_PRESSURE_FR, 27.0);
    write_f32(&mut buf, OFF_TYRES_PRESSURE_RL, 25.5);
    write_f32(&mut buf, OFF_TYRES_PRESSURE_RR, 26.0);

    let t = adapter.normalize(&buf)?;

    assert!((t.current_lap_time_s - 182.5).abs() < 0.01);
    assert!((t.last_lap_time_s - 175.3).abs() < 0.01);
    assert_eq!(t.lap, 2, "0-based(1)+1");
    assert_eq!(t.position, 2);
    assert!((t.rpm - 6500.0).abs() < 0.1);
    assert!((t.max_rpm - 8500.0).abs() < 0.1);
    assert_eq!(t.gear, 4);
    assert!((t.throttle - 0.85).abs() < 0.001);
    assert_eq!(t.brake, 0.0);
    assert!((t.steering_angle - (-0.15)).abs() < 0.001);
    assert!((t.lateral_g - 0.9).abs() < 0.01);
    assert!((t.longitudinal_g - 0.3).abs() < 0.01);
    // ffb = 0.9/3.0 = 0.3
    assert!((t.ffb_scalar - 0.3).abs() < 0.01);
    assert!((t.fuel_percent - 0.5).abs() < 0.001);
    assert_eq!(t.num_gears, 6);
    assert_eq!(t.tire_temps_c[0], 180);
    assert_eq!(t.tire_temps_c[1], 175);
    assert!((t.tire_pressures_psi[0] - 26.5).abs() < 0.01);
    assert!(!t.flags.in_pits);
    Ok(())
}

#[test]
fn scenario_heavy_braking_spin() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_RPM, 3000.0);
    write_f32(&mut buf, OFF_GEAR, 2.0);
    write_f32(&mut buf, OFF_THROTTLE, 0.0);
    write_f32(&mut buf, OFF_BRAKE, 1.0);
    write_f32(&mut buf, OFF_STEER, 0.8);
    write_f32(&mut buf, OFF_GFORCE_LAT, 2.8);
    // Wheels locked, body still moving
    write_f32(&mut buf, OFF_VEL_X, 15.0);
    write_f32(&mut buf, OFF_VEL_Y, 5.0);

    let t = adapter.normalize(&buf)?;

    assert_eq!(t.throttle, 0.0);
    assert!((t.brake - 1.0).abs() < 0.001, "full brake");
    assert!((t.steering_angle - 0.8).abs() < 0.001);
    assert_eq!(t.gear, 2);
    // ffb = 2.8/3.0 = 0.933…
    assert!((t.ffb_scalar - (2.8 / 3.0)).abs() < 0.01);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Determinism
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn deterministic_output() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_RPM, 5500.0);
    write_f32(&mut buf, OFF_GEAR, 3.0);
    write_f32(&mut buf, OFF_THROTTLE, 0.7);
    write_f32(&mut buf, OFF_STEER, -0.3);
    let t1 = adapter.normalize(&buf)?;
    let t2 = adapter.normalize(&buf)?;
    assert_eq!(t1.rpm, t2.rpm);
    assert_eq!(t1.gear, t2.gear);
    assert_eq!(t1.throttle, t2.throttle);
    assert_eq!(t1.steering_angle, t2.steering_angle);
    assert_eq!(t1.slip_ratio, t2.slip_ratio);
    Ok(())
}
