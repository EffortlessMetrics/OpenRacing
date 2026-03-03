//! Deep tests for the RaceRoom Racing Experience (R3E) telemetry adapter.
//!
//! Covers shared-memory parsing, version validation, pause/menu handling,
//! RPM conversion (rad/s → RPM), G-force sign conventions, flag activation,
//! tire temps/pressures, timing, and fuel calculations.

use racing_wheel_telemetry_raceroom::{RaceRoomAdapter, TelemetryAdapter};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const VIEW_SIZE: usize = 4096;
const R3E_VERSION_MAJOR: i32 = 3;
const G_ACCEL: f32 = 9.80665;

// R3E shared-memory byte offsets
const OFF_VERSION_MAJOR: usize = 0;
const OFF_GAME_PAUSED: usize = 20;
const OFF_GAME_IN_MENUS: usize = 24;
const OFF_SPEED: usize = 1392;
const OFF_ENGINE_RPS: usize = 1396;
const OFF_MAX_ENGINE_RPS: usize = 1400;
const OFF_GEAR: usize = 1408;
const OFF_LOCAL_ACCEL_X: usize = 1440;
const OFF_LOCAL_ACCEL_Y: usize = 1444;
const OFF_LOCAL_ACCEL_Z: usize = 1448;
const OFF_FUEL_LEFT: usize = 1456;
const OFF_FUEL_CAPACITY: usize = 1460;
const OFF_THROTTLE: usize = 1500;
const OFF_BRAKE: usize = 1508;
const OFF_CLUTCH: usize = 1516;
const OFF_STEER_INPUT: usize = 1524;
const OFF_POSITION: usize = 988;
const OFF_COMPLETED_LAPS: usize = 1028;
const OFF_LAP_TIME_BEST: usize = 1068;
const OFF_LAP_TIME_PREVIOUS: usize = 1084;
const OFF_LAP_TIME_CURRENT: usize = 1100;
const OFF_DELTA_FRONT: usize = 1124;
const OFF_DELTA_BEHIND: usize = 1128;
const OFF_FLAG_YELLOW: usize = 932;
const OFF_FLAG_BLUE: usize = 964;
const OFF_FLAG_GREEN: usize = 972;
const OFF_FLAG_CHECKERED: usize = 976;
const OFF_IN_PITLANE: usize = 848;
const OFF_PIT_LIMITER: usize = 1572;
const OFF_AID_ABS: usize = 1536;
const OFF_AID_TC: usize = 1540;
const OFF_TIRE_TEMP_FL: usize = 1748;
const OFF_TIRE_TEMP_FR: usize = 1772;
const OFF_TIRE_TEMP_RL: usize = 1796;
const OFF_TIRE_TEMP_RR: usize = 1820;
const OFF_TIRE_PRESSURE_FL: usize = 1712;
const OFF_TIRE_PRESSURE_FR: usize = 1716;
const OFF_TIRE_PRESSURE_RL: usize = 1720;
const OFF_TIRE_PRESSURE_RR: usize = 1724;

fn write_f32(buf: &mut [u8], off: usize, v: f32) {
    buf[off..off + 4].copy_from_slice(&v.to_le_bytes());
}

fn write_i32(buf: &mut [u8], off: usize, v: i32) {
    buf[off..off + 4].copy_from_slice(&v.to_le_bytes());
}

/// Create a valid R3E memory buffer with version set and game active.
fn make_r3e() -> Vec<u8> {
    let mut buf = vec![0u8; VIEW_SIZE];
    write_i32(&mut buf, OFF_VERSION_MAJOR, R3E_VERSION_MAJOR);
    write_i32(&mut buf, OFF_GAME_PAUSED, 0);
    write_i32(&mut buf, OFF_GAME_IN_MENUS, 0);
    buf
}

// ── Adapter identity ─────────────────────────────────────────────────────────

#[test]
fn deep_game_id() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    assert_eq!(adapter.game_id(), "raceroom");
    Ok(())
}

#[test]
fn deep_update_rate() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    assert_eq!(
        adapter.expected_update_rate(),
        std::time::Duration::from_millis(10)
    );
    Ok(())
}

// ── Packet rejection ─────────────────────────────────────────────────────────

#[test]
fn deep_rejects_empty() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    assert!(adapter.normalize(&[]).is_err());
    Ok(())
}

#[test]
fn deep_rejects_too_small() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    assert!(adapter.normalize(&[0u8; 100]).is_err());
    Ok(())
}

#[test]
fn deep_rejects_wrong_version() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut buf = vec![0u8; VIEW_SIZE];
    write_i32(&mut buf, OFF_VERSION_MAJOR, 1); // wrong version
    assert!(adapter.normalize(&buf).is_err());
    Ok(())
}

// ── Pause / menu handling ────────────────────────────────────────────────────

#[test]
fn deep_paused_returns_default() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut buf = make_r3e();
    write_i32(&mut buf, OFF_GAME_PAUSED, 1);
    write_f32(&mut buf, OFF_SPEED, 100.0);
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.rpm, 0.0, "paused → all defaults");
    assert_eq!(t.speed_ms, 0.0);
    Ok(())
}

#[test]
fn deep_in_menus_returns_default() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut buf = make_r3e();
    write_i32(&mut buf, OFF_GAME_IN_MENUS, 1);
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.rpm, 0.0, "in menus → all defaults");
    Ok(())
}

// ── RPM conversion (rad/s → RPM) ────────────────────────────────────────────

#[test]
fn deep_rpm_from_rps() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut buf = make_r3e();
    // RPM = rps × 30/π; target 6000 RPM → rps = 6000 × π/30
    let rps = 6000.0_f32 * std::f32::consts::PI / 30.0;
    write_f32(&mut buf, OFF_ENGINE_RPS, rps);
    let t = adapter.normalize(&buf)?;
    assert!((t.rpm - 6000.0).abs() < 1.0, "rpm={}", t.rpm);
    Ok(())
}

#[test]
fn deep_max_rpm_from_rps() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut buf = make_r3e();
    let max_rps = 8500.0_f32 * std::f32::consts::PI / 30.0;
    write_f32(&mut buf, OFF_MAX_ENGINE_RPS, max_rps);
    let t = adapter.normalize(&buf)?;
    assert!((t.max_rpm - 8500.0).abs() < 1.0, "max_rpm={}", t.max_rpm);
    Ok(())
}

// ── Speed, steering, inputs ──────────────────────────────────────────────────

#[test]
fn deep_speed_abs_value() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut buf = make_r3e();
    write_f32(&mut buf, OFF_SPEED, -30.0); // negative speed → abs
    let t = adapter.normalize(&buf)?;
    assert!((t.speed_ms - 30.0).abs() < 0.01, "speed_ms={}", t.speed_ms);
    Ok(())
}

#[test]
fn deep_steering_clamped() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut buf = make_r3e();
    write_f32(&mut buf, OFF_STEER_INPUT, 2.5);
    let t = adapter.normalize(&buf)?;
    assert!((t.steering_angle - 1.0).abs() < 0.001, "clamped to 1.0");
    Ok(())
}

#[test]
fn deep_throttle_brake_clutch() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut buf = make_r3e();
    write_f32(&mut buf, OFF_THROTTLE, 0.8);
    write_f32(&mut buf, OFF_BRAKE, 0.3);
    write_f32(&mut buf, OFF_CLUTCH, 0.5);
    let t = adapter.normalize(&buf)?;
    assert!((t.throttle - 0.8).abs() < 0.001);
    assert!((t.brake - 0.3).abs() < 0.001);
    assert!((t.clutch - 0.5).abs() < 0.001);
    Ok(())
}

// ── Gear encoding ────────────────────────────────────────────────────────────

#[test]
fn deep_gear_values() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    for (raw, expected) in [(-1i32, -1i8), (0, 0), (1, 1), (5, 5)] {
        let mut buf = make_r3e();
        write_i32(&mut buf, OFF_GEAR, raw);
        let t = adapter.normalize(&buf)?;
        assert_eq!(t.gear, expected, "raw={raw}");
    }
    Ok(())
}

// ── G-force sign conventions ─────────────────────────────────────────────────

#[test]
fn deep_g_force_sign_conventions() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut buf = make_r3e();
    // R3E: +X=left, +Y=up, +Z=back
    write_f32(&mut buf, OFF_LOCAL_ACCEL_X, G_ACCEL); // 1G left
    write_f32(&mut buf, OFF_LOCAL_ACCEL_Y, G_ACCEL); // 1G up
    write_f32(&mut buf, OFF_LOCAL_ACCEL_Z, G_ACCEL); // 1G back
    let t = adapter.normalize(&buf)?;
    // lateral: -(+X/G) = -1.0
    assert!((t.lateral_g - (-1.0)).abs() < 0.01, "lat_g={}", t.lateral_g);
    // vertical: +Y/G = 1.0
    assert!((t.vertical_g - 1.0).abs() < 0.01, "vert_g={}", t.vertical_g);
    // longitudinal: -(+Z/G) = -1.0
    assert!(
        (t.longitudinal_g - (-1.0)).abs() < 0.01,
        "lon_g={}",
        t.longitudinal_g
    );
    Ok(())
}

// ── Fuel calculation ─────────────────────────────────────────────────────────

#[test]
fn deep_fuel_percent_normal() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut buf = make_r3e();
    write_f32(&mut buf, OFF_FUEL_LEFT, 25.0);
    write_f32(&mut buf, OFF_FUEL_CAPACITY, 100.0);
    let t = adapter.normalize(&buf)?;
    assert!((t.fuel_percent - 0.25).abs() < 0.001);
    Ok(())
}

#[test]
fn deep_fuel_zero_capacity() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut buf = make_r3e();
    write_f32(&mut buf, OFF_FUEL_LEFT, 50.0);
    write_f32(&mut buf, OFF_FUEL_CAPACITY, 0.0);
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.fuel_percent, 0.0, "zero capacity → 0%");
    Ok(())
}

// ── Flags ────────────────────────────────────────────────────────────────────

#[test]
fn deep_all_flags() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut buf = make_r3e();
    write_i32(&mut buf, OFF_FLAG_YELLOW, 1);
    write_i32(&mut buf, OFF_FLAG_BLUE, 1);
    write_i32(&mut buf, OFF_FLAG_GREEN, 1);
    write_i32(&mut buf, OFF_FLAG_CHECKERED, 1);
    write_i32(&mut buf, OFF_IN_PITLANE, 1);
    write_i32(&mut buf, OFF_PIT_LIMITER, 1);
    write_i32(&mut buf, OFF_AID_ABS, 5);
    write_i32(&mut buf, OFF_AID_TC, 5);
    let t = adapter.normalize(&buf)?;
    assert!(t.flags.yellow_flag);
    assert!(t.flags.blue_flag);
    assert!(t.flags.green_flag);
    assert!(t.flags.checkered_flag);
    assert!(t.flags.in_pits);
    assert!(t.flags.pit_limiter);
    assert!(t.flags.abs_active);
    assert!(t.flags.traction_control);
    Ok(())
}

#[test]
fn deep_abs_tc_only_active_at_value_5() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut buf = make_r3e();
    write_i32(&mut buf, OFF_AID_ABS, 3); // not 5
    write_i32(&mut buf, OFF_AID_TC, 1); // not 5
    let t = adapter.normalize(&buf)?;
    assert!(!t.flags.abs_active, "ABS needs value 5");
    assert!(!t.flags.traction_control, "TC needs value 5");
    Ok(())
}

// ── Tire temps and pressures ─────────────────────────────────────────────────

#[test]
fn deep_tire_temps_conversion() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut buf = make_r3e();
    write_f32(&mut buf, OFF_TIRE_TEMP_FL, 90.0);
    write_f32(&mut buf, OFF_TIRE_TEMP_FR, 95.0);
    write_f32(&mut buf, OFF_TIRE_TEMP_RL, 88.0);
    write_f32(&mut buf, OFF_TIRE_TEMP_RR, 92.0);
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.tire_temps_c, [90, 95, 88, 92]);
    Ok(())
}

#[test]
fn deep_tire_pressures_kpa_to_psi() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut buf = make_r3e();
    write_f32(&mut buf, OFF_TIRE_PRESSURE_FL, 200.0);
    write_f32(&mut buf, OFF_TIRE_PRESSURE_FR, 200.0);
    write_f32(&mut buf, OFF_TIRE_PRESSURE_RL, 200.0);
    write_f32(&mut buf, OFF_TIRE_PRESSURE_RR, 200.0);
    let t = adapter.normalize(&buf)?;
    // 200 kPa × 0.14504 ≈ 29.0 PSI
    for &psi in &t.tire_pressures_psi {
        assert!((psi - 29.0).abs() < 0.2, "psi={psi}");
    }
    Ok(())
}

// ── Timing fields ────────────────────────────────────────────────────────────

#[test]
fn deep_timing_and_position() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut buf = make_r3e();
    write_i32(&mut buf, OFF_POSITION, 5);
    write_i32(&mut buf, OFF_COMPLETED_LAPS, 12);
    write_f32(&mut buf, OFF_LAP_TIME_CURRENT, 75.3);
    write_f32(&mut buf, OFF_LAP_TIME_BEST, 72.1);
    write_f32(&mut buf, OFF_LAP_TIME_PREVIOUS, 73.5);
    write_f32(&mut buf, OFF_DELTA_FRONT, 1.5);
    write_f32(&mut buf, OFF_DELTA_BEHIND, 2.3);
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.position, 5);
    assert_eq!(t.lap, 12);
    assert!((t.current_lap_time_s - 75.3).abs() < 0.01);
    assert!((t.best_lap_time_s - 72.1).abs() < 0.01);
    assert!((t.last_lap_time_s - 73.5).abs() < 0.01);
    assert!((t.delta_ahead_s - 1.5).abs() < 0.01);
    assert!((t.delta_behind_s - 2.3).abs() < 0.01);
    Ok(())
}

// ── Extended fuel fields ─────────────────────────────────────────────────────

#[test]
fn deep_extended_fuel_fields() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut buf = make_r3e();
    write_f32(&mut buf, OFF_FUEL_LEFT, 35.0);
    write_f32(&mut buf, OFF_FUEL_CAPACITY, 70.0);
    let t = adapter.normalize(&buf)?;
    assert!(t.extended.contains_key("fuel_left_l"));
    assert!(t.extended.contains_key("fuel_capacity_l"));
    Ok(())
}
