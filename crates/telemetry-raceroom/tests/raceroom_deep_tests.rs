//! Deep tests for RaceRoom Racing Experience (R3E) shared memory telemetry.
//!
//! Covers shared-memory layout validation, all data sections (vehicle state,
//! scoring, flags, tires, G-forces, fuel), session-type enumeration, boundary
//! values, and NaN/Inf resilience.

use racing_wheel_telemetry_raceroom::{RaceRoomAdapter, TelemetryAdapter};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const VIEW_SIZE: usize = 4096;
const R3E_VERSION_MAJOR: i32 = 3;
const G_ACCEL: f32 = 9.80665;

// ── R3E shared-memory byte offsets ───────────────────────────────────────────
const OFF_VERSION_MAJOR: usize = 0;
const OFF_GAME_PAUSED: usize = 20;
const OFF_GAME_IN_MENUS: usize = 24;
const OFF_SPEED: usize = 1392;
const OFF_ENGINE_RPS: usize = 1396;
const OFF_MAX_ENGINE_RPS: usize = 1400;
const OFF_GEAR: usize = 1408;
const OFF_NUM_GEARS: usize = 1412;
const OFF_LOCAL_ACCEL_X: usize = 1440;
const OFF_LOCAL_ACCEL_Y: usize = 1444;
const OFF_LOCAL_ACCEL_Z: usize = 1448;
const OFF_FUEL_LEFT: usize = 1456;
const OFF_FUEL_CAPACITY: usize = 1460;
const OFF_ENGINE_TEMP: usize = 1480;
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

fn make_r3e() -> Vec<u8> {
    let mut buf = vec![0u8; VIEW_SIZE];
    write_i32(&mut buf, OFF_VERSION_MAJOR, R3E_VERSION_MAJOR);
    buf
}

// ═══════════════════════════════════════════════════════════════════════════════
// Shared-memory layout: offset sanity checks
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn layout_all_offsets_within_view_size() -> TestResult {
    // Every offset + 4 bytes must fit within the 4096-byte view.
    let offsets = [
        OFF_VERSION_MAJOR, OFF_GAME_PAUSED, OFF_GAME_IN_MENUS, OFF_SPEED,
        OFF_ENGINE_RPS, OFF_MAX_ENGINE_RPS, OFF_GEAR, OFF_NUM_GEARS,
        OFF_LOCAL_ACCEL_X, OFF_LOCAL_ACCEL_Y, OFF_LOCAL_ACCEL_Z,
        OFF_FUEL_LEFT, OFF_FUEL_CAPACITY, OFF_ENGINE_TEMP,
        OFF_THROTTLE, OFF_BRAKE, OFF_CLUTCH, OFF_STEER_INPUT,
        OFF_POSITION, OFF_COMPLETED_LAPS,
        OFF_LAP_TIME_BEST, OFF_LAP_TIME_PREVIOUS, OFF_LAP_TIME_CURRENT,
        OFF_DELTA_FRONT, OFF_DELTA_BEHIND,
        OFF_FLAG_YELLOW, OFF_FLAG_BLUE, OFF_FLAG_GREEN, OFF_FLAG_CHECKERED,
        OFF_IN_PITLANE, OFF_PIT_LIMITER, OFF_AID_ABS, OFF_AID_TC,
        OFF_TIRE_TEMP_FL, OFF_TIRE_TEMP_FR, OFF_TIRE_TEMP_RL, OFF_TIRE_TEMP_RR,
        OFF_TIRE_PRESSURE_FL, OFF_TIRE_PRESSURE_FR, OFF_TIRE_PRESSURE_RL,
        OFF_TIRE_PRESSURE_RR,
    ];
    for &off in &offsets {
        assert!(
            off + 4 <= VIEW_SIZE,
            "offset {off} + 4 exceeds VIEW_SIZE {VIEW_SIZE}"
        );
    }
    Ok(())
}

#[test]
fn layout_no_overlapping_adjacent_fields() -> TestResult {
    // Vehicle state group: speed, engine_rps, max_engine_rps, gear should not
    // overlap each other (each is 4 bytes).
    let vehicle_offsets = [OFF_SPEED, OFF_ENGINE_RPS, OFF_MAX_ENGINE_RPS, OFF_GEAR];
    for i in 0..vehicle_offsets.len() {
        for j in (i + 1)..vehicle_offsets.len() {
            let a = vehicle_offsets[i];
            let b = vehicle_offsets[j];
            assert!(
                a + 4 <= b || b + 4 <= a,
                "vehicle state fields at {a} and {b} overlap"
            );
        }
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Session type / game-state enumeration
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn session_paused_and_in_menus_both_set() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut buf = make_r3e();
    write_i32(&mut buf, OFF_GAME_PAUSED, 1);
    write_i32(&mut buf, OFF_GAME_IN_MENUS, 1);
    write_f32(&mut buf, OFF_SPEED, 80.0);
    write_f32(&mut buf, OFF_ENGINE_RPS, 500.0);
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.speed_ms, 0.0, "both paused+menus → defaults");
    assert_eq!(t.rpm, 0.0);
    Ok(())
}

#[test]
fn session_version_zero_rejected() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let buf = vec![0u8; VIEW_SIZE]; // version_major = 0
    assert!(adapter.normalize(&buf).is_err());
    Ok(())
}

#[test]
fn session_version_future_major_rejected() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut buf = vec![0u8; VIEW_SIZE];
    write_i32(&mut buf, OFF_VERSION_MAJOR, 99);
    assert!(adapter.normalize(&buf).is_err());
    Ok(())
}

#[test]
fn session_exact_view_size_accepted() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut buf = vec![0u8; VIEW_SIZE];
    write_i32(&mut buf, OFF_VERSION_MAJOR, R3E_VERSION_MAJOR);
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.speed_ms, 0.0);
    Ok(())
}

#[test]
fn session_larger_than_view_accepted() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut buf = vec![0u8; VIEW_SIZE + 512];
    write_i32(&mut buf, OFF_VERSION_MAJOR, R3E_VERSION_MAJOR);
    write_f32(&mut buf, OFF_SPEED, 33.0);
    let t = adapter.normalize(&buf)?;
    assert!((t.speed_ms - 33.0).abs() < 0.01);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Vehicle state data section
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn vehicle_rpm_roundtrip_accuracy() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    for target_rpm in [0.0_f32, 800.0, 3000.0, 6000.0, 9500.0, 12000.0] {
        let mut buf = make_r3e();
        let rps = target_rpm * std::f32::consts::PI / 30.0;
        write_f32(&mut buf, OFF_ENGINE_RPS, rps);
        let t = adapter.normalize(&buf)?;
        assert!(
            (t.rpm - target_rpm).abs() < 1.0,
            "target={target_rpm}, got={}",
            t.rpm
        );
    }
    Ok(())
}

#[test]
fn vehicle_steering_negative_full_lock() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut buf = make_r3e();
    write_f32(&mut buf, OFF_STEER_INPUT, -1.0);
    let t = adapter.normalize(&buf)?;
    assert!((t.steering_angle - (-1.0)).abs() < 0.001);
    Ok(())
}

#[test]
fn vehicle_steering_overrange_negative() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut buf = make_r3e();
    write_f32(&mut buf, OFF_STEER_INPUT, -3.5);
    let t = adapter.normalize(&buf)?;
    assert!((t.steering_angle - (-1.0)).abs() < 0.001, "clamped to -1.0");
    Ok(())
}

#[test]
fn vehicle_gear_reverse() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut buf = make_r3e();
    write_i32(&mut buf, OFF_GEAR, -1);
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.gear, -1);
    Ok(())
}

#[test]
fn vehicle_gear_high_value_clamped() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut buf = make_r3e();
    write_i32(&mut buf, OFF_GEAR, 200);
    let t = adapter.normalize(&buf)?;
    // R3E clamps to (-1, 127) then truncates to i8; 200 → 127
    assert_eq!(t.gear, 127, "gear clamped to i8 max, got {}", t.gear);
    Ok(())
}

#[test]
fn vehicle_num_gears_valid() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut buf = make_r3e();
    write_i32(&mut buf, OFF_NUM_GEARS, 7);
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.num_gears, 7);
    Ok(())
}

#[test]
fn vehicle_num_gears_na_not_set() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut buf = make_r3e();
    write_i32(&mut buf, OFF_NUM_GEARS, -1);
    let t = adapter.normalize(&buf)?;
    // num_gears should remain default (0) when N/A
    assert_eq!(t.num_gears, 0, "N/A num_gears → 0");
    Ok(())
}

#[test]
fn vehicle_engine_temp_negative_not_set() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut buf = make_r3e();
    write_f32(&mut buf, OFF_ENGINE_TEMP, -1.0);
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.engine_temp_c, 0.0, "negative engine temp → default");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// G-force data section
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn gforce_zero_accel_gives_zero_g() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let buf = make_r3e();
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.lateral_g, 0.0);
    assert_eq!(t.longitudinal_g, 0.0);
    assert_eq!(t.vertical_g, 0.0);
    Ok(())
}

#[test]
fn gforce_heavy_braking_positive_longitudinal() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut buf = make_r3e();
    // R3E: +Z = back → heavy braking = negative Z → negated → positive lon_g
    write_f32(&mut buf, OFF_LOCAL_ACCEL_Z, -2.0 * G_ACCEL);
    let t = adapter.normalize(&buf)?;
    assert!(
        (t.longitudinal_g - 2.0).abs() < 0.01,
        "braking → positive lon_g, got {}",
        t.longitudinal_g
    );
    Ok(())
}

#[test]
fn gforce_right_turn_positive_lateral() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut buf = make_r3e();
    // R3E: +X = left. In a right turn, accel pushes left (positive X).
    // lateral_g = -(+X/G) = negative → but our convention: right turn → positive
    // Actually: right turn → centrifugal force pushes right → R3E accel_x negative
    write_f32(&mut buf, OFF_LOCAL_ACCEL_X, -1.5 * G_ACCEL);
    let t = adapter.normalize(&buf)?;
    assert!(
        (t.lateral_g - 1.5).abs() < 0.01,
        "right turn → positive lat_g, got {}",
        t.lateral_g
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Fuel data section
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn fuel_full_tank() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut buf = make_r3e();
    write_f32(&mut buf, OFF_FUEL_LEFT, 80.0);
    write_f32(&mut buf, OFF_FUEL_CAPACITY, 80.0);
    let t = adapter.normalize(&buf)?;
    assert!((t.fuel_percent - 1.0).abs() < 0.001);
    Ok(())
}

#[test]
fn fuel_overfilled_clamped_to_one() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut buf = make_r3e();
    write_f32(&mut buf, OFF_FUEL_LEFT, 120.0);
    write_f32(&mut buf, OFF_FUEL_CAPACITY, 80.0);
    let t = adapter.normalize(&buf)?;
    assert!(
        (t.fuel_percent - 1.0).abs() < 0.001,
        "overfilled clamped to 1.0"
    );
    Ok(())
}

#[test]
fn fuel_extended_fields_present() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut buf = make_r3e();
    write_f32(&mut buf, OFF_FUEL_LEFT, 42.0);
    write_f32(&mut buf, OFF_FUEL_CAPACITY, 80.0);
    let t = adapter.normalize(&buf)?;
    assert!(t.extended.contains_key("fuel_left_l"), "fuel_left_l present");
    assert!(
        t.extended.contains_key("fuel_capacity_l"),
        "fuel_capacity_l present"
    );
    Ok(())
}

#[test]
fn fuel_zero_left_no_extended() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut buf = make_r3e();
    write_f32(&mut buf, OFF_FUEL_LEFT, 0.0);
    write_f32(&mut buf, OFF_FUEL_CAPACITY, 60.0);
    let t = adapter.normalize(&buf)?;
    assert!(
        !t.extended.contains_key("fuel_left_l"),
        "zero fuel_left → no extended key"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Flag data section
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn flags_none_active_by_default() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let buf = make_r3e();
    let t = adapter.normalize(&buf)?;
    assert!(!t.flags.yellow_flag);
    assert!(!t.flags.blue_flag);
    assert!(!t.flags.checkered_flag);
    assert!(!t.flags.in_pits);
    assert!(!t.flags.pit_limiter);
    assert!(!t.flags.abs_active);
    assert!(!t.flags.traction_control);
    Ok(())
}

#[test]
fn flags_na_value_treated_as_inactive() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut buf = make_r3e();
    // R3E uses -1 for N/A
    write_i32(&mut buf, OFF_FLAG_YELLOW, -1);
    write_i32(&mut buf, OFF_FLAG_BLUE, -1);
    let t = adapter.normalize(&buf)?;
    assert!(!t.flags.yellow_flag, "-1 = N/A, not active");
    assert!(!t.flags.blue_flag, "-1 = N/A, not active");
    Ok(())
}

#[test]
fn flags_individual_yellow_only() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut buf = make_r3e();
    write_i32(&mut buf, OFF_FLAG_YELLOW, 1);
    let t = adapter.normalize(&buf)?;
    assert!(t.flags.yellow_flag);
    assert!(!t.flags.blue_flag);
    assert!(!t.flags.checkered_flag);
    Ok(())
}

#[test]
fn flags_checkered_only() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut buf = make_r3e();
    write_i32(&mut buf, OFF_FLAG_CHECKERED, 1);
    let t = adapter.normalize(&buf)?;
    assert!(t.flags.checkered_flag);
    assert!(!t.flags.yellow_flag);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Tire data section
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn tires_temps_negative_treated_as_zero() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut buf = make_r3e();
    write_f32(&mut buf, OFF_TIRE_TEMP_FL, -1.0);
    write_f32(&mut buf, OFF_TIRE_TEMP_FR, -1.0);
    write_f32(&mut buf, OFF_TIRE_TEMP_RL, -1.0);
    write_f32(&mut buf, OFF_TIRE_TEMP_RR, -1.0);
    let t = adapter.normalize(&buf)?;
    // All N/A (-1.0) → 0, so tire_temps_c should remain default [0,0,0,0]
    assert_eq!(t.tire_temps_c, [0, 0, 0, 0]);
    Ok(())
}

#[test]
fn tires_temps_high_value_clamped_to_255() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut buf = make_r3e();
    write_f32(&mut buf, OFF_TIRE_TEMP_FL, 300.0);
    write_f32(&mut buf, OFF_TIRE_TEMP_FR, 400.0);
    write_f32(&mut buf, OFF_TIRE_TEMP_RL, 100.0);
    write_f32(&mut buf, OFF_TIRE_TEMP_RR, 50.0);
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.tire_temps_c[0], 255, "FL clamped to 255");
    assert_eq!(t.tire_temps_c[1], 255, "FR clamped to 255");
    assert_eq!(t.tire_temps_c[2], 100);
    assert_eq!(t.tire_temps_c[3], 50);
    Ok(())
}

#[test]
fn tires_pressure_negative_treated_as_zero() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut buf = make_r3e();
    write_f32(&mut buf, OFF_TIRE_PRESSURE_FL, -1.0);
    write_f32(&mut buf, OFF_TIRE_PRESSURE_FR, -1.0);
    write_f32(&mut buf, OFF_TIRE_PRESSURE_RL, -1.0);
    write_f32(&mut buf, OFF_TIRE_PRESSURE_RR, -1.0);
    let t = adapter.normalize(&buf)?;
    for &psi in &t.tire_pressures_psi {
        assert_eq!(psi, 0.0, "negative KPa → 0 PSI");
    }
    Ok(())
}

#[test]
fn tires_pressure_kpa_conversion_accuracy() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut buf = make_r3e();
    // 100 KPa × 0.14504 ≈ 14.504 PSI
    write_f32(&mut buf, OFF_TIRE_PRESSURE_FL, 100.0);
    write_f32(&mut buf, OFF_TIRE_PRESSURE_FR, 100.0);
    write_f32(&mut buf, OFF_TIRE_PRESSURE_RL, 100.0);
    write_f32(&mut buf, OFF_TIRE_PRESSURE_RR, 100.0);
    let t = adapter.normalize(&buf)?;
    for &psi in &t.tire_pressures_psi {
        assert!(
            (psi - 14.504).abs() < 0.1,
            "100 KPa ≈ 14.5 PSI, got {psi}"
        );
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scoring / timing data section
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn scoring_position_zero_not_set() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut buf = make_r3e();
    write_i32(&mut buf, OFF_POSITION, 0);
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.position, 0, "position 0 → default 0");
    Ok(())
}

#[test]
fn scoring_position_large_clamped() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut buf = make_r3e();
    write_i32(&mut buf, OFF_POSITION, 300);
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.position, 255, "position clamped to u8 max");
    Ok(())
}

#[test]
fn scoring_laps_na_not_set() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut buf = make_r3e();
    write_i32(&mut buf, OFF_COMPLETED_LAPS, -1);
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.lap, 0, "N/A laps → default 0");
    Ok(())
}

#[test]
fn scoring_lap_times_negative_not_set() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut buf = make_r3e();
    write_f32(&mut buf, OFF_LAP_TIME_CURRENT, -1.0);
    write_f32(&mut buf, OFF_LAP_TIME_BEST, -1.0);
    write_f32(&mut buf, OFF_LAP_TIME_PREVIOUS, -1.0);
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.current_lap_time_s, 0.0);
    assert_eq!(t.best_lap_time_s, 0.0);
    assert_eq!(t.last_lap_time_s, 0.0);
    Ok(())
}

#[test]
fn scoring_delta_negative_not_set() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut buf = make_r3e();
    write_f32(&mut buf, OFF_DELTA_FRONT, -1.0);
    write_f32(&mut buf, OFF_DELTA_BEHIND, -1.0);
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.delta_ahead_s, 0.0, "negative delta → not set");
    assert_eq!(t.delta_behind_s, 0.0);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// NaN / Inf resilience
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn nan_speed_defaults_to_zero() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut buf = make_r3e();
    write_f32(&mut buf, OFF_SPEED, f32::NAN);
    let t = adapter.normalize(&buf)?;
    assert!(t.speed_ms.is_finite(), "NaN speed → finite");
    Ok(())
}

#[test]
fn inf_throttle_defaults_to_zero() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut buf = make_r3e();
    write_f32(&mut buf, OFF_THROTTLE, f32::INFINITY);
    let t = adapter.normalize(&buf)?;
    assert!(t.throttle.is_finite(), "Inf throttle → finite");
    Ok(())
}

#[test]
fn neg_inf_brake_defaults_to_zero() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut buf = make_r3e();
    write_f32(&mut buf, OFF_BRAKE, f32::NEG_INFINITY);
    let t = adapter.normalize(&buf)?;
    assert!(t.brake.is_finite(), "NEG_INF brake → finite");
    Ok(())
}

#[test]
fn nan_accel_gives_zero_g() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut buf = make_r3e();
    write_f32(&mut buf, OFF_LOCAL_ACCEL_X, f32::NAN);
    write_f32(&mut buf, OFF_LOCAL_ACCEL_Y, f32::NAN);
    write_f32(&mut buf, OFF_LOCAL_ACCEL_Z, f32::NAN);
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.lateral_g, 0.0, "NaN accel → 0 G");
    assert_eq!(t.vertical_g, 0.0);
    assert_eq!(t.longitudinal_g, 0.0);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Full race scenario
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn scenario_race_lap_complete() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut buf = make_r3e();

    // Driving at 180 km/h (50 m/s), 7500 RPM, 4th gear, full throttle
    write_f32(&mut buf, OFF_SPEED, 50.0);
    let rps = 7500.0_f32 * std::f32::consts::PI / 30.0;
    write_f32(&mut buf, OFF_ENGINE_RPS, rps);
    let max_rps = 9000.0_f32 * std::f32::consts::PI / 30.0;
    write_f32(&mut buf, OFF_MAX_ENGINE_RPS, max_rps);
    write_i32(&mut buf, OFF_GEAR, 4);
    write_i32(&mut buf, OFF_NUM_GEARS, 6);
    write_f32(&mut buf, OFF_THROTTLE, 1.0);
    write_f32(&mut buf, OFF_BRAKE, 0.0);
    write_f32(&mut buf, OFF_STEER_INPUT, -0.15);

    // Scoring
    write_i32(&mut buf, OFF_POSITION, 2);
    write_i32(&mut buf, OFF_COMPLETED_LAPS, 8);
    write_f32(&mut buf, OFF_LAP_TIME_CURRENT, 55.2);
    write_f32(&mut buf, OFF_LAP_TIME_BEST, 53.8);
    write_f32(&mut buf, OFF_LAP_TIME_PREVIOUS, 54.1);
    write_f32(&mut buf, OFF_DELTA_FRONT, 0.4);
    write_f32(&mut buf, OFF_DELTA_BEHIND, 1.1);

    // Fuel
    write_f32(&mut buf, OFF_FUEL_LEFT, 15.0);
    write_f32(&mut buf, OFF_FUEL_CAPACITY, 60.0);

    // G-forces: cornering left 0.8G, accelerating 0.3G
    write_f32(&mut buf, OFF_LOCAL_ACCEL_X, 0.8 * G_ACCEL);
    write_f32(&mut buf, OFF_LOCAL_ACCEL_Z, -0.3 * G_ACCEL);

    // Tires
    write_f32(&mut buf, OFF_TIRE_TEMP_FL, 85.0);
    write_f32(&mut buf, OFF_TIRE_TEMP_FR, 90.0);
    write_f32(&mut buf, OFF_TIRE_TEMP_RL, 80.0);
    write_f32(&mut buf, OFF_TIRE_TEMP_RR, 82.0);
    write_f32(&mut buf, OFF_TIRE_PRESSURE_FL, 180.0);
    write_f32(&mut buf, OFF_TIRE_PRESSURE_FR, 182.0);
    write_f32(&mut buf, OFF_TIRE_PRESSURE_RL, 175.0);
    write_f32(&mut buf, OFF_TIRE_PRESSURE_RR, 178.0);

    // Engine temp
    write_f32(&mut buf, OFF_ENGINE_TEMP, 98.0);

    let t = adapter.normalize(&buf)?;

    assert!((t.speed_ms - 50.0).abs() < 0.01);
    assert!((t.rpm - 7500.0).abs() < 1.0);
    assert!((t.max_rpm - 9000.0).abs() < 1.0);
    assert_eq!(t.gear, 4);
    assert_eq!(t.num_gears, 6);
    assert!((t.throttle - 1.0).abs() < 0.001);
    assert_eq!(t.brake, 0.0);
    assert!((t.steering_angle - (-0.15)).abs() < 0.001);
    assert_eq!(t.position, 2);
    assert_eq!(t.lap, 8);
    assert!((t.fuel_percent - 0.25).abs() < 0.001);
    assert!((t.engine_temp_c - 98.0).abs() < 0.1);
    assert!(t.tire_temps_c[0] > 0);
    assert!(t.tire_pressures_psi[0] > 0.0);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Default/Clone trait
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn adapter_default_trait() -> TestResult {
    let adapter = RaceRoomAdapter::default();
    assert_eq!(adapter.game_id(), "raceroom");
    assert_eq!(
        adapter.expected_update_rate(),
        std::time::Duration::from_millis(10)
    );
    Ok(())
}
