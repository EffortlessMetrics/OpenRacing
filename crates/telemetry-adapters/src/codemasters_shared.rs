//! Shared Codemasters Mode 1 UDP packet parsing.
//!
//! Multiple Codemasters-family games (DiRT Rally 2.0, DiRT 3, DiRT 4, GRID 2019,
//! GRID Autosport, GRID Legends) emit the same fixed-layout 264-byte Mode 1 binary
//! stream where every field is a little-endian `f32` at a known byte offset.
//!
//! This module extracts the common offset constants and parsing logic so that each
//! game-specific adapter can delegate to a single implementation.

use crate::{NormalizedTelemetry, TelemetryFlags, TelemetryValue};
use anyhow::{Result, anyhow};

// ── Mode 1 packet layout ────────────────────────────────────────────────────

/// Minimum packet size for a valid Mode 1 packet.
pub const MIN_PACKET_SIZE: usize = 264;

// Byte offsets – all fields are little-endian `f32` (4 bytes each).
pub const OFF_VEL_X: usize = 32;
pub const OFF_VEL_Y: usize = 36;
pub const OFF_VEL_Z: usize = 40;
pub const OFF_WHEEL_SPEED_RL: usize = 100;
pub const OFF_WHEEL_SPEED_RR: usize = 104;
pub const OFF_WHEEL_SPEED_FL: usize = 108;
pub const OFF_WHEEL_SPEED_FR: usize = 112;
pub const OFF_THROTTLE: usize = 116;
pub const OFF_STEER: usize = 120;
pub const OFF_BRAKE: usize = 124;
pub const OFF_GEAR: usize = 132;
pub const OFF_GFORCE_LAT: usize = 136;
pub const OFF_GFORCE_LON: usize = 140;
pub const OFF_CURRENT_LAP: usize = 144;
pub const OFF_RPM: usize = 148;
pub const OFF_CAR_POSITION: usize = 156;
pub const OFF_FUEL_IN_TANK: usize = 180;
pub const OFF_FUEL_CAPACITY: usize = 184;
pub const OFF_IN_PIT: usize = 188;
pub const OFF_BRAKES_TEMP_FL: usize = 212;
pub const OFF_TYRES_PRESSURE_FL: usize = 228;
pub const OFF_LAST_LAP_TIME: usize = 248;
pub const OFF_MAX_RPM: usize = 252;
pub const OFF_MAX_GEARS: usize = 260;

/// Lateral-G normalisation range for the FFB scalar (rally/circuit cars ≤ ±3 G).
pub const FFB_LAT_G_MAX: f32 = 3.0;

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Read a little-endian `f32` from `data` at `offset`. Returns `None` if out of bounds.
pub fn read_f32(data: &[u8], offset: usize) -> Option<f32> {
    data.get(offset..offset + 4)
        .and_then(|b| b.try_into().ok())
        .map(f32::from_le_bytes)
        .filter(|v| v.is_finite())
}

// ── Shared Mode 1 parser ─────────────────────────────────────────────────────

/// Parse a Codemasters Mode 1 UDP packet into [`NormalizedTelemetry`].
///
/// `game_label` is used only for the error message on short packets (e.g.
/// `"DiRT Rally 2.0"`, `"GRID 2019"`).
pub fn parse_codemasters_mode1_common(
    data: &[u8],
    game_label: &str,
) -> Result<NormalizedTelemetry> {
    if data.len() < MIN_PACKET_SIZE {
        return Err(anyhow!(
            "{} packet too short: need at least {} bytes, got {}",
            game_label,
            MIN_PACKET_SIZE,
            data.len()
        ));
    }

    // Speed: average absolute wheel speed (m/s); fall back to velocity magnitude.
    let ws_fl = read_f32(data, OFF_WHEEL_SPEED_FL).unwrap_or(0.0).abs();
    let ws_fr = read_f32(data, OFF_WHEEL_SPEED_FR).unwrap_or(0.0).abs();
    let ws_rl = read_f32(data, OFF_WHEEL_SPEED_RL).unwrap_or(0.0).abs();
    let ws_rr = read_f32(data, OFF_WHEEL_SPEED_RR).unwrap_or(0.0).abs();
    let speed_ms = if ws_fl + ws_fr + ws_rl + ws_rr > 0.0 {
        (ws_fl + ws_fr + ws_rl + ws_rr) / 4.0
    } else {
        let vx = read_f32(data, OFF_VEL_X).unwrap_or(0.0);
        let vy = read_f32(data, OFF_VEL_Y).unwrap_or(0.0);
        let vz = read_f32(data, OFF_VEL_Z).unwrap_or(0.0);
        (vx * vx + vy * vy + vz * vz).sqrt()
    };

    let rpm_raw = read_f32(data, OFF_RPM).unwrap_or(0.0).max(0.0);
    let max_rpm = read_f32(data, OFF_MAX_RPM).unwrap_or(0.0).max(0.0);

    // Gear: 0.0 = reverse (→ -1), 1.0–8.0 = gears 1–8.
    let gear_raw = read_f32(data, OFF_GEAR).unwrap_or(0.0);
    let gear: i8 = if gear_raw < 0.5 {
        -1
    } else {
        (gear_raw.round() as i8).clamp(-1, 8)
    };

    let throttle = read_f32(data, OFF_THROTTLE).unwrap_or(0.0).clamp(0.0, 1.0);
    let steering_angle = read_f32(data, OFF_STEER).unwrap_or(0.0).clamp(-1.0, 1.0);
    let brake = read_f32(data, OFF_BRAKE).unwrap_or(0.0).clamp(0.0, 1.0);

    let lat_g = read_f32(data, OFF_GFORCE_LAT).unwrap_or(0.0);
    let lon_g = read_f32(data, OFF_GFORCE_LON).unwrap_or(0.0);

    // FFB scalar derived from lateral G, normalised to [-1, 1].
    let ffb_scalar = (lat_g / FFB_LAT_G_MAX).clamp(-1.0, 1.0);

    // Lap is 0-indexed in the packet; expose as 1-indexed.
    let lap_raw = read_f32(data, OFF_CURRENT_LAP).unwrap_or(0.0).max(0.0);
    let lap = (lap_raw.round() as u16).saturating_add(1);

    let position = read_f32(data, OFF_CAR_POSITION)
        .map(|p| p.round().clamp(0.0, 255.0) as u8)
        .unwrap_or(0);

    let fuel_in_tank = read_f32(data, OFF_FUEL_IN_TANK).unwrap_or(0.0).max(0.0);
    let fuel_capacity = read_f32(data, OFF_FUEL_CAPACITY).unwrap_or(1.0).max(1.0);
    let fuel_percent = (fuel_in_tank / fuel_capacity).clamp(0.0, 1.0);

    let in_pits = read_f32(data, OFF_IN_PIT)
        .map(|v| v >= 0.5)
        .unwrap_or(false);

    let tire_temps_c = [
        read_f32(data, OFF_BRAKES_TEMP_FL)
            .unwrap_or(0.0)
            .clamp(0.0, 255.0) as u8,
        read_f32(data, OFF_BRAKES_TEMP_FL + 4)
            .unwrap_or(0.0)
            .clamp(0.0, 255.0) as u8,
        read_f32(data, OFF_BRAKES_TEMP_FL + 8)
            .unwrap_or(0.0)
            .clamp(0.0, 255.0) as u8,
        read_f32(data, OFF_BRAKES_TEMP_FL + 12)
            .unwrap_or(0.0)
            .clamp(0.0, 255.0) as u8,
    ];

    let tire_pressures_psi = [
        read_f32(data, OFF_TYRES_PRESSURE_FL).unwrap_or(0.0),
        read_f32(data, OFF_TYRES_PRESSURE_FL + 4).unwrap_or(0.0),
        read_f32(data, OFF_TYRES_PRESSURE_FL + 8).unwrap_or(0.0),
        read_f32(data, OFF_TYRES_PRESSURE_FL + 12).unwrap_or(0.0),
    ];

    let num_gears = read_f32(data, OFF_MAX_GEARS)
        .map(|g| g.round().clamp(0.0, 255.0) as u8)
        .unwrap_or(0);

    let last_lap_time_s = read_f32(data, OFF_LAST_LAP_TIME).unwrap_or(0.0).max(0.0);

    let flags = TelemetryFlags {
        in_pits,
        ..Default::default()
    };

    let mut builder = NormalizedTelemetry::builder()
        .speed_ms(speed_ms)
        .rpm(rpm_raw)
        .gear(gear)
        .throttle(throttle)
        .steering_angle(steering_angle)
        .brake(brake)
        .lateral_g(lat_g)
        .longitudinal_g(lon_g)
        .ffb_scalar(ffb_scalar)
        .lap(lap)
        .position(position)
        .fuel_percent(fuel_percent)
        .tire_temps_c(tire_temps_c)
        .tire_pressures_psi(tire_pressures_psi)
        .num_gears(num_gears)
        .last_lap_time_s(last_lap_time_s)
        .flags(flags)
        .extended("wheel_speed_fl".to_string(), TelemetryValue::Float(ws_fl))
        .extended("wheel_speed_fr".to_string(), TelemetryValue::Float(ws_fr))
        .extended("wheel_speed_rl".to_string(), TelemetryValue::Float(ws_rl))
        .extended("wheel_speed_rr".to_string(), TelemetryValue::Float(ws_rr));

    if max_rpm > 0.0 {
        let rpm_fraction = (rpm_raw / max_rpm).clamp(0.0, 1.0);
        builder = builder.max_rpm(max_rpm).extended(
            "rpm_fraction".to_string(),
            TelemetryValue::Float(rpm_fraction),
        );
    }

    Ok(builder.build())
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_packet(size: usize) -> Vec<u8> {
        vec![0u8; size]
    }

    fn write_f32_le(buf: &mut [u8], offset: usize, value: f32) {
        buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    #[test]
    fn codemasters_shared_rejects_short_packet() -> Result<(), Box<dyn std::error::Error>> {
        let result = parse_codemasters_mode1_common(&[0u8; MIN_PACKET_SIZE - 1], "Test");
        assert!(result.is_err(), "expected error for short packet");
        Ok(())
    }

    #[test]
    fn codemasters_shared_zero_packet_yields_zero_speed_and_rpm()
    -> Result<(), Box<dyn std::error::Error>> {
        let raw = make_packet(MIN_PACKET_SIZE);
        let t = parse_codemasters_mode1_common(&raw, "Test")?;
        assert_eq!(t.speed_ms, 0.0);
        assert_eq!(t.rpm, 0.0);
        Ok(())
    }

    #[test]
    fn codemasters_shared_zero_gear_maps_to_reverse() -> Result<(), Box<dyn std::error::Error>> {
        let raw = make_packet(MIN_PACKET_SIZE);
        let t = parse_codemasters_mode1_common(&raw, "Test")?;
        assert_eq!(t.gear, -1);
        Ok(())
    }

    #[test]
    fn codemasters_shared_forward_gears() -> Result<(), Box<dyn std::error::Error>> {
        for g in 1i8..=8 {
            let mut raw = make_packet(MIN_PACKET_SIZE);
            write_f32_le(&mut raw, OFF_GEAR, f32::from(g));
            let t = parse_codemasters_mode1_common(&raw, "Test")?;
            assert_eq!(t.gear, g, "expected gear {g}");
        }
        Ok(())
    }

    #[test]
    fn codemasters_shared_speed_from_wheel_speeds() -> Result<(), Box<dyn std::error::Error>> {
        let mut raw = make_packet(MIN_PACKET_SIZE);
        write_f32_le(&mut raw, OFF_WHEEL_SPEED_FL, 20.0);
        write_f32_le(&mut raw, OFF_WHEEL_SPEED_FR, 20.0);
        write_f32_le(&mut raw, OFF_WHEEL_SPEED_RL, 20.0);
        write_f32_le(&mut raw, OFF_WHEEL_SPEED_RR, 20.0);
        let t = parse_codemasters_mode1_common(&raw, "Test")?;
        assert!(
            (t.speed_ms - 20.0).abs() < 0.001,
            "speed_ms should be 20.0, got {}",
            t.speed_ms
        );
        Ok(())
    }

    #[test]
    fn codemasters_shared_speed_falls_back_to_velocity_magnitude()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut raw = make_packet(MIN_PACKET_SIZE);
        write_f32_le(&mut raw, OFF_VEL_X, 3.0);
        write_f32_le(&mut raw, OFF_VEL_Y, 0.0);
        write_f32_le(&mut raw, OFF_VEL_Z, 4.0);
        let t = parse_codemasters_mode1_common(&raw, "Test")?;
        assert!(
            (t.speed_ms - 5.0).abs() < 0.001,
            "speed_ms should be 5.0, got {}",
            t.speed_ms
        );
        Ok(())
    }

    #[test]
    fn codemasters_shared_throttle_brake_clamped() -> Result<(), Box<dyn std::error::Error>> {
        let mut raw = make_packet(MIN_PACKET_SIZE);
        write_f32_le(&mut raw, OFF_THROTTLE, 3.0);
        write_f32_le(&mut raw, OFF_BRAKE, 5.0);
        let t = parse_codemasters_mode1_common(&raw, "Test")?;
        assert!(
            t.throttle >= 0.0 && t.throttle <= 1.0,
            "throttle out of range: {}",
            t.throttle
        );
        assert!(
            t.brake >= 0.0 && t.brake <= 1.0,
            "brake out of range: {}",
            t.brake
        );
        Ok(())
    }

    #[test]
    fn codemasters_shared_ffb_scalar_clamped() -> Result<(), Box<dyn std::error::Error>> {
        let mut raw = make_packet(MIN_PACKET_SIZE);
        write_f32_le(&mut raw, OFF_GFORCE_LAT, 10.0);
        let t = parse_codemasters_mode1_common(&raw, "Test")?;
        assert!(
            t.ffb_scalar >= -1.0 && t.ffb_scalar <= 1.0,
            "ffb_scalar out of range: {}",
            t.ffb_scalar
        );
        Ok(())
    }

    #[test]
    fn codemasters_shared_in_pit_flag() -> Result<(), Box<dyn std::error::Error>> {
        let mut raw = make_packet(MIN_PACKET_SIZE);
        write_f32_le(&mut raw, OFF_IN_PIT, 1.0);
        let t = parse_codemasters_mode1_common(&raw, "Test")?;
        assert!(t.flags.in_pits, "in_pits should be true");
        Ok(())
    }

    #[test]
    fn codemasters_shared_rpm_and_fraction() -> Result<(), Box<dyn std::error::Error>> {
        let mut raw = make_packet(MIN_PACKET_SIZE);
        write_f32_le(&mut raw, OFF_RPM, 5000.0);
        write_f32_le(&mut raw, OFF_MAX_RPM, 8000.0);
        let t = parse_codemasters_mode1_common(&raw, "Test")?;
        assert!((t.rpm - 5000.0).abs() < 0.001);
        assert!((t.max_rpm - 8000.0).abs() < 0.001);
        if let Some(TelemetryValue::Float(fraction)) = t.extended.get("rpm_fraction") {
            assert!(
                (fraction - 0.625).abs() < 0.001,
                "rpm_fraction should be 0.625, got {fraction}"
            );
        } else {
            return Err("rpm_fraction not found in extended telemetry".into());
        }
        Ok(())
    }

    #[test]
    fn codemasters_shared_fuel_percent() -> Result<(), Box<dyn std::error::Error>> {
        let mut raw = make_packet(MIN_PACKET_SIZE);
        write_f32_le(&mut raw, OFF_FUEL_IN_TANK, 40.0);
        write_f32_le(&mut raw, OFF_FUEL_CAPACITY, 80.0);
        let t = parse_codemasters_mode1_common(&raw, "Test")?;
        assert!(
            (t.fuel_percent - 0.5).abs() < 0.001,
            "fuel_percent should be 0.5, got {}",
            t.fuel_percent
        );
        Ok(())
    }

    #[test]
    fn codemasters_shared_read_f32_out_of_bounds_returns_none() {
        let data = [0u8; 3];
        assert!(read_f32(&data, 0).is_none());
        assert!(read_f32(&data, 4).is_none());
    }

    #[test]
    fn codemasters_shared_read_f32_valid() {
        let val = 42.5f32;
        let bytes = val.to_le_bytes();
        let mut data = vec![0u8; 8];
        data[4..8].copy_from_slice(&bytes);
        assert_eq!(read_f32(&data, 4), Some(42.5));
    }
}

#[cfg(test)]
mod proptest_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(500))]

        #[test]
        fn codemasters_shared_no_panic_on_arbitrary(
            data in proptest::collection::vec(any::<u8>(), 0..1024)
        ) {
            let _ = parse_codemasters_mode1_common(&data, "PropTest");
        }

        #[test]
        fn codemasters_shared_short_packet_returns_err(len in 0usize..MIN_PACKET_SIZE) {
            let data = vec![0u8; len];
            prop_assert!(parse_codemasters_mode1_common(&data, "PropTest").is_err());
        }
    }
}
