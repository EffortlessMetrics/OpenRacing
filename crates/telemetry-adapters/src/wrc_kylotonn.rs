//! WRC 9 / WRC 10 (Kylotonn) telemetry adapter — custom UDP format on port 64000.
//!
//! Kylotonn-era WRC titles (WRC 9, WRC 10 FIA World Rally Championship) broadcast a
//! custom binary UDP stream. Community documentation describes a packed little-endian
//! struct with the following fields (96 bytes minimum):
//!
//! | Offset | Type  | Field            |
//! |--------|-------|------------------|
//! | 0      | f32   | stage_progress   |
//! | 4      | f32   | road_speed_ms    |
//! | 8      | f32   | steering         |
//! | 12     | f32   | throttle         |
//! | 16     | f32   | brake            |
//! | 20     | f32   | hand_brake       |
//! | 24     | f32   | clutch           |
//! | 28     | u32   | gear             |
//! | 32     | f32   | rpm              |
//! | 36     | f32   | max_rpm          |
//! | 40     | f32   | suspension_fl    |
//! | 44     | f32   | suspension_fr    |
//! | 48     | f32   | suspension_rl    |
//! | 52     | f32   | suspension_rr    |
//! | 56     | f32   | pos_x            |
//! | 60     | f32   | pos_y            |
//! | 64     | f32   | pos_z            |
//! | 68     | f32   | roll             |
//! | 72     | f32   | pitch            |
//! | 76     | f32   | yaw              |
//! | 80     | f32   | wheel_speed_fl   |
//! | 84     | f32   | wheel_speed_fr   |
//! | 88     | f32   | wheel_speed_rl   |
//! | 92     | f32   | wheel_speed_rr   |
//!
//! The adapter is used for both WRC 9 and WRC 10 via the [`WrcKylotonnVariant`] enum.
//! Both games use UDP port 64000 by default.

use crate::{
    NormalizedTelemetry, TelemetryAdapter, TelemetryFrame, TelemetryReceiver, TelemetryValue,
    telemetry_now_ns,
};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::time::Duration;
use tokio::net::UdpSocket as TokioUdpSocket;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

const DEFAULT_PORT: u16 = 64000;
const MIN_PACKET_SIZE: usize = 96;
const MAX_PACKET_SIZE: usize = 256;

// Byte offsets — all fields are little-endian.
const OFF_STAGE_PROGRESS: usize = 0; // f32
const OFF_ROAD_SPEED_MS: usize = 4; // f32
const OFF_STEERING: usize = 8; // f32  −1.0 to 1.0
const OFF_THROTTLE: usize = 12; // f32   0.0 to 1.0
const OFF_BRAKE: usize = 16; // f32   0.0 to 1.0
const OFF_HAND_BRAKE: usize = 20; // f32   0.0 to 1.0
const OFF_CLUTCH: usize = 24; // f32   0.0 to 1.0
const OFF_GEAR: usize = 28; // u32   0=reverse, 1..=7=gear
const OFF_RPM: usize = 32; // f32
const OFF_MAX_RPM: usize = 36; // f32
const OFF_SUSPENSION_FL: usize = 40; // f32
const OFF_SUSPENSION_FR: usize = 44; // f32
const OFF_SUSPENSION_RL: usize = 48; // f32
const OFF_SUSPENSION_RR: usize = 52; // f32
const OFF_POS_X: usize = 56; // f32
const OFF_POS_Y: usize = 60; // f32
const OFF_POS_Z: usize = 64; // f32
const OFF_ROLL: usize = 68; // f32
const OFF_PITCH: usize = 72; // f32
const OFF_YAW: usize = 76; // f32
const OFF_WHEEL_SPEED_FL: usize = 80; // f32
const OFF_WHEEL_SPEED_FR: usize = 84; // f32
const OFF_WHEEL_SPEED_RL: usize = 88; // f32
const OFF_WHEEL_SPEED_RR: usize = 92; // f32

/// Which Kylotonn WRC title this adapter instance represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WrcKylotonnVariant {
    Wrc9,
    Wrc10,
}

impl WrcKylotonnVariant {
    fn game_id(self) -> &'static str {
        match self {
            Self::Wrc9 => "wrc_9",
            Self::Wrc10 => "wrc_10",
        }
    }
}

fn read_f32_le(data: &[u8], offset: usize) -> Option<f32> {
    data.get(offset..offset + 4)
        .and_then(|b| b.try_into().ok())
        .map(f32::from_le_bytes)
}

fn read_u32_le(data: &[u8], offset: usize) -> Option<u32> {
    data.get(offset..offset + 4)
        .and_then(|b| b.try_into().ok())
        .map(u32::from_le_bytes)
}

fn parse_packet(data: &[u8]) -> Result<NormalizedTelemetry> {
    if data.len() < MIN_PACKET_SIZE {
        return Err(anyhow!(
            "WRC Kylotonn packet too short: expected at least {MIN_PACKET_SIZE}, got {}",
            data.len()
        ));
    }

    let road_speed_ms = read_f32_le(data, OFF_ROAD_SPEED_MS).unwrap_or(0.0).max(0.0);
    let steering = read_f32_le(data, OFF_STEERING)
        .unwrap_or(0.0)
        .clamp(-1.0, 1.0);
    let throttle = read_f32_le(data, OFF_THROTTLE)
        .unwrap_or(0.0)
        .clamp(0.0, 1.0);
    let brake = read_f32_le(data, OFF_BRAKE).unwrap_or(0.0).clamp(0.0, 1.0);
    let clutch = read_f32_le(data, OFF_CLUTCH).unwrap_or(0.0).clamp(0.0, 1.0);
    let hand_brake = read_f32_le(data, OFF_HAND_BRAKE)
        .unwrap_or(0.0)
        .clamp(0.0, 1.0);

    // 0=reverse (−1), 1..=7=forward gears 1–7
    let gear: i8 = match read_u32_le(data, OFF_GEAR).unwrap_or(1) {
        0 => -1,
        g => (g as i8).clamp(1, 7),
    };

    let rpm = read_f32_le(data, OFF_RPM).unwrap_or(0.0).max(0.0);
    let max_rpm = read_f32_le(data, OFF_MAX_RPM).unwrap_or(0.0).max(0.0);

    let stage_progress = read_f32_le(data, OFF_STAGE_PROGRESS)
        .unwrap_or(0.0)
        .clamp(0.0, 1.0);

    let susp_fl = read_f32_le(data, OFF_SUSPENSION_FL).unwrap_or(0.0);
    let susp_fr = read_f32_le(data, OFF_SUSPENSION_FR).unwrap_or(0.0);
    let susp_rl = read_f32_le(data, OFF_SUSPENSION_RL).unwrap_or(0.0);
    let susp_rr = read_f32_le(data, OFF_SUSPENSION_RR).unwrap_or(0.0);

    let pos_x = read_f32_le(data, OFF_POS_X).unwrap_or(0.0);
    let pos_y = read_f32_le(data, OFF_POS_Y).unwrap_or(0.0);
    let pos_z = read_f32_le(data, OFF_POS_Z).unwrap_or(0.0);
    let roll = read_f32_le(data, OFF_ROLL).unwrap_or(0.0);
    let pitch = read_f32_le(data, OFF_PITCH).unwrap_or(0.0);
    let yaw = read_f32_le(data, OFF_YAW).unwrap_or(0.0);

    let ws_fl = read_f32_le(data, OFF_WHEEL_SPEED_FL).unwrap_or(0.0);
    let ws_fr = read_f32_le(data, OFF_WHEEL_SPEED_FR).unwrap_or(0.0);
    let ws_rl = read_f32_le(data, OFF_WHEEL_SPEED_RL).unwrap_or(0.0);
    let ws_rr = read_f32_le(data, OFF_WHEEL_SPEED_RR).unwrap_or(0.0);

    let mut builder = NormalizedTelemetry::builder()
        .speed_ms(road_speed_ms)
        .rpm(rpm)
        .gear(gear)
        .throttle(throttle)
        .brake(brake)
        .clutch(clutch)
        .steering_angle(steering)
        .extended("stage_progress", TelemetryValue::Float(stage_progress))
        .extended("hand_brake", TelemetryValue::Float(hand_brake))
        .extended("suspension_fl", TelemetryValue::Float(susp_fl))
        .extended("suspension_fr", TelemetryValue::Float(susp_fr))
        .extended("suspension_rl", TelemetryValue::Float(susp_rl))
        .extended("suspension_rr", TelemetryValue::Float(susp_rr))
        .extended("pos_x", TelemetryValue::Float(pos_x))
        .extended("pos_y", TelemetryValue::Float(pos_y))
        .extended("pos_z", TelemetryValue::Float(pos_z))
        .extended("roll", TelemetryValue::Float(roll))
        .extended("pitch", TelemetryValue::Float(pitch))
        .extended("yaw", TelemetryValue::Float(yaw))
        .extended("wheel_speed_fl", TelemetryValue::Float(ws_fl))
        .extended("wheel_speed_fr", TelemetryValue::Float(ws_fr))
        .extended("wheel_speed_rl", TelemetryValue::Float(ws_rl))
        .extended("wheel_speed_rr", TelemetryValue::Float(ws_rr));

    if max_rpm > 0.0 {
        let rpm_fraction = (rpm / max_rpm).clamp(0.0, 1.0);
        builder = builder
            .max_rpm(max_rpm)
            .extended("rpm_fraction", TelemetryValue::Float(rpm_fraction));
    }

    Ok(builder.build())
}

/// Kylotonn WRC 9 / WRC 10 UDP telemetry adapter.
///
/// Listens on UDP port 64000 (default) and decodes the custom binary packet
/// format used by WRC 9 and WRC 10 FIA World Rally Championship.
pub struct WrcKylotonnAdapter {
    variant: WrcKylotonnVariant,
    bind_port: u16,
    update_rate: Duration,
}

impl WrcKylotonnAdapter {
    pub fn new(variant: WrcKylotonnVariant) -> Self {
        Self {
            variant,
            bind_port: DEFAULT_PORT,
            update_rate: Duration::from_millis(16),
        }
    }

    pub fn with_port(mut self, port: u16) -> Self {
        self.bind_port = port;
        self
    }
}

#[async_trait]
impl TelemetryAdapter for WrcKylotonnAdapter {
    fn game_id(&self) -> &str {
        self.variant.game_id()
    }

    async fn start_monitoring(&self) -> Result<TelemetryReceiver> {
        let (tx, rx) = mpsc::channel(100);
        let bind_port = self.bind_port;
        let update_rate = self.update_rate;
        let game_id = self.variant.game_id();

        tokio::spawn(async move {
            let bind_addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, bind_port));
            let socket = match TokioUdpSocket::bind(bind_addr).await {
                Ok(s) => s,
                Err(e) => {
                    warn!("Failed to bind {game_id} UDP socket on port {bind_port}: {e}");
                    return;
                }
            };
            info!("{game_id} adapter listening on UDP port {bind_port}");
            let mut buf = [0u8; MAX_PACKET_SIZE];
            let mut frame_seq = 0u64;
            let timeout = (update_rate * 4).max(Duration::from_millis(25));

            loop {
                match tokio::time::timeout(timeout, socket.recv(&mut buf)).await {
                    Ok(Ok(len)) => match parse_packet(&buf[..len]) {
                        Ok(normalized) => {
                            let frame =
                                TelemetryFrame::new(normalized, telemetry_now_ns(), frame_seq, len);
                            if tx.send(frame).await.is_err() {
                                debug!("Receiver dropped, stopping {game_id} monitoring");
                                break;
                            }
                            frame_seq = frame_seq.saturating_add(1);
                        }
                        Err(e) => debug!("Failed to parse {game_id} packet: {e}"),
                    },
                    Ok(Err(e)) => warn!("{game_id} UDP receive error: {e}"),
                    Err(_) => debug!("No {game_id} telemetry data received (timeout)"),
                }
            }
            info!("Stopped {game_id} telemetry monitoring");
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
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_packet() -> Vec<u8> {
        vec![0u8; MIN_PACKET_SIZE]
    }

    fn write_f32(buf: &mut [u8], offset: usize, value: f32) {
        buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn write_u32(buf: &mut [u8], offset: usize, value: u32) {
        buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    #[test]
    fn wrc9_game_id() {
        assert_eq!(
            WrcKylotonnAdapter::new(WrcKylotonnVariant::Wrc9).game_id(),
            "wrc_9"
        );
    }

    #[test]
    fn wrc10_game_id() {
        assert_eq!(
            WrcKylotonnAdapter::new(WrcKylotonnVariant::Wrc10).game_id(),
            "wrc_10"
        );
    }

    #[test]
    fn rejects_short_packet() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = WrcKylotonnAdapter::new(WrcKylotonnVariant::Wrc10);
        assert!(adapter.normalize(&[0u8; MIN_PACKET_SIZE - 1]).is_err());
        Ok(())
    }

    #[test]
    fn zero_packet_returns_zero_speed_and_rpm() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = WrcKylotonnAdapter::new(WrcKylotonnVariant::Wrc10);
        let t = adapter.normalize(&make_packet())?;
        assert_eq!(t.speed_ms, 0.0);
        assert_eq!(t.rpm, 0.0);
        Ok(())
    }

    #[test]
    fn gear_zero_is_reverse() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = WrcKylotonnAdapter::new(WrcKylotonnVariant::Wrc10);
        let raw = make_packet();
        // gear field is u32 at OFF_GEAR; 0 = reverse
        let t = adapter.normalize(&raw)?;
        assert_eq!(t.gear, -1);
        Ok(())
    }

    #[test]
    fn gear_one_maps_to_first() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = WrcKylotonnAdapter::new(WrcKylotonnVariant::Wrc10);
        let mut raw = make_packet();
        write_u32(&mut raw, OFF_GEAR, 1);
        let t = adapter.normalize(&raw)?;
        assert_eq!(t.gear, 1);
        Ok(())
    }

    #[test]
    fn speed_extracted() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = WrcKylotonnAdapter::new(WrcKylotonnVariant::Wrc9);
        let mut raw = make_packet();
        write_f32(&mut raw, OFF_ROAD_SPEED_MS, 30.0);
        let t = adapter.normalize(&raw)?;
        assert!((t.speed_ms - 30.0).abs() < 0.001, "speed_ms={}", t.speed_ms);
        Ok(())
    }

    #[test]
    fn throttle_clamped() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = WrcKylotonnAdapter::new(WrcKylotonnVariant::Wrc9);
        let mut raw = make_packet();
        write_f32(&mut raw, OFF_THROTTLE, 5.0);
        let t = adapter.normalize(&raw)?;
        assert!(t.throttle >= 0.0 && t.throttle <= 1.0);
        Ok(())
    }

    #[test]
    fn steering_clamped() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = WrcKylotonnAdapter::new(WrcKylotonnVariant::Wrc9);
        let mut raw = make_packet();
        write_f32(&mut raw, OFF_STEERING, -5.0);
        let t = adapter.normalize(&raw)?;
        assert!(t.steering_angle >= -1.0 && t.steering_angle <= 1.0);
        Ok(())
    }

    #[test]
    fn rpm_fraction_present_when_max_rpm_nonzero() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = WrcKylotonnAdapter::new(WrcKylotonnVariant::Wrc10);
        let mut raw = make_packet();
        write_f32(&mut raw, OFF_RPM, 4000.0);
        write_f32(&mut raw, OFF_MAX_RPM, 8000.0);
        let t = adapter.normalize(&raw)?;
        assert!((t.max_rpm - 8000.0).abs() < 0.001);
        if let Some(TelemetryValue::Float(frac)) = t.extended.get("rpm_fraction") {
            assert!((*frac - 0.5).abs() < 0.001, "rpm_fraction={frac}");
        } else {
            return Err("rpm_fraction not found".into());
        }
        Ok(())
    }

    #[test]
    fn wheel_speeds_in_extended() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = WrcKylotonnAdapter::new(WrcKylotonnVariant::Wrc10);
        let mut raw = make_packet();
        write_f32(&mut raw, OFF_WHEEL_SPEED_FL, 10.0);
        write_f32(&mut raw, OFF_WHEEL_SPEED_FR, 11.0);
        write_f32(&mut raw, OFF_WHEEL_SPEED_RL, 12.0);
        write_f32(&mut raw, OFF_WHEEL_SPEED_RR, 13.0);
        let t = adapter.normalize(&raw)?;
        assert!(t.extended.contains_key("wheel_speed_fl"));
        assert!(t.extended.contains_key("wheel_speed_rr"));
        Ok(())
    }

    #[test]
    fn larger_packet_accepted() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = WrcKylotonnAdapter::new(WrcKylotonnVariant::Wrc10);
        // WRC 8 might send slightly more data — should still parse cleanly.
        let raw = vec![0u8; MIN_PACKET_SIZE + 16];
        assert!(adapter.normalize(&raw).is_ok());
        Ok(())
    }
}

#[cfg(test)]
mod proptest_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn parse_no_panic_on_arbitrary_bytes(
            data in proptest::collection::vec(any::<u8>(), 0..256)
        ) {
            let _ = parse_packet(&data);
        }
    }
}
