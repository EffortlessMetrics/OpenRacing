//! Insta snapshot tests for the Rennsport UDP telemetry adapter.
//!
//! Three scenarios: normal high-speed racing, stationary on grid, and
//! edge case with reverse gear and over-range scalar values.

use racing_wheel_telemetry_rennsport::{RennsportAdapter, TelemetryAdapter};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const RENNSPORT_MIN_PACKET_SIZE: usize = 24;
const RENNSPORT_IDENTIFIER: u8 = 0x52; // 'R'
const OFF_IDENTIFIER: usize = 0;
const OFF_SPEED_KMH: usize = 4;
const OFF_RPM: usize = 8;
const OFF_GEAR: usize = 12;
const OFF_FFB_SCALAR: usize = 16;
const OFF_SLIP_RATIO: usize = 20;

fn make_rennsport_packet(
    speed_kmh: f32,
    rpm: f32,
    gear: i8,
    ffb_scalar: f32,
    slip_ratio: f32,
) -> Vec<u8> {
    let mut data = vec![0u8; RENNSPORT_MIN_PACKET_SIZE];
    data[OFF_IDENTIFIER] = RENNSPORT_IDENTIFIER;
    data[OFF_SPEED_KMH..OFF_SPEED_KMH + 4].copy_from_slice(&speed_kmh.to_le_bytes());
    data[OFF_RPM..OFF_RPM + 4].copy_from_slice(&rpm.to_le_bytes());
    data[OFF_GEAR] = gear as u8;
    data[OFF_FFB_SCALAR..OFF_FFB_SCALAR + 4].copy_from_slice(&ffb_scalar.to_le_bytes());
    data[OFF_SLIP_RATIO..OFF_SLIP_RATIO + 4].copy_from_slice(&slip_ratio.to_le_bytes());
    data
}

// ─── Scenario 1: Normal high-speed racing ───────────────────────────────────
// 4th gear at 180 km/h (~50 m/s), moderate FFB, slight tire slip.

#[test]
fn rennsport_normal_high_speed() -> TestResult {
    let adapter = RennsportAdapter::new();
    let data = make_rennsport_packet(180.0, 7500.0, 4, 0.6, 0.1);
    let normalized = adapter.normalize(&data)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── Scenario 2: Stationary on grid ─────────────────────────────────────────
// Car sitting on grid in neutral, idle RPM, zero FFB and slip.

#[test]
fn rennsport_stationary_grid() -> TestResult {
    let adapter = RennsportAdapter::new();
    let data = make_rennsport_packet(0.0, 850.0, 0, 0.0, 0.0);
    let normalized = adapter.normalize(&data)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── Scenario 3: Reverse gear with over-range values ────────────────────────
// Reverse gear, FFB scalar exceeding 1.0 (should be clamped), slip ratio over
// range (should be clamped), very low speed.

#[test]
fn rennsport_reverse_overrange() -> TestResult {
    let adapter = RennsportAdapter::new();
    let data = make_rennsport_packet(10.0, 2000.0, -1, 5.0, 2.0);
    let normalized = adapter.normalize(&data)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}
