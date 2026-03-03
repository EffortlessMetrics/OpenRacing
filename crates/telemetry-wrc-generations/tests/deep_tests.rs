//! Deep tests for the WRC Generations / EA WRC telemetry adapter.
//!
//! Covers Codemasters Mode 1 UDP packet parsing, gear encoding,
//! speed calculation (wheel speeds vs body velocity), slip ratio,
//! FFB scalar, tire data, and game-specific fields.

use racing_wheel_telemetry_wrc_generations::{
    TelemetryAdapter, TelemetryValue, WrcGenerationsAdapter,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const MIN_PACKET: usize = 264;

// Codemasters Mode 1 byte offsets (all f32 little-endian)
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
fn write_f32(buf: &mut [u8], off: usize, v: f32) {
    buf[off..off + 4].copy_from_slice(&v.to_le_bytes());
}

fn make_packet() -> Vec<u8> {
    vec![0u8; MIN_PACKET]
}

// ── Adapter identity ─────────────────────────────────────────────────────────

#[test]
fn deep_game_id() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    assert_eq!(adapter.game_id(), "wrc_generations");
    Ok(())
}

#[test]
fn deep_update_rate() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    assert_eq!(
        adapter.expected_update_rate(),
        std::time::Duration::from_millis(16)
    );
    Ok(())
}

#[test]
fn deep_with_port() -> TestResult {
    let adapter = WrcGenerationsAdapter::new().with_port(7000);
    assert_eq!(adapter.game_id(), "wrc_generations");
    Ok(())
}

// ── Packet rejection ─────────────────────────────────────────────────────────

#[test]
fn deep_rejects_empty() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    assert!(adapter.normalize(&[]).is_err());
    Ok(())
}

#[test]
fn deep_rejects_short_packet() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    assert!(adapter.normalize(&[0u8; MIN_PACKET - 1]).is_err());
    Ok(())
}

#[test]
fn deep_accepts_oversized_packet() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = vec![0u8; MIN_PACKET + 128];
    write_f32(&mut buf, OFF_RPM, 5000.0);
    let t = adapter.normalize(&buf)?;
    assert!((t.rpm - 5000.0).abs() < 0.01);
    Ok(())
}

// ── Speed calculation ────────────────────────────────────────────────────────

#[test]
fn deep_speed_from_wheel_speeds() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_WHEEL_SPEED_FL, 30.0);
    write_f32(&mut buf, OFF_WHEEL_SPEED_FR, 30.0);
    write_f32(&mut buf, OFF_WHEEL_SPEED_RL, 30.0);
    write_f32(&mut buf, OFF_WHEEL_SPEED_RR, 30.0);
    let t = adapter.normalize(&buf)?;
    assert!((t.speed_ms - 30.0).abs() < 0.01, "speed_ms={}", t.speed_ms);
    Ok(())
}

#[test]
fn deep_speed_fallback_to_body_velocity() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    // wheel speeds zero → use body velocity
    write_f32(&mut buf, OFF_VEL_X, 3.0);
    write_f32(&mut buf, OFF_VEL_Y, 4.0);
    write_f32(&mut buf, OFF_VEL_Z, 0.0);
    let t = adapter.normalize(&buf)?;
    // sqrt(9+16) = 5.0
    assert!((t.speed_ms - 5.0).abs() < 0.01, "speed_ms={}", t.speed_ms);
    Ok(())
}

// ── Gear encoding ────────────────────────────────────────────────────────────

#[test]
fn deep_gear_reverse() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_GEAR, -1.0);
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.gear, -1, "raw -1.0 → reverse");
    Ok(())
}

#[test]
fn deep_gear_neutral() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_GEAR, 0.0);
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.gear, 0, "raw 0.0 → neutral");
    Ok(())
}

#[test]
fn deep_gear_forward_range() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    for g in 1..=6 {
        let mut buf = make_packet();
        write_f32(&mut buf, OFF_GEAR, g as f32);
        let t = adapter.normalize(&buf)?;
        assert_eq!(t.gear, g, "gear {g}");
    }
    Ok(())
}

#[test]
fn deep_gear_clamped_to_8() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_GEAR, 12.0);
    let t = adapter.normalize(&buf)?;
    assert!(t.gear <= 8, "gear clamped to 8, got {}", t.gear);
    Ok(())
}

// ── Throttle, brake, steering ────────────────────────────────────────────────

#[test]
fn deep_throttle_brake_passthrough() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_THROTTLE, 0.85);
    write_f32(&mut buf, OFF_BRAKE, 0.42);
    let t = adapter.normalize(&buf)?;
    assert!((t.throttle - 0.85).abs() < 0.001);
    assert!((t.brake - 0.42).abs() < 0.001);
    Ok(())
}

#[test]
fn deep_throttle_clamped() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_THROTTLE, 2.0);
    let t = adapter.normalize(&buf)?;
    assert!((t.throttle - 1.0).abs() < 0.001, "throttle clamped to 1.0");
    Ok(())
}

#[test]
fn deep_steering_clamped() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_STEER, 1.5);
    let t = adapter.normalize(&buf)?;
    assert!(
        (t.steering_angle - 1.0).abs() < 0.001,
        "steering clamped to 1.0"
    );
    Ok(())
}

// ── FFB scalar from lateral G ────────────────────────────────────────────────

#[test]
fn deep_ffb_scalar_from_lateral_g() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_GFORCE_LAT, 1.5);
    let t = adapter.normalize(&buf)?;
    // ffb_scalar = lat_g / 3.0 = 0.5
    assert!((t.ffb_scalar - 0.5).abs() < 0.01, "ffb={}", t.ffb_scalar);
    Ok(())
}

#[test]
fn deep_ffb_scalar_clamped() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_GFORCE_LAT, 5.0);
    let t = adapter.normalize(&buf)?;
    assert!((t.ffb_scalar - 1.0).abs() < 0.01, "ffb clamped to 1.0");
    Ok(())
}

// ── Fuel calculation ─────────────────────────────────────────────────────────

#[test]
fn deep_fuel_percent() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_FUEL_IN_TANK, 25.0);
    write_f32(&mut buf, OFF_FUEL_CAPACITY, 50.0);
    let t = adapter.normalize(&buf)?;
    assert!((t.fuel_percent - 0.5).abs() < 0.001);
    Ok(())
}

// ── Pit flag ─────────────────────────────────────────────────────────────────

#[test]
fn deep_in_pit_flag() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_IN_PIT, 1.0);
    let t = adapter.normalize(&buf)?;
    assert!(t.flags.in_pits, "in_pits should be true");
    Ok(())
}

#[test]
fn deep_not_in_pit() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let buf = make_packet();
    let t = adapter.normalize(&buf)?;
    assert!(!t.flags.in_pits, "in_pits should be false");
    Ok(())
}

// ── Tire data ────────────────────────────────────────────────────────────────

#[test]
fn deep_tire_temps_clamped_to_u8() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_BRAKES_TEMP_FL, 300.0); // > 255, should clamp
    write_f32(&mut buf, OFF_BRAKES_TEMP_FR, 95.0);
    write_f32(&mut buf, OFF_BRAKES_TEMP_RL, 88.0);
    write_f32(&mut buf, OFF_BRAKES_TEMP_RR, 92.0);
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.tire_temps_c[0], 255, "clamped to 255");
    assert_eq!(t.tire_temps_c[1], 95);
    Ok(())
}

#[test]
fn deep_tire_pressures_psi_passthrough() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_TYRES_PRESSURE_FL, 28.5);
    write_f32(&mut buf, OFF_TYRES_PRESSURE_FR, 29.0);
    write_f32(&mut buf, OFF_TYRES_PRESSURE_RL, 27.5);
    write_f32(&mut buf, OFF_TYRES_PRESSURE_RR, 28.0);
    let t = adapter.normalize(&buf)?;
    assert!((t.tire_pressures_psi[0] - 28.5).abs() < 0.01);
    assert!((t.tire_pressures_psi[1] - 29.0).abs() < 0.01);
    Ok(())
}

// ── Timing and position ─────────────────────────────────────────────────────

#[test]
fn deep_lap_timing() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_LAP_TIME, 45.3);
    write_f32(&mut buf, OFF_LAST_LAP_TIME, 120.5);
    write_f32(&mut buf, OFF_CURRENT_LAP, 2.0);
    write_f32(&mut buf, OFF_CAR_POSITION, 3.0);
    let t = adapter.normalize(&buf)?;
    assert!((t.current_lap_time_s - 45.3).abs() < 0.01);
    assert!((t.last_lap_time_s - 120.5).abs() < 0.01);
    // current_lap is 0-based + 1, so lap 2 → 3
    assert_eq!(t.lap, 3, "lap=0-based(2)+1=3");
    assert_eq!(t.position, 3);
    Ok(())
}

// ── Extended wheel speed fields ──────────────────────────────────────────────

#[test]
fn deep_wheel_speeds_in_extended() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_WHEEL_SPEED_FL, 20.0);
    write_f32(&mut buf, OFF_WHEEL_SPEED_FR, 21.0);
    write_f32(&mut buf, OFF_WHEEL_SPEED_RL, 19.0);
    write_f32(&mut buf, OFF_WHEEL_SPEED_RR, 20.5);
    let t = adapter.normalize(&buf)?;
    assert!(t.extended.contains_key("wheel_speed_fl"));
    assert!(t.extended.contains_key("wheel_speed_fr"));
    assert!(t.extended.contains_key("wheel_speed_rl"));
    assert!(t.extended.contains_key("wheel_speed_rr"));
    Ok(())
}

// ── RPM fraction extended field ──────────────────────────────────────────────

#[test]
fn deep_rpm_fraction_in_extended() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_RPM, 5000.0);
    write_f32(&mut buf, OFF_MAX_RPM, 8000.0);
    let t = adapter.normalize(&buf)?;
    match t.extended.get("rpm_fraction") {
        Some(TelemetryValue::Float(f)) => {
            assert!((*f - 0.625).abs() < 0.001, "rpm_fraction={f}");
        }
        other => return Err(format!("expected Float, got {other:?}").into()),
    }
    Ok(())
}
