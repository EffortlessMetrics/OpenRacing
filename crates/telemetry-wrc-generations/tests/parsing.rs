//! Integration tests for WRC Generations (Codemasters Mode 1) UDP packet parsing.
//!
//! These tests verify the public API of the `racing-wheel-telemetry-wrc-generations`
//! crate: struct layout expectations, field parsing at known byte offsets, value
//! clamping, and edge-case handling.

use racing_wheel_telemetry_wrc_generations::{
    NormalizedTelemetry, TelemetryAdapter, TelemetryValue, WrcGenerationsAdapter,
};

// ---------------------------------------------------------------------------
// Codemasters Mode 1 byte offsets (protocol spec – all little-endian f32).
//
// Verified against community documentation:
//   - dr2_logger udp_data.py (ErlerPhilipp/dr2_logger)
//   - Codemasters telemetry spreadsheet (DR1/DR4/DR2.0 field map)
//   - dirt-rally-time-recorder receiver.py and gearTracker.py
// ---------------------------------------------------------------------------
const MIN_PACKET_SIZE: usize = 264;

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

/// Build a "realistic" packet with plausible values for a car mid-stage.
fn realistic_packet() -> Vec<u8> {
    let mut buf = make_packet(MIN_PACKET_SIZE);
    write_f32(&mut buf, OFF_WHEEL_SPEED_FL, 22.5);
    write_f32(&mut buf, OFF_WHEEL_SPEED_FR, 22.3);
    write_f32(&mut buf, OFF_WHEEL_SPEED_RL, 23.0);
    write_f32(&mut buf, OFF_WHEEL_SPEED_RR, 22.8);
    write_f32(&mut buf, OFF_THROTTLE, 0.72);
    write_f32(&mut buf, OFF_BRAKE, 0.0);
    write_f32(&mut buf, OFF_STEER, -0.15);
    write_f32(&mut buf, OFF_GEAR, 4.0);
    write_f32(&mut buf, OFF_RPM, 5500.0);
    write_f32(&mut buf, OFF_MAX_RPM, 8000.0);
    write_f32(&mut buf, OFF_GFORCE_LAT, 0.8);
    write_f32(&mut buf, OFF_GFORCE_LON, 0.3);
    write_f32(&mut buf, OFF_CURRENT_LAP, 2.0);
    write_f32(&mut buf, OFF_CAR_POSITION, 3.0);
    write_f32(&mut buf, OFF_FUEL_IN_TANK, 30.0);
    write_f32(&mut buf, OFF_FUEL_CAPACITY, 60.0);
    write_f32(&mut buf, OFF_IN_PIT, 0.0);
    write_f32(&mut buf, OFF_BRAKES_TEMP_FL, 120.0);
    write_f32(&mut buf, OFF_BRAKES_TEMP_FR, 118.0);
    write_f32(&mut buf, OFF_BRAKES_TEMP_RL, 95.0);
    write_f32(&mut buf, OFF_BRAKES_TEMP_RR, 97.0);
    write_f32(&mut buf, OFF_TYRES_PRESSURE_FL, 28.5);
    write_f32(&mut buf, OFF_TYRES_PRESSURE_FR, 28.3);
    write_f32(&mut buf, OFF_TYRES_PRESSURE_RL, 27.0);
    write_f32(&mut buf, OFF_TYRES_PRESSURE_RR, 27.2);
    write_f32(&mut buf, OFF_LAST_LAP_TIME, 185.7);
    write_f32(&mut buf, OFF_MAX_GEARS, 6.0);
    buf
}

// ===========================================================================
// 1. Struct / packet size verification
// ===========================================================================

#[test]
fn min_packet_size_is_264_bytes() -> Result<(), Box<dyn std::error::Error>> {
    // The Codemasters Mode 1 packet must be at least 264 bytes.
    // OFF_MAX_GEARS (260) + 4 bytes = 264.
    assert_eq!(OFF_MAX_GEARS + 4, MIN_PACKET_SIZE);
    Ok(())
}

#[test]
fn rejects_packet_shorter_than_minimum() -> Result<(), Box<dyn std::error::Error>> {
    for len in [0, 1, 100, 263] {
        let result = parse(&make_packet(len));
        assert!(result.is_err(), "expected error for {len}-byte packet");
    }
    Ok(())
}

#[test]
fn accepts_exact_minimum_size_packet() -> Result<(), Box<dyn std::error::Error>> {
    let t = parse(&make_packet(MIN_PACKET_SIZE))?;
    assert_eq!(t.speed_ms, 0.0);
    Ok(())
}

#[test]
fn accepts_oversized_packet() -> Result<(), Box<dyn std::error::Error>> {
    let t = parse(&make_packet(MIN_PACKET_SIZE + 256))?;
    assert_eq!(t.speed_ms, 0.0);
    Ok(())
}

// ===========================================================================
// 2. Byte-level parsing
// ===========================================================================

#[test]
fn realistic_packet_parses_correctly() -> Result<(), Box<dyn std::error::Error>> {
    let raw = realistic_packet();
    let t = parse(&raw)?;

    // Speed: average of wheel speeds ≈ 22.65 m/s
    let expected_speed = (22.5 + 22.3 + 23.0 + 22.8) / 4.0;
    assert!(
        (t.speed_ms - expected_speed).abs() < 0.01,
        "speed_ms: expected ~{expected_speed}, got {}",
        t.speed_ms
    );

    assert!((t.throttle - 0.72).abs() < 0.001);
    assert!((t.brake - 0.0).abs() < 0.001);
    assert!((t.steering_angle - (-0.15)).abs() < 0.001);
    assert_eq!(t.gear, 4);
    assert!((t.rpm - 5500.0).abs() < 0.01);
    assert!((t.max_rpm - 8000.0).abs() < 0.01);
    assert!((t.lateral_g - 0.8).abs() < 0.001);
    assert!((t.longitudinal_g - 0.3).abs() < 0.001);

    // Lap is raw + 1 → 2.0 + 1 = 3
    assert_eq!(t.lap, 3);
    assert_eq!(t.position, 3);

    // Fuel: 30 / 60 = 0.5
    assert!((t.fuel_percent - 0.5).abs() < 0.001);
    assert!(!t.flags.in_pits);

    // Tire temps (clamped to u8)
    assert_eq!(t.tire_temps_c, [120, 118, 95, 97]);

    // Tire pressures
    assert!((t.tire_pressures_psi[0] - 28.5).abs() < 0.01);
    assert!((t.tire_pressures_psi[1] - 28.3).abs() < 0.01);
    assert!((t.tire_pressures_psi[2] - 27.0).abs() < 0.01);
    assert!((t.tire_pressures_psi[3] - 27.2).abs() < 0.01);

    assert!((t.last_lap_time_s - 185.7).abs() < 0.01);
    assert_eq!(t.num_gears, 6);
    Ok(())
}

#[test]
fn each_f32_field_at_correct_offset() -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = make_packet(MIN_PACKET_SIZE);
    write_f32(&mut buf, OFF_THROTTLE, 0.42);
    write_f32(&mut buf, OFF_BRAKE, 0.37);
    write_f32(&mut buf, OFF_STEER, -0.55);
    write_f32(&mut buf, OFF_RPM, 3200.0);
    write_f32(&mut buf, OFF_MAX_RPM, 7500.0);
    write_f32(&mut buf, OFF_GEAR, 2.0);
    write_f32(&mut buf, OFF_GFORCE_LAT, 1.5);
    write_f32(&mut buf, OFF_GFORCE_LON, -0.4);
    write_f32(&mut buf, OFF_CURRENT_LAP, 0.0);
    write_f32(&mut buf, OFF_CAR_POSITION, 5.0);
    write_f32(&mut buf, OFF_FUEL_IN_TANK, 20.0);
    write_f32(&mut buf, OFF_FUEL_CAPACITY, 40.0);
    write_f32(&mut buf, OFF_LAST_LAP_TIME, 92.3);
    write_f32(&mut buf, OFF_MAX_GEARS, 5.0);

    let t = parse(&buf)?;

    assert!((t.throttle - 0.42).abs() < 0.001);
    assert!((t.brake - 0.37).abs() < 0.001);
    assert!((t.steering_angle - (-0.55)).abs() < 0.001);
    assert!((t.rpm - 3200.0).abs() < 0.01);
    assert!((t.max_rpm - 7500.0).abs() < 0.01);
    assert_eq!(t.gear, 2);
    assert!((t.lateral_g - 1.5).abs() < 0.001);
    assert!((t.longitudinal_g - (-0.4)).abs() < 0.001);
    assert_eq!(t.lap, 1); // 0 + 1
    assert_eq!(t.position, 5);
    assert!((t.fuel_percent - 0.5).abs() < 0.001);
    assert!((t.last_lap_time_s - 92.3).abs() < 0.01);
    assert_eq!(t.num_gears, 5);
    Ok(())
}

// ===========================================================================
// 3. Speed calculation
// ===========================================================================

#[test]
fn speed_from_wheel_speeds_is_average() -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = make_packet(MIN_PACKET_SIZE);
    write_f32(&mut buf, OFF_WHEEL_SPEED_FL, 10.0);
    write_f32(&mut buf, OFF_WHEEL_SPEED_FR, 20.0);
    write_f32(&mut buf, OFF_WHEEL_SPEED_RL, 30.0);
    write_f32(&mut buf, OFF_WHEEL_SPEED_RR, 40.0);

    let t = parse(&buf)?;
    let expected = (10.0 + 20.0 + 30.0 + 40.0) / 4.0;
    assert!(
        (t.speed_ms - expected).abs() < 0.001,
        "expected {expected}, got {}",
        t.speed_ms
    );
    Ok(())
}

#[test]
fn speed_falls_back_to_velocity_magnitude() -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = make_packet(MIN_PACKET_SIZE);
    // Wheel speeds all zero → fallback to velocity vector.
    write_f32(&mut buf, OFF_VEL_X, 3.0);
    write_f32(&mut buf, OFF_VEL_Y, 4.0);
    write_f32(&mut buf, OFF_VEL_Z, 0.0);

    let t = parse(&buf)?;
    // sqrt(9 + 16) = 5.0
    assert!(
        (t.speed_ms - 5.0).abs() < 0.001,
        "expected 5.0 from velocity fallback, got {}",
        t.speed_ms
    );
    Ok(())
}

#[test]
fn negative_wheel_speeds_use_absolute_value() -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = make_packet(MIN_PACKET_SIZE);
    write_f32(&mut buf, OFF_WHEEL_SPEED_FL, -15.0);
    write_f32(&mut buf, OFF_WHEEL_SPEED_FR, -15.0);
    write_f32(&mut buf, OFF_WHEEL_SPEED_RL, -15.0);
    write_f32(&mut buf, OFF_WHEEL_SPEED_RR, -15.0);

    let t = parse(&buf)?;
    assert!(
        (t.speed_ms - 15.0).abs() < 0.001,
        "expected 15.0 (abs), got {}",
        t.speed_ms
    );
    Ok(())
}

// ===========================================================================
// 4. Gear mapping
// ===========================================================================

#[test]
fn gear_zero_raw_maps_to_neutral() -> Result<(), Box<dyn std::error::Error>> {
    // Verified: raw 0.0 = neutral in Codemasters Mode 1 (dr2_logger, gearTracker.py).
    let buf = make_packet(MIN_PACKET_SIZE); // gear offset = 0.0
    let t = parse(&buf)?;
    assert_eq!(t.gear, 0, "0.0 should map to neutral (0)");
    Ok(())
}

#[test]
fn gear_negative_one_raw_maps_to_reverse() -> Result<(), Box<dyn std::error::Error>> {
    // Verified: DR2.0/WRC sends -1.0 for reverse (gearTracker.py: "Handle reverse gear = -1").
    let mut buf = make_packet(MIN_PACKET_SIZE);
    write_f32(&mut buf, OFF_GEAR, -1.0);
    let t = parse(&buf)?;
    assert_eq!(t.gear, -1, "-1.0 should map to reverse (-1)");
    Ok(())
}

#[test]
fn gear_forward_values() -> Result<(), Box<dyn std::error::Error>> {
    for expected in 1i8..=8 {
        let mut buf = make_packet(MIN_PACKET_SIZE);
        write_f32(&mut buf, OFF_GEAR, expected as f32);
        let t = parse(&buf)?;
        assert_eq!(
            t.gear, expected,
            "raw {expected}.0 should parse as gear {expected}"
        );
    }
    Ok(())
}

#[test]
fn gear_above_eight_clamped() -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = make_packet(MIN_PACKET_SIZE);
    write_f32(&mut buf, OFF_GEAR, 12.0);
    let t = parse(&buf)?;
    assert!(t.gear <= 8, "gear must be <=8, got {}", t.gear);
    Ok(())
}

// ===========================================================================
// 5. Value clamping and ranges
// ===========================================================================

#[test]
fn throttle_above_one_clamped() -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = make_packet(MIN_PACKET_SIZE);
    write_f32(&mut buf, OFF_THROTTLE, 2.5);
    let t = parse(&buf)?;
    assert!(
        t.throttle >= 0.0 && t.throttle <= 1.0,
        "throttle must be in [0,1], got {}",
        t.throttle
    );
    Ok(())
}

#[test]
fn throttle_negative_clamped() -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = make_packet(MIN_PACKET_SIZE);
    write_f32(&mut buf, OFF_THROTTLE, -0.5);
    let t = parse(&buf)?;
    assert!(
        t.throttle >= 0.0,
        "throttle must be >=0, got {}",
        t.throttle
    );
    Ok(())
}

#[test]
fn brake_above_one_clamped() -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = make_packet(MIN_PACKET_SIZE);
    write_f32(&mut buf, OFF_BRAKE, 3.0);
    let t = parse(&buf)?;
    assert!(
        t.brake >= 0.0 && t.brake <= 1.0,
        "brake must be in [0,1], got {}",
        t.brake
    );
    Ok(())
}

#[test]
fn steering_clamped_to_neg_one_to_one() -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = make_packet(MIN_PACKET_SIZE);
    write_f32(&mut buf, OFF_STEER, 5.0);
    let t = parse(&buf)?;
    assert!(
        t.steering_angle >= -1.0 && t.steering_angle <= 1.0,
        "steering must be in [-1,1], got {}",
        t.steering_angle
    );

    write_f32(&mut buf, OFF_STEER, -5.0);
    let t = parse(&buf)?;
    assert!(
        t.steering_angle >= -1.0 && t.steering_angle <= 1.0,
        "steering must be in [-1,1], got {}",
        t.steering_angle
    );
    Ok(())
}

#[test]
fn fuel_percent_clamped_to_unit() -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = make_packet(MIN_PACKET_SIZE);
    write_f32(&mut buf, OFF_FUEL_IN_TANK, 100.0);
    write_f32(&mut buf, OFF_FUEL_CAPACITY, 50.0);
    let t = parse(&buf)?;
    assert!(
        t.fuel_percent >= 0.0 && t.fuel_percent <= 1.0,
        "fuel_percent must be in [0,1], got {}",
        t.fuel_percent
    );
    Ok(())
}

#[test]
fn fuel_zero_capacity_does_not_divide_by_zero() -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = make_packet(MIN_PACKET_SIZE);
    write_f32(&mut buf, OFF_FUEL_IN_TANK, 10.0);
    write_f32(&mut buf, OFF_FUEL_CAPACITY, 0.0);
    let t = parse(&buf)?;
    assert!(
        t.fuel_percent.is_finite(),
        "fuel_percent must be finite, got {}",
        t.fuel_percent
    );
    Ok(())
}

#[test]
fn ffb_scalar_clamped() -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = make_packet(MIN_PACKET_SIZE);
    write_f32(&mut buf, OFF_GFORCE_LAT, 10.0);
    let t = parse(&buf)?;
    assert!(
        t.ffb_scalar >= -1.0 && t.ffb_scalar <= 1.0,
        "ffb_scalar must be in [-1,1], got {}",
        t.ffb_scalar
    );

    write_f32(&mut buf, OFF_GFORCE_LAT, -10.0);
    let t = parse(&buf)?;
    assert!(
        t.ffb_scalar >= -1.0 && t.ffb_scalar <= 1.0,
        "ffb_scalar must be in [-1,1], got {}",
        t.ffb_scalar
    );
    Ok(())
}

#[test]
fn ffb_scalar_proportional_to_lateral_g() -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = make_packet(MIN_PACKET_SIZE);
    write_f32(&mut buf, OFF_GFORCE_LAT, 1.5);
    let t = parse(&buf)?;
    // 1.5 / 3.0 = 0.5
    assert!(
        (t.ffb_scalar - 0.5).abs() < 0.001,
        "expected ffb_scalar ~0.5, got {}",
        t.ffb_scalar
    );
    Ok(())
}

#[test]
fn rpm_negative_treated_as_zero() -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = make_packet(MIN_PACKET_SIZE);
    write_f32(&mut buf, OFF_RPM, -500.0);
    let t = parse(&buf)?;
    assert!(t.rpm >= 0.0, "rpm must be >=0, got {}", t.rpm);
    Ok(())
}

// ===========================================================================
// 6. Lap / position
// ===========================================================================

#[test]
fn lap_is_raw_plus_one() -> Result<(), Box<dyn std::error::Error>> {
    let cases: &[(f32, u16)] = &[(0.0, 1), (1.0, 2), (5.0, 6), (99.0, 100)];
    for &(raw_val, expected_lap) in cases {
        let mut buf = make_packet(MIN_PACKET_SIZE);
        write_f32(&mut buf, OFF_CURRENT_LAP, raw_val);
        let t = parse(&buf)?;
        assert_eq!(
            t.lap, expected_lap,
            "raw lap {raw_val} -> expected {expected_lap}, got {}",
            t.lap
        );
    }
    Ok(())
}

#[test]
fn position_rounds_correctly() -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = make_packet(MIN_PACKET_SIZE);
    write_f32(&mut buf, OFF_CAR_POSITION, 7.6);
    let t = parse(&buf)?;
    assert_eq!(t.position, 8, "7.6 should round to 8");
    Ok(())
}

// ===========================================================================
// 7. Tire temps and pressures
// ===========================================================================

#[test]
fn tire_temps_at_correct_offsets() -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = make_packet(MIN_PACKET_SIZE);
    write_f32(&mut buf, OFF_BRAKES_TEMP_FL, 100.0);
    write_f32(&mut buf, OFF_BRAKES_TEMP_FR, 110.0);
    write_f32(&mut buf, OFF_BRAKES_TEMP_RL, 90.0);
    write_f32(&mut buf, OFF_BRAKES_TEMP_RR, 95.0);

    let t = parse(&buf)?;
    // Order in array: [FL, FR, RL, RR]
    assert_eq!(t.tire_temps_c, [100, 110, 90, 95]);
    Ok(())
}

#[test]
fn tire_temps_clamped_to_u8_range() -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = make_packet(MIN_PACKET_SIZE);
    write_f32(&mut buf, OFF_BRAKES_TEMP_FL, 999.0);
    write_f32(&mut buf, OFF_BRAKES_TEMP_FR, -50.0);

    let t = parse(&buf)?;
    assert_eq!(t.tire_temps_c[0], 255, "999 C should clamp to 255");
    assert_eq!(t.tire_temps_c[1], 0, "negative should clamp to 0");
    Ok(())
}

#[test]
fn tire_pressures_at_correct_offsets() -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = make_packet(MIN_PACKET_SIZE);
    write_f32(&mut buf, OFF_TYRES_PRESSURE_FL, 28.0);
    write_f32(&mut buf, OFF_TYRES_PRESSURE_FR, 29.0);
    write_f32(&mut buf, OFF_TYRES_PRESSURE_RL, 26.0);
    write_f32(&mut buf, OFF_TYRES_PRESSURE_RR, 27.0);

    let t = parse(&buf)?;
    // Order: [FL, FR, RL, RR]
    assert!((t.tire_pressures_psi[0] - 28.0).abs() < 0.01);
    assert!((t.tire_pressures_psi[1] - 29.0).abs() < 0.01);
    assert!((t.tire_pressures_psi[2] - 26.0).abs() < 0.01);
    assert!((t.tire_pressures_psi[3] - 27.0).abs() < 0.01);
    Ok(())
}

// ===========================================================================
// 8. Pit flag
// ===========================================================================

#[test]
fn in_pits_false_when_below_threshold() -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = make_packet(MIN_PACKET_SIZE);
    write_f32(&mut buf, OFF_IN_PIT, 0.0);
    let t = parse(&buf)?;
    assert!(!t.flags.in_pits);

    write_f32(&mut buf, OFF_IN_PIT, 0.49);
    let t = parse(&buf)?;
    assert!(!t.flags.in_pits, "0.49 should not trigger in_pits");
    Ok(())
}

#[test]
fn in_pits_true_when_at_or_above_half() -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = make_packet(MIN_PACKET_SIZE);
    write_f32(&mut buf, OFF_IN_PIT, 0.5);
    let t = parse(&buf)?;
    assert!(t.flags.in_pits, "0.5 should trigger in_pits");

    write_f32(&mut buf, OFF_IN_PIT, 1.0);
    let t = parse(&buf)?;
    assert!(t.flags.in_pits);
    Ok(())
}

// ===========================================================================
// 9. Extended data (wheel speeds, rpm_fraction)
// ===========================================================================

#[test]
fn extended_contains_wheel_speeds() -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = make_packet(MIN_PACKET_SIZE);
    write_f32(&mut buf, OFF_WHEEL_SPEED_FL, 11.0);
    write_f32(&mut buf, OFF_WHEEL_SPEED_FR, 12.0);
    write_f32(&mut buf, OFF_WHEEL_SPEED_RL, 13.0);
    write_f32(&mut buf, OFF_WHEEL_SPEED_RR, 14.0);

    let t = parse(&buf)?;

    for (key, expected) in [
        ("wheel_speed_fl", 11.0f32),
        ("wheel_speed_fr", 12.0),
        ("wheel_speed_rl", 13.0),
        ("wheel_speed_rr", 14.0),
    ] {
        match t.extended.get(key) {
            Some(TelemetryValue::Float(v)) => assert!(
                (v - expected).abs() < 0.001,
                "{key}: expected {expected}, got {v}"
            ),
            other => return Err(format!("{key}: expected Float({expected}), got {other:?}").into()),
        }
    }
    Ok(())
}

#[test]
fn extended_rpm_fraction_present_when_max_rpm_positive() -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = make_packet(MIN_PACKET_SIZE);
    write_f32(&mut buf, OFF_RPM, 4000.0);
    write_f32(&mut buf, OFF_MAX_RPM, 8000.0);

    let t = parse(&buf)?;
    match t.extended.get("rpm_fraction") {
        Some(TelemetryValue::Float(v)) => assert!(
            (v - 0.5).abs() < 0.001,
            "rpm_fraction: expected 0.5, got {v}"
        ),
        other => return Err(format!("rpm_fraction: expected Float(0.5), got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn extended_rpm_fraction_absent_when_max_rpm_zero() -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = make_packet(MIN_PACKET_SIZE);
    write_f32(&mut buf, OFF_RPM, 4000.0);
    // max_rpm defaults to 0.0

    let t = parse(&buf)?;
    assert!(
        !t.extended.contains_key("rpm_fraction"),
        "rpm_fraction should not be present when max_rpm is 0"
    );
    Ok(())
}

// ===========================================================================
// 10. NaN / Infinity handling
// ===========================================================================

#[test]
fn nan_in_throttle_treated_as_zero() -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = make_packet(MIN_PACKET_SIZE);
    write_f32(&mut buf, OFF_THROTTLE, f32::NAN);
    let t = parse(&buf)?;
    assert!(
        t.throttle.is_finite(),
        "NaN throttle must produce finite result, got {}",
        t.throttle
    );
    Ok(())
}

#[test]
fn infinity_in_rpm_treated_as_zero() -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = make_packet(MIN_PACKET_SIZE);
    write_f32(&mut buf, OFF_RPM, f32::INFINITY);
    let t = parse(&buf)?;
    assert!(
        t.rpm.is_finite(),
        "infinite RPM must produce finite result, got {}",
        t.rpm
    );
    Ok(())
}

#[test]
fn neg_infinity_in_steer_treated_as_zero() -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = make_packet(MIN_PACKET_SIZE);
    write_f32(&mut buf, OFF_STEER, f32::NEG_INFINITY);
    let t = parse(&buf)?;
    assert!(
        t.steering_angle.is_finite(),
        "neg-inf steering must produce finite result, got {}",
        t.steering_angle
    );
    Ok(())
}

// ===========================================================================
// 11. Adapter trait conformance
// ===========================================================================

#[test]
fn game_id_is_wrc_generations() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(adapter().game_id(), "wrc_generations");
    Ok(())
}

#[test]
fn expected_update_rate_is_reasonable() -> Result<(), Box<dyn std::error::Error>> {
    let rate = adapter().expected_update_rate();
    assert!(
        rate.as_millis() > 0 && rate.as_millis() <= 100,
        "update rate {:?} out of plausible range",
        rate
    );
    Ok(())
}

// ===========================================================================
// 12. Zero packet defaults
// ===========================================================================

#[test]
fn zero_packet_defaults() -> Result<(), Box<dyn std::error::Error>> {
    let t = parse(&make_packet(MIN_PACKET_SIZE))?;
    assert_eq!(t.speed_ms, 0.0);
    assert_eq!(t.rpm, 0.0);
    assert_eq!(t.throttle, 0.0);
    assert_eq!(t.brake, 0.0);
    assert_eq!(t.steering_angle, 0.0);
    assert_eq!(t.gear, 0); // 0.0 raw -> neutral (verified: Codemasters Mode 1 spec)
    assert_eq!(t.lateral_g, 0.0);
    assert_eq!(t.longitudinal_g, 0.0);
    assert_eq!(t.ffb_scalar, 0.0);
    assert!(!t.flags.in_pits);
    assert_eq!(t.tire_temps_c, [0, 0, 0, 0]);
    Ok(())
}

#[test]
fn all_output_fields_are_finite() -> Result<(), Box<dyn std::error::Error>> {
    let t = parse(&realistic_packet())?;

    let fields = [
        ("speed_ms", t.speed_ms),
        ("rpm", t.rpm),
        ("max_rpm", t.max_rpm),
        ("throttle", t.throttle),
        ("brake", t.brake),
        ("steering_angle", t.steering_angle),
        ("lateral_g", t.lateral_g),
        ("longitudinal_g", t.longitudinal_g),
        ("ffb_scalar", t.ffb_scalar),
        ("fuel_percent", t.fuel_percent),
        ("last_lap_time_s", t.last_lap_time_s),
    ];
    for (name, val) in fields {
        assert!(val.is_finite(), "{name} must be finite, got {val}");
    }
    for (i, p) in t.tire_pressures_psi.iter().enumerate() {
        assert!(
            p.is_finite(),
            "tire_pressures_psi[{i}] must be finite, got {p}"
        );
    }
    Ok(())
}
