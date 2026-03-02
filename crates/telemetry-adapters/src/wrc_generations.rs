//! WRC Generations / EA WRC telemetry adapter for Codemasters/RallyEngine Mode 1 UDP format.
//!
//! Enable UDP telemetry in-game (Accessibility → UDP Telemetry), default port 6777.
//!
//! ## Protocol summary
//!
//! The packet layout is identical to the Codemasters Mode 1 legacy format used by
//! DiRT Rally 2.0 — a fixed-layout binary stream of 264+ bytes (66 × `f32`) where
//! every field is a little-endian `f32` at a known byte offset.
//!
//! ## Verified against community sources
//!
//! Byte offsets and field semantics were cross-checked against:
//! - Codemasters telemetry spreadsheet (DR1/DR4/DR2.0 field map):
//!   <https://docs.google.com/spreadsheets/d/1Xsv5E9jwgJsiXCZQlM5Ae2hH5mUnjdHlTtEadnSnaeI>
//! - `ErlerPhilipp/dr2_logger` – `source/dirt_rally/udp_data.py`
//! - `soong-construction/dirt-rally-time-recorder` – `timerecorder/gearTracker.py`,
//!   `timerecorder/receiver.py`
//!
//! ## DR2.0 vs EA WRC differences
//!
//! | Property         | DiRT Rally 2.0       | WRC Generations / EA WRC   |
//! |------------------|----------------------|----------------------------|
//! | Default UDP port | 20777                | 6777                       |
//! | Config location  | hardware_settings_config.xml (`extradata="3"`) | In-game menu |
//! | Packet size      | 264 bytes (66 × f32) | 264 bytes (66 × f32)       |
//! | Endianness       | Little-endian        | Little-endian              |
//!
//! **Note on RPM encoding:** DR2.0 community tools (dr2_logger, dirt-rally-time-recorder)
//! document offset 148 (engine rate) and offset 252 (max RPM) as "rpm / 10", meaning
//! the raw value must be multiplied by 10 for realistic RPM.  WRC Generations / EA WRC
//! may send direct RPM values (no ×10 scaling).  This adapter passes values as-is.

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

const DEFAULT_PORT: u16 = 6777;
const MIN_PACKET_SIZE: usize = 264;
const MAX_PACKET_SIZE: usize = 2048;
const DEFAULT_HEARTBEAT_TIMEOUT_MS: u64 = 1_500;

const ENV_PORT: &str = "OPENRACING_WRC_GENERATIONS_UDP_PORT";
const ENV_HEARTBEAT_TIMEOUT_MS: &str = "OPENRACING_WRC_GENERATIONS_HEARTBEAT_TIMEOUT_MS";

// Byte offsets for Codemasters Mode 1 / RallyEngine packet fields (all f32, little-endian).
// Verified against: dr2_logger udp_data.py, Codemasters telemetry spreadsheet.
// Field index in parentheses: offset = index × 4.
const OFF_VEL_X: usize = 32; //  [8] velocity_x (m/s)
const OFF_VEL_Y: usize = 36; //  [9] velocity_y (m/s)
const OFF_VEL_Z: usize = 40; // [10] velocity_z (m/s)
const OFF_WHEEL_SPEED_FL: usize = 108; // [27] wheel patch speed front-left (m/s)
const OFF_WHEEL_SPEED_FR: usize = 112; // [28] wheel patch speed front-right (m/s)
const OFF_WHEEL_SPEED_RL: usize = 100; // [25] wheel patch speed rear-left (m/s)
const OFF_WHEEL_SPEED_RR: usize = 104; // [26] wheel patch speed rear-right (m/s)
const OFF_THROTTLE: usize = 116; // [29] throttle input 0.0–1.0
const OFF_STEER: usize = 120; // [30] steering input -1.0..+1.0
const OFF_BRAKE: usize = 124; // [31] brake input 0.0–1.0
const OFF_GEAR: usize = 132; // [33] gear: -1=reverse, 0=neutral, 1+=forward
const OFF_GFORCE_LAT: usize = 136; // [34] lateral g-force
const OFF_GFORCE_LON: usize = 140; // [35] longitudinal g-force
const OFF_CURRENT_LAP: usize = 144; // [36] current lap (0-based)
const OFF_RPM: usize = 148; // [37] engine rate (see RPM note in module docs)
const OFF_CAR_POSITION: usize = 156; // [39] race position
const OFF_FUEL_IN_TANK: usize = 180; // [45] fuel in tank
const OFF_FUEL_CAPACITY: usize = 184; // [46] fuel capacity
const OFF_IN_PIT: usize = 188; // [47] in pit (0/1)
const OFF_BRAKES_TEMP_RL: usize = 204; // [51] brake temp rear-left (°C)
const OFF_BRAKES_TEMP_RR: usize = 208; // [52] brake temp rear-right (°C)
const OFF_BRAKES_TEMP_FL: usize = 212; // [53] brake temp front-left (°C)
const OFF_BRAKES_TEMP_FR: usize = 216; // [54] brake temp front-right (°C)
const OFF_TYRES_PRESSURE_RL: usize = 220; // [55] tyre pressure rear-left (PSI)
const OFF_TYRES_PRESSURE_RR: usize = 224; // [56] tyre pressure rear-right (PSI)
const OFF_TYRES_PRESSURE_FL: usize = 228; // [57] tyre pressure front-left (PSI)
const OFF_TYRES_PRESSURE_FR: usize = 232; // [58] tyre pressure front-right (PSI)
const OFF_LAST_LAP_TIME: usize = 248; // [62] last lap time (seconds)
const OFF_MAX_RPM: usize = 252; // [63] max RPM (see RPM note in module docs)
const OFF_MAX_GEARS: usize = 260; // [65] max gears

/// Lateral G normalisation range for the FFB scalar.
const FFB_LAT_G_MAX: f32 = 3.0;

/// WRC Generations / WRC 23 adapter for RallyEngine UDP telemetry.
#[derive(Clone)]
pub struct WrcGenerationsAdapter {
    bind_port: u16,
    update_rate: Duration,
    heartbeat_timeout: Duration,
    last_packet_ns: Arc<AtomicU64>,
}

impl Default for WrcGenerationsAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl WrcGenerationsAdapter {
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
        .filter(|v| v.is_finite())
}

fn parse_packet(data: &[u8]) -> Result<NormalizedTelemetry> {
    if data.len() < MIN_PACKET_SIZE {
        return Err(anyhow!(
            "WRC Generations packet too short: need at least {} bytes, got {}",
            MIN_PACKET_SIZE,
            data.len()
        ));
    }

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

    // Gear encoding (verified against dirt-rally-time-recorder gearTracker.py and
    // dr2_logger udp_data.py): -1.0 = reverse, 0.0 = neutral, 1.0+ = forward gears.
    // DR1 legacy uses 10.0 for reverse, which we clamp to 8 (not applicable here).
    let gear_raw = read_f32(data, OFF_GEAR).unwrap_or(0.0);
    let gear: i8 = if gear_raw < -0.5 {
        -1 // reverse (raw -1.0)
    } else if gear_raw < 0.5 {
        0 // neutral (raw 0.0)
    } else {
        (gear_raw.round() as i8).clamp(1, 8)
    };

    let throttle = read_f32(data, OFF_THROTTLE).unwrap_or(0.0).clamp(0.0, 1.0);
    let steering_angle = read_f32(data, OFF_STEER).unwrap_or(0.0).clamp(-1.0, 1.0);
    let brake = read_f32(data, OFF_BRAKE).unwrap_or(0.0).clamp(0.0, 1.0);

    let lat_g = read_f32(data, OFF_GFORCE_LAT).unwrap_or(0.0);
    let lon_g = read_f32(data, OFF_GFORCE_LON).unwrap_or(0.0);
    let ffb_scalar = (lat_g / FFB_LAT_G_MAX).clamp(-1.0, 1.0);

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
        read_f32(data, OFF_BRAKES_TEMP_FR)
            .unwrap_or(0.0)
            .clamp(0.0, 255.0) as u8,
        read_f32(data, OFF_BRAKES_TEMP_RL)
            .unwrap_or(0.0)
            .clamp(0.0, 255.0) as u8,
        read_f32(data, OFF_BRAKES_TEMP_RR)
            .unwrap_or(0.0)
            .clamp(0.0, 255.0) as u8,
    ];

    let tire_pressures_psi = [
        read_f32(data, OFF_TYRES_PRESSURE_FL).unwrap_or(0.0),
        read_f32(data, OFF_TYRES_PRESSURE_FR).unwrap_or(0.0),
        read_f32(data, OFF_TYRES_PRESSURE_RL).unwrap_or(0.0),
        read_f32(data, OFF_TYRES_PRESSURE_RR).unwrap_or(0.0),
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

#[async_trait]
impl TelemetryAdapter for WrcGenerationsAdapter {
    fn game_id(&self) -> &str {
        "wrc_generations"
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
                        "WRC Generations UDP socket bind failed"
                    );
                    return;
                }
            };

            info!(port = bind_port, "WRC Generations UDP adapter bound");

            let mut frame_idx = 0u64;
            let mut buf = vec![0u8; MAX_PACKET_SIZE];
            let timeout = (update_rate * 4).max(Duration::from_millis(25));

            loop {
                let recv = tokio::time::timeout(timeout, socket.recv(&mut buf)).await;
                let len = match recv {
                    Ok(Ok(len)) => len,
                    Ok(Err(error)) => {
                        warn!(error = %error, "WRC Generations UDP receive error");
                        continue;
                    }
                    Err(_) => {
                        debug!("WRC Generations UDP receive timeout");
                        continue;
                    }
                };

                let data = &buf[..len];
                let normalized = match parse_packet(data) {
                    Ok(n) => n,
                    Err(error) => {
                        warn!(error = %error, "Failed to parse WRC Generations packet");
                        continue;
                    }
                };

                last_packet_ns.store(telemetry_now_ns(), Ordering::Relaxed);

                let frame = TelemetryFrame::new(normalized, telemetry_now_ns(), frame_idx, len);
                if tx.send(frame).await.is_err() {
                    break;
                }

                frame_idx = frame_idx.saturating_add(1);
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
        let adapter = WrcGenerationsAdapter::new();
        let result = adapter.normalize(&[0u8; MIN_PACKET_SIZE - 1]);
        assert!(result.is_err(), "expected error for short packet");
        Ok(())
    }

    #[test]
    fn zero_packet_returns_zero_speed_and_rpm() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = WrcGenerationsAdapter::new();
        let raw = make_packet(MIN_PACKET_SIZE);
        let t = adapter.normalize(&raw)?;
        assert_eq!(t.speed_ms, 0.0);
        assert_eq!(t.rpm, 0.0);
        Ok(())
    }

    #[test]
    fn zero_gear_maps_to_neutral() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = WrcGenerationsAdapter::new();
        let raw = make_packet(MIN_PACKET_SIZE);
        // Raw 0.0 = neutral per Codemasters Mode 1 spec (verified: gearTracker.py).
        let t = adapter.normalize(&raw)?;
        assert_eq!(t.gear, 0, "raw 0.0 should map to neutral (0)");
        Ok(())
    }

    #[test]
    fn game_id_is_correct() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = WrcGenerationsAdapter::new();
        assert_eq!(adapter.game_id(), "wrc_generations");
        Ok(())
    }

    #[test]
    fn speed_extracted_from_wheel_speeds() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = WrcGenerationsAdapter::new();
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
    fn in_pit_flag_set_when_one() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = WrcGenerationsAdapter::new();
        let mut raw = make_packet(MIN_PACKET_SIZE);
        write_f32(&mut raw, OFF_IN_PIT, 1.0);
        let t = adapter.normalize(&raw)?;
        assert!(t.flags.in_pits, "in_pits should be true");
        Ok(())
    }

    #[test]
    fn rpm_and_rpm_fraction_extracted() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = WrcGenerationsAdapter::new();
        let mut raw = make_packet(MIN_PACKET_SIZE);
        write_f32(&mut raw, OFF_RPM, 5000.0);
        write_f32(&mut raw, OFF_MAX_RPM, 8000.0);
        let t = adapter.normalize(&raw)?;
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
    fn empty_input_returns_error() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = WrcGenerationsAdapter::new();
        assert!(
            adapter.normalize(&[]).is_err(),
            "empty input must return an error"
        );
        Ok(())
    }

    #[test]
    fn known_good_payload_throttle_brake_gear() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = WrcGenerationsAdapter::new();
        let mut raw = make_packet(MIN_PACKET_SIZE);
        write_f32(&mut raw, OFF_WHEEL_SPEED_FL, 25.0);
        write_f32(&mut raw, OFF_WHEEL_SPEED_FR, 25.0);
        write_f32(&mut raw, OFF_WHEEL_SPEED_RL, 25.0);
        write_f32(&mut raw, OFF_WHEEL_SPEED_RR, 25.0);
        write_f32(&mut raw, OFF_THROTTLE, 0.8);
        write_f32(&mut raw, OFF_BRAKE, 0.3);
        write_f32(&mut raw, OFF_GEAR, 3.0);
        let t = adapter.normalize(&raw)?;
        assert!((t.speed_ms - 25.0).abs() < 0.001, "speed_ms={}", t.speed_ms);
        assert!((t.throttle - 0.8).abs() < 0.001, "throttle={}", t.throttle);
        assert!((t.brake - 0.3).abs() < 0.001, "brake={}", t.brake);
        assert_eq!(t.gear, 3, "gear={}", t.gear);
        Ok(())
    }

    #[test]
    fn speed_is_nonnegative() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = WrcGenerationsAdapter::new();
        let mut raw = make_packet(MIN_PACKET_SIZE);
        write_f32(&mut raw, OFF_WHEEL_SPEED_FL, 15.0);
        write_f32(&mut raw, OFF_WHEEL_SPEED_FR, 15.0);
        write_f32(&mut raw, OFF_WHEEL_SPEED_RL, 15.0);
        write_f32(&mut raw, OFF_WHEEL_SPEED_RR, 15.0);
        let t = adapter.normalize(&raw)?;
        assert!(
            t.speed_ms >= 0.0,
            "speed_ms must be non-negative, got {}",
            t.speed_ms
        );
        Ok(())
    }

    #[test]
    fn throttle_clamped_to_unit_range() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = WrcGenerationsAdapter::new();
        let mut raw = make_packet(MIN_PACKET_SIZE);
        write_f32(&mut raw, OFF_THROTTLE, 3.0);
        let t = adapter.normalize(&raw)?;
        assert!(
            t.throttle >= 0.0 && t.throttle <= 1.0,
            "throttle={} must be in [0.0, 1.0]",
            t.throttle
        );
        Ok(())
    }

    #[test]
    fn brake_clamped_to_unit_range() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = WrcGenerationsAdapter::new();
        let mut raw = make_packet(MIN_PACKET_SIZE);
        write_f32(&mut raw, OFF_BRAKE, 5.0);
        let t = adapter.normalize(&raw)?;
        assert!(
            t.brake >= 0.0 && t.brake <= 1.0,
            "brake={} must be in [0.0, 1.0]",
            t.brake
        );
        Ok(())
    }

    #[test]
    fn gear_forward_stays_in_range() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = WrcGenerationsAdapter::new();
        for g in 1i32..=8 {
            let mut raw = make_packet(MIN_PACKET_SIZE);
            write_f32(&mut raw, OFF_GEAR, g as f32);
            let t = adapter.normalize(&raw)?;
            assert!(
                t.gear >= 1 && t.gear <= 8,
                "gear {} out of expected range 1..=8",
                t.gear
            );
        }
        Ok(())
    }

    #[test]
    fn negative_one_gear_maps_to_reverse() -> Result<(), Box<dyn std::error::Error>> {
        // Verified: DR2.0 sends -1.0 for reverse (gearTracker.py, udp_data.py).
        let adapter = WrcGenerationsAdapter::new();
        let mut raw = make_packet(MIN_PACKET_SIZE);
        write_f32(&mut raw, OFF_GEAR, -1.0);
        let t = adapter.normalize(&raw)?;
        assert_eq!(t.gear, -1, "raw -1.0 should map to reverse (-1)");
        Ok(())
    }
}

#[cfg(test)]
mod proptest_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(500))]

        #[test]
        fn parse_no_panic_on_arbitrary(
            data in proptest::collection::vec(any::<u8>(), 0..1024)
        ) {
            let adapter = WrcGenerationsAdapter::new();
            let _ = adapter.normalize(&data);
        }
    }
}
