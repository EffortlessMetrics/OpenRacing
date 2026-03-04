//! Insta snapshot tests for the LFS OutGauge telemetry adapter.
//!
//! Three scenarios: normal race pace with shift light, pit stop / pit lane,
//! and edge case with all dashboard lights active.

use racing_wheel_telemetry_lfs::{LFSAdapter, TelemetryAdapter};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// OutGauge byte offsets (same layout as BeamNG OutGauge).
const OFF_GEAR: usize = 10;
const OFF_SPEED: usize = 12;
const OFF_RPM: usize = 16;
const OFF_TURBO: usize = 20;
const OFF_ENG_TEMP: usize = 24;
const OFF_FUEL: usize = 28;
const OFF_OIL_PRESSURE: usize = 32;
const OFF_OIL_TEMP: usize = 36;
const OFF_SHOW_LIGHTS: usize = 44;
const OFF_THROTTLE: usize = 48;
const OFF_BRAKE: usize = 52;
const OFF_CLUTCH: usize = 56;

// Dashboard light flags (from LFS InSim.txt).
const DL_SHIFT: u32 = 0x0001;
const DL_PITSPEED: u32 = 0x0008;
const DL_TC: u32 = 0x0010;
const DL_ABS: u32 = 0x0400;

fn write_f32_le(buf: &mut [u8], offset: usize, value: f32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_u32_le(buf: &mut [u8], offset: usize, value: u32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

// ─── Scenario 1: Normal race pace with shift light ──────────────────────────
// Flat out in 4th gear at ~200 km/h approaching the rev limiter, shift light
// illuminated, engine warm, half fuel, moderate turbo boost.

#[test]
fn lfs_normal_race_pace_shift_light() -> TestResult {
    let mut buf = vec![0u8; 96];
    buf[OFF_GEAR] = 5; // OutGauge 5 → normalized 4th gear
    write_f32_le(&mut buf, OFF_SPEED, 55.6); // ~200 km/h
    write_f32_le(&mut buf, OFF_RPM, 7200.0);
    write_f32_le(&mut buf, OFF_THROTTLE, 1.0);
    write_f32_le(&mut buf, OFF_BRAKE, 0.0);
    write_f32_le(&mut buf, OFF_CLUTCH, 0.0);
    write_f32_le(&mut buf, OFF_FUEL, 0.48);
    write_f32_le(&mut buf, OFF_ENG_TEMP, 92.0);
    write_f32_le(&mut buf, OFF_TURBO, 0.85);
    write_f32_le(&mut buf, OFF_OIL_PRESSURE, 4.5);
    write_f32_le(&mut buf, OFF_OIL_TEMP, 108.0);
    write_u32_le(&mut buf, OFF_SHOW_LIGHTS, DL_SHIFT);

    let adapter = LFSAdapter::new();
    let normalized = adapter.normalize(&buf)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── Scenario 2: Pit stop / pit lane ────────────────────────────────────────
// Crawling through the pit lane in 1st gear at the pit speed limit, pit limiter
// active, low RPM, full fuel (just refuelled), clutch partially engaged.

#[test]
fn lfs_pit_stop_pit_lane() -> TestResult {
    let mut buf = vec![0u8; 96];
    buf[OFF_GEAR] = 2; // OutGauge 2 → normalized 1st gear
    write_f32_le(&mut buf, OFF_SPEED, 16.7); // ~60 km/h pit limiter
    write_f32_le(&mut buf, OFF_RPM, 3200.0);
    write_f32_le(&mut buf, OFF_THROTTLE, 0.25);
    write_f32_le(&mut buf, OFF_BRAKE, 0.0);
    write_f32_le(&mut buf, OFF_CLUTCH, 0.3);
    write_f32_le(&mut buf, OFF_FUEL, 0.95);
    write_f32_le(&mut buf, OFF_ENG_TEMP, 85.0);
    write_f32_le(&mut buf, OFF_TURBO, 0.0);
    write_f32_le(&mut buf, OFF_OIL_PRESSURE, 3.0);
    write_f32_le(&mut buf, OFF_OIL_TEMP, 90.0);
    write_u32_le(&mut buf, OFF_SHOW_LIGHTS, DL_PITSPEED);

    let adapter = LFSAdapter::new();
    let normalized = adapter.normalize(&buf)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── Scenario 3: Edge case – all dashboard lights ───────────────────────────
// Reverse gear at high RPM with every known dashboard light flag set, full
// brake, extreme temperatures, nearly empty fuel.

#[test]
fn lfs_edge_case_all_dashboard_lights() -> TestResult {
    let mut buf = vec![0u8; 96];
    buf[OFF_GEAR] = 0; // OutGauge 0 → reverse
    write_f32_le(&mut buf, OFF_SPEED, 5.0); // slow reverse
    write_f32_le(&mut buf, OFF_RPM, 8500.0);
    write_f32_le(&mut buf, OFF_THROTTLE, 0.0);
    write_f32_le(&mut buf, OFF_BRAKE, 1.0);
    write_f32_le(&mut buf, OFF_CLUTCH, 1.0);
    write_f32_le(&mut buf, OFF_FUEL, 0.02);
    write_f32_le(&mut buf, OFF_ENG_TEMP, 125.0);
    write_f32_le(&mut buf, OFF_TURBO, 2.5);
    write_f32_le(&mut buf, OFF_OIL_PRESSURE, 6.8);
    write_f32_le(&mut buf, OFF_OIL_TEMP, 150.0);
    write_u32_le(
        &mut buf,
        OFF_SHOW_LIGHTS,
        DL_SHIFT | DL_PITSPEED | DL_TC | DL_ABS,
    );

    let adapter = LFSAdapter::new();
    let normalized = adapter.normalize(&buf)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}
