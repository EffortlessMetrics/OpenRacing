//! GRID Legends telemetry adapter for Codemasters Mode 1 UDP format.
//!
//! Enable UDP telemetry in-game: Options → Accessibility → UDP Telemetry, port 20777.
//!
//! The packet layout is the fixed-layout Codemasters Mode 1 legacy binary stream
//! (252+ bytes, little-endian `f32` at known byte offsets), shared with DiRT Rally 2.0,
//! GRID Autosport, GRID 2019, and the broader Codemasters series.

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

const ENV_PORT: &str = "OPENRACING_GRID_LEGENDS_UDP_PORT";
const ENV_HEARTBEAT_TIMEOUT_MS: &str = "OPENRACING_GRID_LEGENDS_HEARTBEAT_TIMEOUT_MS";

// Byte offsets for Codemasters Mode 1 packet fields (all f32, little-endian).
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

/// Lateral G normalisation range for FFB scalar.
const FFB_LAT_G_MAX: f32 = 3.0;

/// GRID Legends adapter for Codemasters Mode 1 UDP telemetry.
#[derive(Clone)]
pub struct GridLegendsAdapter {
    bind_port: u16,
    update_rate: Duration,
    heartbeat_timeout: Duration,
    last_packet_ns: Arc<AtomicU64>,
}

impl Default for GridLegendsAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl GridLegendsAdapter {
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

fn read_f32(data: &[u8], offset: usize) -> Option<f32> {
    data.get(offset..offset + 4)
        .and_then(|b| b.try_into().ok())
        .map(f32::from_le_bytes)
}

fn parse_packet(data: &[u8]) -> Result<NormalizedTelemetry> {
    if data.len() < MIN_PACKET_SIZE {
        return Err(anyhow!(
            "GRID Legends packet too short: need at least {} bytes, got {}",
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
    let ffb_scalar = (lat_g / FFB_LAT_G_MAX).clamp(-1.0, 1.0);

    let lap_raw = read_f32(data, OFF_CURRENT_LAP).unwrap_or(0.0).max(0.0);
    let lap = (lap_raw.round() as u16).saturating_add(1);

    let position = read_f32(data, OFF_CAR_POSITION)
        .map(|p| p.round().clamp(0.0, 255.0) as u8)
        .unwrap_or(0);

    let fuel_in_tank = read_f32(data, OFF_FUEL_IN_TANK).unwrap_or(0.0).max(0.0);
    let fuel_capacity = read_f32(data, OFF_FUEL_CAPACITY).unwrap_or(1.0).max(1.0);
    let fuel_percent = (fuel_in_tank / fuel_capacity).clamp(0.0, 1.0) * 100.0;

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

#[async_trait]
impl TelemetryAdapter for GridLegendsAdapter {
    fn game_id(&self) -> &str {
        "grid_legends"
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
                        "GRID Legends UDP socket bind failed"
                    );
                    return;
                }
            };

            info!(port = bind_port, "GRID Legends UDP adapter bound");

            let mut frame_seq = 0u64;
            let mut buf = vec![0u8; MAX_PACKET_SIZE];
            let timeout = (update_rate * 4).max(Duration::from_millis(25));

            loop {
                let recv = tokio::time::timeout(timeout, socket.recv(&mut buf)).await;
                let len = match recv {
                    Ok(Ok(len)) => len,
                    Ok(Err(error)) => {
                        warn!(error = %error, "GRID Legends UDP receive error");
                        continue;
                    }
                    Err(_) => {
                        debug!("GRID Legends UDP receive timeout");
                        continue;
                    }
                };

                let data = &buf[..len];
                let normalized = match parse_packet(data) {
                    Ok(n) => n,
                    Err(error) => {
                        warn!(error = %error, "Failed to parse GRID Legends packet");
                        continue;
                    }
                };

                last_packet_ns.store(telemetry_now_ns(), Ordering::Relaxed);

                let frame = TelemetryFrame::new(normalized, telemetry_now_ns(), frame_seq, len);
                if tx.send(frame).await.is_err() {
                    break;
                }

                frame_seq = frame_seq.saturating_add(1);
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

    #[test]
    fn rejects_short_packet() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = GridLegendsAdapter::new();
        let result = adapter.normalize(&[0u8; MIN_PACKET_SIZE - 1]);
        assert!(result.is_err(), "expected error for short packet");
        Ok(())
    }

    #[test]
    fn zero_packet_returns_zero_speed_and_rpm() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = GridLegendsAdapter::new();
        let raw = make_packet(MIN_PACKET_SIZE);
        let t = adapter.normalize(&raw)?;
        assert_eq!(t.speed_ms, 0.0);
        assert_eq!(t.rpm, 0.0);
        Ok(())
    }

    #[test]
    fn gear_zero_maps_to_reverse() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = GridLegendsAdapter::new();
        let raw = make_packet(MIN_PACKET_SIZE);
        let t = adapter.normalize(&raw)?;
        assert_eq!(t.gear, -1);
        Ok(())
    }

    #[test]
    fn game_id_is_grid_legends() {
        assert_eq!(GridLegendsAdapter::new().game_id(), "grid_legends");
    }
}
