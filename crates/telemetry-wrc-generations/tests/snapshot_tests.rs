//! Insta snapshot tests for the WRC Generations telemetry adapter.
//!
//! These tests lock down the normalized output format so that any change to
//! the adapter's output is caught as a snapshot diff.

use racing_wheel_telemetry_wrc_generations::{TelemetryAdapter, WrcGenerationsAdapter};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ---------------------------------------------------------------------------
// Codemasters Mode 1 byte offsets (all little-endian f32)
//
// Verified against dr2_logger udp_data.py and Codemasters telemetry spreadsheet.
// ---------------------------------------------------------------------------

const MIN_PACKET_SIZE: usize = 264;

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
const OFF_IN_PIT: usize = 188;

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

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn write_f32(buf: &mut [u8], offset: usize, value: f32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn adapter() -> WrcGenerationsAdapter {
    WrcGenerationsAdapter::new()
}

/// Build a realistic mid-stage WRC packet.
fn realistic_mid_stage_packet() -> Vec<u8> {
    let mut buf = vec![0u8; MIN_PACKET_SIZE];
    write_f32(&mut buf, OFF_WHEEL_SPEED_FL, 25.2);
    write_f32(&mut buf, OFF_WHEEL_SPEED_FR, 25.0);
    write_f32(&mut buf, OFF_WHEEL_SPEED_RL, 26.1);
    write_f32(&mut buf, OFF_WHEEL_SPEED_RR, 25.8);
    write_f32(&mut buf, OFF_THROTTLE, 0.78);
    write_f32(&mut buf, OFF_BRAKE, 0.0);
    write_f32(&mut buf, OFF_STEER, -0.22);
    write_f32(&mut buf, OFF_GEAR, 5.0);
    write_f32(&mut buf, OFF_RPM, 6200.0);
    write_f32(&mut buf, OFF_MAX_RPM, 7800.0);
    write_f32(&mut buf, OFF_GFORCE_LAT, 1.1);
    write_f32(&mut buf, OFF_GFORCE_LON, 0.4);
    write_f32(&mut buf, OFF_CURRENT_LAP, 3.0);
    write_f32(&mut buf, OFF_CAR_POSITION, 1.0);
    write_f32(&mut buf, OFF_FUEL_IN_TANK, 42.0);
    write_f32(&mut buf, OFF_FUEL_CAPACITY, 80.0);
    write_f32(&mut buf, OFF_IN_PIT, 0.0);
    write_f32(&mut buf, OFF_BRAKES_TEMP_FL, 135.0);
    write_f32(&mut buf, OFF_BRAKES_TEMP_FR, 132.0);
    write_f32(&mut buf, OFF_BRAKES_TEMP_RL, 105.0);
    write_f32(&mut buf, OFF_BRAKES_TEMP_RR, 108.0);
    write_f32(&mut buf, OFF_TYRES_PRESSURE_FL, 30.5);
    write_f32(&mut buf, OFF_TYRES_PRESSURE_FR, 30.2);
    write_f32(&mut buf, OFF_TYRES_PRESSURE_RL, 28.8);
    write_f32(&mut buf, OFF_TYRES_PRESSURE_RR, 29.0);
    write_f32(&mut buf, OFF_LAST_LAP_TIME, 210.4);
    write_f32(&mut buf, OFF_MAX_GEARS, 6.0);
    buf
}

/// Snapshot a typical mid-stage WRC frame.
#[test]
fn snapshot_wrc_mid_stage() -> TestResult {
    let norm = adapter().normalize(&realistic_mid_stage_packet())?;
    insta::assert_yaml_snapshot!("wrc_mid_stage_frame", norm);
    Ok(())
}

/// Snapshot heavy braking into a hairpin.
#[test]
fn snapshot_wrc_heavy_braking() -> TestResult {
    let mut buf = vec![0u8; MIN_PACKET_SIZE];
    write_f32(&mut buf, OFF_WHEEL_SPEED_FL, 12.0);
    write_f32(&mut buf, OFF_WHEEL_SPEED_FR, 11.8);
    write_f32(&mut buf, OFF_WHEEL_SPEED_RL, 13.5);
    write_f32(&mut buf, OFF_WHEEL_SPEED_RR, 13.2);
    write_f32(&mut buf, OFF_THROTTLE, 0.0);
    write_f32(&mut buf, OFF_BRAKE, 0.88);
    write_f32(&mut buf, OFF_STEER, 0.65);
    write_f32(&mut buf, OFF_GEAR, 2.0);
    write_f32(&mut buf, OFF_RPM, 4800.0);
    write_f32(&mut buf, OFF_MAX_RPM, 7800.0);
    write_f32(&mut buf, OFF_GFORCE_LAT, -1.8);
    write_f32(&mut buf, OFF_GFORCE_LON, -2.5);
    write_f32(&mut buf, OFF_CURRENT_LAP, 1.0);
    write_f32(&mut buf, OFF_CAR_POSITION, 3.0);
    write_f32(&mut buf, OFF_FUEL_IN_TANK, 55.0);
    write_f32(&mut buf, OFF_FUEL_CAPACITY, 80.0);
    write_f32(&mut buf, OFF_IN_PIT, 0.0);
    write_f32(&mut buf, OFF_BRAKES_TEMP_FL, 220.0);
    write_f32(&mut buf, OFF_BRAKES_TEMP_FR, 215.0);
    write_f32(&mut buf, OFF_BRAKES_TEMP_RL, 180.0);
    write_f32(&mut buf, OFF_BRAKES_TEMP_RR, 178.0);
    write_f32(&mut buf, OFF_TYRES_PRESSURE_FL, 31.0);
    write_f32(&mut buf, OFF_TYRES_PRESSURE_FR, 30.8);
    write_f32(&mut buf, OFF_TYRES_PRESSURE_RL, 29.5);
    write_f32(&mut buf, OFF_TYRES_PRESSURE_RR, 29.3);
    write_f32(&mut buf, OFF_LAST_LAP_TIME, 195.2);
    write_f32(&mut buf, OFF_MAX_GEARS, 6.0);

    let norm = adapter().normalize(&buf)?;
    insta::assert_yaml_snapshot!("wrc_heavy_braking_frame", norm);
    Ok(())
}

/// Snapshot a service park (in pits) scenario.
#[test]
fn snapshot_wrc_service_park() -> TestResult {
    let mut buf = vec![0u8; MIN_PACKET_SIZE];
    write_f32(&mut buf, OFF_WHEEL_SPEED_FL, 0.0);
    write_f32(&mut buf, OFF_WHEEL_SPEED_FR, 0.0);
    write_f32(&mut buf, OFF_WHEEL_SPEED_RL, 0.0);
    write_f32(&mut buf, OFF_WHEEL_SPEED_RR, 0.0);
    write_f32(&mut buf, OFF_THROTTLE, 0.0);
    write_f32(&mut buf, OFF_BRAKE, 0.0);
    write_f32(&mut buf, OFF_STEER, 0.0);
    write_f32(&mut buf, OFF_GEAR, 1.0); // neutral
    write_f32(&mut buf, OFF_RPM, 850.0);
    write_f32(&mut buf, OFF_MAX_RPM, 7800.0);
    write_f32(&mut buf, OFF_GFORCE_LAT, 0.0);
    write_f32(&mut buf, OFF_GFORCE_LON, 0.0);
    write_f32(&mut buf, OFF_CURRENT_LAP, 0.0);
    write_f32(&mut buf, OFF_CAR_POSITION, 5.0);
    write_f32(&mut buf, OFF_FUEL_IN_TANK, 80.0);
    write_f32(&mut buf, OFF_FUEL_CAPACITY, 80.0);
    write_f32(&mut buf, OFF_IN_PIT, 1.0);
    write_f32(&mut buf, OFF_BRAKES_TEMP_FL, 45.0);
    write_f32(&mut buf, OFF_BRAKES_TEMP_FR, 44.0);
    write_f32(&mut buf, OFF_BRAKES_TEMP_RL, 40.0);
    write_f32(&mut buf, OFF_BRAKES_TEMP_RR, 41.0);
    write_f32(&mut buf, OFF_TYRES_PRESSURE_FL, 28.0);
    write_f32(&mut buf, OFF_TYRES_PRESSURE_FR, 28.0);
    write_f32(&mut buf, OFF_TYRES_PRESSURE_RL, 27.0);
    write_f32(&mut buf, OFF_TYRES_PRESSURE_RR, 27.0);
    write_f32(&mut buf, OFF_LAST_LAP_TIME, 0.0);
    write_f32(&mut buf, OFF_MAX_GEARS, 6.0);

    let norm = adapter().normalize(&buf)?;
    insta::assert_yaml_snapshot!("wrc_service_park_frame", norm);
    Ok(())
}
