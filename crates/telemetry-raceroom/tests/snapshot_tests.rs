//! Insta snapshot tests for the RaceRoom telemetry adapter.
//!
//! These tests lock down the normalized output format so that any change to
//! the adapter's output is caught as a snapshot diff.

use racing_wheel_telemetry_raceroom::{RaceRoomAdapter, TelemetryAdapter};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ---------------------------------------------------------------------------
// R3E shared memory constants (must match adapter internals)
// ---------------------------------------------------------------------------

const R3E_VIEW_SIZE: usize = 4096;
const R3E_VERSION_MAJOR: i32 = 3;

const OFF_VERSION_MAJOR: usize = 0;
const OFF_GAME_PAUSED: usize = 20;
const OFF_GAME_IN_MENUS: usize = 24;
const OFF_SPEED: usize = 1392;
const OFF_ENGINE_RPS: usize = 1396;
const OFF_MAX_ENGINE_RPS: usize = 1400;
const OFF_GEAR: usize = 1408;
const OFF_FUEL_LEFT: usize = 1456;
const OFF_FUEL_CAPACITY: usize = 1460;
const OFF_THROTTLE: usize = 1500;
const OFF_BRAKE: usize = 1508;
const OFF_CLUTCH: usize = 1516;
const OFF_STEER_INPUT: usize = 1524;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn write_i32(buf: &mut [u8], offset: usize, value: i32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_f32(buf: &mut [u8], offset: usize, value: f32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

/// Build a valid R3E shared memory buffer with the given telemetry values.
#[allow(clippy::too_many_arguments)]
fn make_r3e_memory(
    rpm: f32,
    speed: f32,
    steering: f32,
    throttle: f32,
    brake: f32,
    clutch: f32,
    gear: i32,
    fuel_left: f32,
    fuel_capacity: f32,
) -> Vec<u8> {
    let mut data = vec![0u8; R3E_VIEW_SIZE];
    write_i32(&mut data, OFF_VERSION_MAJOR, R3E_VERSION_MAJOR);
    write_i32(&mut data, OFF_GAME_PAUSED, 0);
    write_i32(&mut data, OFF_GAME_IN_MENUS, 0);
    let rps = rpm * (std::f32::consts::PI / 30.0);
    write_f32(&mut data, OFF_ENGINE_RPS, rps);
    let max_rps = 8000.0f32 * (std::f32::consts::PI / 30.0);
    write_f32(&mut data, OFF_MAX_ENGINE_RPS, max_rps);
    write_f32(&mut data, OFF_SPEED, speed);
    write_f32(&mut data, OFF_STEER_INPUT, steering);
    write_f32(&mut data, OFF_THROTTLE, throttle);
    write_f32(&mut data, OFF_BRAKE, brake);
    write_f32(&mut data, OFF_CLUTCH, clutch);
    write_i32(&mut data, OFF_GEAR, gear);
    write_f32(&mut data, OFF_FUEL_LEFT, fuel_left);
    write_f32(&mut data, OFF_FUEL_CAPACITY, fuel_capacity);
    data
}

/// Snapshot a typical mid-race driving frame.
#[test]
fn snapshot_raceroom_typical_driving() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let data = make_r3e_memory(
        6200.0, // rpm
        42.5,   // speed m/s (~153 km/h)
        -0.18,  // steering (slight left)
        0.82,   // throttle
        0.0,    // brake
        0.0,    // clutch
        4,      // gear
        35.0,   // fuel left
        65.0,   // fuel capacity
    );
    let norm = adapter.normalize(&data)?;
    insta::assert_yaml_snapshot!("raceroom_typical_driving", norm);
    Ok(())
}

/// Snapshot a heavy braking frame.
#[test]
fn snapshot_raceroom_heavy_braking() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let data = make_r3e_memory(
        3800.0, // rpm (downshifting)
        28.0,   // speed m/s (~100 km/h)
        0.35,   // steering (right turn entry)
        0.0,    // throttle
        0.95,   // brake
        0.0,    // clutch
        2,      // gear
        32.0,   // fuel left
        65.0,   // fuel capacity
    );
    let norm = adapter.normalize(&data)?;
    insta::assert_yaml_snapshot!("raceroom_heavy_braking", norm);
    Ok(())
}

/// Snapshot a standing start (clutch engaged, first gear).
#[test]
fn snapshot_raceroom_standing_start() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let data = make_r3e_memory(
        4500.0, // rpm (revving on grid)
        0.0,    // speed
        0.0,    // steering
        0.6,    // throttle
        0.0,    // brake
        0.85,   // clutch (holding)
        1,      // gear
        65.0,   // fuel left (full tank)
        65.0,   // fuel capacity
    );
    let norm = adapter.normalize(&data)?;
    insta::assert_yaml_snapshot!("raceroom_standing_start", norm);
    Ok(())
}
