//! Deep tests for the Forza Motorsport / Forza Horizon telemetry adapter.
//!
//! Covers Sled (232 byte), CarDash (311 byte), FM8 (331 byte), and
//! FH4 (324 byte) packet formats, field extraction, gear encoding,
//! temperature conversions, and edge cases.

use racing_wheel_telemetry_forza::{ForzaAdapter, TelemetryAdapter};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const SLED_SIZE: usize = 232;
const CARDASH_SIZE: usize = 311;
const FM8_SIZE: usize = 331;
const FH4_SIZE: usize = 324;

// Sled offsets
const OFF_IS_RACE_ON: usize = 0;
const OFF_ENGINE_MAX_RPM: usize = 8;
const OFF_CURRENT_RPM: usize = 16;
const OFF_ACCEL_X: usize = 20;
const OFF_ACCEL_Y: usize = 24;
const OFF_ACCEL_Z: usize = 28;
const OFF_VEL_X: usize = 32;
const OFF_VEL_Y: usize = 36;
const OFF_VEL_Z: usize = 40;
const OFF_TIRE_SLIP_FL: usize = 84;
const OFF_TIRE_SLIP_FR: usize = 88;
const OFF_TIRE_SLIP_RL: usize = 92;
const OFF_TIRE_SLIP_RR: usize = 96;

// CarDash offsets (base, no horizon offset)
const OFF_DASH_TIRE_TEMP_FL: usize = 256;
const OFF_DASH_TIRE_TEMP_FR: usize = 260;
const OFF_DASH_TIRE_TEMP_RL: usize = 264;
const OFF_DASH_TIRE_TEMP_RR: usize = 268;
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

fn write_f32(buf: &mut [u8], off: usize, v: f32) {
    buf[off..off + 4].copy_from_slice(&v.to_le_bytes());
}

fn write_i32(buf: &mut [u8], off: usize, v: i32) {
    buf[off..off + 4].copy_from_slice(&v.to_le_bytes());
}

fn make_sled(race_on: bool) -> Vec<u8> {
    let mut buf = vec![0u8; SLED_SIZE];
    write_i32(&mut buf, OFF_IS_RACE_ON, if race_on { 1 } else { 0 });
    buf
}

fn make_cardash() -> Vec<u8> {
    let mut buf = vec![0u8; CARDASH_SIZE];
    write_i32(&mut buf, OFF_IS_RACE_ON, 1);
    buf
}

// ── Adapter identity ─────────────────────────────────────────────────────────

#[test]
fn deep_game_id() -> TestResult {
    let adapter = ForzaAdapter::new();
    assert_eq!(adapter.game_id(), "forza_motorsport");
    Ok(())
}

#[test]
fn deep_update_rate() -> TestResult {
    let adapter = ForzaAdapter::new();
    assert_eq!(
        adapter.expected_update_rate(),
        std::time::Duration::from_millis(16)
    );
    Ok(())
}

// ── Packet rejection ─────────────────────────────────────────────────────────

#[test]
fn deep_rejects_empty_packet() -> TestResult {
    let adapter = ForzaAdapter::new();
    assert!(adapter.normalize(&[]).is_err());
    Ok(())
}

#[test]
fn deep_rejects_unknown_packet_size() -> TestResult {
    let adapter = ForzaAdapter::new();
    // 100 bytes doesn't match any known format
    assert!(adapter.normalize(&[0u8; 100]).is_err());
    Ok(())
}

#[test]
fn deep_rejects_short_sled() -> TestResult {
    let adapter = ForzaAdapter::new();
    assert!(adapter.normalize(&[0u8; SLED_SIZE - 1]).is_err());
    Ok(())
}

// ── Sled format (232 bytes) ──────────────────────────────────────────────────

#[test]
fn deep_sled_race_not_on_returns_defaults() -> TestResult {
    let adapter = ForzaAdapter::new();
    let buf = make_sled(false);
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.speed_ms, 0.0);
    assert_eq!(t.rpm, 0.0);
    Ok(())
}

#[test]
fn deep_sled_speed_from_velocity() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut buf = make_sled(true);
    write_f32(&mut buf, OFF_VEL_X, 3.0);
    write_f32(&mut buf, OFF_VEL_Y, 4.0);
    write_f32(&mut buf, OFF_VEL_Z, 0.0);
    let t = adapter.normalize(&buf)?;
    // sqrt(3² + 4²) = 5.0
    assert!((t.speed_ms - 5.0).abs() < 0.01, "speed_ms={}", t.speed_ms);
    Ok(())
}

#[test]
fn deep_sled_rpm_extraction() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut buf = make_sled(true);
    write_f32(&mut buf, OFF_CURRENT_RPM, 6500.0);
    write_f32(&mut buf, OFF_ENGINE_MAX_RPM, 9000.0);
    let t = adapter.normalize(&buf)?;
    assert!((t.rpm - 6500.0).abs() < 0.1);
    assert!((t.max_rpm - 9000.0).abs() < 0.1);
    Ok(())
}

#[test]
fn deep_sled_g_forces() -> TestResult {
    let adapter = ForzaAdapter::new();
    let g = 9.806_65_f32;
    let mut buf = make_sled(true);
    write_f32(&mut buf, OFF_ACCEL_X, 2.0 * g); // lateral
    write_f32(&mut buf, OFF_ACCEL_Y, 1.0 * g); // vertical
    write_f32(&mut buf, OFF_ACCEL_Z, -0.5 * g); // longitudinal
    let t = adapter.normalize(&buf)?;
    assert!(
        (t.lateral_g - 2.0).abs() < 0.01,
        "lateral_g={}",
        t.lateral_g
    );
    assert!(
        (t.vertical_g - 1.0).abs() < 0.01,
        "vertical_g={}",
        t.vertical_g
    );
    assert!(
        (t.longitudinal_g - (-0.5)).abs() < 0.01,
        "lon_g={}",
        t.longitudinal_g
    );
    Ok(())
}

#[test]
fn deep_sled_slip_ratio_average() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut buf = make_sled(true);
    write_f32(&mut buf, OFF_TIRE_SLIP_FL, 0.1);
    write_f32(&mut buf, OFF_TIRE_SLIP_FR, 0.2);
    write_f32(&mut buf, OFF_TIRE_SLIP_RL, 0.3);
    write_f32(&mut buf, OFF_TIRE_SLIP_RR, 0.4);
    let t = adapter.normalize(&buf)?;
    // avg(0.1, 0.2, 0.3, 0.4) = 0.25
    assert!((t.slip_ratio - 0.25).abs() < 0.01, "slip={}", t.slip_ratio);
    Ok(())
}

// ── CarDash format (311 bytes) ───────────────────────────────────────────────

#[test]
fn deep_cardash_throttle_brake_from_u8() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut buf = make_cardash();
    buf[OFF_DASH_ACCEL] = 255; // full throttle
    buf[OFF_DASH_BRAKE] = 128; // ~50% brake
    buf[OFF_DASH_CLUTCH] = 64; // ~25% clutch
    let t = adapter.normalize(&buf)?;
    assert!((t.throttle - 1.0).abs() < 0.01, "throttle={}", t.throttle);
    assert!((t.brake - 128.0 / 255.0).abs() < 0.01, "brake={}", t.brake);
    assert!(
        (t.clutch - 64.0 / 255.0).abs() < 0.01,
        "clutch={}",
        t.clutch
    );
    Ok(())
}

#[test]
fn deep_cardash_gear_encoding() -> TestResult {
    let adapter = ForzaAdapter::new();
    // gear: 0=Reverse→-1, 1=Neutral→0, 2=1st→1, etc.
    for (raw, expected) in [(0u8, -1i8), (1, 0), (2, 1), (3, 2), (7, 6)] {
        let mut buf = make_cardash();
        buf[OFF_DASH_GEAR] = raw;
        let t = adapter.normalize(&buf)?;
        assert_eq!(
            t.gear, expected,
            "raw={raw} expected={expected} got={}",
            t.gear
        );
    }
    Ok(())
}

#[test]
fn deep_cardash_steer_i8_to_float() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut buf = make_cardash();
    buf[OFF_DASH_STEER] = 127u8; // i8 = 127 → +1.0
    let t = adapter.normalize(&buf)?;
    assert!(
        (t.steering_angle - 1.0).abs() < 0.01,
        "steer={}",
        t.steering_angle
    );

    let mut buf2 = make_cardash();
    buf2[OFF_DASH_STEER] = (-127i8) as u8; // i8 = -127 → -1.0
    let t2 = adapter.normalize(&buf2)?;
    assert!(
        (t2.steering_angle - (-1.0)).abs() < 0.01,
        "steer={}",
        t2.steering_angle
    );
    Ok(())
}

#[test]
fn deep_cardash_tire_temps_fahrenheit_to_celsius() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut buf = make_cardash();
    // 212°F = 100°C
    write_f32(&mut buf, OFF_DASH_TIRE_TEMP_FL, 212.0);
    write_f32(&mut buf, OFF_DASH_TIRE_TEMP_FR, 212.0);
    write_f32(&mut buf, OFF_DASH_TIRE_TEMP_RL, 212.0);
    write_f32(&mut buf, OFF_DASH_TIRE_TEMP_RR, 212.0);
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.tire_temps_c[0], 100, "FL temp");
    assert_eq!(t.tire_temps_c[1], 100, "FR temp");
    assert_eq!(t.tire_temps_c[2], 100, "RL temp");
    assert_eq!(t.tire_temps_c[3], 100, "RR temp");
    Ok(())
}

#[test]
fn deep_cardash_lap_timing() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut buf = make_cardash();
    write_f32(&mut buf, OFF_DASH_BEST_LAP, 85.3);
    write_f32(&mut buf, OFF_DASH_LAST_LAP, 86.1);
    write_f32(&mut buf, OFF_DASH_CUR_LAP, 42.5);
    buf[OFF_DASH_LAP_NUMBER..OFF_DASH_LAP_NUMBER + 2].copy_from_slice(&7u16.to_le_bytes());
    buf[OFF_DASH_RACE_POS] = 3;
    let t = adapter.normalize(&buf)?;
    assert!((t.best_lap_time_s - 85.3).abs() < 0.01);
    assert!((t.last_lap_time_s - 86.1).abs() < 0.01);
    assert!((t.current_lap_time_s - 42.5).abs() < 0.01);
    assert_eq!(t.lap, 7);
    assert_eq!(t.position, 3);
    Ok(())
}

#[test]
fn deep_cardash_fuel_percent() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut buf = make_cardash();
    write_f32(&mut buf, OFF_DASH_FUEL, 0.73);
    let t = adapter.normalize(&buf)?;
    assert!(
        (t.fuel_percent - 0.73).abs() < 0.01,
        "fuel={}",
        t.fuel_percent
    );
    Ok(())
}

// ── FM8 CarDash (331 bytes) ──────────────────────────────────────────────────

#[test]
fn deep_fm8_cardash_accepted() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut buf = vec![0u8; FM8_SIZE];
    write_i32(&mut buf, OFF_IS_RACE_ON, 1);
    write_f32(&mut buf, OFF_CURRENT_RPM, 4000.0);
    buf[OFF_DASH_ACCEL] = 200;
    let t = adapter.normalize(&buf)?;
    assert!((t.rpm - 4000.0).abs() < 0.1);
    assert!(t.throttle > 0.5);
    Ok(())
}

// ── FH4 CarDash (324 bytes) ──────────────────────────────────────────────────

#[test]
fn deep_fh4_cardash_offset_shift() -> TestResult {
    let adapter = ForzaAdapter::new();
    let ho = 12usize; // FH4 horizon offset
    let mut buf = vec![0u8; FH4_SIZE];
    write_i32(&mut buf, OFF_IS_RACE_ON, 1);
    write_f32(&mut buf, OFF_CURRENT_RPM, 5500.0);
    buf[OFF_DASH_ACCEL + ho] = 180;
    buf[OFF_DASH_GEAR + ho] = 4; // raw 4 → gear 3
    let t = adapter.normalize(&buf)?;
    assert!((t.rpm - 5500.0).abs() < 0.1);
    assert_eq!(t.gear, 3, "FH4 gear raw=4→3");
    assert!(t.throttle > 0.5);
    Ok(())
}

// ── Extended fields ──────────────────────────────────────────────────────────

#[test]
fn deep_wheel_speed_in_extended() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut buf = make_cardash();
    // Wheel rotation speed offsets (sled section)
    write_f32(&mut buf, 100, 15.0); // FL
    write_f32(&mut buf, 104, 16.0); // FR
    write_f32(&mut buf, 108, 14.0); // RL
    write_f32(&mut buf, 112, 15.5); // RR
    let t = adapter.normalize(&buf)?;
    assert!(t.extended.contains_key("wheel_speed_fl"));
    assert!(t.extended.contains_key("wheel_speed_rr"));
    Ok(())
}
