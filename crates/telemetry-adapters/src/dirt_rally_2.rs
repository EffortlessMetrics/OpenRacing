//! DiRT Rally 2.0 telemetry adapter for Codemasters Mode 1 (legacy) UDP format.
//!
//! Enable in-game: Settings → Accessibility → UDP Telemetry, port 20777, mode 1.
//!
//! The Mode 1 packet is a fixed-layout binary stream of 252+ bytes where every
//! field is a little-endian `f32` at a known byte offset.

use crate::{
    NormalizedTelemetry, TelemetryAdapter, TelemetryFlags, TelemetryFrame, TelemetryReceiver,
    TelemetryValue, telemetry_now_ns,
};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::Duration;
use tokio::net::UdpSocket as TokioUdpSocket;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

const DEFAULT_PORT: u16 = 20777;
const MIN_PACKET_SIZE: usize = 252;
const MAX_PACKET_SIZE: usize = 2048;
const DEFAULT_HEARTBEAT_TIMEOUT_MS: u64 = 1_500;

const ENV_PORT: &str = "OPENRACING_DIRT_RALLY_2_UDP_PORT";
const ENV_HEARTBEAT_TIMEOUT_MS: &str = "OPENRACING_DIRT_RALLY_2_HEARTBEAT_TIMEOUT_MS";

// Byte offsets for Mode 1 legacy packet fields (all f32, little-endian).
const OFF_VEL_X: usize = 28;
const OFF_VEL_Y: usize = 32;
const OFF_VEL_Z: usize = 36;
const OFF_WHEEL_SPEED_FL: usize = 92;
const OFF_WHEEL_SPEED_FR: usize = 96;
const OFF_WHEEL_SPEED_RL: usize = 100;
const OFF_WHEEL_SPEED_RR: usize = 104;
const OFF_THROTTLE: usize = 108;
const OFF_STEER: usize = 112;
const OFF_BRAKE: usize = 116;
const OFF_GEAR: usize = 124;
const OFF_GFORCE_LAT: usize = 128;
const OFF_GFORCE_LON: usize = 132;
const OFF_CURRENT_LAP: usize = 136;
const OFF_RPM: usize = 140;
const OFF_CAR_POSITION: usize = 148;
const OFF_FUEL_IN_TANK: usize = 172;
const OFF_FUEL_CAPACITY: usize = 176;
const OFF_IN_PIT: usize = 180;
const OFF_BRAKES_TEMP_FL: usize = 196;
const OFF_TYRES_PRESSURE_FL: usize = 212;
const OFF_LAST_LAP_TIME: usize = 236;
const OFF_MAX_RPM: usize = 240;
const OFF_MAX_GEARS: usize = 248;

/// Lateral G normalisation range for the FFB scalar (rally cars routinely reach ±3 G).
const FFB_LAT_G_MAX: f32 = 3.0;

/// DiRT Rally 2.0 adapter for Codemasters Mode 1 UDP telemetry.
#[derive(Clone)]
pub struct DirtRally2Adapter {
    bind_port: u16,
    update_rate: Duration,
    heartbeat_timeout: Duration,
    last_packet_ns: Arc<AtomicU64>,
}

impl Default for DirtRally2Adapter {
    fn default() -> Self {
        Self::new()
    }
}

impl DirtRally2Adapter {
    pub fn new() -> Self {
        let bind_port = std::env::var(ENV_PORT)
            .ok()
            .and_then(|v| v.parse::<u16>().ok())
            .filter(|&p| p > 0)
            .unwrap_or(DEFAULT_PORT);

        let heartbeat_ms = std::env::var(ENV_HEARTBEAT_TIMEOUT_MS)
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|&t| t > 0)
            .unwrap_or(DEFAULT_HEARTBEAT_TIMEOUT_MS);

        Self {
            bind_port,
            update_rate: Duration::from_millis(16),
            heartbeat_timeout: Duration::from_millis(heartbeat_ms),
            last_packet_ns: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn with_port(mut self, port: u16) -> Self {
        self.bind_port = port;
        self
    }

    fn is_recent_packet(&self) -> bool {
        let last = self.last_packet_ns.load(Ordering::Relaxed);
        if last == 0 {
            return false;
        }
        let now = u128::from(telemetry_now_ns());
        let elapsed_ns = now.saturating_sub(u128::from(last));
        elapsed_ns <= self.heartbeat_timeout.as_nanos()
    }
}

/// Read a little-endian `f32` from `data` at `offset`. Returns `None` if out of bounds.
fn read_f32(data: &[u8], offset: usize) -> Option<f32> {
    data.get(offset..offset + 4)
        .and_then(|b| b.try_into().ok())
        .map(f32::from_le_bytes)
}

fn parse_packet(data: &[u8]) -> Result<NormalizedTelemetry> {
    if data.len() < MIN_PACKET_SIZE {
        return Err(anyhow!(
            "DiRT Rally 2.0 packet too short: need at least {} bytes, got {}",
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
    let fuel_percent = (fuel_in_tank / fuel_capacity).clamp(0.0, 1.0) * 100.0;

    let in_pits = read_f32(data, OFF_IN_PIT).map(|v| v >= 0.5).unwrap_or(false);

    // Brake temperatures (°C) clamped to u8 range.
    let tire_temps_c = [
        read_f32(data, OFF_BRAKES_TEMP_FL).unwrap_or(0.0).clamp(0.0, 255.0) as u8,
        read_f32(data, OFF_BRAKES_TEMP_FL + 4).unwrap_or(0.0).clamp(0.0, 255.0) as u8,
        read_f32(data, OFF_BRAKES_TEMP_FL + 8).unwrap_or(0.0).clamp(0.0, 255.0) as u8,
        read_f32(data, OFF_BRAKES_TEMP_FL + 12).unwrap_or(0.0).clamp(0.0, 255.0) as u8,
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
        builder = builder
            .max_rpm(max_rpm)
            .extended("rpm_fraction".to_string(), TelemetryValue::Float(rpm_fraction));
    }

    Ok(builder.build())
}

#[async_trait]
impl TelemetryAdapter for DirtRally2Adapter {
    fn game_id(&self) -> &str {
        "dirt_rally_2"
    }

    async fn start_monitoring(&self) -> Result<TelemetryReceiver> {
        let bind_port = self.bind_port;
        let update_rate = self.update_rate;
        let last_packet_ns = Arc::clone(&self.last_packet_ns);
        let (tx, rx) = mpsc::channel(100);

        tokio::spawn(async move {
            let bind_addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, bind_port));
            let socket = match TokioUdpSocket::bind(bind_addr).await {
                Ok(s) => s,
                Err(error) => {
                    warn!(
                        error = %error,
                        port = bind_port,
                        "DiRT Rally 2.0 UDP socket bind failed"
                    );
                    return;
                }
            };

            info!(port = bind_port, "DiRT Rally 2.0 UDP adapter bound");

            let mut sequence = 0u64;
            let mut buf = vec![0u8; MAX_PACKET_SIZE];
            let timeout = (update_rate * 4).max(Duration::from_millis(25));

            loop {
                let recv = tokio::time::timeout(timeout, socket.recv(&mut buf)).await;
                let len = match recv {
                    Ok(Ok(len)) => len,
                    Ok(Err(error)) => {
                        warn!(error = %error, "DiRT Rally 2.0 UDP receive error");
                        continue;
                    }
                    Err(_) => {
                        debug!("DiRT Rally 2.0 UDP receive timeout");
                        continue;
                    }
                };

                let data = &buf[..len];
                let normalized = match parse_packet(data) {
                    Ok(n) => n,
                    Err(error) => {
                        warn!(error = %error, "Failed to parse DiRT Rally 2.0 packet");
                        continue;
                    }
                };

                last_packet_ns.store(telemetry_now_ns(), Ordering::Relaxed);

                let frame = TelemetryFrame::new(normalized, telemetry_now_ns(), sequence, len);
                if tx.send(frame).await.is_err() {
                    break;
                }

                sequence = sequence.saturating_add(1);
            }
        });

        Ok(rx)
    }

    async fn stop_monitoring(&self) -> Result<()> {
        Ok(())
    }

    fn normalize(&self, raw: &[u8]) -> Result<NormalizedTelemetry> {
        parse_packet(raw)
    }

    fn expected_update_rate(&self) -> Duration {
        self.update_rate
    }

    async fn is_game_running(&self) -> Result<bool> {
        Ok(self.is_recent_packet())
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    fn make_min_packet() -> Vec<u8> {
        vec![0u8; MIN_PACKET_SIZE]
    }

    fn write_f32_le(buf: &mut [u8], offset: usize, value: f32) {
        buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(500))]

        /// Any packet shorter than MIN_PACKET_SIZE must return Err, never panic.
        #[test]
        fn prop_short_packet_returns_err(len in 0usize..MIN_PACKET_SIZE) {
            let data = vec![0u8; len];
            prop_assert!(parse_packet(&data).is_err());
        }

        /// Arbitrary bytes at or above MIN_PACKET_SIZE must never panic.
        #[test]
        fn prop_arbitrary_packet_no_panic(
            data in proptest::collection::vec(any::<u8>(), MIN_PACKET_SIZE..=MIN_PACKET_SIZE * 2)
        ) {
            let _ = parse_packet(&data);
        }

        /// FFB scalar is always clamped to [-1, 1] regardless of lateral G input.
        #[test]
        fn prop_ffb_scalar_in_range(lat_g in -10.0f32..=10.0f32) {
            let mut buf = make_min_packet();
            write_f32_le(&mut buf, OFF_GFORCE_LAT, lat_g);
            let t = parse_packet(&buf).expect("parse must succeed for valid-size packet");
            prop_assert!(
                t.ffb_scalar >= -1.0 && t.ffb_scalar <= 1.0,
                "ffb_scalar {} must be in [-1, 1]",
                t.ffb_scalar
            );
        }

        /// Throttle is always clamped to [0, 1].
        #[test]
        fn prop_throttle_clamped(throttle in -5.0f32..=5.0f32) {
            let mut buf = make_min_packet();
            write_f32_le(&mut buf, OFF_THROTTLE, throttle);
            let t = parse_packet(&buf).expect("parse must succeed for valid-size packet");
            prop_assert!(
                t.throttle >= 0.0 && t.throttle <= 1.0,
                "throttle {} must be in [0, 1]",
                t.throttle
            );
        }

        /// Brake is always clamped to [0, 1].
        #[test]
        fn prop_brake_clamped(brake in -5.0f32..=5.0f32) {
            let mut buf = make_min_packet();
            write_f32_le(&mut buf, OFF_BRAKE, brake);
            let t = parse_packet(&buf).expect("parse must succeed for valid-size packet");
            prop_assert!(
                t.brake >= 0.0 && t.brake <= 1.0,
                "brake {} must be in [0, 1]",
                t.brake
            );
        }

        /// Steering angle is always clamped to [-1, 1].
        #[test]
        fn prop_steering_clamped(steer in -5.0f32..=5.0f32) {
            let mut buf = make_min_packet();
            write_f32_le(&mut buf, OFF_STEER, steer);
            let t = parse_packet(&buf).expect("parse must succeed for valid-size packet");
            prop_assert!(
                t.steering_angle >= -1.0 && t.steering_angle <= 1.0,
                "steering_angle {} must be in [-1, 1]",
                t.steering_angle
            );
        }

        /// Gear field is always in the valid range [-1, 8].
        #[test]
        fn prop_gear_in_valid_range(gear_val in 0.0f32..=10.0f32) {
            let mut buf = make_min_packet();
            write_f32_le(&mut buf, OFF_GEAR, gear_val);
            let t = parse_packet(&buf).expect("parse must succeed for valid-size packet");
            prop_assert!(
                t.gear >= -1 && t.gear <= 8,
                "gear {} must be in [-1, 8]",
                t.gear
            );
        }

        /// Speed from normal wheel-speed values is finite and non-negative.
        #[test]
        fn prop_speed_non_negative_from_wheel_speeds(ws in 0.0f32..=100.0f32) {
            let mut buf = make_min_packet();
            write_f32_le(&mut buf, OFF_WHEEL_SPEED_FL, ws);
            write_f32_le(&mut buf, OFF_WHEEL_SPEED_FR, ws);
            write_f32_le(&mut buf, OFF_WHEEL_SPEED_RL, ws);
            write_f32_le(&mut buf, OFF_WHEEL_SPEED_RR, ws);
            let t = parse_packet(&buf).expect("parse must succeed for valid-size packet");
            prop_assert!(
                t.speed_ms >= 0.0 && t.speed_ms.is_finite(),
                "speed_ms {} must be finite and non-negative",
                t.speed_ms
            );
        }

        /// RPM is always non-negative (negative inputs are clamped to 0).
        #[test]
        fn prop_rpm_non_negative(rpm in -1000.0f32..=20000.0f32) {
            let mut buf = make_min_packet();
            write_f32_le(&mut buf, OFF_RPM, rpm);
            let t = parse_packet(&buf).expect("parse must succeed for valid-size packet");
            prop_assert!(t.rpm >= 0.0, "rpm {} must be non-negative", t.rpm);
        }

        /// Fuel percent is always in [0, 100] when capacity is positive.
        #[test]
        fn prop_fuel_percent_in_range(
            fuel_in in 0.0f32..=200.0f32,
            fuel_cap in 1.0f32..=300.0f32,
        ) {
            let mut buf = make_min_packet();
            write_f32_le(&mut buf, OFF_FUEL_IN_TANK, fuel_in);
            write_f32_le(&mut buf, OFF_FUEL_CAPACITY, fuel_cap);
            let t = parse_packet(&buf).expect("parse must succeed for valid-size packet");
            prop_assert!(
                t.fuel_percent >= 0.0 && t.fuel_percent <= 100.0,
                "fuel_percent {} must be in [0, 100]",
                t.fuel_percent
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_packet(size: usize) -> Vec<u8> {
        vec![0u8; size]
    }

    fn write_f32(buf: &mut [u8], offset: usize, value: f32) {
        buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    #[test]
    fn rejects_short_packet() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = DirtRally2Adapter::new();
        let result = adapter.normalize(&[0u8; MIN_PACKET_SIZE - 1]);
        assert!(result.is_err(), "expected error for short packet");
        Ok(())
    }

    #[test]
    fn zero_packet_returns_zero_speed_and_rpm() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = DirtRally2Adapter::new();
        let raw = make_packet(MIN_PACKET_SIZE);
        let t = adapter.normalize(&raw)?;
        assert_eq!(t.speed_ms, 0.0);
        assert_eq!(t.rpm, 0.0);
        Ok(())
    }

    #[test]
    fn zero_gear_maps_to_reverse() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = DirtRally2Adapter::new();
        let raw = make_packet(MIN_PACKET_SIZE);
        let t = adapter.normalize(&raw)?;
        // 0.0 in packet → reverse = -1
        assert_eq!(t.gear, -1);
        Ok(())
    }

    #[test]
    fn speed_extracted_from_wheel_speeds() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = DirtRally2Adapter::new();
        let mut raw = make_packet(MIN_PACKET_SIZE);
        write_f32(&mut raw, OFF_WHEEL_SPEED_FL, 20.0);
        write_f32(&mut raw, OFF_WHEEL_SPEED_FR, 20.0);
        write_f32(&mut raw, OFF_WHEEL_SPEED_RL, 20.0);
        write_f32(&mut raw, OFF_WHEEL_SPEED_RR, 20.0);
        let t = adapter.normalize(&raw)?;
        assert!(
            (t.speed_ms - 20.0).abs() < 0.001,
            "speed_ms should be 20.0, got {}",
            t.speed_ms
        );
        Ok(())
    }

    #[test]
    fn speed_falls_back_to_velocity_magnitude() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = DirtRally2Adapter::new();
        let mut raw = make_packet(MIN_PACKET_SIZE);
        // All wheel speeds remain 0; set velocity components for a 3-4-5 right triangle.
        write_f32(&mut raw, OFF_VEL_X, 3.0);
        write_f32(&mut raw, OFF_VEL_Y, 0.0);
        write_f32(&mut raw, OFF_VEL_Z, 4.0);
        let t = adapter.normalize(&raw)?;
        assert!(
            (t.speed_ms - 5.0).abs() < 0.001,
            "speed_ms should be 5.0, got {}",
            t.speed_ms
        );
        Ok(())
    }

    #[test]
    fn gear_extraction_forward_gears() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = DirtRally2Adapter::new();
        for g in 1i8..=6i8 {
            let mut raw = make_packet(MIN_PACKET_SIZE);
            write_f32(&mut raw, OFF_GEAR, f32::from(g));
            let t = adapter.normalize(&raw)?;
            assert_eq!(t.gear, g, "expected gear {g}");
        }
        Ok(())
    }

    #[test]
    fn gear_extraction_reverse() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = DirtRally2Adapter::new();
        let mut raw = make_packet(MIN_PACKET_SIZE);
        write_f32(&mut raw, OFF_GEAR, 0.0);
        let t = adapter.normalize(&raw)?;
        assert_eq!(t.gear, -1);
        Ok(())
    }

    #[test]
    fn in_pit_flag_set_when_one() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = DirtRally2Adapter::new();
        let mut raw = make_packet(MIN_PACKET_SIZE);
        write_f32(&mut raw, OFF_IN_PIT, 1.0);
        let t = adapter.normalize(&raw)?;
        assert!(t.flags.in_pits, "in_pits should be true");
        Ok(())
    }

    #[test]
    fn in_pit_flag_clear_when_zero() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = DirtRally2Adapter::new();
        let raw = make_packet(MIN_PACKET_SIZE);
        let t = adapter.normalize(&raw)?;
        assert!(!t.flags.in_pits, "in_pits should be false");
        Ok(())
    }

    #[test]
    fn rpm_and_rpm_fraction_extracted() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = DirtRally2Adapter::new();
        let mut raw = make_packet(MIN_PACKET_SIZE);
        write_f32(&mut raw, OFF_RPM, 5000.0);
        write_f32(&mut raw, OFF_MAX_RPM, 8000.0);
        let t = adapter.normalize(&raw)?;
        assert!((t.rpm - 5000.0).abs() < 0.001, "rpm should be 5000.0");
        assert!((t.max_rpm - 8000.0).abs() < 0.001, "max_rpm should be 8000.0");
        if let Some(TelemetryValue::Float(fraction)) = t.extended.get("rpm_fraction") {
            assert!(
                (fraction - 0.625).abs() < 0.001,
                "rpm_fraction should be 0.625, got {fraction}"
            );
        } else {
            panic!("rpm_fraction not found in extended telemetry");
        }
        Ok(())
    }
}
